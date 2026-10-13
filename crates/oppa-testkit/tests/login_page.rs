//! Headless proof of the portable login page
//! (`crates/oppa-controls/src/login.rs`, desktop entry
//! `crates/oppa-controls/examples/login.rs`): the same `LoginApp`
//! that `run_desktop`, `WasmHost::mount_root`, and the Android
//! `SceneCore` mount — no window, no platform code.

use oppa::{SharedString, Signal};
use oppa_controls::{email_error, password_error, LoginApp, LoginState};
use oppa_testkit::Harness;

fn mount_login() -> (
    Harness,
    Signal<SharedString>,
    Signal<SharedString>,
    Signal<SharedString>,
) {
    let app = Harness::new();
    let email = app.host().runtime().signal(SharedString::from(""));
    let password = app.host().runtime().signal(SharedString::from(""));
    let tried = app.host().runtime().signal(false);
    let status = app.host().runtime().signal(SharedString::from(""));
    app.mount(
        "Login",
        LoginState {
            email: email.clone(),
            password: password.clone(),
            tried: tried.clone(),
            status: status.clone(),
        },
        LoginApp,
    );
    (app, email, password, status)
}

#[test]
fn login_validation_rules() {
    assert!(email_error("").is_some(), "empty email rejected");
    assert!(email_error("not-an-email").is_some(), "bare word rejected");
    assert!(
        email_error("you@example.com").is_none(),
        "plain email accepted"
    );
    assert!(password_error("").is_some(), "empty password rejected");
    assert!(password_error("short7").is_some(), "7 chars rejected");
    assert!(password_error("long-enough").is_none(), "8+ chars accepted");
}

#[test]
fn login_empty_submit_asks_for_fixes() {
    let (app, _email, _password, status) = mount_login();
    app.tap("login-submit");
    assert_eq!(
        status.get(),
        SharedString::from("Fix the highlighted fields"),
        "empty submit does not welcome"
    );
}

#[test]
fn login_valid_submit_welcomes() {
    let (app, email, password, status) = mount_login();
    email.set(SharedString::from("you@example.com"));
    password.set(SharedString::from("long-enough"));
    app.run_idle();
    app.tap("login-submit");
    assert_eq!(
        status.get(),
        SharedString::from("Welcome, you@example.com!"),
        "valid submit welcomes"
    );
}
