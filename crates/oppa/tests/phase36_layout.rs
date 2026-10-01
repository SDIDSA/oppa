//! Phase 36 PR2a (decision 353 — G15): minimal Grid, flex shares, and
//! min/max clamping over hand-built retained trees (the `m3_layout`
//! Rig shape, textless — explicit-size Divs only, so no `TextService`
//! is needed). This is the crate's consolidated Phase-36 layout
//! binary — `m3_layout.rs` stays untouched.

#![allow(non_snake_case)]

use oppa::{
    Column, Div, Grid, GridTrack, Interner, LayoutLedger, LayoutStats, Reconciler, Row, Runtime,
    Style, VNode,
};

struct Rig {
    rt: Runtime,
    rec: Reconciler,
    styles: Interner<Style>,
    ledger: LayoutLedger,
}

impl Rig {
    fn new() -> Self {
        let rt = Runtime::new();
        Self {
            ledger: LayoutLedger::new(&rt),
            rt,
            rec: Reconciler::new(),
            styles: Interner::new(),
        }
    }

    fn layout(&mut self, vnode: VNode) -> LayoutStats {
        self.rec.reconcile(&self.rt, &mut self.styles, false, vnode);
        self.ledger
            .run(&mut self.rec, &self.styles, None, 800.0, 600.0)
    }

    fn only(&self, debug: &str) -> oppa::NodeId {
        let ids = self.rec.find_by_debug(debug);
        assert_eq!(ids.len(), 1, "expected one {debug} node, got {}", ids.len());
        ids[0]
    }

    fn geom(&self, debug: &str) -> (f32, f32, f32, f32) {
        let b = LayoutLedger::committed(&self.rec, self.only(debug)).expect("committed box");
        (b.x, b.y, b.w, b.h)
    }
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

// ---------------------------------------------------------------------------
// Minimal Grid: Px / Fr / Auto tracks, auto-flow, spans
// ---------------------------------------------------------------------------

#[test]
fn grid_px_fr_auto_tracks_size_and_place() {
    let mut rig = Rig::new();
    let tree: VNode = Div("root").child(
        Grid("g")
            .style(Style::new().grid_cols(vec![
                GridTrack::Px(oppa::Px::of(100.0)),
                GridTrack::Fr(oppa::Px::of(1.0)),
                GridTrack::Auto,
            ]))
            .children([
                Div("a").style(Style::new().size(50, 20)).build(),
                Div("b").style(Style::new().size(30, 10)).build(),
                Div("c").style(Style::new().size(70, 10)).build(),
            ]),
    );
    rig.layout(tree);
    // Block root fills the 800 viewport: 100 + fr + 70; fr = 630.
    let (ax, _, aw, _) = rig.geom("a");
    let (bx, _, bw, _) = rig.geom("b");
    let (cx, _, cw, _) = rig.geom("c");
    assert!(approx(ax, 0.0), "a.x = {ax}");
    assert!(approx(aw, 50.0));
    assert!(approx(bx, 100.0), "b.x = {bx}");
    assert!(approx(bw, 30.0));
    assert!(approx(cx, 730.0), "c.x = {cx}");
    assert!(approx(cw, 70.0));
    let (_, gy, gw, gh) = rig.geom("g");
    assert!(approx(gy, 0.0));
    assert!(approx(gw, 800.0), "grid fills block width, got {gw}");
    assert!(approx(gh, 20.0), "auto row = max child h, got {gh}");
}

#[test]
fn grid_implicit_rows_append_beyond_template() {
    let mut rig = Rig::new();
    let tree: VNode = Div("root").child(
        Grid("g")
            .style(Style::new().grid_cols(vec![GridTrack::Auto, GridTrack::Auto]))
            .children([
                Div("a").style(Style::new().size(60, 20)).build(),
                Div("b").style(Style::new().size(60, 10)).build(),
                Div("c").style(Style::new().size(60, 10)).build(),
            ]),
    );
    rig.layout(tree);
    let (_, ay, _, _) = rig.geom("a");
    let (_, by, _, _) = rig.geom("b");
    let (_, cy, _, ch) = rig.geom("c");
    assert!(approx(ay, 0.0) && approx(by, 0.0), "row 0: {ay} {by}");
    assert!(
        approx(cy, 20.0),
        "implicit row 1 under row 0 (h 20), got {cy}"
    );
    assert!(approx(ch, 10.0));
    let (_, _, _, gh) = rig.geom("g");
    assert!(approx(gh, 30.0), "20 + 10, got {gh}");
}

#[test]
fn grid_col_span_covers_tracks_and_gaps() {
    let mut rig = Rig::new();
    let tree: VNode = Div("root").child(
        Grid("g")
            .style(
                Style::new()
                    .grid_cols(vec![
                        GridTrack::Px(oppa::Px::of(100.0)),
                        GridTrack::Px(oppa::Px::of(100.0)),
                    ])
                    .gap(10),
            )
            .children([
                // Explicit sizes win over the cell (CSS item-size rule):
                // the span reserves both tracks; the child keeps 50.
                Div("wide")
                    .style(Style::new().size(50, 20).col_span(2))
                    .build(),
                Div("narrow").style(Style::new().size(40, 10)).build(),
            ]),
    );
    rig.layout(tree);
    let (wx, wy, ww, _) = rig.geom("wide");
    assert!(approx(wx, 0.0) && approx(wy, 0.0));
    assert!(
        approx(ww, 50.0),
        "explicit size wins over the cell, got {ww}"
    );
    let (nx, ny, _, _) = rig.geom("narrow");
    assert!(
        approx(nx, 0.0) && approx(ny, 20.0 + 10.0),
        "span pushes the next item to row 1, got {nx} {ny}"
    );
    // Auto children stretch into their cell width (phase-2 re-layout).
    let mut rig2 = Rig::new();
    rig2.layout(
        Div("root").child(
            Grid("g2")
                .style(Style::new().grid_cols(vec![
                    GridTrack::Px(oppa::Px::of(100.0)),
                    GridTrack::Px(oppa::Px::of(100.0)),
                ]))
                .children([Div("auto").child(Div("in").style(Style::new().size(30, 10)).build())]),
        ),
    );
    let (_, _, aw, _) = rig2.geom("auto");
    assert!(approx(aw, 100.0), "auto child fills its track, got {aw}");
}

#[test]
#[should_panic(expected = "wider than")]
fn grid_col_span_beyond_template_refuses_loudly() {
    let mut rig = Rig::new();
    let tree: VNode = Div("root").child(
        Grid("g")
            .style(Style::new().grid_cols(vec![GridTrack::Auto, GridTrack::Auto]))
            .children([Div("wide")
                .style(Style::new().size(50, 20).col_span(3))
                .build()]),
    );
    rig.layout(tree);
}

#[test]
#[should_panic(expected = "span of 0")]
fn grid_zero_span_refuses_loudly() {
    let mut rig = Rig::new();
    let tree: VNode = Div("root").child(
        Grid("g")
            .style(Style::new().grid_cols(vec![GridTrack::Auto]))
            .children([Div("z")
                .style(Style::new().size(50, 20).col_span(0))
                .build()]),
    );
    rig.layout(tree);
}

#[test]
#[should_panic(expected = "flex_wrap=Wrap on Grid")]
fn grid_wrap_refuses_loudly() {
    let mut rig = Rig::new();
    let tree: VNode = Div("root").child(
        Grid("g")
            .style(
                Style::new()
                    .grid_cols(vec![GridTrack::Auto])
                    .flex_wrap(oppa::FlexWrap::Wrap),
            )
            .children([Div("a").style(Style::new().size(50, 20)).build()]),
    );
    rig.layout(tree);
}

// ---------------------------------------------------------------------------
// Flex shares: weighted grow, opt-in shrink, fill preservation
// ---------------------------------------------------------------------------

#[test]
fn flex_grow_splits_remainder_by_weight_over_base() {
    // Flex children are content-driven (explicit `w` always wins, so
    // the bases come from the inner fixed boxes, not sizes).
    let mut rig = Rig::new();
    let tree: VNode = Div("root").child(
        Row("r").style(Style::new().size(400, 20)).children([
            Div("fixed").style(Style::new().size(100, 20)).build(),
            Div("f1")
                .style(Style::new().flex_grow(1))
                .child(Div("i1").style(Style::new().size(20, 20)).build()),
            Div("f3")
                .style(Style::new().flex_grow(3))
                .child(Div("i3").style(Style::new().size(20, 20)).build()),
        ]),
    );
    rig.layout(tree);
    // Remainder 400 − 100 − 40 = 260; weights 1:3 → 65 / 195 over bases.
    let (x1, _, w1, _) = rig.geom("f1");
    let (x3, _, w3, _) = rig.geom("f3");
    assert!(approx(x1, 100.0), "f1.x = {x1}");
    assert!(approx(w1, 85.0), "20 + 65, got {w1}");
    assert!(approx(x3, 185.0), "f3.x = {x3}");
    assert!(approx(w3, 215.0), "20 + 195, got {w3}");
}

#[test]
fn flex_absent_preserves_equal_fill_split() {
    let mut rig = Rig::new();
    let tree: VNode = Div("root").child(Row("r").style(Style::new().size(400, 20)).children([
        Div("fixed").style(Style::new().size(100, 20)).build(),
        Div("a").style(Style::new().fill_width().h(20)).build(),
        Div("b").style(Style::new().fill_width().h(20)).build(),
    ]));
    rig.layout(tree);
    let (xa, _, wa, _) = rig.geom("a");
    let (xb, _, wb, _) = rig.geom("b");
    assert!(
        approx(wa, 150.0) && approx(wb, 150.0),
        "equal split: {wa} {wb}"
    );
    assert!(
        approx(xa, 100.0) && approx(xb, 250.0),
        "positions: {xa} {xb}"
    );
}

#[test]
fn flex_shrink_absorbs_overflow_proportionally() {
    // Shrink needs a laid (content-driven) width: explicit `w` opts
    // out, so the shrinkable box wraps a fixed inner box.
    let mut rig = Rig::new();
    let tree: VNode = Div("root").child(
        Row("r").style(Style::new().size(200, 20)).children([
            Div("a")
                .style(Style::new().flex_shrink(1))
                .child(Div("ia").style(Style::new().size(150, 20)).build()),
            Div("b").style(Style::new().size(150, 20)).build(),
        ]),
    );
    rig.layout(tree);
    // Overflow 100; only `a` opted in → it absorbs everything.
    let (_, _, wa, _) = rig.geom("a");
    let (xb, _, wb, _) = rig.geom("b");
    assert!(approx(wa, 50.0), "a absorbs the excess, got {wa}");
    assert!(approx(wb, 150.0), "b never shrinks, got {wb}");
    assert!(approx(xb, 50.0), "b.x = {xb}");
    let (_, _, rw, _) = rig.geom("r");
    assert!(approx(rw, 200.0));
}

#[test]
fn column_flex_grow_splits_height_by_weight() {
    // Height bases are content-driven too (explicit `h` opts out).
    let mut rig = Rig::new();
    let tree: VNode = Div("root").child(
        Column::new()
            .gap(0)
            .style(Style::new().size(100, 300))
            .children([
                Div("fixed").style(Style::new().size(100, 100)).build(),
                Div("f1")
                    .style(Style::new().flex_grow(1))
                    .child(Div("j1").style(Style::new().size(100, 40)).build()),
                Div("f3")
                    .style(Style::new().flex_grow(3))
                    .child(Div("j3").style(Style::new().size(100, 40)).build()),
            ]),
    );
    rig.layout(tree);
    // Remainder 300 − 100 − 80 = 120; weights 1:3 → 30 / 90 over bases.
    let (_, y1, _, h1) = rig.geom("f1");
    let (_, y3, _, h3) = rig.geom("f3");
    assert!(approx(y1, 100.0), "f1.y = {y1}");
    assert!(approx(h1, 70.0), "40 + 30, got {h1}");
    assert!(approx(y3, 170.0), "f3.y = {y3}");
    assert!(approx(h3, 130.0), "40 + 90, got {h3}");
}

#[test]
fn div_ignores_flex_shares() {
    // Block-lite spans every child to the content width — flex adds
    // nothing: the flex child matches its plain sibling exactly.
    let mut rig = Rig::new();
    let tree: VNode = Div("root").child(
        Div("d").style(Style::new().size(400, 100)).children([
            Div("plain").child(Div("k1").style(Style::new().size(20, 20)).build()),
            Div("flex")
                .style(Style::new().flex_grow(9))
                .child(Div("k2").style(Style::new().size(20, 20)).build()),
        ]),
    );
    rig.layout(tree);
    let (_, _, wp, _) = rig.geom("plain");
    let (_, _, w, _) = rig.geom("flex");
    assert!(
        approx(wp, 400.0) && approx(w, 400.0),
        "flex adds nothing: {wp} {w}"
    );
}

// ---------------------------------------------------------------------------
// Min/max clamping: resolved sizes clamp, explicit contradictions refuse
// ---------------------------------------------------------------------------

#[test]
fn clamp_pins_resolved_sizes() {
    // Explicit sizes outside the clamp refuse (next tests), so
    // clamping resolved sizes needs content-driven boxes — and block
    // children span the full width, so the clamp boxes ride a Row
    // (intrinsic widths) instead.
    let mut rig = Rig::new();
    let tree2: VNode = Div("root").child(
        Row("r").children([
            Div("grow")
                .style(Style::new().max_w(400))
                .child(Div("inner").style(Style::new().size(500, 50)).build()),
            Div("floor")
                .style(Style::new().min_w(200))
                .child(Div("inner2").style(Style::new().size(10, 50)).build()),
            Div("ceil")
                .style(Style::new().min_h(120))
                .child(Div("inner3").style(Style::new().size(50, 50)).build()),
        ]),
    );
    rig.layout(tree2);
    let (_, _, w, _) = rig.geom("grow");
    assert!(approx(w, 400.0), "resolved width clamps to max, got {w}");
    let (_, _, w2, _) = rig.geom("floor");
    assert!(approx(w2, 200.0), "resolved width clamps to min, got {w2}");
    let (_, _, _, h3) = rig.geom("ceil");
    assert!(approx(h3, 120.0), "resolved height clamps to min, got {h3}");
}

#[test]
#[should_panic(expected = "outside clamp")]
fn clamp_explicit_width_contradiction_refuses() {
    let mut rig = Rig::new();
    rig.layout(
        Div("root").child(
            Div("bad")
                .style(Style::new().size(500, 50).max_w(400))
                .build(),
        ),
    );
}

#[test]
#[should_panic(expected = "outside clamp")]
fn clamp_explicit_height_contradiction_refuses() {
    let mut rig = Rig::new();
    rig.layout(
        Div("root").child(
            Div("bad")
                .style(Style::new().size(50, 50).min_h(120))
                .build(),
        ),
    );
}

#[test]
#[should_panic(expected = "contradictory clamp")]
fn clamp_min_over_max_refuses() {
    let mut rig = Rig::new();
    rig.layout(
        Div("root").child(
            Div("bad")
                .style(Style::new().size(50, 50).min_w(300).max_w(100))
                .build(),
        ),
    );
}
