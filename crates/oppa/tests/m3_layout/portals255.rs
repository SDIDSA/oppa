use super::*;

// ---------------------------------------------------------------------------
// Round 1.4 (decision 255): overlay portals escape parent flow and
// anchor to the viewport.
// ---------------------------------------------------------------------------

#[test]
fn portal_escapes_parent_flow_and_anchors_to_viewport() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Rig viewport is 800x600. Column holds a fixed 100x10 child plus a
    // portal (listed first — order must not matter); the portal carries
    // a 60x12 box. Parent height excludes the portal; the portal spans
    // the viewport at the viewport origin — full height by default
    // (Round 7.21: unconstrained portals dim the whole window).
    let tree: VNode = Div("root").child(Column::new().children([
        Portal("p").child(Div("in").style(Style::new().size(60, 12)).build()),
        Div("a").style(Style::new().size(100, 10)).build(),
    ]));
    rig.layout(tree);
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let p = LayoutLedger::committed(&rig.rec, rig.only("p")).expect("portal");
    let inner = LayoutLedger::committed(&rig.rec, rig.only("in")).expect("inner");
    assert!(approx(a.x, 0.0) && approx(a.y, 0.0), "flow child at top");
    assert!(
        approx(p.x, 0.0) && approx(p.y, 0.0),
        "portal at viewport origin"
    );
    assert!(
        approx(p.w, 800.0),
        "portal spans viewport width, got {}",
        p.w
    );
    assert!(
        approx(p.h, 600.0),
        "portal defaults to viewport height, got {}",
        p.h
    );
    assert!(
        approx(inner.x, 0.0) && approx(inner.y, 0.0),
        "portal child overlaid at the content origin"
    );
    // A portal nested in a Row (non-first position) behaves the same.
    let tree2: VNode =
        Div("root").child(Row("r").style(Style::new().size(200, 40).gap(4)).children([
            Div("fixed").style(Style::new().size(60, 10)).build(),
            Portal("p2").child(Div("in2").style(Style::new().size(60, 12)).build()),
        ]));
    rig.relayout(tree2);
    let r = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    let p2 = LayoutLedger::committed(&rig.rec, rig.only("p2")).expect("portal2");
    assert!(
        approx(r.w, 200.0) && approx(r.h, 40.0),
        "explicit row box kept"
    );
    assert!(
        approx(r.content_w, 60.0),
        "portal adds no row extent or gap share, got {}",
        r.content_w
    );
    assert!(
        approx(p2.w, 800.0) && approx(p2.h, 600.0),
        "nested portal still viewport-anchored, got {}x{}",
        p2.w,
        p2.h
    );
}

/// Round 7.21 (decision 296): a portal with `x`/`absolute_y`
/// offsets anchors at its parent container's content origin (not
/// the viewport origin), keeps the explicit width, and hugs its
/// content height — the dropdown-popup shape.
#[test]
fn portal_with_offsets_anchors_to_parent_origin() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Root pads push the column content origin to (10, 20); the
    // portal offsets from there.
    let tree: VNode = Div("root").style(Style::new().pad_x(10).pad_y(20)).child(
        Column::new().children([
            Div("a").style(Style::new().size(100, 10)).build(),
            Portal("p")
                .style(Style::new().x(5).absolute_y(30).w(160))
                .child(Div("in").style(Style::new().size(60, 12)).build()),
        ]),
    );
    rig.layout(tree);
    let p = LayoutLedger::committed(&rig.rec, rig.only("p")).expect("portal");
    assert!(
        approx(p.x, 15.0) && approx(p.y, 50.0),
        "portal anchors at parent origin + offsets, got ({}, {})",
        p.x,
        p.y
    );
    assert!(approx(p.w, 160.0), "explicit width wins, got {}", p.w);
    assert!(
        approx(p.h, 12.0),
        "anchored portal hugs content height, got {}",
        p.h
    );
    let inner = LayoutLedger::committed(&rig.rec, rig.only("in")).expect("inner");
    assert!(
        approx(inner.x, 15.0) && approx(inner.y, 50.0),
        "portal child rides the anchor, got ({}, {})",
        inner.x,
        inner.y
    );
    assert!(
        approx(inner.w, 60.0),
        "content width follows the portal box, not the window, got {}",
        inner.w
    );
    let col = LayoutLedger::committed(&rig.rec, rig.only("column")).expect("column");
    assert!(
        approx(col.h, 10.0),
        "parent flow excludes the popup, got {}",
        col.h
    );
}

/// Round 7.21 follow-up (user eyeball): a repositioned ancestor
/// must not drag a viewport-anchored portal along — the padded root
/// pushes the column to (50, 40), but the offset-less portal stays
/// at the viewport origin while the offset portal tracks its parent.
#[test]
fn viewport_portal_ignores_parent_reposition() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").style(Style::new().pad_x(50).pad_y(40)).child(
        Column::new().children([
            Div("a").style(Style::new().size(100, 10)).build(),
            Portal("p").child(Div("in").style(Style::new().size(60, 12)).build()),
            Portal("q")
                .style(Style::new().x(5).absolute_y(30))
                .child(Div("in-q").style(Style::new().size(60, 12)).build()),
        ]),
    );
    rig.layout(tree);
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    assert!(
        approx(a.x, 50.0) && approx(a.y, 40.0),
        "flow child rides the padded origin, got ({}, {})",
        a.x,
        a.y
    );
    let p = LayoutLedger::committed(&rig.rec, rig.only("p")).expect("portal");
    assert!(
        approx(p.x, 0.0) && approx(p.y, 0.0),
        "viewport portal stays at the origin, got ({}, {})",
        p.x,
        p.y
    );
    assert!(
        approx(p.w, 800.0) && approx(p.h, 600.0),
        "viewport portal still spans the window, got {}x{}",
        p.w,
        p.h
    );
    let inner = LayoutLedger::committed(&rig.rec, rig.only("in")).expect("inner");
    assert!(
        approx(inner.x, 0.0) && approx(inner.y, 0.0),
        "viewport child stays put, got ({}, {})",
        inner.x,
        inner.y
    );
    let q = LayoutLedger::committed(&rig.rec, rig.only("q")).expect("anchored");
    assert!(
        approx(q.x, 55.0) && approx(q.y, 70.0),
        "anchored portal tracks its parent, got ({}, {})",
        q.x,
        q.y
    );
}

/// Round 7.21 (decision 296): a `fill_height` portal child lays
/// out window-tall through the `given_h` channel, so viewport
/// layers center like explicitly-sized containers — the modal
/// backdrop shape (full-bleed + both-axis centering).
#[test]
fn portal_fill_height_child_expands_and_centers() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(
        Portal("p").child(
            Div("fill")
                .style(
                    Style::new()
                        .fill_width()
                        .fill_height()
                        .justify_content(JustifyContent::Center)
                        .align_items(AlignItems::Center),
                )
                .child(Div("in").style(Style::new().size(60, 12)).build()),
        ),
    );
    rig.layout(tree);
    let fill = LayoutLedger::committed(&rig.rec, rig.only("fill")).expect("fill");
    assert!(
        approx(fill.w, 800.0) && approx(fill.h, 600.0),
        "fill child spans the viewport, got {}x{}",
        fill.w,
        fill.h
    );
    let p = LayoutLedger::committed(&rig.rec, rig.only("p")).expect("portal");
    assert!(
        approx(p.w, 800.0) && approx(p.h, 600.0),
        "portal spans the viewport, got {}x{}",
        p.w,
        p.h
    );
    let inner = LayoutLedger::committed(&rig.rec, rig.only("in")).expect("inner");
    assert!(
        approx(inner.x, 370.0) && approx(inner.y, 294.0),
        "child centers on both axes, got ({}, {})",
        inner.x,
        inner.y
    );
}
