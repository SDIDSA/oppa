//! Reconciler: old VNode tree + new VNode tree → [`TreeDiff`]
//! (DESIGN §2.2 renderer contract, locked #4/#13).
//!
//! Keying is **slot-keyed, not item-keyed** (lock #13): children carrying the
//! same `key` diff in place no matter what item they now display. Recycling
//! is not a pool — it is the reconciler emitting *nothing* when a slot's
//! key is unchanged and only value payloads moved. A pure offset change
//! therefore yields zero structure ops by construction, and a sub-row
//! offset change (all binding values equal under the memo gate) yields zero
//! ops at all.
//!
//! PassMask mapping (what drives LAYOUT/PAINT/A11Y stubs downstream):
//!
//! - Add / Remove / Move → `STRUCTURE | LAYOUT | PAINT` on the node; the
//!   parent takes `LAYOUT` (its child list changed).
//! - Style change → `STYLE | PAINT`, plus `LAYOUT` when a layout-affecting
//!   field (`w/h/x/absolute_y/fill_width/fill_height/pad_x/margin_x/margin_y/gap/content_size`) moved.
//! - Text change → `TEXT | PAINT`. Semantics change → `SEMANTICS`.
//! - Handler kind-set change → `PAINT` (the commit carrier; the payload
//!   itself rides the diff).
//!
//! Handler identity (M2 rule): a node's handler identity is
//! `(NodeId, EventKind)` — one handler per kind per node. Re-runs rebind
//! fresh closures under the *retained* ids, so steady-state commits carry
//! no handler churn and the registry stays bounded. A changed kind-set
//! emits an update. Per-kind multiples and payload-carrying routing are M5
//! input-scope, stated, not assumed.
//!
//! `suppress_transitions` (§9.4, lock #22) arrives as data: the host passes
//! the scheduler's per-commit binding-edge flag in, and the diff carries it
//! for exactly that commit. The TIME interpolator / CSS mapping that honors
//! it is M8.

use std::collections::{HashMap, HashSet};

use crate::arena::{NodeArena, NodeId};
use crate::handlers::HandlerId;
use crate::interner::{Interner, StyleId};
use crate::layout::{LayoutBox, MeasuredText};
use crate::pass_mask::PassMask;
use crate::reactive::Runtime;
use crate::semantics::Semantics;
use crate::shell::EventKind;
use crate::style::Style;
use crate::vnode::{Element, Tag, TextClass, VNode};

/// Retained node: what renderers and layout see (DESIGN §2.2). Identity is
/// the arena id; handlers are ids only (locked #11 — copyable by id,
/// serializable, hot-reload-safe).
///
/// M3: the engine-owned [`LayoutBox`] lives here (DESIGN §2.2's sketch),
/// written by the LAYOUT phase only, plus the text measure cache. Both are
/// reconciler-side state, so hot swaps never touch them (lock #25): nodes
/// that survive a swap (slot-keyed reuse) keep their boxes; replaced nodes
/// are re-measured through the dirty-mask path.
#[derive(Debug)]
pub struct RetainedNode {
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub tag: Tag,
    pub debug: String,
    pub key: Option<u64>,
    /// Owning component instance (decision 519): the instance whose run
    /// built this node (innermost `Ctx::child` wins at stamp time;
    /// text leaves inherit the parent owner). The host's
    /// subtree-unmount sweep reads this to identify dead instances;
    /// layout/paint/hit never consult it, so plans stay byte-identical.
    pub owner: Option<u64>,
    pub style: StyleId,
    pub text: Option<std::sync::Arc<str>>,
    pub text_hint: Option<TextClass>,
    /// Span table for multi-span text (Phase 36 PR3, decision 355):
    /// `Some` exactly on `Tag::Text` nodes built from
    /// [`VNode::RichText`](crate::vnode::VNode) — each span shapes
    /// with its own weight and paints with its own ink. Diffed like
    /// text (`TEXT|PAINT`); `None` elsewhere.
    pub rich_text: Option<Vec<crate::vnode::TextSpan>>,
    /// Image source for `Tag::Image` nodes (round 4.4): the cache id
    /// whose key names the image (URL on web, cache key natively).
    /// Diffed — a src change re-renders. `None` elsewhere.
    pub image: Option<crate::vnode::ImageId>,
    /// Vector payload for `Tag::Path` nodes (decision 291): the SVG
    /// data plus its fill/stroke. Diffed — a payload change repaints
    /// (never re-lays-out). `None` elsewhere.
    pub path: Option<crate::vnode::PathSpec>,
    /// Canvas payload for `Tag::Canvas` nodes (Phase 36 PR4, decision
    /// 358): retained vector ops in local space. Diffed — a payload
    /// change repaints (never re-lays-out: geometry comes from the
    /// style box). `None` elsewhere.
    pub canvas: Option<crate::vnode::CanvasSpec>,
    pub semantics: Option<Semantics>,
    pub handlers: Vec<(EventKind, HandlerId)>,
    pub pass_dirty: PassMask,
    /// Committed layout box (None until the first LAYOUT pass covers the
    /// node). Written by the layout engine only; read by renderers, a11y,
    /// and settled-metric effects.
    pub layout: Option<LayoutBox>,
    /// Cached text measurement (keyed by bytes + resolved style, so flag
    /// dirt alone never re-shapes). Engine-private.
    pub(crate) measured: Option<MeasuredText>,
}

/// One structural or payload edit in a commit.
#[derive(Clone, Debug)]
pub enum DiffOp {
    Add {
        id: NodeId,
        parent: Option<NodeId>,
        index: usize,
        tag: Tag,
        key: Option<u64>,
    },
    Remove {
        id: NodeId,
        /// Owner snapshot at retire time (decision 519): the arena
        /// retires the node in the same step, so the host cannot look
        /// the owner up later — the op carries it. `None` = unstamped
        /// (hand-built trees never stamp; nothing owns them).
        owner: Option<u64>,
    },
    Move {
        id: NodeId,
        new_index: usize,
    },
    Update {
        id: NodeId,
        mask: PassMask,
        style_changed: bool,
        text_changed: bool,
        image_changed: bool,
        /// Vector payload changed (decision 291 — always rides
        /// `PAINT`; path data never affects layout).
        path_changed: bool,
        /// Canvas payload changed (Phase 36 PR4, decision 358 —
        /// always rides `PAINT`; spec ops never affect layout).
        canvas_changed: bool,
        semantics_changed: bool,
        handlers_changed: bool,
    },
}

impl DiffOp {
    pub fn is_structure(&self) -> bool {
        !matches!(self, DiffOp::Update { .. })
    }
}

/// One pinned node of a deferred exit (decision 520): everything the
/// paint paths need after the arena retires the ids — by value, so
/// arena generation recycling can never alias a live node
/// ("generation-pinned"). Handlers and semantics ride no exit
/// (ghosts take no input and announce nothing).
#[derive(Clone, Debug)]
pub struct ExitNode {
    /// The retired id. Evaluator exit tracks key on this; recycled
    /// arena slots mint fresh generations, so keys never collide
    /// with live nodes.
    pub id: NodeId,
    pub tag: Tag,
    /// Debug label (re-show cancel matches `(owner, debug)` — the
    /// same instance rendering the same-labeled root supersedes its
    /// live exit; collisions over-cancel benignly toward live paint).
    pub debug: String,
    pub style: StyleId,
    pub owner: Option<u64>,
    /// Pinned committed box (layout closes ranks without the removed
    /// subtree — the ghost paints where it was, never where layout
    /// would put it now).
    pub layout: LayoutBox,
    pub text: Option<std::sync::Arc<str>>,
    pub text_hint: Option<TextClass>,
    pub rich_text: Option<Vec<crate::vnode::TextSpan>>,
    pub image: Option<crate::vnode::ImageId>,
    pub path: Option<crate::vnode::PathSpec>,
    pub canvas: Option<crate::vnode::CanvasSpec>,
    pub children: Vec<ExitNode>,
}

impl ExitNode {
    /// Every pinned id in preorder (track synthesis + release).
    pub fn ids(&self) -> Vec<NodeId> {
        let mut out = vec![self.id];
        for child in &self.children {
            out.extend(child.ids());
        }
        out
    }

    /// Every pinned owner (the 519 sweep exemption).
    pub fn owners(&self) -> Vec<Option<u64>> {
        let mut out = vec![self.owner];
        for child in &self.children {
            out.extend(child.owners());
        }
        out
    }
}

/// One deferred exit (decision 520): the pinned subtree of an
/// outermost declaring removed root. The host stores these past
/// removal (paint past removal + evaluator drive); paint, hit-test,
/// and layout each treat them per the decision-520 contract.
#[derive(Clone, Debug)]
pub struct ExitPin {
    /// Exit timing from the declaring style.
    pub exit: crate::style::Transition,
    /// The live parent at removal (ink-seed for paint walks; `None`
    /// for whole-tree removal).
    pub parent: Option<NodeId>,
    pub root: ExitNode,
}

/// Per-commit edit script: payload deltas plus, for exactly one commit, the
/// binding-edge stamp.
#[derive(Clone, Debug, Default)]
pub struct TreeDiff {
    pub ops: Vec<DiffOp>,
    pub suppress_transitions: bool,
    /// Owners displaced without a `Remove` (decision 519): slot-keyed
    /// recycling can pair a surviving node with a new owner's VNode
    /// (conditional branches sharing one unkeyed slot), transferring
    /// ownership with no removal op. The host treats these like
    /// `Remove` owners when hunting dead instances (a displaced owner
    /// with no other live node is dead too).
    pub displaced_owners: Vec<u64>,
    /// Deferred exits pinned by this commit (decision 520): one per
    /// outermost declaring removed root, in removal order. The host
    /// evaluator synthesizes the fade tracks from these (same diffs,
    /// same order — deterministic twins on every backend).
    pub exits: Vec<ExitPin>,
}

impl TreeDiff {
    pub fn structure_ops(&self) -> usize {
        self.ops.iter().filter(|op| op.is_structure()).count()
    }

    pub fn update_ops(&self) -> usize {
        self.ops.len() - self.structure_ops()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

pub struct Reconciler {
    arena: NodeArena<RetainedNode>,
    root: Option<NodeId>,
    current: Option<VNode>,
    diffs: Vec<TreeDiff>,
    /// Set when a removal retired a node holding a committed box (the
    /// layout ledger treats it as a settled change).
    boxes_dropped: bool,
}

impl Reconciler {
    pub fn new() -> Self {
        Self {
            arena: NodeArena::new(),
            root: None,
            current: None,
            diffs: Vec::new(),
            boxes_dropped: false,
        }
    }

    /// Reconciles `new` against the last committed tree, retires/builds
    /// retained nodes, registers pending handler closures, sets pass-dirty
    /// flags, and pushes one [`TreeDiff`]. `binding_fired` is the
    /// scheduler's per-commit flag (consumed here, not peeked).
    pub fn reconcile(
        &mut self,
        rt: &Runtime,
        styles: &mut Interner<Style>,
        binding_fired: bool,
        new: VNode,
    ) -> TreeDiff {
        let old = self.current.take();
        let mut diff = TreeDiff {
            ops: Vec::new(),
            suppress_transitions: binding_fired,
            displaced_owners: Vec::new(),
            exits: Vec::new(),
        };
        match (old, new) {
            (None, VNode::Hole) => {
                self.root = None;
                self.current = Some(VNode::Hole);
            }
            (None, n) => {
                let id = self.build_subtree(rt, styles, None, &n, &mut diff);
                self.root = Some(id);
                self.current = Some(n);
            }
            (Some(_), VNode::Hole) => {
                if let Some(root) = self.root.take() {
                    self.remove_subtree(styles, root, &mut diff);
                }
                self.current = Some(VNode::Hole);
            }
            (Some(o), n) => {
                if !compatible(&o, &n) {
                    if let Some(root) = self.root.take() {
                        self.remove_subtree(styles, root, &mut diff);
                    }
                    let id = self.build_subtree(rt, styles, None, &n, &mut diff);
                    self.root = Some(id);
                } else {
                    let root = self.root.expect("reconciler has a tree but no root");
                    self.diff_node(rt, styles, root, &o, &n, &mut diff);
                }
                self.current = Some(n);
            }
        }
        self.diffs.push(diff.clone());
        diff
    }

    pub fn last_diff(&self) -> Option<TreeDiff> {
        self.diffs.last().cloned()
    }

    /// Diffs from `index` on (the paint pass commits every new diff in
    /// order — the tail alone would drop commits when a frame settles
    /// through multiple effect runs).
    pub fn diffs_from(&self, index: usize) -> Vec<TreeDiff> {
        self.diffs
            .get(index.min(self.diffs.len())..)
            .unwrap_or(&[])
            .to_vec()
    }

    pub fn diff_count(&self) -> usize {
        self.diffs.len()
    }

    pub fn retained_count(&self) -> usize {
        self.arena.len()
    }

    pub fn find_by_debug(&self, debug: &str) -> Vec<NodeId> {
        self.arena
            .iter_alive()
            .filter(|(_, n)| n.debug == debug)
            .map(|(id, _)| NodeId::from_gen(id))
            .collect()
    }

    /// Test/diagnostic read of a retained node.
    pub fn get(&self, id: NodeId) -> Option<&RetainedNode> {
        self.arena.try_get(id.gen()).ok()
    }

    /// The retained root (the layout engine's traversal entry).
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    /// Outermost overlay portals in tree (document) order (Round 1.4,
    /// decision 255): pre-order walk that does not descend into found
    /// portals, so nested portals emit/hit with their parent. Hit
    /// testing iterates this reversed (latest portal wins ties — the
    /// later-sibling rule, lifted to layers); the FramePlan builder
    /// emits in this order (later portals paint on top).
    pub fn outermost_portals(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let Some(root) = self.root else {
            return out;
        };
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let Some(n) = self.get(id) else {
                continue;
            };
            if n.tag == crate::vnode::Tag::Portal {
                out.push(id);
                continue;
            }
            for child in n.children.iter().rev() {
                stack.push(*child);
            }
        }
        out
    }

    /// Engine-side mutable access (M3 layout writes boxes + measure cache).
    pub(crate) fn node_mut(&mut self, id: NodeId) -> Option<&mut RetainedNode> {
        self.arena.try_get_mut(id.gen()).ok()
    }

    /// Every live retained id (the engine's dirty-set scan).
    pub(crate) fn alive_ids(&self) -> Vec<NodeId> {
        self.arena
            .iter_alive()
            .map(|(id, _)| NodeId::from_gen(id))
            .collect()
    }

    /// Every live retained id — public read for presenter-side FramePlan
    /// builders (M4: the builder walks the tree through this + [`get`](Reconciler::get)).
    pub fn retained_ids(&self) -> Vec<NodeId> {
        self.alive_ids()
    }

    /// Takes the given pass-mask bits from every dirty node, returning
    /// `(id, mask-before-clear)` per touched node. The M4 damage
    /// discipline input: the FramePlan builder drains
    /// `STRUCTURE|STYLE|PAINT|TEXT` here (LAYOUT stays owned by M3's
    /// engine run, SEMANTICS by the A11Y phase).
    pub fn take_paint_masks(&mut self, mask: PassMask) -> Vec<(NodeId, PassMask)> {
        let mut out = Vec::new();
        for id in self.alive_ids() {
            let dirty = match self.arena.try_get(id.gen()) {
                Ok(n) => n.pass_dirty,
                Err(_) => continue,
            };
            if dirty.intersects(mask) {
                let hit = PassMask::from_bits(dirty.bits() & mask.bits());
                if let Ok(n) = self.arena.try_get_mut(id.gen()) {
                    n.pass_dirty = n.pass_dirty.without(mask);
                }
                out.push((id, hit));
            }
        }
        out
    }

    /// Marks `ids` PAINT-dirty (M8: the TIME animation re-dirties live
    /// interpolations every frame so their progress rebuilds through the
    /// builder and re-splices into backend retained lists — without this
    /// the surface would freeze on the first interpolated frame, since
    /// interpolation progress itself writes no signals). Retired ids are
    /// skipped (the evaluator prunes them separately); returns the marked
    /// count.
    pub fn mark_paint_dirty(&mut self, ids: &[NodeId]) -> usize {
        let mut marked = 0;
        for id in ids {
            if let Ok(n) = self.arena.try_get_mut(id.gen()) {
                n.pass_dirty |= PassMask::PAINT;
                marked += 1;
            }
        }
        marked
    }

    /// Marks `ids` LAYOUT-dirty (decision 248: a viewport change
    /// re-roots the whole layout pass — positions derive from the
    /// viewport, so refit alone leaves every box stale). PAINT rides
    /// along (fresh-mount rule: a re-laid-out subtree always
    /// rebuilds its plan). Retired ids are skipped, same as
    /// [`mark_paint_dirty`](Self::mark_paint_dirty); returns the
    /// marked count.
    pub fn mark_layout_dirty(&mut self, ids: &[NodeId]) -> usize {
        let mut marked = 0;
        for id in ids {
            if let Ok(n) = self.arena.try_get_mut(id.gen()) {
                n.pass_dirty |= PassMask::LAYOUT | PassMask::PAINT;
                marked += 1;
            }
        }
        marked
    }

    /// Takes the removal-dropped-boxes flag (the ledger's publish input).
    pub(crate) fn take_boxes_dropped(&mut self) -> bool {
        std::mem::replace(&mut self.boxes_dropped, false)
    }

    // -- construction --------------------------------------------------------

    fn build_subtree(
        &mut self,
        rt: &Runtime,
        styles: &mut Interner<Style>,
        parent: Option<NodeId>,
        vnode: &VNode,
        diff: &mut TreeDiff,
    ) -> NodeId {
        match vnode {
            VNode::Hole => panic!("build_subtree reached Hole — holes filter at the child list"),
            VNode::Fragment(_) => {
                panic!("M2 roots and keyed slots hold single Elements; fragments only compose child lists")
            }
            VNode::Text(text) => {
                // Decision 519: text leaves carry no stamp — they inherit
                // the parent owner (attribution for hand-built trees).
                let owner = self.parent_owner(parent);
                let id = self.alloc_node(NewNode {
                    parent,
                    tag: Tag::Text,
                    debug: "text",
                    key: None,
                    owner,
                    style: styles.intern(Style::default()),
                    text: Some(text.clone()),
                    text_hint: None,
                    rich_text: None,
                    image: None,
                    path: None,
                    canvas: None,
                    semantics: None,
                    handlers: Vec::new(),
                });
                let index = self.child_index(parent, id);
                diff.ops.push(DiffOp::Add {
                    id,
                    parent,
                    index,
                    tag: Tag::Text,
                    key: None,
                });
                // M4 finding F2: fresh text leaves carried no dirty flags
                // (the Element arm sets STRUCTURE|LAYOUT|PAINT; this arm set
                // nothing — unobservable while no consumer read paint
                // masks). The documented Add mapping covers all fresh
                // nodes, so text leaves get the same bits.
                self.arena.get_mut(id.gen()).pass_dirty =
                    PassMask::STRUCTURE | PassMask::LAYOUT | PassMask::PAINT;
                id
            }
            VNode::RichText(rich) => {
                // Multi-span leaf (Phase 36 PR3): same retained shape
                // as a text leaf (`Tag::Text`, joined bytes) plus the
                // span table — backends key off the tag, layout shapes
                // per span. Same dirty bits as text leaves (the M4 F2
                // rule — fresh nodes feed every pass).
                let joined: std::sync::Arc<str> = rich.joined_text().into();
                // Decision 519: same inheritance as text leaves above.
                let owner = self.parent_owner(parent);
                let id = self.alloc_node(NewNode {
                    parent,
                    tag: Tag::Text,
                    debug: "rich",
                    key: None,
                    owner,
                    style: styles.intern(Style::default()),
                    text: Some(joined),
                    text_hint: Some(rich.style),
                    rich_text: Some(rich.spans.clone()),
                    image: None,
                    path: None,
                    canvas: None,
                    semantics: None,
                    handlers: Vec::new(),
                });
                let index = self.child_index(parent, id);
                diff.ops.push(DiffOp::Add {
                    id,
                    parent,
                    index,
                    tag: Tag::Text,
                    key: None,
                });
                self.arena.get_mut(id.gen()).pass_dirty =
                    PassMask::STRUCTURE | PassMask::LAYOUT | PassMask::PAINT;
                id
            }
            VNode::Element(e) => {
                let style_id = styles.intern(e.style.clone());
                // First commit: register pending closures under the builder
                // ids; the retained node adopts those ids (M2 identity rule).
                // The attachment's render-time owner stamp (M8, F6) beats
                // the running owner — drains happen in the root effect.
                for h in &e.handlers {
                    if let Some(f) = h.pending.borrow_mut().take() {
                        rt.register_handler_owned_as(h.id, f, h.owner.take());
                    }
                }
                let handler_ids: Vec<(EventKind, HandlerId)> =
                    e.handlers.iter().map(|h| (h.kind, h.id)).collect();
                // Decision 519: the element stamp wins (innermost
                // `Ctx::child` stamped it); unstamped elements inherit
                // the parent owner like text leaves.
                let owner = e.owner.get().or_else(|| self.parent_owner(parent));
                let id = self.alloc_node(NewNode {
                    parent,
                    tag: e.tag,
                    debug: &e.debug,
                    key: e.key,
                    owner,
                    style: style_id,
                    text: None,
                    text_hint: e.text_hint,
                    rich_text: None,
                    image: e.image,
                    path: e.path.clone(),
                    canvas: e.canvas.clone(),
                    semantics: e.semantics.clone(),
                    handlers: handler_ids,
                });
                let index = self.child_index(parent, id);
                diff.ops.push(DiffOp::Add {
                    id,
                    parent,
                    index,
                    tag: e.tag,
                    key: e.key,
                });
                for child in flattened_children(e) {
                    let cid = self.build_subtree(rt, styles, Some(id), child, diff);
                    self.arena.get_mut(id.gen()).children.push(cid);
                }
                // A fresh node is dirty for every pass it feeds.
                self.arena.get_mut(id.gen()).pass_dirty =
                    PassMask::STRUCTURE | PassMask::LAYOUT | PassMask::PAINT;
                id
            }
        }
    }

    fn alloc_node(&mut self, p: NewNode<'_>) -> NodeId {
        let id = self.arena.alloc(RetainedNode {
            parent: p.parent,
            children: Vec::new(),
            tag: p.tag,
            debug: p.debug.to_string(),
            key: p.key,
            owner: p.owner,
            style: p.style,
            text: p.text,
            text_hint: p.text_hint,
            rich_text: p.rich_text,
            image: p.image,
            path: p.path,
            canvas: p.canvas,
            semantics: p.semantics,
            handlers: p.handlers,
            pass_dirty: PassMask::EMPTY,
            layout: None,
            measured: None,
        });
        NodeId::from_gen(id)
    }

    fn child_index(&self, parent: Option<NodeId>, id: NodeId) -> usize {
        match parent {
            None => 0,
            Some(p) => self
                .arena
                .get(p.gen())
                .children
                .iter()
                .position(|c| *c == id)
                .unwrap_or(0),
        }
    }

    /// Owner of the parent retained node (`None` at the root or for
    /// retired parents): text leaves and unstamped elements inherit
    /// this at build time (decision 519 — hand-built VNodes never
    /// stamp, so inheritance is the only attribution they get).
    fn parent_owner(&self, parent: Option<NodeId>) -> Option<u64> {
        parent.and_then(|p| self.arena.try_get(p.gen()).ok().and_then(|n| n.owner))
    }

    fn remove_subtree(&mut self, styles: &Interner<Style>, id: NodeId, diff: &mut TreeDiff) {
        // Decision 520: exit-deferral is per outermost declaring
        // removed root — when this entry node declares `Style.exit`,
        // the whole entry pins and fades (declaring descendants ride
        // frozen under the one envelope; a declaring node nested in a
        // non-declaring entry snaps with it, never ghosts over new
        // content). The snapshot lands before any retire below.
        if let Some(pin) = self.snapshot_exit(styles, id) {
            diff.exits.push(pin);
        }
        self.remove_one(id, diff);
    }

    /// Snapshots one exit pin (decision 520): `Some` exactly when
    /// `id` declares `Style.exit` and holds a committed box (unlaid
    /// nodes skip quietly — nothing ever painted, so fading is a
    /// no-op, never a snap of visible content).
    fn snapshot_exit(&self, styles: &Interner<Style>, id: NodeId) -> Option<ExitPin> {
        let node = self.arena.try_get(id.gen()).ok()?;
        let exit = styles.get(node.style).and_then(|s| s.exit)?;
        let parent = node.parent;
        let root = self.snapshot_node(id)?;
        Some(ExitPin { exit, parent, root })
    }

    /// Snapshots one pinned node plus its laid descendants (`None`
    /// for unlaid nodes — the branch prunes there).
    fn snapshot_node(&self, id: NodeId) -> Option<ExitNode> {
        let node = self.arena.try_get(id.gen()).ok()?;
        let layout = node.layout.clone()?;
        let mut children = Vec::with_capacity(node.children.len());
        for child in node.children.clone() {
            if let Some(snap) = self.snapshot_node(child) {
                children.push(snap);
            }
        }
        Some(ExitNode {
            id,
            tag: node.tag,
            debug: node.debug.clone(),
            style: node.style,
            owner: node.owner,
            layout,
            text: node.text.clone(),
            text_hint: node.text_hint,
            rich_text: node.rich_text.clone(),
            image: node.image,
            path: node.path.clone(),
            canvas: node.canvas.clone(),
            children,
        })
    }

    fn remove_one(&mut self, id: NodeId, diff: &mut TreeDiff) {
        let children = std::mem::take(&mut self.arena.get_mut(id.gen()).children);
        for child in children {
            self.remove_one(child, diff);
        }
        let parent = self.arena.get(id.gen()).parent;
        if let Some(p) = parent {
            self.arena.get_mut(p.gen()).children.retain(|c| *c != id);
            self.arena.get_mut(p.gen()).pass_dirty |= PassMask::LAYOUT;
        }
        if self.arena.get(id.gen()).layout.is_some() {
            self.boxes_dropped = true;
        }
        // Decision 519: snapshot the owner before retiring — the host
        // identifies dead instances from `Remove` owners (the arena
        // cannot answer after this line).
        let owner = self.arena.get(id.gen()).owner;
        let _ = self.arena.retire(id.gen());
        diff.ops.push(DiffOp::Remove { id, owner });
    }

    // -- diffing -------------------------------------------------------------

    fn diff_node(
        &mut self,
        rt: &Runtime,
        styles: &mut Interner<Style>,
        id: NodeId,
        old: &VNode,
        new: &VNode,
        diff: &mut TreeDiff,
    ) {
        match (old, new) {
            (VNode::Text(a), VNode::Text(b)) => {
                if a != b {
                    self.arena.get_mut(id.gen()).text = Some(b.clone());
                    self.arena.get_mut(id.gen()).pass_dirty |= PassMask::TEXT | PassMask::PAINT;
                    diff.ops.push(DiffOp::Update {
                        id,
                        mask: PassMask::TEXT | PassMask::PAINT,
                        style_changed: false,
                        text_changed: true,
                        image_changed: false,
                        path_changed: false,
                        canvas_changed: false,
                        semantics_changed: false,
                        handlers_changed: false,
                    });
                }
            }
            (VNode::RichText(a), VNode::RichText(b)) => {
                // Span tables diff like text (a span change re-shapes
                // and repaints; geometry re-derives through the TEXT
                // dirty path like every text change — never a silent
                // stale span).
                if a != b {
                    let joined: std::sync::Arc<str> = b.joined_text().into();
                    let n = self.arena.get_mut(id.gen());
                    n.text = Some(joined);
                    n.text_hint = Some(b.style);
                    n.rich_text = Some(b.spans.clone());
                    n.pass_dirty |= PassMask::TEXT | PassMask::PAINT;
                    diff.ops.push(DiffOp::Update {
                        id,
                        mask: PassMask::TEXT | PassMask::PAINT,
                        style_changed: false,
                        text_changed: true,
                        image_changed: false,
                        path_changed: false,
                        canvas_changed: false,
                        semantics_changed: false,
                        handlers_changed: false,
                    });
                }
            }
            (VNode::Element(a), VNode::Element(b)) => {
                debug_assert_eq!(a.tag, b.tag, "incompatible tags must replace, not diff");
                if a.debug != b.debug {
                    self.arena.get_mut(id.gen()).debug = b.debug.clone();
                }
                // Decision 519: ownership follows the latest stamp —
                // slot-keyed recycling can re-home a surviving node
                // under a new owner's VNode with no removal op. The
                // displaced owner is recorded for the host sweep (dead
                // exactly when it owns no other live node).
                if let Some(stamp) = b.owner.get() {
                    let cur = self.arena.get(id.gen()).owner;
                    if cur != Some(stamp) {
                        if let Some(old) = cur {
                            diff.displaced_owners.push(old);
                        }
                        self.arena.get_mut(id.gen()).owner = Some(stamp);
                    }
                }
                let mut mask = PassMask::EMPTY;
                let mut style_changed = false;
                let mut semantics_changed = false;
                let mut handlers_changed = false;
                let text_changed = false;
                // Image sources diff like style (round 4.4): a src
                // change is paint (new pixels/URL), never layout
                // (geometry comes from the style box, unchanged).
                let mut image_changed = false;
                if self.arena.get(id.gen()).image != b.image {
                    self.arena.get_mut(id.gen()).image = b.image;
                    mask |= PassMask::PAINT;
                    image_changed = true;
                }
                // Vector payloads diff like images (decision 291): new
                // data/fill/stroke repaints (geometry still comes from
                // the style box, so never layout).
                let mut path_changed = false;
                if self.arena.get(id.gen()).path != b.path {
                    self.arena.get_mut(id.gen()).path = b.path.clone();
                    mask |= PassMask::PAINT;
                    path_changed = true;
                }
                // Canvas payloads diff like paths (Phase 36 PR4,
                // decision 358): a spec change repaints, never
                // re-lays-out (geometry comes from the style box).
                let mut canvas_changed = false;
                if self.arena.get(id.gen()).canvas != b.canvas {
                    self.arena.get_mut(id.gen()).canvas = b.canvas.clone();
                    mask |= PassMask::PAINT;
                    canvas_changed = true;
                }

                let old_style = styles.intern(a.style.clone());
                let new_style = styles.intern(b.style.clone());
                if old_style != new_style {
                    style_changed = true;
                    mask |= PassMask::STYLE | PassMask::PAINT;
                    if style_layout_bits(&a.style) != style_layout_bits(&b.style) {
                        mask |= PassMask::LAYOUT;
                    }
                    self.arena.get_mut(id.gen()).style = new_style;
                }
                if a.text_hint != b.text_hint {
                    self.arena.get_mut(id.gen()).text_hint = b.text_hint;
                    // A hint change resizes measured text (M3) — it is
                    // layout-affecting, not paint-only.
                    mask |= PassMask::LAYOUT | PassMask::PAINT;
                    style_changed = true;
                }
                if a.semantics != b.semantics {
                    self.arena.get_mut(id.gen()).semantics = b.semantics.clone();
                    mask |= PassMask::SEMANTICS;
                    semantics_changed = true;
                }
                // Handler identity is (NodeId, kind): fresh closures from
                // this run rebind under the *retained* ids, so steady-state
                // commits carry no handler churn; only a changed kind-set
                // is an update.
                let old_handlers = self.arena.get(id.gen()).handlers.clone();
                let old_kinds: HashSet<EventKind> = old_handlers.iter().map(|(k, _)| *k).collect();
                let new_kinds: HashSet<EventKind> = b.handlers.iter().map(|h| h.kind).collect();
                if old_kinds != new_kinds {
                    let ids: Vec<(EventKind, HandlerId)> =
                        b.handlers.iter().map(|h| (h.kind, h.id)).collect();
                    for h in &b.handlers {
                        if let Some(f) = h.pending.borrow_mut().take() {
                            rt.register_handler_owned_as(h.id, f, h.owner.take());
                        }
                    }
                    self.arena.get_mut(id.gen()).handlers = ids;
                    mask |= PassMask::PAINT;
                    handlers_changed = true;
                } else {
                    let by_kind: HashMap<EventKind, HandlerId> = old_handlers.into_iter().collect();
                    for h in &b.handlers {
                        if let Some(rid) = by_kind.get(&h.kind) {
                            if let Some(f) = h.pending.borrow_mut().take() {
                                rt.register_handler_owned_as(*rid, f, h.owner.take());
                            }
                        }
                    }
                }

                if !mask.is_empty() {
                    self.arena.get_mut(id.gen()).pass_dirty |= mask;
                    diff.ops.push(DiffOp::Update {
                        id,
                        mask,
                        style_changed,
                        text_changed,
                        image_changed,
                        path_changed,
                        canvas_changed,
                        semantics_changed,
                        handlers_changed,
                    });
                }
                self.diff_children(rt, styles, id, a, b, diff);
            }
            _ => {
                panic!("diff_node reached incompatible pair — the child matcher must replace those")
            }
        }
    }

    fn diff_children(
        &mut self,
        rt: &Runtime,
        styles: &mut Interner<Style>,
        parent: NodeId,
        old_e: &Element,
        new_e: &Element,
        diff: &mut TreeDiff,
    ) {
        let old_children: Vec<&VNode> = flattened_children(old_e);
        let new_children: Vec<&VNode> = flattened_children(new_e);
        let retained_old: Vec<NodeId> = self.arena.get(parent.gen()).children.clone();
        debug_assert_eq!(
            retained_old.len(),
            old_children.len(),
            "retained children must mirror the last committed child list"
        );

        // Old key → (retained id, position in old list).
        let mut old_by_key: HashMap<u64, (NodeId, usize)> = HashMap::new();
        let mut old_unkeyed: Vec<(NodeId, usize)> = Vec::new();
        for (i, (rid, ov)) in retained_old.iter().zip(old_children.iter()).enumerate() {
            match ov.key() {
                Some(k) => {
                    old_by_key.insert(k, (*rid, i));
                }
                None => old_unkeyed.push((*rid, i)),
            }
        }

        let mut new_ids: Vec<NodeId> = Vec::with_capacity(new_children.len());
        let mut used_old: HashSet<usize> = HashSet::new();
        let mut unkeyed_cursor = 0;

        for nv in new_children {
            match nv.key() {
                Some(k) => match old_by_key.get(&k) {
                    Some((rid, oi)) => {
                        used_old.insert(*oi);
                        if compatible(old_children[*oi], nv) {
                            self.diff_node(rt, styles, *rid, old_children[*oi], nv, diff);
                            new_ids.push(*rid);
                        } else {
                            // Same slot key, incompatible content: replace.
                            self.remove_subtree(styles, *rid, diff);
                            let nid = self.build_subtree(rt, styles, Some(parent), nv, diff);
                            new_ids.push(nid);
                        }
                    }
                    None => {
                        let nid = self.build_subtree(rt, styles, Some(parent), nv, diff);
                        new_ids.push(nid);
                    }
                },
                None => {
                    if unkeyed_cursor < old_unkeyed.len() {
                        let (rid, oi) = old_unkeyed[unkeyed_cursor];
                        unkeyed_cursor += 1;
                        used_old.insert(oi);
                        if compatible(old_children[oi], nv) {
                            self.diff_node(rt, styles, rid, old_children[oi], nv, diff);
                            new_ids.push(rid);
                        } else {
                            self.remove_subtree(styles, rid, diff);
                            let nid = self.build_subtree(rt, styles, Some(parent), nv, diff);
                            new_ids.push(nid);
                        }
                    } else {
                        let nid = self.build_subtree(rt, styles, Some(parent), nv, diff);
                        new_ids.push(nid);
                    }
                }
            }
        }
        // Leftovers: old subtrees with no new counterpart are removed.
        for (i, rid) in retained_old.iter().enumerate() {
            if !used_old.contains(&i) {
                self.remove_subtree(styles, *rid, diff);
            }
        }
        // Keyed reorder without add/remove still moves retained slots.
        for (i, rid) in new_ids.iter().enumerate() {
            if position_of(&retained_old, *rid) != Some(i) {
                diff.ops.push(DiffOp::Move {
                    id: *rid,
                    new_index: i,
                });
                self.arena.get_mut(rid.gen()).pass_dirty |=
                    PassMask::STRUCTURE | PassMask::LAYOUT | PassMask::PAINT;
            }
        }
        let before = self.arena.get(parent.gen()).children.clone();
        if before != new_ids {
            self.arena.get_mut(parent.gen()).children = new_ids;
            self.arena.get_mut(parent.gen()).pass_dirty |= PassMask::LAYOUT;
        }
    }
}

impl Default for Reconciler {
    fn default() -> Self {
        Self::new()
    }
}

/// Retained-node construction parameters (keeps `alloc_node` under the
/// argument-count lint while the payload grows toward M3 layout fields).
struct NewNode<'a> {
    parent: Option<NodeId>,
    tag: Tag,
    debug: &'a str,
    key: Option<u64>,
    /// Owning component instance (decision 519 — see
    /// [`RetainedNode::owner`]).
    owner: Option<u64>,
    style: StyleId,
    text: Option<std::sync::Arc<str>>,
    text_hint: Option<TextClass>,
    rich_text: Option<Vec<crate::vnode::TextSpan>>,
    image: Option<crate::vnode::ImageId>,
    path: Option<crate::vnode::PathSpec>,
    canvas: Option<crate::vnode::CanvasSpec>,
    semantics: Option<Semantics>,
    handlers: Vec<(EventKind, HandlerId)>,
}

fn position_of(ids: &[NodeId], id: NodeId) -> Option<usize> {
    ids.iter().position(|c| *c == id)
}

/// Same-variant (and for elements, same-tag) pairs diff in place;
/// everything else replaces. Fragments never reach here (flattened).
fn compatible(old: &VNode, new: &VNode) -> bool {
    match (old, new) {
        (VNode::Text(_), VNode::Text(_)) => true,
        (VNode::RichText(_), VNode::RichText(_)) => true,
        (VNode::Element(a), VNode::Element(b)) => a.tag == b.tag,
        _ => false,
    }
}

/// Fragment-transparent, hole-filtered child view: holes reconcile to
/// absent (DESIGN §2.2), so they never hold retained slots.
fn flattened_children(e: &Element) -> Vec<&VNode> {
    let mut out = Vec::new();
    flatten_into(&e.children, &mut out);
    out
}

fn flatten_into<'a>(nodes: &'a [VNode], out: &mut Vec<&'a VNode>) {
    for n in nodes {
        match n {
            VNode::Fragment(fs) => flatten_into(fs, out),
            VNode::Hole => {}
            _ => out.push(n),
        }
    }
}

/// The layout-affecting subset of a style: changes here (and only here)
/// dirty LAYOUT on top of STYLE|PAINT.
fn style_layout_bits(s: &Style) -> impl Eq + '_ {
    // Nested: std implements `Eq` for tuples only up to 12 elements,
    // and the set outgrew a flat tuple (Decision 249) — nesting
    // preserves the element-wise comparison exactly. Round 11.1 adds
    // the per-side third row (paint-only fields — bg, borders,
    // gradients, ink, cursor, transitions, and all radii — stay out
    // like before: they never move layout).
    (
        (s.w, s.h, s.x, s.absolute_y, s.fill_width, s.fill_height),
        (
            s.pad_x,
            s.pad_y,
            s.margin_x,
            s.margin_y,
            s.gap,
            s.align_items,
            s.justify_content,
            s.flex_wrap,
            s.content_size,
        ),
        (
            s.pad_top,
            s.pad_bottom,
            s.pad_left,
            s.pad_right,
            s.margin_top,
            s.margin_bottom,
            s.margin_left,
            s.margin_right,
        ),
        // Phase 36 PR2a (decision 353): flex shares, clamps, and
        // grid templates/spans all move layout — a fourth row (the
        // tuple-nesting precedent above; `Vec<GridTrack>` is `Eq`).
        (
            s.flex_grow,
            s.flex_shrink,
            s.min_w,
            s.min_h,
            s.max_w,
            s.max_h,
            s.col_span,
            s.row_span,
        ),
        (s.grid_cols.clone(), s.grid_rows.clone()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vnode::{Div, Row};

    fn setup() -> (Runtime, Interner<Style>) {
        (Runtime::new(), Interner::new())
    }

    #[test]
    fn mount_then_value_change_yields_update_only() {
        let (rt, mut styles) = setup();
        let mut rec = Reconciler::new();
        let v1: VNode = Div("a").style(Style::new().size(10, 10)).build();
        let d1 = rec.reconcile(&rt, &mut styles, false, v1);
        assert!(d1.structure_ops() > 0, "mount must create structure");
        assert!(!d1.suppress_transitions);
        let v2: VNode = Div("a")
            .style(Style::new().size(10, 10).bg(crate::style::Color(1)))
            .build();
        let d2 = rec.reconcile(&rt, &mut styles, false, v2);
        assert_eq!(d2.structure_ops(), 0);
        assert_eq!(d2.update_ops(), 1);
        assert!(!d2.suppress_transitions);
    }

    #[test]
    fn binding_flag_stamps_exactly_one_commit() {
        let (rt, mut styles) = setup();
        let mut rec = Reconciler::new();
        rec.reconcile(&rt, &mut styles, false, Div("a").build());
        let d = rec.reconcile(&rt, &mut styles, true, Div("a").build());
        assert!(d.suppress_transitions);
        assert!(d.is_empty(), "stamp carries no phantom ops");
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").build());
        assert!(!d.suppress_transitions);
    }

    #[test]
    fn keyed_children_recycle_in_place() {
        let (rt, mut styles) = setup();
        let mut rec = Reconciler::new();
        let list = |labels: &[&str]| {
            Row("list").children(labels.iter().enumerate().map(|(slot, label)| {
                Row("slot")
                    .key(slot as u64)
                    .child(VNode::Text((*label).into()))
            }))
        };
        rec.reconcile(&rt, &mut styles, false, list(&["a", "b", "c"]));
        let n0 = rec.retained_count();
        let d = rec.reconcile(&rt, &mut styles, true, list(&["b", "c", "d"]));
        assert_eq!(
            d.structure_ops(),
            0,
            "slot keys unchanged → no structure ops"
        );
        assert_eq!(rec.retained_count(), n0, "no nodes created or destroyed");
        assert!(d.suppress_transitions);
    }

    /// Phase 36 PR3: `RichText` mounts as a `Tag::Text` node carrying
    /// the joined bytes plus the span table; span edits diff in place
    /// (`TEXT|PAINT`, no structure ops); identical spans diff empty.
    #[test]
    fn rich_text_mounts_joined_and_diffs_spans_in_place() {
        use crate::vnode::{RichText, TextSpan};
        let (rt, mut styles) = setup();
        let mut rec = Reconciler::new();
        let rich = |spans: Vec<TextSpan>| -> VNode {
            Div("w").child(VNode::RichText(RichText::new(spans)))
        };
        let d1 = rec.reconcile(
            &rt,
            &mut styles,
            false,
            rich(vec![TextSpan::new("ab"), TextSpan::new("cd")]),
        );
        assert!(d1.structure_ops() > 0, "mount creates structure");
        let text = rec.find_by_debug("rich");
        assert_eq!(text.len(), 1, "rich leaf retained");
        let n = rec.get(text[0]).expect("rich node");
        assert_eq!(n.tag, crate::vnode::Tag::Text);
        assert_eq!(n.text.as_deref(), Some("abcd"));
        assert_eq!(
            n.rich_text.as_ref().map(|s| s.len()),
            Some(2),
            "span table retained"
        );
        // Identical spans: empty diff.
        let d2 = rec.reconcile(
            &rt,
            &mut styles,
            false,
            rich(vec![TextSpan::new("ab"), TextSpan::new("cd")]),
        );
        assert!(d2.is_empty(), "identical spans diff empty");
        // Edited span: TEXT|PAINT update, no structure ops.
        let d3 = rec.reconcile(
            &rt,
            &mut styles,
            false,
            rich(vec![TextSpan::new("ab"), TextSpan::new("ce")]),
        );
        assert_eq!(d3.structure_ops(), 0);
        assert_eq!(d3.update_ops(), 1);
        let n = rec.get(text[0]).expect("rich node");
        assert_eq!(n.text.as_deref(), Some("abce"), "joined bytes follow");
    }
}
