//! `Extended`-mode showcase (decision 482): a frameless window whose
//! in-app [`HeaderBar`](oppa_controls::HeaderBar) owns the title bar.
//!
//! The OS keeps managing the window — drag the bare bar chrome to
//! move it (Windows `HTCAPTION` hit-tests over the runner-published
//! drag region, Linux compositor `drag_window` on bar presses),
//! borders still resize, snapping/tiling still works — while the
//! three caption buttons bridge back into the loop: minimize /
//! maximize forward through the installed [`WindowControl`](oppa::WindowControl)
//! (drained from runner-visible signals by the `run_desktop_with`
//! poll hook — component code cannot reach the control directly),
//! and close arms `ctx.request_close()` through the same outbox so
//! the close veto still consults.
//!
//! The other styles ride the same one-line builders:
//! `WindowOptions::new(..).decorated()` (the default — standard OS
//! window), `.undecorated()` (frameless, solid), `.transparent()`
//! (frameless, `alpha = 0` clear + premultiplied swapchain).
//!
//! Run with `cargo run -p sera-login --example extended`.

use std::cell::RefCell;
use std::rc::Rc;

use oppa::{Column, Ctx, Props, Signal, Text, VNode};
use oppa_app::{run_desktop_with, WindowOptions};
use oppa_controls::{Button, ButtonProps, HeaderBar, HeaderBarProps};

/// Runner-visible caption outbox (the studio `StudioHooks`
/// publish-once precedent — signals are runtime-bound, so the root
/// publishes them on first render and the poll hook drains them).
#[derive(Clone, Default)]
struct ChromeHooks {
    inner: Rc<RefCell<Option<ChromeInner>>>,
}

#[derive(Clone)]
struct ChromeInner {
    minimize: Signal<bool>,
    maximize: Signal<bool>,
    close: Signal<bool>,
}

impl ChromeHooks {
    fn publish(&self, inner: ChromeInner) {
        self.inner.borrow_mut().replace(inner);
    }

    fn get(&self) -> Option<ChromeInner> {
        self.inner.borrow().clone()
    }
}

#[derive(Clone)]
struct ShowcaseProps {
    hooks: ChromeHooks,
}

impl Props for ShowcaseProps {}

fn showcase_root(ctx: &Ctx, props: &ShowcaseProps) -> VNode {
    // Level-triggered caption requests (drained exactly once by the
    // poll hook below — the studio `exit_requested` shape).
    let minimize = ctx.signal(false);
    let maximize = ctx.signal(false);
    let close = ctx.signal(false);
    props.hooks.publish(ChromeInner {
        minimize: minimize.clone(),
        maximize: maximize.clone(),
        close: close.clone(),
    });
    let clicks = ctx.signal(0u32);
    let press_count = clicks.clone();
    let (m_min, m_max, m_close) = (minimize.clone(), maximize.clone(), close.clone());
    let header = HeaderBarProps::new("Extended showcase")
        .on_minimize(move || m_min.set(true))
        .on_maximize(move || m_max.set(true))
        .on_close(move || m_close.set(true));
    let bump = ButtonProps::new("Click me", move || press_count.set(press_count.get() + 1));
    Column::new().children([
        ctx.child("showcase::header", 1, &header, HeaderBar),
        VNode::from(Text::new(format!("button presses: {}", clicks.get())).size(14)),
        ctx.child("showcase::button", 2, &bump, Button),
    ])
}

fn main() {
    // Decision 485 opt-out demo: native caption zones default on,
    // which would shadow these custom buttons on Windows — this
    // showcase disables them so the poll-hook bridge below owns
    // every press (the `apps/stylebook` `--native-buttons` flag
    // demos the other half).
    let options = WindowOptions::new("Extended showcase", 720, 480)
        .extended()
        .with_native_caption_buttons(false);
    let hooks = ChromeHooks::default();
    let props = ShowcaseProps {
        hooks: hooks.clone(),
    };
    if let Err(e) = run_desktop_with(options, props, showcase_root, |loop_| {
        let poll_hooks = hooks.clone();
        loop_.set_poll_hook(Some(Box::new(move |loop_| {
            let Some(h) = poll_hooks.get() else {
                return;
            };
            if h.minimize.get() {
                h.minimize.set(false);
                loop_.minimize();
            }
            if h.maximize.get() {
                h.maximize.set(false);
                loop_.toggle_maximize();
            }
            if h.close.get() {
                h.close.set(false);
                loop_.host().request_close();
            }
        })));
    }) {
        eprintln!("extended showcase: FATAL: {e}");
        std::process::exit(1);
    }
}
