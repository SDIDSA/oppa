//! Round 25.1 (decision 336): headless proof of the `hello` example
//! shape (`crates/oppa-controls/examples/hello.rs`) — tap increments,
//! no window. The example owns its signal via `ctx.signal`; this test
//! mirrors the shape with a host-owned signal (`host.runtime().signal`,
//! the controls-suite idiom) so the count is assertable from outside.

use oppa::{AlignItems, Column, Ctx, Props, SharedString, Signal, Style, Text, VNode};
use oppa_controls::{Button, ButtonProps};
use oppa_testkit::Harness;

#[derive(Clone)]
struct HelloProps {
    count: Signal<i32>,
}

impl Props for HelloProps {}

fn hello_root(ctx: &Ctx, props: &HelloProps) -> VNode {
    let pressed = props.count.clone();
    let shown = props.count.clone();
    Column::new()
        .style(
            Style::new()
                .align_items(AlignItems::Center)
                .pad_x(24)
                .pad_y(20)
                .gap(12),
        )
        .children([
            VNode::from(Text::new("Hello, Oppa").size(22).bold()),
            VNode::from(Text {
                text: SharedString::from(format!("Clicked {} times", shown.get())),
                style: Text::body_secondary,
            }),
            ctx.child(
                "hello::Click",
                1,
                &ButtonProps::new("Click me", move || pressed.set(pressed.get() + 1))
                    .debug("hello-button"),
                Button,
            ),
        ])
}

#[test]
fn hello_counter_tap_increments() {
    let app = Harness::new();
    let count = app.host().runtime().signal(0i32);
    app.mount(
        "Hello",
        HelloProps {
            count: count.clone(),
        },
        hello_root,
    );
    assert_eq!(count.get(), 0, "starts at zero");
    app.tap("hello-button");
    assert_eq!(count.get(), 1, "one tap increments");
    app.tap("hello-button");
    app.tap("hello-button");
    assert_eq!(count.get(), 3, "taps accumulate");
}
