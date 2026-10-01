//! Hello, web: the copy-out starter (Round 29.2, decision 345).
//! Same counter shape as the desktop hello, behind ~25 lines of
//! `#[wasm_bindgen]` glue over [`WasmHost`]: the harness owns the
//! rig, this crate only names its root and forwards browser events.
//!
//! Build per README.md (wasm target + wasm-bindgen + static server).

use oppa::{AlignItems, Column, Ctx, SharedString, Style, Text, VNode};
use oppa_controls::{Button, ButtonProps};
use oppa_web::WasmHost;
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

#[wasm_bindgen]
pub struct HelloApp {
    app: WasmHost,
}

#[wasm_bindgen]
impl HelloApp {
    #[wasm_bindgen(constructor)]
    pub fn new() -> HelloApp {
        HelloApp {
            app: WasmHost::mount_root("Hello", (), hello),
        }
    }

    pub fn html(&mut self) -> String {
        self.app.html()
    }

    pub fn click(&mut self, x: f32, y: f32) -> Option<String> {
        self.app.click(x, y)
    }

    pub fn hover(&mut self, x: f32, y: f32) -> Option<String> {
        self.app.hover(x, y)
    }

    pub fn key(&mut self, code: u32, pressed: bool) -> Option<String> {
        self.app.key(code, pressed)
    }

    pub fn text(&mut self, pid: &str, value: &str) -> Option<String> {
        self.app.text(pid, value)
    }

    pub fn tick(&mut self, now_ms: f64) -> Option<String> {
        self.app.tick(now_ms)
    }

    pub fn sync_system_theme(&mut self) -> Option<String> {
        self.app.sync_system_theme()
    }
}
