//! Hello, Oppa (Round 25.1, decision 336): the minimal desktop app —
//! one signal, one button, one text row. Copy this file to start your
//! own app; every larger example (`showcase`, `kitchen_sink`,
//! `task_studio`) is this shape with more controls.
//!
//! Run: `cargo run -p oppa-controls --example hello`
//! (Windows/Linux desktops; needs a display server).
//! Escape with nothing focused exits (root exit); `OPPA_RENDERER=cpu`
//! forces the software path when the GPU refuses.
//!
//! Headless proof of this exact shape (tap increments, no window):
//! `crates/oppa-testkit/tests/hello_counter.rs`.

use oppa::{AlignItems, Column, Ctx, SharedString, Style, Text, VNode};
use oppa_app::{run_desktop, WindowOptions};
use oppa_controls::{Button, ButtonProps};

fn hello(ctx: &Ctx, _props: &()) -> VNode {
    let count = ctx.signal(0i32);
    let pressed = count.clone();
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
                text: SharedString::from(format!("Clicked {} times", count.get())),
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

fn main() {
    if let Err(e) = run_desktop(WindowOptions::new("Hello, Oppa", 400, 300), (), hello) {
        eprintln!("hello: FATAL: {e}");
        std::process::exit(1);
    }
}
