//! Hello, web: the copy-out starter.
//! Same counter shape as the desktop hello, behind one
//! [`export_app`](oppa_web::export_app) line: the harness owns the
//! rig, this crate only names its root.
//!
//! Run it with one command (build + bindgen + serve + open):
//! `cargo oppa serve` (from your project directory).

use oppa::{AlignItems, Column, Ctx, SharedString, Style, Text, VNode};
use oppa_controls::{Button, ButtonProps};
use oppa_web::export_app;
use wasm_bindgen::prelude::*;

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

export_app!(HelloApp, "Hello", hello);
