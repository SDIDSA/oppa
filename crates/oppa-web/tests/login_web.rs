//! Login page on web: the shared `login_root` mounts through
//! `WebApp::new_login` and the empty-submit validation patch must not
//! duplicate fields. Headless (no browser): the Edge leg rides
//! `spike/web/login.mjs` over the wasm `new_login` constructor
//! against `web/login.html`.
//!
//! Coordinates are never invented: presses read the committed box
//! through the [`WebApp::host`] escape hatch (the testkit rule), and
//! the Sign in press resolves through the tab order + retained
//! semantics (the `kitchen_sink_web` idiom).

use oppa::{find_retained_by_debug, NodeId, RendererBackend};
use oppa_cpu::{CpuBackend, FramePlanBuilder};
use oppa_web::WebApp;

fn input_count(html: &str) -> usize {
    html.matches("<input").count()
}

/// Pids of the rendered `<input>` tags, in document order (the
/// bootstrap `input` handler reads the same `data-pid` attribute).
fn input_pids(html: &str) -> Vec<String> {
    let mut pids = Vec::new();
    let mut offset = 0;
    while let Some(start) = html[offset..].find("<input") {
        let tag_start = offset + start;
        let tag_end = html[tag_start..]
            .find('>')
            .map(|e| tag_start + e)
            .unwrap_or(html.len());
        let tag = &html[tag_start..tag_end];
        if let Some(p) = tag.find("data-pid=\"") {
            let pid = &tag[p + 10..];
            pids.push(pid[..pid.find('"').expect("pid closes")].to_string());
        }
        offset = tag_end + 1;
    }
    pids
}

/// Presses the tab-order button carrying `label` (semantics never go
/// stale across in-place diffs). Returns the patch JSON — a press
/// that touches nothing fails loudly (a dead button is a wiring
/// bug, never a quiet None).
fn press_labeled(app: &mut WebApp, label: &str) -> String {
    let id = app
        .host()
        .tab_order()
        .into_iter()
        .find(|id| {
            app.host()
                .retained_semantics(*id)
                .map(|s| s.label.as_deref() == Some(label))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("button {label:?} is in the tab order"));
    let b = app.host().committed_box(id).expect("button laid out");
    app.click(b.x + b.w / 2.0, b.y + b.h / 2.0)
        .unwrap_or_else(|| panic!("press on {label:?} touched the DOM"))
}

fn node_by_debug(app: &WebApp, debug: &str) -> NodeId {
    find_retained_by_debug(app.host(), debug)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no retained node {debug:?}"))
}

/// Paints the current tree through a white 800×600 CPU surface
/// (committing only new diffs) and snapshots raw pixels (the
/// `kitchen_sink_web::paint_now` shape).
fn paint_now(
    app: &WebApp,
    cpu: &mut CpuBackend,
    surf: oppa::SurfaceId,
    cursor: &mut usize,
) -> Vec<(u8, u8, u8, u8)> {
    for d in app.host().diffs_from(*cursor) {
        cpu.commit(&d).expect("cpu commits the tree");
    }
    *cursor = app.host().diff_count();
    let builder = FramePlanBuilder::new(1.0);
    let plan = app
        .host()
        .with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    cpu.paint(surf, &plan).expect("cpu paints the plan");
    cpu.pixmap(surf)
        .expect("pixmap reads back")
        .pixels()
        .iter()
        .map(|p| (p.red(), p.green(), p.blue(), p.alpha()))
        .collect()
}

fn white_surface(cpu: &mut CpuBackend) -> oppa::SurfaceId {
    cpu.create_surface(oppa::SurfaceDesc {
        width_px: 800,
        height_px: 600,
        background: oppa::Color(0xFF_FF_FF),
    })
    .expect("cpu surface builds")
}

#[test]
fn login_renders_sign_in_with_two_fields() {
    let mut app = WebApp::new_login();
    let html = app.html();
    assert!(html.contains("Sign in"), "title renders");
    assert_eq!(input_count(&html), 2, "exactly two fields: {html}");
    // Both fields stay laid out (no NaN geometry in the shared tree).
    for debug in ["login-email", "login-password", "login-submit"] {
        let id = node_by_debug(&app, debug);
        let b = app.host().committed_box(id).expect("box laid out");
        assert!(
            b.x.is_finite() && b.y.is_finite() && b.w.is_finite() && b.h.is_finite(),
            "finite {debug}, got {b:?}"
        );
    }
}

#[test]
fn login_empty_submit_validates_without_duplicating_fields() {
    let mut app = WebApp::new_login();
    let _ = app.html();
    let patch = press_labeled(&mut app, "Sign in");
    assert!(
        patch.contains("Fix the highlighted fields"),
        "status patch carries the error: {patch}"
    );
    // Kind-change retag (the wrapper reuses the field's node id):
    // the container swaps input→div wholesale, so no places op
    // parents children into the void live input.
    assert!(
        patch.contains(r#""pid":"5-0","html":"<div"#),
        "container retags by swap: {patch}"
    );
    assert!(
        !patch.contains(r#""parent":"5-0""#),
        "no places under the swapped parent: {patch}"
    );
    // Boot consumes the stanza into a discarded patch (the full
    // page's `<body style>` never survives the `innerHTML` mount),
    // so the first interaction re-emits the page chrome instead of
    // sitting light under dark controls forever.
    assert!(
        patch.contains(r#""theme":{"bg":"#),
        "first patch carries the theme stanza: {patch}"
    );
    let html = app.html();
    assert_eq!(input_count(&html), 2, "still two fields after submit");
    assert!(
        html.contains("Email is required") && html.contains("Password is required"),
        "per-field errors render"
    );
}

#[test]
fn login_valid_submit_welcomes_and_paints() {
    let mut app = WebApp::new_login();
    let _ = app.html();
    // Drive the value channel the way bootstrap.js does (pid + full
    // value per input event — pids come from the live markup).
    let pids = input_pids(&app.html());
    assert_eq!(pids.len(), 2, "two field pids");
    let _ = app.text(&pids[0], "you@example.com");
    let _ = app.text(&pids[1], "long-enough");
    let patch = press_labeled(&mut app, "Sign in");
    assert!(
        patch.contains("Welcome, you@example.com!"),
        "welcome rides the patch: {patch}"
    );
    let mut cpu = CpuBackend::new();
    let surf = white_surface(&mut cpu);
    let mut cursor = 0;
    let px = paint_now(&app, &mut cpu, surf, &mut cursor);
    assert!(
        px.iter().any(|p| *p != (255, 255, 255, 255)),
        "welcome paints non-white pixels"
    );
}
