//! M2 acceptance: the reconciler + component-authoring model.
//!
//! Component function names are PascalCase by design (Toggle, ContactList…),
//! mirroring the locked §4 examples verbatim — hence the file-level allow.

#![allow(non_snake_case)]

//! The two locked §4 examples (toggle switch, virtualized ContactList /
//! ContactRow) ported to run against the new reconciler, plus one test per
//! proof the round demands. Where the locked text cannot compile as written
//! on stable Rust, the port takes the minimal mechanical delta and the M2
//! ROUNDS entry names every one — nothing was silently reshaped.
//!
//! Mechanical deltas (full list in ROUNDS.md; the honest-reading rule):
//! D1 signal/memo reads spell `.get()`/`.read()`, not `()` — a named type
//!    cannot implement `Fn()` on stable Rust; the call syntax needs the M2b
//!    authoring macro's rewrite, which is out of M2 scope.
//! D2 component instantiation spells `ctx.child("Name", key, &props, Name)`
//!    (props by reference), not `ContactRow { item, .. }` — struct-literal
//!    syntax for fn components needs RSX-class sugar (explicitly deferred,
//!    BUILD-ORDER §5.2); props-by-ref is forced by opaque storage (a move
//!    out of borrowed storage cannot typecheck).
//! D3 `Store<ContactId, Contact>` (two parameters): one parameter cannot
//!    type both `get(index) -> ContactId` and `lookup(id) -> Contact`.
//! D4 childless-element chains end with one mechanical `.build()` (a bare
//!    chained builder is not a `VNode`); `Img`/`Text` field numbers spell
//!    `36.0` (integer literals never infer to `f32`).
//! D5 `emit(id)` carries no payload in M2 (payload routing is M5
//!    `InputEvent` scope); `offset` reads spell `.get()`/`.row()` on the
//!    `ScrollOffset` handle; theme/token tables are test-local values.
//! D6 `image_cache` is the core-side stub map (async decode is M2b/M4).

use oppa::{
    find_retained_by_debug, Color, ComponentHost, Ctx, Ease, Event, EventKind, HandlerId,
    ImageCache, MsExt, PassMask, Semantics, SharedString, Store, Style, Text, Transition, VNode,
};
use oppa_macros::{component, Props};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Round 4.4 (decision 267): image sources thread into retained
// nodes and diff on change (backends resolve the id through the
// cache — the `Img` conversion used to drop `src`).
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct ImgProps {
    src: oppa::Signal<oppa::ImageId>,
}

fn img_scene(_ctx: &Ctx, p: &ImgProps) -> VNode {
    VNode::from(oppa::Img {
        src: p.src.get(),
        size: 32.0,
        radius: 0.0,
    })
}

#[test]
fn image_src_threads_and_diffs() {
    let host = ComponentHost::new();
    let cache = ImageCache::new();
    let a = cache.load("img/a.png");
    let b = cache.load("img/b.png");
    assert_eq!(cache.key_of(a), Some("img/a.png".to_string()));
    assert_eq!(cache.key_of(oppa::ImageId(999)), None, "foreign id");
    let src = host.runtime().signal(a);
    host.mount("ImgScene", ImgProps { src: src.clone() }, img_scene);
    host.run_until_idle();
    let id = find_retained_by_debug(&host, "img")[0];
    assert_eq!(
        host.retained_image(id),
        Some(a),
        "src threads into the node"
    );
    // Same-src commit diffs nothing image-wise.
    let cursor = host.diff_count();
    host.run_until_idle();
    let quiet: Vec<_> = host
        .diffs_from(cursor)
        .iter()
        .flat_map(|d| d.ops.iter())
        .filter_map(|op| match op {
            oppa::DiffOp::Update { image_changed, .. } => Some(*image_changed),
            _ => None,
        })
        .collect();
    assert!(quiet.iter().all(|c| !c), "steady commits flag nothing");
    // Src change flags exactly the image bit (+paint) and lands.
    src.set(b);
    host.run_until_idle();
    let cursor2 = host.diff_count() - 1;
    let mut flagged = false;
    for diff in host.diffs_from(cursor2) {
        for op in &diff.ops {
            if let oppa::DiffOp::Update {
                image_changed,
                mask,
                ..
            } = op
            {
                if *image_changed {
                    flagged = true;
                    assert!(mask.contains(oppa::PassMask::PAINT), "src change repaints");
                }
            }
        }
    }
    assert!(flagged, "src change raises image_changed");
    assert_eq!(host.retained_image(id), Some(b), "new src lands");
}

// ---------------------------------------------------------------------------
// Shared fakes: contacts, theme tokens
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ContactId(u64);

#[derive(Clone, PartialEq, Debug)]
struct Contact {
    display_name: SharedString,
    status: SharedString,
    avatar_small: String,
}

impl Contact {
    fn placeholder() -> Self {
        Self {
            display_name: Arc::from(""),
            status: Arc::from(""),
            avatar_small: String::new(),
        }
    }
}

fn seed_contacts(n: usize) -> (Vec<ContactId>, HashMap<ContactId, Contact>) {
    let mut ids = Vec::with_capacity(n);
    let mut values = HashMap::new();
    for i in 0..n {
        let id = ContactId(i as u64);
        ids.push(id);
        values.insert(
            id,
            Contact {
                display_name: Arc::from(format!("Contact {i:03}")),
                status: Arc::from(if i % 3 == 0 { "online" } else { "offline" }),
                avatar_small: format!("av{}", i % 8),
            },
        );
    }
    (ids, values)
}

#[derive(Clone, Copy, Debug)]
struct ToggleTheme {
    track_disabled: Color,
    track_pressed: Color,
    track_hover: Color,
    track_on: Color,
    track_off: Color,
    knob: Color,
    knob_shadow: Color,
}

const THEME: ToggleTheme = ToggleTheme {
    track_disabled: Color(0x11_11_11),
    track_pressed: Color(0x22_22_22),
    track_hover: Color(0x33_33_33),
    track_on: Color(0x44_44_44),
    track_off: Color(0x55_55_55),
    knob: Color(0x66_66_66),
    knob_shadow: Color(0x77_77_77),
};

const SELECTION_BG: Color = Color(0x88_88_88);

// ---------------------------------------------------------------------------
// §4.1 Toggle switch port
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct ToggleProps {
    label: SharedString,
    initial: bool,
    enabled: bool,
    on_change: HandlerId,
    theme: ToggleTheme,
}

#[component]
fn Toggle(ctx: &Ctx, props: &ToggleProps) -> VNode {
    let is_on = ctx.signal(props.initial);
    let hovered = ctx.hovered();
    let pressed = ctx.pressed();
    let _focused = ctx.focused();

    // D1: reads spell `.get()`.
    let track = match (props.enabled, pressed.get(), hovered.get(), is_on.get()) {
        (false, _, _, _) => props.theme.track_disabled,
        (_, true, _, _) => props.theme.track_pressed,
        (_, _, true, _) => props.theme.track_hover,
        (_, _, _, true) => props.theme.track_on,
        _ => props.theme.track_off,
    };
    let knob_x = if is_on.get() { 23.0 } else { 3.0 };

    // D5: `emit` carries the handler id; the payload rides M5 routing.
    let rt = ctx.runtime();
    let on_change = props.on_change;
    let knob_bg = props.theme.knob;
    let knob_shadow = props.theme.knob_shadow;

    oppa::Div("track")
        .style(
            Style::new()
                .size(44, 24)
                .radius(12)
                .bg(track)
                .opacity(props.enabled.then_some(1.0))
                .transition(Transition::new(120.ms(), Ease::Out)),
        )
        .semantics(
            Semantics::switch()
                .checked(is_on.get())
                .label(&props.label)
                .disabled(!props.enabled),
        )
        .on_press(move || {
            is_on.set(!is_on.get());
            rt.dispatch(Event {
                kind: EventKind::Press,
                handler: on_change,
            });
        })
        .child(
            // D4: one mechanical `.build()` terminates the childless chain.
            oppa::Div("knob")
                .style(
                    Style::new()
                        .size(18, 18)
                        .circle()
                        .bg(knob_bg)
                        .x(knob_x)
                        .shadow(1, 2, knob_shadow),
                )
                .build(),
        )
}

// ---------------------------------------------------------------------------
// §4.2 Virtualized ContactList / ContactRow port
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct ListProps {
    store: Store<ContactId, Contact>,
    cache: ImageCache,
    viewport_h: f32,
    row_h: f32,
}

#[derive(Clone, Props)]
struct RowProps {
    store: Store<ContactId, Contact>,
    cache: ImageCache,
    item: oppa::Memo<Option<ContactId>>,
    selected: oppa::Signal<Option<ContactId>>,
    row_h: f32,
}

#[component]
fn ContactList(ctx: &Ctx, props: &ListProps) -> VNode {
    let offset = ctx.scroll_offset();
    let n_slots = (props.viewport_h / props.row_h).ceil() as usize + 2;
    let selected = ctx.signal(None::<ContactId>);

    oppa::ScrollArea("list")
        .content_size(props.store.len() as f32 * props.row_h)
        .style(Style::new().h(props.viewport_h).fill_width())
        .children((0..n_slots).map(|slot| {
            // Slot identity is stable; the *item binding* is what changes.
            let item = ctx.binding({
                let store = props.store.clone();
                let offset = offset.clone();
                let row_h = props.row_h;
                move || store.get(offset.row(row_h) + slot)
            });
            let row_props = RowProps {
                store: props.store.clone(),
                cache: props.cache.clone(),
                // D2: the row's params ride one props struct by reference.
                item: item.clone(),
                selected: selected.clone(),
                row_h: props.row_h,
            };
            oppa::Row("slot")
                .style(
                    Style::new()
                        .absolute_y(slot as f32 * props.row_h)
                        .h(props.row_h)
                        .fill_width(),
                )
                .key(slot as u64)
                .child(ctx.child("ContactRow", slot as u64, &row_props, ContactRow))
        }))
}

#[component]
fn ContactRow(ctx: &Ctx, props: &RowProps) -> VNode {
    // Re-derives whenever the slot re-binds OR the store mutates that id.
    let contact = ctx.memo({
        let store = props.store.clone();
        let item = props.item.clone();
        // D3b: out-of-window slots show the placeholder (the locked text
        // assumes in-range indexing; overscroll must typecheck too).
        move || {
            item.read()
                .and_then(|id| store.lookup(&id))
                .unwrap_or_else(Contact::placeholder)
        }
    });
    let avatar = ctx.memo({
        let cache = props.cache.clone();
        let contact = contact.clone();
        move || cache.load(&contact.read().avatar_small)
    });
    let is_sel = ctx.memo({
        let selected = props.selected.clone();
        let item = props.item.clone();
        move || selected.get() == item.read()
    });
    // The one new concept recycling asks of component authors (§4.2):
    // per-item side-state, surviving slot rebinds, LRU-evicted.
    let _slide = ctx.keyed_state::<u32>(props.item.read().map(|id| id.0).unwrap_or(u64::MAX), || 0);

    let selected = props.selected.clone();
    let item = props.item.clone();
    oppa::Row("cell")
        .style(
            Style::new()
                .h(props.row_h)
                .pad_x(12)
                .bg(is_sel.read().then_some(SELECTION_BG)),
        )
        .semantics(
            Semantics::list_item()
                .selected(is_sel.read())
                .label(&contact.read().display_name),
        )
        .on_press(move || {
            if let Some(id) = item.read() {
                selected.set(Some(id));
            }
        })
        .children([
            oppa::Img {
                src: avatar.read(),
                size: 36.0,
                radius: 18.0,
            }
            .into(),
            oppa::Column::new().gap(2).children([
                Text {
                    text: contact.read().display_name.clone(),
                    style: Text::title_small,
                }
                .into(),
                Text {
                    text: contact.read().status.clone(),
                    style: Text::body_secondary,
                }
                .into(),
            ]),
        ])
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn mount_list(slots_host: &ComponentHost, n_contacts: usize) -> oppa::MountHandle<ListProps> {
    let rt = slots_host.runtime();
    let (ids, values) = seed_contacts(n_contacts);
    let store = Store::new(&rt, ids, values);
    slots_host.mount(
        "ContactList",
        ListProps {
            store,
            cache: ImageCache::new(),
            viewport_h: 600.0,
            row_h: 56.0,
        },
        ContactList,
    )
}

fn press_retained(host: &ComponentHost, debug: &str, index: usize) -> HandlerId {
    let nodes = find_retained_by_debug(host, debug);
    assert!(!nodes.is_empty(), "no retained node named {debug}");
    let handlers = host.retained_handlers(nodes[index]);
    handlers
        .iter()
        .find(|(k, _)| *k == EventKind::Press)
        .map(|(_, id)| *id)
        .expect("press handler on retained node")
}

// ---------------------------------------------------------------------------
// Proof 1 — the toggle acceptance target runs end to end
// ---------------------------------------------------------------------------

#[test]
fn toggle_switch_runs_end_to_end() {
    let host = ComponentHost::new();
    let changes = Rc::new(Cell::new(0u32));
    let on_change = HandlerId::from_symbol("test.toggle.change");
    host.runtime().register_handler(on_change, {
        let changes = changes.clone();
        move || changes.set(changes.get() + 1)
    });

    let props = ToggleProps {
        label: Arc::from("Wi-Fi"),
        initial: false,
        enabled: true,
        on_change,
        theme: THEME,
    };
    let handle = host.mount("Toggle", props, Toggle);
    let d0 = host.last_diff().expect("mount commits");
    assert!(d0.structure_ops() > 0, "mount must create structure");
    assert!(!d0.suppress_transitions, "mount carries no binding stamp");

    // Semantics ride the same expression as the visuals (§4.1 table).
    let track = find_retained_by_debug(&host, "track");
    assert_eq!(track.len(), 1);
    let sem = host.retained_semantics(track[0]).expect("switch semantics");
    assert_eq!(sem.checked, Some(false));
    assert_eq!(sem.label.as_deref(), Some("Wi-Fi"));

    // Press through the registry (handlers-as-ids round-trip, lock #11).
    let press = press_retained(&host, "track", 0);
    let style_before = host.retained_style(track[0]);
    host.runtime().dispatch(Event {
        kind: EventKind::Press,
        handler: press,
    });
    host.run_until_idle();
    assert_eq!(changes.get(), 1, "on_change resolved through the registry");

    // Value-only change: STYLE updates, zero structure ops (lock #4 — the
    // §4.2 trace's diff half, asserted on the toggle first).
    let d1 = host.last_diff().expect("press commits");
    assert_eq!(
        d1.structure_ops(),
        0,
        "toggle flip must not touch structure"
    );
    assert!(d1.update_ops() >= 2, "track bg + knob x + semantics update");
    assert!(!d1.suppress_transitions, "no binding edge involved");
    // PassMask proof: the flip carries STYLE dirty flags (track bg swap) and
    // LAYOUT for the knob's `x` move (layout-affecting style subset) — the
    // flags the LAYOUT/PAINT stubs consume downstream.
    let mut saw_style = false;
    let mut saw_layout = false;
    for op in &d1.ops {
        if let oppa::DiffOp::Update { mask, .. } = op {
            saw_style |= mask.contains(PassMask::STYLE);
            saw_layout |= mask.contains(PassMask::LAYOUT);
        }
    }
    assert!(saw_style, "bg swap must dirty STYLE");
    assert!(saw_layout, "knob x move must dirty LAYOUT");
    assert_ne!(host.retained_style(track[0]), style_before);
    let sem = host.retained_semantics(track[0]).expect("switch semantics");
    assert_eq!(
        sem.checked,
        Some(true),
        "role/state cannot drift from visuals"
    );

    // Handler identity is (NodeId, kind): the retained id is stable across
    // re-runs, the fresh closure is rebound under it.
    let press2 = press_retained(&host, "track", 0);
    assert_eq!(press, press2, "handler id stable across commits");
    let _ = handle;
}

// ---------------------------------------------------------------------------
// Proof 2 — component-level cycle budget (M0's budget test, one level up)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct CycleLinkProps {
    read: oppa::Signal<i32>,
    write: oppa::Signal<i32>,
    limit: i32,
    tag: String,
}

#[component]
fn CycleLink(ctx: &Ctx, props: &CycleLinkProps) -> VNode {
    // Convergent write-back: only writes on actual change, so the loop
    // settles instead of livelocking on equal-value invalidations (signals
    // have no equality gate by lock — §9.1 gates are memo-only).
    let r = props.read.get();
    if r < props.limit && props.write.get() != r + 1 {
        props.write.set(r + 1);
    }
    // One signal per call site keeps the instance map exercised too.
    let _local = ctx.signal(0i32);
    oppa::Div(&props.tag).build()
}

#[derive(Clone, Props)]
struct CycleRootProps {
    s1: oppa::Signal<i32>,
    s2: oppa::Signal<i32>,
    limit: i32,
}

#[component]
fn CycleRoot(ctx: &Ctx, props: &CycleRootProps) -> VNode {
    let a = ctx.child(
        "CycleLink",
        1,
        &CycleLinkProps {
            read: props.s2.clone(),
            write: props.s1.clone(),
            limit: props.limit,
            tag: "a".to_string(),
        },
        CycleLink,
    );
    let b = ctx.child(
        "CycleLink",
        2,
        &CycleLinkProps {
            read: props.s1.clone(),
            write: props.s2.clone(),
            limit: props.limit,
            tag: "b".to_string(),
        },
        CycleLink,
    );
    oppa::Div("root").children([a, b])
}

#[test]
fn component_rerun_write_back_settles_within_budget() {
    let host = ComponentHost::new();
    let rt = host.runtime();
    let s1 = rt.signal(0i32);
    let s2 = rt.signal(0i32);
    host.mount(
        "CycleRoot",
        CycleRootProps {
            s1: s1.clone(),
            s2: s2.clone(),
            limit: 3,
        },
        CycleRoot,
    );
    host.run_until_idle();
    // A writes s1 from s2, B writes s2 from s1: the write-back converges
    // (s1 reaches the limit through A; B stops once s1 is there) within
    // the 3-pass budget (§9.1) — values, not just silence.
    assert_eq!(s1.get(), 3);
    assert_eq!(s2.get(), 2);
    assert!(
        rt.stats().passes_last_frame <= 3,
        "passes_last_frame = {}",
        rt.stats().passes_last_frame
    );
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "re-entry budget")]
fn component_rerun_divergence_asserts_like_m0() {
    let host = ComponentHost::new();
    let rt = host.runtime();
    let s1 = rt.signal(0i32);
    let s2 = rt.signal(0i32);
    host.mount(
        "CycleRoot",
        CycleRootProps {
            s1: s1.clone(),
            s2: s2.clone(),
            limit: i32::MAX,
        },
        CycleRoot,
    );
    // Never converges: past 3 passes the unsettled set is a reported bug
    // (debug-assert with cycle path), never a silent livelock — §9.1.
    host.run_until_idle();
}

/// Release twin of the divergence proof (M0's defer-then-park contract, one
/// level up): past the budget the dirt defers once, then parks — the
/// subtree stops updating with a logged reason, never a silent livelock.
#[test]
#[cfg(not(debug_assertions))]
fn component_rerun_divergence_defers_then_parks_like_m0() {
    let host = ComponentHost::new();
    let rt = host.runtime();
    let s1 = rt.signal(0i32);
    let s2 = rt.signal(0i32);
    host.mount(
        "CycleRoot",
        CycleRootProps {
            s1: s1.clone(),
            s2: s2.clone(),
            limit: i32::MAX,
        },
        CycleRoot,
    );
    let frames = host.run_until_idle();
    assert_eq!(
        frames, 2,
        "one frame attempts, one frame retries, then park"
    );
    let parked = s1.get();
    host.run_until_idle();
    assert_eq!(s1.get(), parked, "parked subtree stops updating");
}

// ---------------------------------------------------------------------------
// Proof 3 — slot-keyed recycling: the reconciler doing nothing
// ---------------------------------------------------------------------------

#[test]
fn slot_keyed_recycling_zero_structure_ops() {
    let host = ComponentHost::new();
    let handle = mount_list(&host, 200);
    let d0 = host.last_diff().expect("mount commits");
    assert!(d0.structure_ops() > 0);
    let n0 = host.retained_count();

    // Slot instances are keyed by slot, not item (lock #13).
    let root = handle.root_instance();
    let slot0 = host.lookup_child(root, 0).expect("slot 0 instance");
    let (sym, key, parent) = host.instance_info(slot0).expect("slot info");
    assert_eq!(sym, oppa::SymbolHash::of("ContactRow"));
    assert_eq!(key, Some(0));
    assert_eq!(parent, Some(root));

    // Pure offset change: +3 rows. Every slot rebinds; structure is silent.
    let scroll = host.instance_scroll(root).expect("list scroll seam");
    scroll.set(3.0 * 56.0);
    host.run_until_idle();
    let d = host.last_diff().expect("scroll commits");
    assert_eq!(
        d.structure_ops(),
        0,
        "slot keys unchanged → zero structure ops (recycle pool = doing nothing)"
    );
    assert!(d.update_ops() > 0, "rebound slots still repaint content");
    assert_eq!(host.retained_count(), n0, "no nodes created or destroyed");
    assert!(
        d.suppress_transitions,
        "rebind commit carries the one-commit stamp (§9.4)"
    );
    // Same slot key → same component instance across the item swap.
    assert_eq!(host.lookup_child(root, 0), Some(slot0));

    // Sub-row change: no slot rebinds (binding equality gate holds) → the
    // commit stream is silent — not even updates.
    let diffs_before = host.diff_count();
    scroll.set(3.0 * 56.0 + 14.0);
    host.run_until_idle();
    assert_eq!(
        host.diff_count(),
        diffs_before,
        "no rebind → no commit at all"
    );
}

// ---------------------------------------------------------------------------
// Proof 4 — rebind suppression, and its control case
// ---------------------------------------------------------------------------

#[test]
fn rebind_suppresses_transition_but_real_change_does_not() {
    let host = ComponentHost::new();
    let handle = mount_list(&host, 200);
    let root = handle.root_instance();
    let scroll = host.instance_scroll(root).expect("list scroll seam");

    // Control first: a real state change with no rebind (select row 0 via
    // its own retained press handler) must NOT suppress.
    let cell_press = press_retained(&host, "cell", 0);
    host.runtime().dispatch(Event {
        kind: EventKind::Press,
        handler: cell_press,
    });
    host.run_until_idle();
    let d_sel = host.last_diff().expect("selection commits");
    assert_eq!(d_sel.structure_ops(), 0);
    assert!(!d_sel.suppress_transitions, "real change stays animatable");
    assert!(d_sel.update_ops() >= 1, "the selected row restyles");

    // Then the rebind case mid-"transition": force a slot rebind and assert
    // the stamp lands on exactly that commit and clears after.
    scroll.set(5.0 * 56.0);
    host.run_until_idle();
    let d = host.last_diff().expect("rebind commits");
    assert_eq!(d.structure_ops(), 0);
    assert!(
        d.suppress_transitions,
        "rebound values jump, no interpolator"
    );
    let _ = handle;
}

// ---------------------------------------------------------------------------
// Proof 5 — keyed_state: survives rebind-back, evicts when far
// ---------------------------------------------------------------------------

#[test]
fn keyed_state_lru_survives_and_evicts() {
    let host = ComponentHost::new();
    let rt = host.runtime();

    // Fill to exactly capacity (64): key 1 + keys 2..=64. Key 1 is the
    // least-recently-used here — note the test must NOT re-read it before
    // the eviction step (a read is a touch, which would refresh it).
    rt.keyed_state::<u32>(1, || 0).set(5);
    for k in 2..=64u64 {
        rt.keyed_state::<u32>(k, || 0);
    }
    assert_eq!(rt.keyed_len(), 64);

    // "Sufficiently far" is concrete: capacity is 64 (KeyedStore::CAPACITY);
    // touching a 65th distinct key evicts the least-recently-used (key 1).
    rt.keyed_state::<u32>(65, || 0);
    assert!(!rt.keyed_contains::<u32>(1), "LRU eviction past capacity");
    assert!(rt.keyed_contains::<u32>(2), "recent keys survive");
    // An evicted key re-seeds to init — never stale, never shuffled.
    assert_eq!(rt.keyed_state::<u32>(1, || 0).get(), 0);

    // Rebind-then-rebind-back within capacity: same key, same state.
    rt.keyed_state::<u32>(2, || 0).set(7);
    for k in 100..110u64 {
        rt.keyed_state::<u32>(k, || 0);
    }
    assert_eq!(rt.keyed_state::<u32>(2, || 0).get(), 7);
}

#[test]
fn keyed_state_capacity_is_configurable_not_hardcoded() {
    let host = ComponentHost::new();
    let rt = host.runtime();
    assert_eq!(rt.keyed_capacity(), 64, "documented default");
    host.set_keyed_capacity(8);
    assert_eq!(rt.keyed_capacity(), 8);
    for k in 0..8u64 {
        rt.keyed_state::<u32>(k, || 0);
    }
    // Ninth distinct key under capacity 8 evicts the LRU (key 0) — the
    // mechanism follows the configured bound, not the number 64.
    rt.keyed_state::<u32>(8, || 0);
    assert!(
        !rt.keyed_contains::<u32>(0),
        "LRU follows configured capacity"
    );
    assert!(rt.keyed_contains::<u32>(7), "recent keys survive");
    assert_eq!(
        rt.keyed_state::<u32>(0, || 42).get(),
        42,
        "evicted re-seeds"
    );
    // Shrinking a live store evicts LRU-first down to the new bound.
    host.set_keyed_capacity(4);
    assert_eq!(rt.keyed_len(), 4);
}

#[derive(Clone, Props)]
struct ProbeProps {
    key: u64,
    init: u32,
    stamp: oppa::Signal<u32>,
}
#[component]
fn KeyProbe(ctx: &Ctx, props: &ProbeProps) -> VNode {
    let s = ctx.keyed_state(props.key, || props.init);
    props.stamp.set(s.get());
    oppa::Div("probe").build()
}

#[test]
fn keyed_state_survives_rebind_back_through_components() {
    let host = ComponentHost::new();
    let rt = host.runtime();
    let stamp = rt.signal(0u32);
    let handle = host.mount(
        "KeyProbe",
        ProbeProps {
            key: 7,
            init: 0,
            stamp: stamp.clone(),
        },
        KeyProbe,
    );
    assert_eq!(stamp.get(), 0);
    rt.keyed_state::<u32>(7, || 0).set(9);
    // Same key re-read through a fresh component run: value survived.
    handle.set_props(ProbeProps {
        key: 7,
        init: 0,
        stamp: stamp.clone(),
    });
    host.run_until_idle();
    assert_eq!(stamp.get(), 9, "rebind-back returns surviving state");

    // Flood past capacity → the component path re-seeds to init.
    for k in 100..170u64 {
        rt.keyed_state::<u32>(k, || 0);
    }
    handle.set_props(ProbeProps {
        key: 7,
        init: 0,
        stamp: stamp.clone(),
    });
    host.run_until_idle();
    assert_eq!(stamp.get(), 0, "far-out key evicted, re-seeded");
}

// ---------------------------------------------------------------------------
// Proof 6 — signal reseeding on body-edit ordering (round-5 rule)
// ---------------------------------------------------------------------------

thread_local! {
    static RESEED_BODY: RefCell<fn(&Ctx, &ReseedProps) -> VNode> = RefCell::new(reseed_v1);
}

#[derive(Clone, Props)]
struct ReseedProps {
    tick: oppa::Signal<i32>,
    seen: oppa::Signal<(i32, i32, i32)>,
}

fn reseed_v1(ctx: &Ctx, props: &ReseedProps) -> VNode {
    let _t = props.tick.get();
    let a = ctx.signal(10i32);
    let b = ctx.signal(20i32);
    a.set(11);
    b.set(21);
    props.seen.set((a.get(), b.get(), 0));
    oppa::Div("reseed").build()
}

fn reseed_v2(ctx: &Ctx, props: &ReseedProps) -> VNode {
    let _t = props.tick.get();
    // Body edit: one new signal inserted above — later sites shift.
    let n = ctx.signal(99i32);
    let a = ctx.signal(10i32);
    let b = ctx.signal(20i32);
    props.seen.set((n.get(), a.get(), b.get()));
    oppa::Div("reseed").build()
}

fn reseed_dispatch(ctx: &Ctx, props: &ReseedProps) -> VNode {
    RESEED_BODY.with(|b| b.borrow()(ctx, props))
}

#[test]
fn body_edit_reseeds_later_signals_instead_of_shuffling() {
    let host = ComponentHost::new();
    let rt = host.runtime();
    let tick = rt.signal(0i32);
    let seen = rt.signal((0i32, 0i32, 0i32));
    RESEED_BODY.with(|b| *b.borrow_mut() = reseed_v1);
    host.mount(
        "Reseed",
        ReseedProps {
            tick: tick.clone(),
            seen: seen.clone(),
        },
        reseed_dispatch,
    );
    host.run_until_idle();
    assert_eq!(seen.get(), (11, 21, 0), "v1 state established");

    // The hot-reload-adjacent edit: same instance, new body version.
    RESEED_BODY.with(|b| *b.borrow_mut() = reseed_v2);
    tick.set(1);
    host.run_until_idle();
    // Re-seed (documented §5.1 rule): every shifted site re-initializes.
    // A shuffle (positional/ordinal keying) would read (11, 21, 20) — v1's
    // `a` state leaking into v2's `n`.
    assert_eq!(
        seen.get(),
        (99, 10, 20),
        "inserted signal re-seeds later sites, never shuffles"
    );
}

// ---------------------------------------------------------------------------
// Proof 7 — opaque props residence (lock #25 shape, headless form)
// ---------------------------------------------------------------------------

#[test]
fn opaque_props_clone_and_generation_tag() {
    use oppa::OpaqueProps;
    let host = ComponentHost::new();
    let gen = host.runtime().generation();
    let p = OpaqueProps::new(vec![1u32, 2, 3], gen);
    assert_eq!(p.generation(), gen);
    assert_eq!(p.get::<Vec<u32>>(), &vec![1u32, 2, 3]);
    let q = p.clone();
    assert_eq!(q.get::<Vec<u32>>(), &vec![1u32, 2, 3]);
}

// ---------------------------------------------------------------------------
// Proof 8 — list scroll evicts per-item keyed_state once far out of window
// ---------------------------------------------------------------------------

#[test]
fn list_scroll_evicts_keyed_state_far_out_of_window() {
    let host = ComponentHost::new();
    let rt = host.runtime();
    let handle = mount_list(&host, 200);
    let root = handle.root_instance();
    let scroll = host.instance_scroll(root).expect("list scroll seam");

    // Contact 0's slide-anim entry exists after the first window commits.
    assert!(
        rt.keyed_contains::<u32>(0),
        "row 0's keyed_state committed with the first window"
    );
    // Scroll down in small steps: each 13-slot window introduces new item
    // keys and re-touches the visible ones. Past 64 distinct keys since key
    // 0's last access, key 0 is "sufficiently far" and evicts — while the
    // diff still carries zero structure ops throughout (recycling holds
    // under eviction; the two mechanisms are independent).
    let mut saw_commit = false;
    let mut step = 0;
    while step <= 150 {
        let dc = host.diff_count();
        scroll.set(step as f32 * 56.0);
        host.run_until_idle();
        // Equal-value scrolls (step 0 repeats the mount offset) commit
        // nothing at all — the binding equality gate holds — so only
        // assert the tail when a commit actually landed.
        if host.diff_count() != dc {
            saw_commit = true;
            let tail = host.last_diff().expect("scroll commits");
            assert_eq!(tail.structure_ops(), 0);
        }
        step += 5;
    }
    assert!(saw_commit, "the sweep must commit at least once");
    assert!(
        !rt.keyed_contains::<u32>(0),
        "key 0 evicted once far out of window (>64 keys touched)"
    );
    // The newly visible window's keys are live.
    assert!(rt.keyed_contains::<u32>(150));
}
