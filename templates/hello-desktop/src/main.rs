//! Hello, desktop: the copy-out starter (Round 28.1, decision 343).
//! Same shape as `cargo run -p oppa-controls --example hello` but in
//! your own crate: one signal, one button, one text row.
//!
//! Your app grows from here: add controls from `oppa-controls`
//! (see `docs/ARCHITECTURE.md`), headless tests from `oppa-testkit`.
//! Run it with `cargo run` (the desktop equivalent of web
//! `cargo oppa serve`).

use oppa::{AlignItems, Column, Ctx, SharedString, Style, Text, VNode};
use oppa_app::desktop_main;
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

desktop_main!("Hello, Oppa", 400, 300, hello);
