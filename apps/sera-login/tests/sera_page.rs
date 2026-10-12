//! Headless proof of the Sera-style sign-in page (`SeraApp`): the
//! same tree `run_desktop`, `export_app!`, and the Android runner
//! mount — no window, no platform code.

use oppa::{SharedString, Signal};
use oppa_testkit::Harness;
use sera_login::{default_logins, deposit_icons, SavedLogin, SeraApp, SeraState};

#[allow(clippy::type_complexity)]
fn mount_sera() -> (
    Harness,
    Signal<SharedString>,
    Signal<SharedString>,
    Signal<SharedString>,
    Signal<Vec<SavedLogin>>,
    Signal<bool>,
    Signal<bool>,
) {
    let app = Harness::new();
    let email = app.host().runtime().signal(SharedString::from(""));
    let password = app.host().runtime().signal(SharedString::from(""));
    let save = app.host().runtime().signal(true);
    let tried = app.host().runtime().signal(false);
    let status = app.host().runtime().signal(SharedString::from(""));
    let logins = app.host().runtime().signal(default_logins());
    let icons = deposit_icons(&app.host().images());
    let email_focused = app.host().runtime().signal(false);
    let password_focused = app.host().runtime().signal(false);
    app.mount(
        "Sera",
        SeraState {
            email: email.clone(),
            password: password.clone(),
            save: save.clone(),
            tried: tried.clone(),
            status: status.clone(),
            logins: logins.clone(),
            icons,
            email_focused: email_focused.clone(),
            password_focused: password_focused.clone(),
        },
        SeraApp,
    );
    (
        app,
        email,
        password,
        status,
        logins,
        email_focused,
        password_focused,
    )
}

#[test]
fn sera_saved_row_fills_email() {
    let (app, email, _password, _status, _logins, ..) = mount_sera();
    app.tap("saved-1");
    assert_eq!(
        email.get(),
        SharedString::from("karim@clinic.com"),
        "selecting a saved login fills the form email"
    );
}

#[test]
fn sera_delete_removes_row() {
    let (app, _email, _password, _status, logins, ..) = mount_sera();
    assert_eq!(logins.get().len(), 3, "three shortcuts at boot");
    app.tap("delete-3");
    let kept = logins.get();
    assert_eq!(kept.len(), 2, "delete drops exactly one row");
    assert!(
        kept.iter().all(|l| l.id != 3),
        "the tapped row is the one removed"
    );
}

#[test]
fn sera_empty_submit_asks_for_fixes() {
    let (app, _email, _password, status, _logins, ..) = mount_sera();
    app.tap("sera-submit");
    assert_eq!(
        status.get(),
        SharedString::from("Fix the highlighted fields"),
        "empty submit does not welcome"
    );
}

#[test]
fn sera_valid_submit_welcomes() {
    let (app, email, password, status, _logins, ..) = mount_sera();
    email.set(SharedString::from("you@clinic.com"));
    password.set(SharedString::from("long-enough"));
    app.run_idle();
    app.tap("sera-submit");
    assert_eq!(
        status.get(),
        SharedString::from("Welcome, you@clinic.com!"),
        "valid submit welcomes"
    );
}

#[test]
fn sera_input_focuses_and_updates_focus_signal() {
    let (app, _email, _password, _status, _logins, email_focused, password_focused) = mount_sera();
    assert!(!email_focused.get(), "email unfocused at start");
    assert!(!password_focused.get(), "password unfocused at start");

    // Tap email field
    app.tap("sera-email");
    assert!(email_focused.get(), "email field is focused");
    assert!(!password_focused.get(), "password field remains unfocused");

    // Tap password field
    app.tap("sera-password");
    assert!(!email_focused.get(), "email field blurred");
    assert!(password_focused.get(), "password field is focused");
}

#[test]
fn sera_icons_render_as_native_vector_paths() {
    let app = Harness::new();
    let icons = deposit_icons(&app.host().images());
    for (spec, expected_size) in [
        (&icons.mail, 20.0),
        (&icons.lock, 20.0),
        (&icons.trash, 18.0),
        (&icons.login, 18.0),
        (&icons.bell, 20.0),
        (&icons.info, 20.0),
        (&icons.cloud, 20.0),
        (&icons.more, 20.0),
    ] {
        assert_eq!(spec.size, expected_size, "native vector icon size matches");
        assert!(!spec.data.is_empty(), "icon has non-empty SVG path data");
        let vnode = oppa::Path::new(spec.debug)
            .data(&spec.data)
            .fill(spec.color)
            .size(spec.size, spec.size)
            .build();
        let oppa::VNode::Element(el) = vnode else {
            panic!("icon leaf is an element");
        };
        assert_eq!(el.tag, oppa::Tag::Path, "icon builds as Tag::Path");
        assert!(el.path.is_some(), "icon contains PathSpec");
    }
}

#[test]
fn sera_icon_spec_loads_from_file_path() {
    let spec = sera_login::IconSpec::from_file(
        "test-mail",
        "icons/mail.svg",
        20.0,
        oppa::Color(0xFF_919191),
    );
    assert_eq!(spec.size, 20.0);
    assert!(!spec.data.is_empty(), "SVG file path parses into path data");
}

/// Vertical composition under real shaping (rustybuzz + bundled
/// DejaVu, the `keyboard_text` precedent — no platform gate): the
/// card sits on the true viewport center and the footer hugs the
/// bottom edge, at the desktop window size.
#[test]
fn sera_card_centers_with_footer_at_bottom() {
    use sera_login::sera_root;

    let app = Harness::new();
    app.set_viewport(1280.0, 800.0);
    let (svc, _) = oppa_text_rustybuzz::RustybuzzService::from_bytes_with_chain(
        &[("DejaVuSans.ttf", oppa_fonts::DEJAVU_SANS)],
        &[],
    )
    .expect("bundled font parses");
    app.host().set_text_service(Box::new(svc));
    app.host().set_layout_config(oppa::LayoutTextConfig {
        family: oppa_fonts::DEJAVU_SANS_FAMILY.to_string(),
        ..Default::default()
    });
    app.mount("Sera", (), sera_root);
    app.run_idle();

    let box_of = |debug: &str| {
        let id = app.node(debug);
        app.host()
            .committed_box(id)
            .unwrap_or_else(|| panic!("no committed box for {debug:?}"))
    };
    let card = box_of("sera-card");
    let foot = box_of("sera-foot");
    let top = box_of("sera-top");
    let card_center = card.y + card.h / 2.0;
    assert!(
        (card_center - 400.0).abs() <= 2.0,
        "card on the true viewport center, got {card_center:.1} (top {:.1}, h {:.1})",
        card.y,
        card.h,
    );
    assert!(
        (top.h - foot.h).abs() <= 2.0,
        "equal chrome above/below (top {:.1}, foot {:.1})",
        top.h,
        foot.h,
    );
    assert!(
        800.0 - (foot.y + foot.h) <= 26.0,
        "footer hugs the bottom edge, got bottom gap {:.1}",
        800.0 - (foot.y + foot.h),
    );
}
