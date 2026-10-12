//! Portable login form — the app, written once.
//!
//! Only `oppa` + `oppa-controls` vocabulary (signals, `Column`/
//! `Text`, `TextInput` with validation marks, `Button`). No `cfg`,
//! no shell/renderer/file/clipboard imports, so this file compiles
//! unchanged into every runner:
//!
//! - Desktop (Windows + Linux): `src/main.rs` mounts `login_root`
//!   via `run_desktop` — run with `cargo run`.
//! - Web: `src/lib.rs` exports `login_root` via `export_app!` — run
//!   with `cargo oppa serve`.
//! - Android: `crates/oppa-android-app` mounts any root through
//!   `mount_app` — point it at `login_root` (see README.md;
//!   needs a device, so the on-device leg stays manual).

// Control components are `CamelCase` functions by framework convention
// (`#[component] fn Name` — M2 authoring surface).
#![allow(non_snake_case)]

use oppa::{
    AlignItems, Color, Column, Ctx, Div, JustifyContent, Props, Px, SharedString, Signal, Style,
    Text, VNode,
};

use oppa_controls::{Button, ButtonProps, TextInput, TextInputProps};

// ---------------------------------------------------------------------------
// Pure validation (host-testable, no UI dependency)
// ---------------------------------------------------------------------------

/// Email rule: non-empty, one `@`, a `.` after the `@`.
pub fn email_error(email: &str) -> Option<SharedString> {
    let email = email.trim();
    if email.is_empty() {
        return Some(SharedString::from("Email is required"));
    }
    match email.find('@') {
        Some(at) if at > 0 && email[at + 1..].contains('.') => None,
        _ => Some(SharedString::from("Enter a valid email")),
    }
}

/// Password rule: non-empty, at least 8 characters.
pub fn password_error(password: &str) -> Option<SharedString> {
    if password.is_empty() {
        return Some(SharedString::from("Password is required"));
    }
    if password.chars().count() < 8 {
        return Some(SharedString::from("At least 8 characters"));
    }
    None
}

// ---------------------------------------------------------------------------
// Shared state + component
// ---------------------------------------------------------------------------

/// Author-owned login state (controls stay controlled — signals in,
/// nothing retained).
#[derive(Clone)]
pub struct LoginState {
    pub email: Signal<SharedString>,
    pub password: Signal<SharedString>,
    pub tried: Signal<bool>,
    pub status: Signal<SharedString>,
}

impl Props for LoginState {}

/// Portable login form over [`LoginState`].
pub fn LoginApp(ctx: &Ctx, state: &LoginState) -> VNode {
    let email_err = email_error(&state.email.get());
    let pass_err = password_error(&state.password.get());
    let tried = state.tried.get();
    let show_email_err = tried.then(|| email_err.clone()).flatten();
    let show_pass_err = tried.then(|| pass_err.clone()).flatten();

    let email = state.email.clone();
    let password = state.password.clone();
    let tried_sig = state.tried.clone();
    let status = state.status.clone();
    let on_sign_in = move || {
        tried_sig.set(true);
        if email_error(&email.get()).is_none() && password_error(&password.get()).is_none() {
            status.set(SharedString::from(format!(
                "Welcome, {}!",
                email.get().trim()
            )));
        } else {
            status.set(SharedString::from("Fix the highlighted fields"));
        }
    };

    let mut email_props =
        TextInputProps::new("Email", state.email.clone()).placeholder("you@example.com");
    email_props.width = 284.0;
    email_props.height = 44.0;
    email_props.debug = SharedString::from("login-email");
    email_props.required = true;
    if let Some(msg) = show_email_err {
        email_props.invalid = true;
        email_props.error_message = Some(msg);
    }
    let mut pass_props = TextInputProps::new("Password", state.password.clone())
        .placeholder("Enter password")
        .masked(true);
    pass_props.width = 284.0;
    pass_props.height = 44.0;
    pass_props.debug = SharedString::from("login-password");
    pass_props.required = true;
    if let Some(msg) = show_pass_err {
        pass_props.invalid = true;
        pass_props.error_message = Some(msg);
    }

    // Runtime screen-mode toggle (decision 431): flips the host
    // status + navigation bars from a plain press handler (the
    // `ctx.host()` + theme-toggle precedent — no runner handle, no
    // shell imports, so this file stays portable). Android applies
    // the flip through the bars bridge while the scene stays
    // full-bleed; desktop/Web keep the signal update with no
    // platform effect. The label reads the tracked mode, so it
    // follows the flip on every runner.
    let bars_host = ctx.host();
    let bars_label = if ctx.system_bars_visible().get() {
        "Hide system bars"
    } else {
        "Show system bars"
    };
    let toggle_bars = move || {
        let shown = bars_host.system_bars_visible().get();
        bars_host.set_system_bars_visible(!shown);
    };

    // Presentation only below: same signals, same validators, same
    // debug names. Everything colors through the theme tokens, so
    // light and dark palettes both work with no app branching —
    // the only hardcoded ink is white on primary fills (the same
    // rule the themed Button labels follow).
    let t = ctx.theme().tokens();
    let white = Color(0xFF_FF_FF);
    let mut card_style = Style::new()
        .bg(t.surface)
        .radius(20)
        .border(1, t.border)
        .shadow(0, 10, Color(0x33_00_00_00))
        .shadow_blur(28)
        .pad_x(28)
        .pad_y(28)
        .gap(14)
        .align_items(AlignItems::Center)
        .build();
    // Width-only cards patch the built style (the Modal precedent —
    // no width-only builder); height stays content-wrapped so error
    // rows expand the card instead of clipping.
    card_style.w = Some(Px::of(340.0));
    let card = Div("login-card")
        .style(card_style)
        .children([
            Div("login-monogram")
                .style(
                    Style::new()
                        .size(56, 56)
                        .bg(t.primary)
                        .radius(28)
                        .ink(white)
                        .align_items(AlignItems::Center)
                        .justify_content(JustifyContent::Center),
                )
                .child(VNode::from(Text::new("O").size(26).bold().build())),
            VNode::from(Text::new("Welcome back").size(24).bold().build()),
            VNode::from(Text {
                text: SharedString::from("Sign in to your account"),
                style: Text::body_secondary,
            }),
            ctx.child_auto(&email_props, TextInput),
            ctx.child_auto(&pass_props, TextInput),
            ctx.child_auto(
                &ButtonProps::new("Sign in", on_sign_in)
                    .size(284.0, 46.0)
                    .debug("login-submit"),
                Button,
            ),
            ctx.child_auto(
                &ButtonProps::new(bars_label, toggle_bars)
                    .size(284.0, 46.0)
                    .debug("login-bars"),
                Button,
            ),
            VNode::from(Text {
                text: state.status.get(),
                style: Text::body_secondary,
            }),
            VNode::from(Text {
                text: SharedString::from("One form — Windows, Linux, Web, Android."),
                style: Text::body_secondary,
            }),
        ]);
    Column::new()
        .style(
            Style::new()
                .bg_gradient(t.primary, t.primary_pressed)
                .fill_width()
                .fill_height()
                .align_items(AlignItems::Center)
                .justify_content(JustifyContent::Center)
                .pad_x(24)
                .pad_y(24),
        )
        .children([card])
}

/// Self-owned root for real runners (`()` props): desktop
/// `run_desktop`, web `export_app!`, Android `mount_app` all mount
/// exactly this.
pub fn login_root(ctx: &Ctx, _props: &()) -> VNode {
    let state = LoginState {
        email: ctx.signal(SharedString::from("")),
        password: ctx.signal(SharedString::from("")),
        tried: ctx.signal(false),
        status: ctx.signal(SharedString::from("")),
    };
    ctx.child_auto(&state, LoginApp)
}
