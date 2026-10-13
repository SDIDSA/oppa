//! Headless proof of the Sera-style sign-in page (`SeraApp`): the
//! same tree `run_desktop`, `export_app!`, and the Android runner
//! mount — no window, no platform code. Shapes with the bundled
//! Readex Pro chain (the desktop `with_app_fonts` input), so the
//! geometry below proves the shipping typeface, not a stand-in.

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
        (&icons.mail, 18.0),
        (&icons.lock, 18.0),
        (&icons.trash, 20.0),
        (&icons.login, 20.0),
        (&icons.bell, 16.0),
        (&icons.info, 16.0),
        (&icons.cloud, 16.0),
        (&icons.more, 16.0),
        (&icons.logo, 20.0),
    ] {
        assert_eq!(spec.size, expected_size, "native vector icon size matches");
        assert!(!spec.data.is_empty(), "icon has non-empty SVG path data");
        let vnode = oppa::Path::new(&spec.debug)
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
fn sera_readex_chain_resolves_all_app_weights() {
    use oppa::text::{FontStyle, FontWeight, TextService, TextStyle};

    // The exact desktop `with_app_fonts` input: three static faces
    // under one family, so BOLD/SEMI_BOLD shape their own outlines
    // instead of falling back to Regular.
    let (svc, skipped) = oppa_text_rustybuzz::RustybuzzService::from_bytes_with_chain(
        sera_login::SERA_FONT_FACES,
        &[],
    )
    .expect("bundled Readex Pro parses");
    assert!(skipped.is_empty(), "no bundled face skipped: {skipped:?}");
    let families = svc.families().to_vec();
    assert_eq!(families, vec!["Readex Pro".to_string()]);
    let mut weights: Vec<u16> = svc
        .enumerate_fonts()
        .iter()
        .map(|info| info.weight.0)
        .collect();
    weights.sort_unstable();
    assert_eq!(weights, vec![400, 600, 700]);

    // End-to-end weight proof: a BOLD run lands on the 700 face's
    // bytes (not Regular standing in silently).
    let mut bold = TextStyle::new(sera_login::SERA_FONT_FAMILY, 20.0);
    bold.weight = FontWeight::BOLD;
    bold.style = FontStyle::Normal;
    let shaped = svc
        .shape("Sign in", &bold)
        .expect("Readex Pro shapes latin");
    let run = shaped.runs.first().expect("shaped run");
    let (bytes, _) = svc.face_bytes(run.font_id).expect("shaped id has a face");
    assert_eq!(
        bytes.as_ref(),
        oppa_fonts::READEX_PRO_BOLD,
        "BOLD shapes the Bold face"
    );

    // And NORMAL lands on Regular.
    let shaped = svc
        .shape(
            "Sign in",
            &TextStyle::new(sera_login::SERA_FONT_FAMILY, 20.0),
        )
        .expect("Readex Pro shapes latin");
    let run = shaped.runs.first().expect("shaped run");
    let (bytes, _) = svc.face_bytes(run.font_id).expect("shaped id has a face");
    assert_eq!(
        bytes.as_ref(),
        oppa_fonts::READEX_PRO_REGULAR,
        "NORMAL shapes the Regular face"
    );
}

#[test]
fn sera_icon_spec_loads_from_file_path() {
    let spec = sera_login::VectorIcon::from_file(
        "test-mail",
        "icons/mail.svg",
        20.0,
        oppa::Color(0xFF_919191),
    );
    assert_eq!(spec.size, 20.0);
    assert!(!spec.data.is_empty(), "SVG file path parses into path data");
}

/// Vertical composition under real shaping (rustybuzz + bundled
/// Readex Pro, the `keyboard_text` precedent — no platform gate):
/// the card reads ~900px wide with equal flexible space above and
/// below it (the twin fillers split the leftover evenly), the
/// saved-list viewport holds exactly three rows, the divider spans
/// the full card height, and the footer hugs the bottom edge with
/// 20px under its text, at the desktop window size.
#[test]
fn sera_card_centers_with_footer_at_bottom() {
    use sera_login::sera_root;

    let app = Harness::new();
    app.set_viewport(1400.0, 770.0);
    let (svc, _) = oppa_text_rustybuzz::RustybuzzService::from_bytes_with_chain(
        sera_login::SERA_FONT_FACES,
        &[],
    )
    .expect("bundled Readex Pro parses");
    app.host().set_text_service(Box::new(svc));
    app.host().set_layout_config(oppa::LayoutTextConfig {
        family: sera_login::SERA_FONT_FAMILY.to_string(),
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
    let fill_top = box_of("sera-fill-top");
    let fill_bottom = box_of("sera-fill-bottom");
    let email_box = box_of("sera-email-chrome");
    let email_field = box_of("sera-email");
    let haze_blue = box_of("sera-haze-blue");
    let haze_olive = box_of("sera-haze-olive");
    let signup = box_of("sera-new");
    let list = box_of("sera-saved-list");
    let divider = box_of("sera-divider");
    assert!(
        (card.w - 900.0).abs() <= 4.0,
        "card reads ~900px wide, got {:.1}",
        card.w,
    );
    assert!(
        (fill_top.h - fill_bottom.h).abs() <= 2.0,
        "equal flexible space above/below the card, got top {:.1}, bottom {:.1}",
        fill_top.h,
        fill_bottom.h,
    );
    assert!(
        (list.h - 236.0).abs() <= 1.0,
        "list viewport holds exactly three rows (3x72 + 2x10), got {:.1}",
        list.h,
    );
    assert!(
        (divider.h - card.h).abs() <= 2.0,
        "divider spans the full card height, got divider {:.1} vs card {:.1}",
        divider.h,
        card.h,
    );
    // Box theory: one 16px entry line (20px) + 14px pads top/bottom
    // = 48px committed (the 1.3px border paints outside layout, so
    // the rendered box reads line + 30.6). The field leaf takes
    // exactly the line — no double padding.
    assert!(
        (email_box.h - 48.0).abs() <= 1.0,
        "input box reads line + pads, got {:.1}",
        email_box.h,
    );
    assert!(
        (email_field.h - 20.0).abs() <= 1.0,
        "field leaf takes exactly the text line, got {:.1}",
        email_field.h,
    );
    // The signup link sizes to its own text (the underline hugs
    // the words, never the full panel width).
    assert!(
        signup.w < 200.0,
        "link hugs its text, got {:.1}px wide (panel is 400)",
        signup.w,
    );
    // Backdrop clouds: full-viewport radial fills (true gradients —
    // the ring expansion is proven in the framework suite), primary
    // low-left, accent high-right.
    assert!(
        (haze_blue.w - 1400.0).abs() <= 1.0
            && (haze_blue.h - 770.0).abs() <= 1.0
            && haze_blue.x.abs() <= 1.0
            && haze_blue.y.abs() <= 1.0,
        "blue cloud fills the viewport, got ({:.1}, {:.1}, {:.1}, {:.1})",
        haze_blue.x,
        haze_blue.y,
        haze_blue.w,
        haze_blue.h,
    );
    assert!(
        (haze_olive.w - 1400.0).abs() <= 1.0
            && (haze_olive.h - 770.0).abs() <= 1.0
            && haze_olive.x.abs() <= 1.0
            && haze_olive.y.abs() <= 1.0,
        "olive cloud fills the viewport, got ({:.1}, {:.1}, {:.1}, {:.1})",
        haze_olive.x,
        haze_olive.y,
        haze_olive.w,
        haze_olive.h,
    );
    assert!(
        770.0 - (foot.y + foot.h) <= 2.0,
        "footer hugs the bottom edge, got bottom gap {:.1}",
        770.0 - (foot.y + foot.h),
    );
}

#[test]
fn sera_trash_hover_arms_delete_tooltip() {
    use oppa::input::InputEvent;

    let (app, ..) = mount_sera();
    let box_of = |debug: &str| {
        let id = app.node(debug);
        app.host()
            .committed_box(id)
            .unwrap_or_else(|| panic!("no committed box for {debug:?}"))
    };
    let hover_at = |x: f32, y: f32| {
        app.host().inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Move,
            x,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
        app.run_idle();
    };
    // The delete affordance is the shared `Tooltip` control (immediate,
    // above-placed, error-red): one `tooltip-card` while a trash mark is
    // hovered, none otherwise.
    let tipped = || !oppa::find_retained_by_debug(app.host(), "tooltip-card").is_empty();
    assert!(!tipped(), "no tooltip without hover");

    // Hovering the row body (avatar area — row instance, not trash):
    // no tip; per-instance hover keeps the two apart.
    let row = box_of("saved-1");
    hover_at(row.x + 20.0, row.y + row.h / 2.0);
    assert!(!tipped(), "row hover alone never tips");

    // Hovering the trash: the error-red tip appears above it.
    let trash = box_of("delete-1");
    hover_at(trash.x + trash.w / 2.0, trash.y + trash.h / 2.0);
    assert!(tipped(), "trash hover arms the delete tooltip");
    let tip = box_of("tooltip-card");
    assert!(
        tip.y + tip.h <= trash.y,
        "tip floats above the trash, got tip bottom {:.1} vs trash top {:.1}",
        tip.y + tip.h,
        trash.y,
    );

    // Leaving clears it.
    hover_at(10.0, 10.0);
    assert!(!tipped(), "leaving dismisses the tooltip");
}
