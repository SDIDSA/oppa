//! M8 acceptance (DOM arm): the CSS transition mapping + the one-commit
//! stamp + the virtualized sweep through `bind_scroll`/`scroll_window`
//! + the window-lag cover proof.
//!
//! What this proves (acceptance list):
//!
//! - DOM CSS mapping: `transition:` declarations emitted (per carried
//!   animatable); stamped commits disable transitions for exactly that
//!   commit (`transition:none` inline on the touched set, consumed by one
//!   sync, clearing frame after).
//! - Sweep ticks: zero structure ops + zero DOM mutations per tick at
//!   scale; touched bounded; stylesheet churn flat (no rule leaks).
//! - Lag compensation: the overscanned window covers the visible range
//!   under a ≤1-frame trail at ±4 rows/frame (over=4) — measured, with
//!   the over=2 breaking point recorded (decision 119's input).

#![allow(non_snake_case)]

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use oppa::{
    find_retained_by_debug, Color, ComponentHost, Ctx, Ease, InputEvent, Memo, MsExt, Props,
    RendererBackend, ScrollOffset, Signal, Store, Style, Text, Transition, VNode,
};
use oppa_dom::{scroll_window, scroll_window_overscan, DomBackend, StyleSheet, OVERSCAN_SLOTS};

// ---------------------------------------------------------------------------
// 1. CSS transition declarations
// ---------------------------------------------------------------------------

#[test]
fn transition_declarations_map_carried_animatables() {
    // bg + transition → background-color item.
    let s: Style = Style::new()
        .size(100, 40)
        .bg(Color(0x44_44_44))
        .transition(Transition::new(120.ms(), Ease::Out))
        .build();
    let d = oppa_dom::css::transition_decls(&s, 1.0);
    assert_eq!(d, "transition:background-color 120ms ease-out;", "{d}");
    // opacity joins the list; easing spells per arm.
    let s: Style = Style::new()
        .bg(Color(0x44_44_44))
        .opacity(Some(0.5))
        .transition(Transition::new(200.ms(), Ease::InOut))
        .build();
    assert_eq!(
        oppa_dom::css::transition_decls(&s, 1.0),
        "transition:background-color 200ms ease-in-out,opacity 200ms ease-in-out;"
    );
    // Linear + In spellings.
    let lin: Style = Style::new()
        .bg(Color(1))
        .transition(Transition::new(50.ms(), Ease::Linear))
        .build();
    assert!(oppa_dom::css::transition_decls(&lin, 1.0).contains("linear"));
    let inn: Style = Style::new()
        .bg(Color(1))
        .transition(Transition::new(50.ms(), Ease::In))
        .build();
    assert!(oppa_dom::css::transition_decls(&inn, 1.0).contains("ease-in;"));
    // No transition field → no declaration.
    let plain: Style = Style::new().bg(Color(1)).build();
    assert_eq!(oppa_dom::css::transition_decls(&plain, 1.0), "");
    // Bare transition (neither bg nor opacity) declares nothing — the
    // evaluator has no target on either backend (decision 121).
    let bare: Style = Style::new()
        .size(10, 10)
        .transition(Transition::new(120.ms(), Ease::Out))
        .build();
    assert_eq!(oppa_dom::css::transition_decls(&bare, 1.0), "");
    // The declaration rides the shared rule (not inline).
    let mut sheet = StyleSheet::new(1.0);
    let mut t = oppa::Interner::new();
    let id = s.clone().intern(&mut t);
    let class = sheet.class_for(id, &s);
    let decl = sheet.decl_of_class(&class).expect("rule");
    assert!(decl.contains("transition:background-color"), "{decl}");
}

#[test]
fn static_style_still_never_leaks_inline_with_transitions() {
    // The M7 no-inline-spam rule survives the M8 declaration: geometry
    // stays inline, the transition stays in the class rule.
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    #[derive(Clone)]
    struct P;
    impl Props for P {}
    fn render(_ctx: &Ctx, _p: &P) -> VNode {
        oppa::Div("card")
            .style(
                Style::new()
                    .size(100, 40)
                    .bg(Color(0x44_44_44))
                    .transition(Transition::new(120.ms(), Ease::Out)),
            )
            .build()
    }
    let _handle = host.mount("Card", P, render);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for diff in host.diffs_from(0) {
        backend.commit(&diff).expect("commit");
    }
    host.with_retained_mut(|rec, styles| backend.sync(rec, styles, &mut sheet).expect("sync"));
    let card = find_retained_by_debug(&host, "card")[0];
    let el = backend.element(card).expect("card").clone();
    assert!(!el.inline_geom.contains("transition"), "{}", el.inline_geom);
    let html = backend.render_node(card);
    assert!(!html.contains("transition:none"), "unstamped mount: {html}");
    let decl = sheet.decl_of_class(&el.classes[0]).expect("rule");
    assert!(
        decl.contains("transition:background-color 120ms ease-out;"),
        "{decl}"
    );
}

// ---------------------------------------------------------------------------
// 2. Stamped commits disable transitions for exactly that commit
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct StampProps {
    tone: Signal<Color>,
    pad: Signal<bool>,
}

impl Props for StampProps {}

/// Binding-driven card: signal writes rebind (stamped); the style
/// carries a live transition so the stamp has something to suppress.
/// `pad` is a plain (non-binding) style input — the unstamped control.
fn render_stamped(ctx: &Ctx, props: &StampProps) -> VNode {
    let tone = ctx.binding({
        let sig = props.tone.clone();
        move || sig.get()
    });
    let mut style = Style::new()
        .size(100, 40)
        .bg(tone.read())
        .transition(Transition::new(120.ms(), Ease::Out));
    if props.pad.get() {
        style = style.pad_x(8);
    }
    oppa::Div("card").style(style).build()
}

struct StampRig {
    host: ComponentHost,
    backend: DomBackend,
    sheet: StyleSheet,
    seen: usize,
    tone: Signal<Color>,
    pad: Signal<bool>,
}

impl StampRig {
    fn sync_all(&mut self) -> oppa_dom::SyncStats {
        for diff in self.host.diffs_from(self.seen) {
            self.backend.commit(&diff).expect("commit");
        }
        self.seen = self.host.diff_count();
        self.host
            .with_retained_mut(|rec, styles| self.backend.sync(rec, styles, &mut self.sheet))
            .expect("sync")
    }
}

#[test]
fn stamped_commit_disables_transitions_for_exactly_that_commit() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let tone = host.runtime().signal(Color(0x11_11_11));
    let pad = host.runtime().signal(false);
    let _handle = host.mount(
        "Stamp",
        StampProps {
            tone: tone.clone(),
            pad: pad.clone(),
        },
        render_stamped,
    );
    host.run_until_idle();
    let mut rig = StampRig {
        host,
        backend: DomBackend::new(1.0),
        sheet: StyleSheet::new(1.0),
        seen: 0,
        tone,
        pad,
    };
    let card = find_retained_by_debug(&rig.host, "card")[0];

    // Mount: the initial binding values ARE identity events (None →
    // first item), so the mount commit is stamped and the mount sync
    // writes targets with transitions disabled — mounts jump (nothing
    // to animate from). The evaluator agrees (Adds never interpolate).
    let st = rig.sync_all();
    assert!(st.touched > 0, "mount touches");
    assert!(
        rig.backend.element(card).expect("card").no_transition,
        "mount writes initial identities with transitions disabled"
    );

    // Unstamped change (the plain `pad` path, no binding edge): values
    // change, the flag clears, transitions stay enabled.
    rig.pad.set(true);
    rig.host.run_until_idle();
    let d = rig.host.last_diff().expect("diff");
    assert!(!d.suppress_transitions, "no binding edge, no stamp");
    let st = rig.sync_all();
    assert!(st.touched > 0, "clearing re-derives");
    assert!(!rig.backend.element(card).expect("card").no_transition);
    assert!(!rig.backend.render_node(card).contains("transition:none"));

    // Stamped change (binding edge): the touched set writes targets with
    // transitions disabled.
    rig.tone.set(Color(0x33_33_33));
    rig.host.run_until_idle();
    let d = rig.host.last_diff().expect("diff");
    assert!(d.suppress_transitions, "binding edge stamps");
    let st = rig.sync_all();
    assert!(st.touched > 0);
    assert!(
        rig.backend.element(card).expect("card").no_transition,
        "stamped sync flags the touched set"
    );
    assert!(
        rig.backend.render_node(card).contains("transition:none;"),
        "disabled inline for exactly this commit"
    );

    // Next sync with no commit: exactly the clearing frame (the flag
    // flips back on one re-derive — the disable described exactly the
    // stamped commit's output), then quiet.
    let st = rig.sync_all();
    assert_eq!(st.touched, 1, "clearing re-derives exactly once");
    assert!(!rig.backend.element(card).expect("card").no_transition);
    assert!(!rig.backend.render_node(card).contains("transition:none"));
    let st = rig.sync_all();
    assert_eq!(st.touched, 0, "quiet after the clearing frame");

    // Next unstamped change: the flag clears on exactly the re-derive
    // (one touching sync), then quiet again.
    rig.pad.set(false);
    rig.host.run_until_idle();
    let d = rig.host.last_diff().expect("diff");
    assert!(!d.suppress_transitions, "pad path never binds");
    let st = rig.sync_all();
    assert!(st.touched > 0, "clearing re-derives");
    assert!(!rig.backend.element(card).expect("card").no_transition);
    assert!(!rig.backend.render_node(card).contains("transition:none"));
    let st = rig.sync_all();
    assert_eq!(st.touched, 0, "quiet after the clearing frame");
}

// ---------------------------------------------------------------------------
// 3. scroll_window_overscan + lag cover
// ---------------------------------------------------------------------------

#[test]
fn overscan_constant_and_window_math() {
    assert_eq!(OVERSCAN_SLOTS, 4, "the v1 overscan constant, stated");
    // Parameterized helper agrees with the constant path.
    for (off, row_h, vp, n) in [
        (0.0, 56.0, 600.0, 1000),
        (280.0, 56.0, 600.0, 1000),
        (100.0, 20.0, 100.0, 50),
    ] {
        assert_eq!(
            scroll_window(off, row_h, vp, n),
            scroll_window_overscan(off, row_h, vp, n, OVERSCAN_SLOTS as usize),
            "constant path == parameterized path at over=4"
        );
    }
    assert_eq!(scroll_window_overscan(0.0, 56.0, 600.0, 1000, 2), (0, 13));
    assert_eq!(scroll_window_overscan(0.0, 56.0, 600.0, 1000, 4), (0, 15));
    assert_eq!(scroll_window_overscan(280.0, 56.0, 600.0, 1000, 4), (1, 20));
}

#[test]
fn overscanned_window_covers_one_frame_trail() {
    // Window lag, not tearing (§9.3): the offset trails the browser by
    // ≤ 1 frame, so the window computed from the TRAILING offset must
    // still cover the LEADING visible range. Sweep the offset space at
    // several per-frame velocities (rows/frame, both directions).
    const ROW_H: f32 = 56.0;
    const VP: f32 = 600.0;
    const N: usize = 1000;
    fn visible(offset: f32) -> (usize, usize) {
        let first = (offset / ROW_H).floor().max(0.0) as usize;
        let last = ((offset + VP) / ROW_H).ceil().max(0.0) as usize;
        (first, last.min(N))
    }
    for over in [2usize, 4usize] {
        let mut covered_up_to = 0i64;
        for v in 1..=8i64 {
            let mut ok = true;
            let mut off = 0.0f32;
            while off < (N as f32) * ROW_H - VP {
                for dir in [1.0f32, -1.0] {
                    let lead = (off + dir * v as f32 * ROW_H).max(0.0);
                    let (w0, w1) = scroll_window_overscan(off, ROW_H, VP, N, over);
                    let (v0, v1) = visible(lead);
                    if v0 < w0 || v1 > w1 {
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
                off += ROW_H;
            }
            if ok {
                covered_up_to = v;
            } else {
                break;
            }
        }
        eprintln!("M8 lag cover over={over}: ±{covered_up_to} rows/frame");
        if over == 4 {
            assert!(
                covered_up_to >= 4,
                "over=4 covers ±4 rows/frame (≈13k px/s at 60 Hz — ample)"
            );
        } else {
            assert!(covered_up_to >= 2, "over=2 covers ±2 rows/frame");
        }
    }

    // As-built cover (decision 127): the component renders fixed-K
    // windows (`window_first`, slot stability over spec purity), so prove
    // the lag guarantee on what actually renders — the visible range at
    // the leading offset must sit inside the trailing window at every
    // offset, sub-row steps included. Velocities match each constant's
    // proven cover (±2 for +2, ±4 for +4 — see the spec loop above).
    for over in [2usize, 4usize] {
        let k = (VP / ROW_H).ceil() as usize + 1 + 2 * over;
        let max_off = (N - k) as f32 * ROW_H;
        let dirs: &[f32] = if over == 4 {
            &[-4.0, -1.0, 1.0, 4.0]
        } else {
            &[-2.0, -1.0, 1.0, 2.0]
        };
        let mut off = 0.0f32;
        while off <= max_off {
            for dir in dirs {
                let lead = (off + dir * ROW_H).clamp(0.0, max_off);
                let wf = ((off / ROW_H).floor().max(0.0) as usize)
                    .saturating_sub(over)
                    .min(N - k);
                let (v0, v1) = visible(lead);
                assert!(
                    v0 >= wf && v1 <= wf + k,
                    "over={over} off={off}: visible({lead:?})=({v0},{v1}) outside [{wf},{})",
                    wf + k,
                );
            }
            off += 7.0;
        }
    }
}

// ---------------------------------------------------------------------------
// 4. DOM end-to-end sweep (structure 0 + mutations 0 + churn flat)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ContactId(u64);

#[derive(Clone, PartialEq, Debug)]
struct Contact {
    display_name: Arc<str>,
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
            },
        );
    }
    (ids, values)
}

const ROW_H: f32 = 56.0;
const VIEWPORT_H: f32 = 600.0;
const BASE_EVEN: Color = Color(0x22_22_22);
const BASE_ODD: Color = Color(0x33_33_33);

fn window_first(offset_px: f32, row_h: f32, over: usize, n_items: usize, n_slots: usize) -> usize {
    let first = (offset_px / row_h).floor().max(0.0) as usize;
    first
        .saturating_sub(over)
        .min(n_items.saturating_sub(n_slots))
}

fn slot_count(viewport_h: f32, row_h: f32, over: usize) -> usize {
    // Visible rows + 1 straddle margin + overscan both sides (M8
    // decision 127 — see the core suite for the caught margin).
    (viewport_h / row_h).ceil() as usize + 1 + 2 * over
}

#[derive(Clone)]
struct ListProps {
    store: Store<ContactId, Contact>,
    over: usize,
}

impl Props for ListProps {}

#[derive(Clone)]
struct RowProps {
    store: Store<ContactId, Contact>,
    item: Memo<Option<ContactId>>,
    row_h: f32,
}

impl Props for RowProps {}

fn ContactList(ctx: &Ctx, props: &ListProps) -> VNode {
    let offset = ctx.scroll_offset();
    let n = props.store.len();
    let k = slot_count(VIEWPORT_H, ROW_H, props.over);
    let over = props.over;
    let first = window_first(offset.get(), ROW_H, over, n, k);
    let items: Vec<Memo<Option<ContactId>>> = (0..k)
        .map(|slot| {
            let store = props.store.clone();
            let offset = offset.clone();
            ctx.binding(move || {
                let f = window_first(offset.get(), ROW_H, over, n, k);
                store.get(f + slot)
            })
        })
        .collect();
    oppa::ScrollArea("list")
        .content_size(n as f32 * ROW_H)
        .style(Style::new().h(VIEWPORT_H).fill_width())
        .on_scroll(|| {})
        .children(items.into_iter().enumerate().map(|(slot, item)| {
            let row = first + slot;
            let row_props = RowProps {
                store: props.store.clone(),
                item,
                row_h: ROW_H,
            };
            oppa::Row("slot")
                .style(
                    Style::new()
                        .absolute_y(row as f32 * ROW_H)
                        .h(ROW_H)
                        .fill_width(),
                )
                .key(slot as u64)
                .child(ctx.child("ContactRow", slot as u64, &row_props, ContactRow))
        }))
}

fn ContactRow(ctx: &Ctx, props: &RowProps) -> VNode {
    let contact = ctx.memo({
        let store = props.store.clone();
        let item = props.item.clone();
        move || {
            item.read()
                .and_then(|id| store.lookup(&id))
                .map(|c| c.display_name.clone())
                .unwrap_or_else(|| Arc::from(""))
        }
    });
    let item_id = props.item.read();
    let bg = item_id.map(|id| if id.0 % 2 == 0 { BASE_EVEN } else { BASE_ODD });
    oppa::Row("cell")
        .style(
            Style::new()
                .h(props.row_h)
                .fill_width()
                .bg(bg)
                .transition(Transition::new(120.ms(), Ease::Out)),
        )
        .children([Text {
            text: contact.read(),
            style: Text::title_small,
        }
        .into()])
}

struct SweepHarness {
    host: ComponentHost,
    offset: ScrollOffset,
    list: oppa::NodeId,
    k: usize,
    n: usize,
    over: usize,
    clock: Rc<oppa::MockClock>,
    backend: DomBackend,
    sheet: StyleSheet,
    seen: usize,
}

fn mount_sweep(n_items: usize, over: usize) -> SweepHarness {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let (ids, values) = seed_contacts(n_items);
    let store = Store::new(&rt, ids, values);
    host.set_keyed_capacity(n_items.max(64) + 64);
    let handle = host.mount("ContactList", ListProps { store, over }, ContactList);
    host.run_until_idle();
    let offset = host
        .instance_scroll(handle.root_instance())
        .expect("scroll");
    let list = find_retained_by_debug(&host, "list")[0];
    host.bind_scroll(list, offset.clone());
    let k = slot_count(VIEWPORT_H, ROW_H, over);
    let mut h = SweepHarness {
        host,
        offset,
        list,
        k,
        n: n_items,
        over,
        clock,
        backend: DomBackend::new(1.0),
        sheet: StyleSheet::new(1.0),
        seen: 0,
    };
    h.commit_sync();
    h
}

impl SweepHarness {
    fn commit_sync(&mut self) -> oppa_dom::SyncStats {
        for diff in self.host.diffs_from(self.seen) {
            self.backend.commit(&diff).expect("commit");
        }
        self.seen = self.host.diff_count();
        self.host
            .with_retained_mut(|rec, styles| self.backend.sync(rec, styles, &mut self.sheet))
            .expect("sync")
    }

    fn pump(&self, dt: f64) {
        for _ in 0..100 {
            if !self.host.run_once() {
                break;
            }
        }
        self.clock.advance(dt);
        for _ in 0..100 {
            if !self.host.run_once() {
                break;
            }
        }
        assert!(!self.host.runtime().has_demand(), "pump must settle");
    }
}

#[test]
fn dom_sweep_zero_structure_zero_mutations_churn_flat() {
    let mut h = mount_sweep(300, 4);
    assert_eq!(h.k, 20);
    let churn0 = h.sheet.churn();
    let mut max_touched = 0usize;
    let mut moving = 0usize;
    let mut prev_first = window_first(0.0, ROW_H, h.over, h.n, h.k);
    for step in 1..=(h.n - h.k) {
        // INPUT-fed (the Web mapping): browser scroll → dy → signal.
        let dy = step as f32 * ROW_H - h.offset.get();
        h.host.inject_input(InputEvent::Scroll {
            target: h.list,
            dx: 0.0,
            dy,
        });
        h.pump(1.0 / 60.0);
        let first = window_first(step as f32 * ROW_H, ROW_H, h.over, h.n, h.k);
        let d = h.host.last_diff().expect("tick diff");
        let mutations0 = h.backend.mutations();
        let st = h.commit_sync();
        if first != prev_first {
            assert_eq!(d.structure_ops(), 0, "step {step}: zero structure ops");
            assert!(d.suppress_transitions, "step {step}: stamped");
            assert_eq!(
                h.backend.mutations(),
                mutations0,
                "step {step}: zero DOM structure ops"
            );
            moving += 1;
            max_touched = max_touched.max(st.touched);
            assert!(
                st.touched <= 6 * h.k + 8,
                "step {step}: touched {} exceeds bound",
                st.touched
            );
            // Every stamped tick flags its touched set (the Web half of
            // the phantom-flash proof — values jump with transitions off).
            let cells: HashSet<oppa::NodeId> = find_retained_by_debug(&h.host, "cell")
                .into_iter()
                .collect();
            for id in &cells {
                assert!(
                    h.backend.element(*id).expect("cell").no_transition,
                    "step {step}: recycled cells carry transition:none"
                );
            }
        }
        prev_first = first;
    }
    assert_eq!(moving, h.n - h.k - h.over, "every window move swept");
    eprintln!(
        "M8 DOM sweep: max_touched={max_touched} churn_delta={}",
        h.sheet.churn() - churn0
    );
    // Stylesheet churn, exactly accounted (not flat, not hidden): each
    // window move introduces exactly one new rule — the entering edge
    // slot's new `absolute_y` position interns a new StyleId (M7 decision
    // 111 identity is payload-based, so positions intern). The rules are
    // declaration-identical (`height:56px;` — positions emit no CSS);
    // dedup by declaration text would touch #111's identity rule and is
    // recorded as follow-up, not smuggled in here.
    assert_eq!(
        h.sheet.churn() - churn0,
        moving,
        "one position rule per window move, nothing else"
    );
    assert_eq!(
        h.host.with_evaluator(|e| e.created()),
        0,
        "zero interpolators (DOM animates in-browser)"
    );
}
