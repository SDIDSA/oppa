use super::*;

// ---------------------------------------------------------------------------
// Phase 39a (decision 378): value-loop swap preservation (headless half)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct LoopProps {
    value: oppa::Signal<oppa::SharedString>,
    count: oppa::Signal<u32>,
}

fn render_loop(ctx: &Ctx, p: &LoopProps) -> VNode {
    use oppa_controls::{TextInput, TextInputProps};
    Div("loop").children([
        ctx.child(
            "oppa::LoopField",
            1,
            &TextInputProps::new("Name", p.value.clone()),
            TextInput,
        ),
        VNode::from(Text {
            text: oppa::SharedString::from(format!("ticks {}", p.count.get())),
            style: Text::body_secondary,
        }),
    ])
}

/// U8 swap preservation (decision 378 — headless half): typing
/// through the value channel then ticking unrelated state leaves
/// the focused field untouched at the DOM layer — zero new
/// mutations, value intact. (The browser half — native
/// focus/caret retention across the same ticks — rides the M7
/// editing suite through real `<input>`s plus the keyed-patch
/// focal save/restore in `bootstrap.js`.)
#[test]
fn value_loop_unrelated_tick_leaves_focused_field_untouched() {
    let host = ComponentHost::new();
    host.set_viewport(300.0, 200.0);
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(oppa::SharedString::from(""));
    let count = host.runtime().signal(0u32);
    host.mount(
        "Loop",
        LoopProps {
            value: value.clone(),
            count: count.clone(),
        },
        render_loop,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    let mut seen = 0usize;
    let mut commit_sync = |backend: &mut DomBackend, sheet: &mut StyleSheet| {
        use oppa::RendererBackend;
        for d in host.diffs_from(seen) {
            backend.commit(&d).expect("commit");
        }
        seen = host.diff_count();
        backend.with_sync(&host, sheet).expect("sync");
    };
    commit_sync(&mut backend, &mut sheet);
    // Focus the field (tap center), then type through the U8 feed.
    let field = oppa::find_retained_by_debug(&host, "text-input")[0];
    let b = host.committed_box(field).expect("field hit box");
    host.inject_input(InputEvent::pointer_down(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.inject_input(InputEvent::pointer_up(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.run_until_idle();
    assert_eq!(host.focused_node(), Some(field));
    host.inject_input(InputEvent::text(field, "abc"));
    host.run_until_idle();
    assert_eq!(value.get().to_string(), "abc");
    commit_sync(&mut backend, &mut sheet);
    let html = backend.render_node(field);
    assert!(html.contains("value=\"abc\""), "typed value serves: {html}");
    let mutations = backend.mutations();
    // Unrelated tick: the counter re-renders around the field.
    count.set(1);
    host.run_until_idle();
    commit_sync(&mut backend, &mut sheet);
    assert_eq!(
        backend.mutations(),
        mutations,
        "zero DOM ops around the focused field"
    );
    let html = backend.render_node(field);
    assert!(
        html.contains("value=\"abc\""),
        "value intact across the tick: {html}"
    );
    assert_eq!(value.get().to_string(), "abc");
}
