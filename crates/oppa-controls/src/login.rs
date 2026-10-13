//! Portable login page: one shared component with zero platform code.
//!
//! The app code below names only `oppa` + `oppa-controls` vocabulary
//! (signals, `Column`/`Text`, `TextInput`, `Button`, validation marks).
//! It compiles unchanged into every runner:
//!
//! - Desktop (Windows + Linux, same source): `run_desktop(opts, (), login_root)`
//!   — see `examples/login.rs`.
//! - Web: `WasmHost::mount_root("Login", (), login_root)` + the stock
//!   `bootstrap.js` event glue (no app-code change).
//! - Android: the `oppa-android-app` `SceneCore` mounts the same
//!   `login_root` (no app-code change; restart-only reload still applies).
//!
//! Deliberately avoided so no per-platform adaptation leaks in:
//! file pickers, `app_data_dir`, clipboard, custom GPU flags, PWA shell,
//! multi-window, variable-height rows.

use oppa::{AlignItems, Column, Ctx, Props, SharedString, Signal, Style, Text, VNode};

use crate::{Button, ButtonProps, TextInput, TextInputProps};

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

/// Author-owned login state (the testkit idiom: the host owns these so
/// headless tests can assert without a window).
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
    email_props.width = 280.0;
    email_props.debug = SharedString::from("login-email");
    email_props.required = true;
    if let Some(msg) = show_email_err {
        email_props.invalid = true;
        email_props.error_message = Some(msg);
    }
    let mut pass_props = TextInputProps::new("Password", state.password.clone())
        .placeholder("••••••••")
        .masked(true);
    pass_props.width = 280.0;
    pass_props.debug = SharedString::from("login-password");
    pass_props.required = true;
    if let Some(msg) = show_pass_err {
        pass_props.invalid = true;
        pass_props.error_message = Some(msg);
    }

    Column::new()
        .style(
            Style::new()
                .align_items(AlignItems::Center)
                .pad_x(24)
                .pad_y(32)
                .gap(12),
        )
        .children([
            VNode::from(Text::new("Sign in").size(22).bold()),
            VNode::from(Text {
                text: SharedString::from("One form — Windows, Linux, Web, Android."),
                style: Text::body_secondary,
            }),
            ctx.child_auto(&email_props, TextInput),
            ctx.child_auto(&pass_props, TextInput),
            ctx.child_auto(
                &ButtonProps::new("Sign in", on_sign_in)
                    .size(280.0, 36.0)
                    .debug("login-submit"),
                Button,
            ),
            VNode::from(Text {
                text: state.status.get(),
                style: Text::body_secondary,
            }),
        ])
}

/// Self-owned root for real runners (`()` props): desktop
/// `run_desktop`, web `WasmHost::mount_root`, Android `SceneCore`
/// all mount exactly this.
pub fn login_root(ctx: &Ctx, _props: &()) -> VNode {
    let state = LoginState {
        email: ctx.signal(SharedString::from("")),
        password: ctx.signal(SharedString::from("")),
        tried: ctx.signal(false),
        status: ctx.signal(SharedString::from("")),
    };
    ctx.child_auto(&state, LoginApp)
}
