use super::*;

// ---------------------------------------------------------------------------
// 4. SemanticsDiff dump on toggle + list shapes (payloads flow; emitters M10).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SemProps {
    on: bool,
    rows: usize,
}

impl Props for SemProps {}

fn render_sem(_ctx: &Ctx, props: &SemProps) -> VNode {
    let mut kids = vec![Div("track")
        .style(Style::new().size(44, 24))
        .semantics(Semantics::switch().checked(props.on).label("Wi-Fi"))
        .build()];
    for i in 0..props.rows {
        kids.push(
            Row("row")
                .key(i as u64)
                .style(Style::new().size(100, 20))
                .semantics(Semantics::list_item().selected(i == 1).label(if i == 0 {
                    "a"
                } else {
                    "b"
                }))
                .build(),
        );
    }
    Column::new().children(kids)
}

#[test]
fn semantics_diff_toggle_and_list() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 200.0);
    let handle = host.mount("sem", SemProps { on: true, rows: 2 }, render_sem);
    host.run_until_idle();

    let mut snap = SemanticsSnapshot::new();
    let d1 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    assert_eq!(d1.upserted.len(), 3, "toggle + 2 rows");
    assert!(d1.removed.is_empty());
    let dump = d1.dump();
    assert!(dump.contains("Wi-Fi"), "dump:\n{dump}");
    assert!(dump.contains("Switch"), "dump:\n{dump}");
    assert_eq!(snap.len(), 3);

    // Toggle flip → exactly one upsert with the new payload.
    handle.set_props(SemProps { on: false, rows: 2 });
    host.run_until_idle();
    let d2 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    assert_eq!(d2.upserted.len(), 1);
    assert_eq!(d2.upserted[0].semantics.checked, Some(false));
    assert!(d2.removed.is_empty());

    // Row removal → exactly one removal.
    handle.set_props(SemProps { on: false, rows: 1 });
    host.run_until_idle();
    let d3 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    assert_eq!(d3.removed.len(), 1, "dump:\n{}", d3.dump());
    assert!(d3.upserted.is_empty(), "survivors unchanged: {}", d3.dump());
    assert_eq!(snap.len(), 2);
}
