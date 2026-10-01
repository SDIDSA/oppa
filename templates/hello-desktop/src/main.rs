//! Hello, desktop: the copy-out starter (Round 28.1, decision 343).
//! Same shape as `cargo run -p oppa-controls --example hello` but in
//! your own crate: one signal, one button, one text row.
//!
//! Your app grows from here: add controls from `oppa-controls`
//! (see `docs/ARCHITECTURE.md`), headless tests from `oppa-testkit`.

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
            ctx.child_auto(
                &ButtonProps::new("Click me", move || pressed.set(pressed.get() + 1))
                    .debug("hello-button"),
                Button,
            ),
        ])
}

fn main() {
    if let Err(e) = run_desktop(WindowOptions::new("Hello, Oppa", 400, 300), (), hello) {
        eprintln!("hello-desktop: FATAL: {e}");
        std::process::exit(1);
    }
}
