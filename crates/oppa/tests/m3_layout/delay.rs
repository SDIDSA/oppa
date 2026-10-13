#![allow(non_snake_case)]

use super::*;

// ---------------------------------------------------------------------------
// One-frame-delay contract (host-level: LAYOUT-phase publish, EFFECTS read)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct PanelProps {
    text: String,
}

fn Panel(_ctx: &Ctx, props: &PanelProps) -> VNode {
    Div("panel").child(VNode::Text(Arc::from(props.text.as_str())))
}

#[test]
fn settled_reads_observe_previous_frame_then_rerun() {
    let (fake, calls) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    let handle = host.mount(
        "Panel",
        PanelProps {
            text: "hello".to_string(),
        },
        Panel,
    );
    let text_id = find_retained_by_debug(&host, "text")[0];
    // Reader effect: settled content width into a signal (write-only inside).
    let rt = host.runtime();
    let seen = rt.signal(-1.0f32);
    let count = Rc::new(Cell::new(0u32));
    let seen2 = seen.clone();
    let host2 = host.clone();
    let count2 = count.clone();
    let _effect = rt.effect(move || {
        let w = host2
            .settled_box(text_id)
            .map(|b| b.content_w)
            .unwrap_or(-1.0);
        seen2.set(w);
        count2.set(count2.get() + 1);
    });
    // Synchronous creation run observes pre-layout state.
    assert_eq!(seen.get(), -1.0);
    assert_eq!(count.get(), 1);
    // Frame 1: LAYOUT publishes (gen 0→1); the reader still holds the old.
    assert!(rt.run_once());
    assert_eq!(seen.get(), -1.0, "previous-frame value during frame 1");
    assert_eq!(count.get(), 1);
    // Frame 2: the reader re-runs in EFFECTS with the settled box.
    assert!(rt.run_once());
    assert_eq!(count.get(), 2);
    assert!(
        approx(seen.get(), 43.75),
        "5 chars × 8.75, got {}",
        seen.get()
    );
    assert!(!rt.run_once(), "idle: no spurious frames");
    // Update the text (TEXT dirt, no LAYOUT flag): exactly one re-measure,
    // and the delay repeats — frame 1 still shows the old width.
    let calls_before = calls.get();
    handle.set_props(PanelProps {
        text: "hello!!".to_string(),
    });
    assert!(rt.run_once());
    assert_eq!(calls.get(), calls_before + 1, "exactly the dirty leaf");
    assert!(
        approx(seen.get(), 43.75),
        "old width during frame 1, got {}",
        seen.get()
    );
    assert!(rt.run_once());
    assert!(
        approx(seen.get(), 61.25),
        "7 chars × 8.75, got {}",
        seen.get()
    );
    assert!(!rt.run_once(), "idle again");
}
