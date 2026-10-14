use super::*;
use crate::style::PaintXf;
use crate::vnode::Tag;

impl ComponentHost {
    /// Committed box (untracked — paint/a11y/tests, never effects).
    pub fn committed_box(&self, id: NodeId) -> Option<LayoutBox> {
        let rec = self.inner.rec.borrow();
        LayoutLedger::committed(&rec, id)
    }

    /// Settled box for effects (tracked — re-runs next frame on publish).
    pub fn settled_box(&self, id: NodeId) -> Option<LayoutBox> {
        let rec = self.inner.rec.borrow();
        self.inner.layout.borrow().settled(&rec, id)
    }

    /// Tracked layout generation read: subscribes the caller to layout publishes.
    pub fn track_layout_generation(&self) -> u64 {
        self.inner.layout.borrow().track_generation()
    }

    /// Settled box by debug label (tracked — subscribes to layout publish even if
    /// the node was not yet retained during initial mount).
    pub fn settled_box_by_debug(&self, debug: &str) -> Option<LayoutBox> {
        let _ = self.track_layout_generation();
        let id = self
            .inner
            .rec
            .borrow()
            .find_by_debug(debug)
            .into_iter()
            .next()?;
        let rec = self.inner.rec.borrow();
        self.inner.layout.borrow().settled(&rec, id)
    }

    /// Committed box by debug label (untracked — paint/a11y/overlays).
    pub fn committed_box_by_debug(&self, debug: &str) -> Option<LayoutBox> {
        let id = self
            .inner
            .rec
            .borrow()
            .find_by_debug(debug)
            .into_iter()
            .next()?;
        let rec = self.inner.rec.borrow();
        LayoutLedger::committed(&rec, id)
    }

    /// Effective box for paint/hit at `now` (decisions 521-522): the
    /// fence interpolator's current box while one runs, else `None`
    /// (callers fall back to the committed box — one accessor, one
    /// answer, never dual truth). Untracked, like `committed_box`;
    /// effects keep reading `settled_box` (engine truth, never the
    /// interpolated lag).
    pub fn effective_box(&self, id: NodeId, now: f64) -> Option<LayoutBox> {
        let dpr = self.layout_config().device_pixel_ratio.max(f32::EPSILON);
        self.inner.evaluator.borrow().resolve_box(id, now, dpr)
    }

    /// Last LAYOUT run's engine stats (wrap round-trip counts live here).
    pub fn layout_stats(&self) -> LayoutStats {
        self.inner.layout.borrow().last_stats()
    }

    /// Current settled generation (untracked frame bookkeeping).
    pub fn layout_generation(&self) -> u64 {
        self.inner.layout.borrow().generation_value()
    }

    pub fn runtime(&self) -> Runtime {
        self.inner.rt.clone()
    }

    /// Latest committed diff (tests assert the commit stream's tail; every
    /// effect run commits, so multi-pass settles leave one diff per run).
    pub fn last_diff(&self) -> Option<TreeDiff> {
        self.inner.rec.borrow().last_diff()
    }

    /// Committed diffs from `index` on (paint-pass commit order).
    pub fn diffs_from(&self, index: usize) -> Vec<TreeDiff> {
        self.inner.rec.borrow().diffs_from(index)
    }

    pub fn diff_count(&self) -> usize {
        self.inner.rec.borrow().diff_count()
    }

    pub fn retained_count(&self) -> usize {
        self.inner.rec.borrow().retained_count()
    }

    /// True when the host runtime has outstanding frame demand.
    pub fn has_demand(&self) -> bool {
        self.inner.rt.has_demand()
    }

    /// Diagnostic query of which sources currently contribute to demand.
    pub fn demand_sources(&self) -> Vec<&'static str> {
        self.inner.rt.demand_sources()
    }

    /// Diagnostic query of labels for items currently in `pending`.
    pub fn pending_labels(&self) -> Vec<String> {
        self.inner.rt.pending_labels()
    }

    /// Test/diagnostic reads of retained payloads (what the A11Y/PAINT
    /// stubs would consume downstream).
    pub fn retained_handlers(&self, id: NodeId) -> Vec<(EventKind, HandlerId)> {
        self.inner
            .rec
            .borrow()
            .get(id)
            .map(|n| n.handlers.clone())
            .unwrap_or_default()
    }

    pub fn retained_semantics(&self, id: NodeId) -> Option<crate::semantics::Semantics> {
        self.inner
            .rec
            .borrow()
            .get(id)
            .and_then(|n| n.semantics.clone())
    }

    /// Retained image source for `Tag::Image` nodes (round 4.4 —
    /// what backends resolve through the cache). Test/diagnostic
    /// read, same shelf as the handlers/semantics readers above.
    pub fn retained_image(&self, id: NodeId) -> Option<crate::vnode::ImageId> {
        self.inner.rec.borrow().get(id).and_then(|n| n.image)
    }

    pub fn retained_style(&self, id: NodeId) -> Option<crate::interner::StyleId> {
        self.inner.rec.borrow().get(id).map(|n| n.style)
    }

    /// Override the keyed-state LRU capacity for this host's runtime
    /// (default 64; per-list derivation needs M8 namespacing — decision 50).
    pub fn set_keyed_capacity(&self, n: usize) {
        self.inner.rt.set_keyed_capacity(n);
    }

    /// Retained nodes carrying `TextField` or `TextArea` semantics,
    /// depth-first (U8 bind UX: the app layer matches these to its
    /// value signals — e.g. by label — and calls `bind_text` for
    /// shapes the fallback cannot see; session-owning controls
    /// self-wire through the [`feed_text_to_owner_session`](Self::feed_text_to_owner_session)
    /// fallback, so they need no app-layer bind call).
    pub fn text_fields(&self) -> Vec<NodeId> {
        let rec = self.inner.rec.borrow();
        let mut out = Vec::new();
        if let Some(root) = rec.root() {
            Self::text_walk(&rec, root, &mut out);
        }
        out
    }

    fn text_walk(rec: &Reconciler, id: NodeId, out: &mut Vec<NodeId>) {
        let Some(node) = rec.get(id) else {
            return;
        };
        if node.semantics.as_ref().is_some_and(|s| {
            s.role == crate::semantics::Role::TextField
                || s.role == crate::semantics::Role::TextArea
        }) {
            out.push(id);
        }
        for child in node.children.clone() {
            Self::text_walk(rec, child, out);
        }
    }

    /// Core-side hit-test over committed boxes (renderer-independent).
    /// Fenced boxes substitute while a fence runs (decisions 521-522
    /// — hit follows the lagging visual, never the committed target).
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeId> {
        let rec = self.inner.rec.borrow();
        let styles = self.inner.styles.borrow();
        let now = self.inner.rt.now_secs();
        let dpr = self.layout_config().device_pixel_ratio.max(f32::EPSILON);
        input::hit_test(
            &rec,
            &styles,
            x,
            y,
            &|id| {
                let retained = styles.get(rec.get(id)?.style).and_then(|s| s.transform);
                // Evaluator-live transforms win over retained ones (hit
                // follows the mid-flight visual at event time); settled
                // nodes resolve to their committed target here.
                Some(
                    self.inner
                        .evaluator
                        .borrow()
                        .resolve_transform(id, retained, now),
                )
            },
            &|id| self.inner.evaluator.borrow().resolve_box(id, now, dpr),
        )
    }

    /// Accumulated paint transform from `id` up through its
    /// ancestors (decision 517 — full affine, rotation included):
    /// evaluator-live values win (same sampling as
    /// [`ComponentHost::hit_test`]), composing child inside parent
    /// down to the nearest enclosing `Portal` — or the root when none
    /// encloses. Portals reset accumulation (ancestors above a portal
    /// never move its content — the builder rule, and the DOM rule
    /// where portal content lives outside transformed ancestors'
    /// elements). Identity for untransformed chains (the byte-identical
    /// fast path — every consumer below is a no-op then). Untracked.
    pub fn accumulated_transform(&self, id: NodeId) -> PaintXf {
        let rec = self.inner.rec.borrow();
        let styles = self.inner.styles.borrow();
        let now = self.inner.rt.now_secs();
        let ev = self.inner.evaluator.borrow();
        let mut acc = PaintXf::identity();
        let mut cur = Some(id);
        while let Some(c) = cur {
            let Some(node) = rec.get(c) else {
                break;
            };
            let retained = styles.get(node.style).and_then(|s| s.transform);
            let t = ev.resolve_transform(c, retained, now);
            if !t.is_identity() {
                t.require_supported();
                if let Some(b) = &node.layout {
                    let local = t.to_full_affine(b.x + b.w * 0.5, b.y + b.h * 0.5);
                    acc = PaintXf::compose(local, acc);
                }
            }
            if node.tag == Tag::Portal {
                break;
            }
            cur = node.parent;
        }
        acc
    }

    /// Device-space point mapped into `id`'s local (layout) space
    /// (decision 517 — the inverse of the node's accumulated full
    /// paint transform, rotation included). Geometric consumers (field
    /// caret/drag, handle grabs, bar cells) read layout-space geometry
    /// through this, so taps land where the pixels are on every
    /// backend. Identity for untransformed chains (bit-exact — existing
    /// consumers behave exactly as before). Untracked.
    pub fn local_point(&self, id: NodeId, x: f32, y: f32) -> (f32, f32) {
        let acc = self.accumulated_transform(id);
        if acc.is_identity() {
            return (x, y);
        }
        acc.unmap_point(x, y)
    }
    /// then walks up through retained parents returning the first
    /// node carrying a `Scroll` handler — the inject-compatible
    /// contract (`kind_handler` refuses handlerless targets loudly,
    /// even feed-bound ones, so a feed-only node would hand the
    /// caller a guaranteed panic and is walked past, not returned).
    /// The canonical scrollable pairs both (`.on_scroll` + bound
    /// feed — every M8/DOM sweep does). `None` when the leaf misses
    /// or no scrollable ancestor exists (the runner's quiet-miss
    /// contract — scrolling dead space is not a wiring bug).
    /// Coordinates are device px (committed-box space, like every
    /// other hit-test caller).
    pub fn scroll_target_at(&self, x: f32, y: f32) -> Option<NodeId> {
        let rec = self.inner.rec.borrow();
        let mut curr = self.hit_test(x, y)?;
        loop {
            let node = rec.get(curr)?;
            if node.handlers.iter().any(|(k, _)| *k == EventKind::Scroll) {
                return Some(curr);
            }
            curr = node.parent?;
        }
    }

    /// Deterministic Tab order over the retained tree (press-handler
    /// nodes, depth-first pre-order).
    pub fn tab_order(&self) -> Vec<NodeId> {
        let rec = self.inner.rec.borrow();
        input::tab_order(&rec)
    }

    /// Test/diagnostic read of an instance's reactive flags as
    /// `(hovered, pressed, focused)`, untracked (never schedules).
    /// Tests assert these alongside the node-level router state.
    pub fn debug_instance_flags(&self, instance: u64) -> (bool, bool, bool) {
        let instances = self.inner.instances.borrow();
        let rec = instances
            .get(&instance)
            .expect("flag read on a dead instance");
        let read = |s: &Option<Signal<bool>>| s.clone().map(|sig| untrack(|| sig.get()));
        (
            read(&rec.hovered).unwrap_or(false),
            read(&rec.pressed).unwrap_or(false),
            read(&rec.focused).unwrap_or(false),
        )
    }

    /// Current router state (tests assert both this node level and the
    /// per-instance flag signals).
    pub fn hovered_node(&self) -> Option<NodeId> {
        self.inner.input.borrow().hover
    }

    /// Legacy capture read (M5): the lowest-id live capture
    /// (deterministic — single-pointer flows see their capture, so
    /// every M5 assertion holds byte-identically under G11).
    pub fn capture_node(&self) -> Option<NodeId> {
        self.inner
            .input
            .borrow()
            .captures
            .iter()
            .min_by_key(|(id, _)| **id)
            .map(|(_, node)| *node)
    }

    /// Capture of one pointer id (`None` = id not down).
    pub fn capture_node_for(&self, id: u32) -> Option<NodeId> {
        self.inner.input.borrow().captures.get(&id).copied()
    }

    /// Live capture count (diagnostics/tests — multi-pointer proof).
    pub fn capture_count(&self) -> usize {
        self.inner.input.borrow().captures.len()
    }

    /// Armed long-press holds (diagnostics/tests — disarmed arms
    /// keep their Down origin for the Up gesture decision but can no
    /// longer fire, so they do not count as armed).
    pub fn longpress_armed_count(&self) -> usize {
        self.inner
            .longpress
            .borrow()
            .values()
            .filter(|arm| !arm.disarmed)
            .count()
    }

    pub fn focused_node(&self) -> Option<NodeId> {
        self.inner.input.borrow().focus
    }

    /// Pointer cursor for the hovered node (Round 8.3, decision 299):
    /// the nearest self-or-ancestor `Style::cursor` (CSS-inherit rule —
    /// a label inside a button reads the button's hand). `None` means
    /// the platform arrow (no styled ancestor). Untracked.
    pub fn hover_cursor(&self) -> Option<CursorIcon> {
        let rec = self.inner.rec.borrow();
        let styles = self.inner.styles.borrow();
        let mut cur = self.inner.input.borrow().hover;
        while let Some(id) = cur {
            let node = rec.get(id)?;
            let style = styles.get(node.style).cloned().unwrap_or_default();
            if let Some(c) = style.cursor {
                return Some(c);
            }
            cur = node.parent;
        }
        None
    }

    /// Presenter-side read/write of the retained tree + style table (M4:
    /// the paint pass builds FramePlans through this — the builder drains
    /// STRUCTURE/STYLE/PAINT/TEXT masks here). Never runs component code.
    pub fn with_retained_mut<R>(
        &self,
        f: impl FnOnce(&mut Reconciler, &Interner<Style>) -> R,
    ) -> R {
        let mut rec = self.inner.rec.borrow_mut();
        let styles = self.inner.styles.borrow();
        f(&mut rec, &styles)
    }

    /// Test hook: instance identity (component symbol, child key, parent).
    /// Slot-keyed recycling tests assert the *slot's* instance persists
    /// across item swaps through this.
    pub fn instance_info(&self, instance: u64) -> Option<(SymbolHash, Option<u64>, Option<u64>)> {
        self.inner
            .instances
            .borrow()
            .get(&instance)
            .map(|rec| (rec.component, rec.key, rec.parent))
    }

    /// Test hook: look up an inline child instance without creating it.
    pub fn lookup_child(&self, parent: u64, key: u64) -> Option<u64> {
        self.inner
            .instances
            .borrow()
            .get(&parent)
            .and_then(|rec| rec.children.get(&key).copied())
    }
}
