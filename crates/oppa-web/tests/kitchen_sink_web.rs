//! Kitchen sink on web (Round 7.17): the shared `KitchenSinkApp`
//! mounts through `WebApp::new_with_root` and every tab drives
//! through the real bindings (click → inject → run → sync → patch),
//! with CPU paints proving new pixels per stage. Headless (no
//! browser): the Edge leg rides `spike/web/sink.mjs` over the wasm
//! `new_sink` constructor against `web/sink.html`.
//!
//! Coordinates are never invented: every press reads the committed
//! box through the [`WebApp::host`] escape hatch (the testkit rule),
//! and label-addressed presses resolve through the tab order +
//! retained semantics (a renamed label fails loudly, never drifts).

use oppa::{find_retained_by_debug, NodeId, RendererBackend};
use oppa_controls::KitchenSinkApp;
use oppa_cpu::{CpuBackend, FramePlanBuilder};
use oppa_web::WebApp;
/// Committed-box center in CSS px (dpr 1 headless — pump first, so
/// unboxed nodes fail loudly here, never as silent misses).
fn center(app: &WebApp, debug: &str) -> (f32, f32) {
    let id = find_retained_by_debug(app.host(), debug)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no retained node {debug:?}"));
    let b = app
        .host()
        .committed_box(id)
        .unwrap_or_else(|| panic!("node {debug:?} has no committed box — pump first"));
    (b.x + b.w / 2.0, b.y + b.h / 2.0)
}

fn node_by_debug(app: &WebApp, debug: &str) -> NodeId {
    find_retained_by_debug(app.host(), debug)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no retained node {debug:?}"))
}

/// Committed-box center of a known node id.
fn center_of(app: &WebApp, id: NodeId) -> (f32, f32) {
    let b = app.host().committed_box(id).expect("node laid out");
    (b.x + b.w / 2.0, b.y + b.h / 2.0)
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

/// Paints the current tree through a white 800×600 CPU surface
/// (committing only new diffs) and snapshots raw pixels.
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

fn is_blank(px: &[(u8, u8, u8, u8)]) -> bool {
    px.iter().all(|p| *p == (255, 255, 255, 255))
}

/// Form mounts by default: every control role renders, fields are
/// real inputs, and the Select chevron is inline SVG (no glyph).
#[test]
fn sink_mounts_form_with_roles_fields_and_vectors() {
    let mut app = WebApp::new_with_root("KitchenSink", (), KitchenSinkApp);
    let html = app.html();
    assert!(html.contains("Oppa Kitchen Sink"), "title renders");
    // Form-tab roles (progressbar + dialog live on the Overlays
    // tab — asserted there, never here).
    for role in [
        "checkbox", "switch", "slider", "combobox", "radio", "tablist", "tab", "button",
    ] {
        assert!(html.contains(&format!("role=\"{role}\"")), "{role} renders");
    }
    assert!(html.contains("<input"), "TextInput is a real input");
    assert!(html.contains("<textarea"), "TextArea is real");
    assert!(
        html.contains("viewBox=\"0 0 12 8\""),
        "vector chevron renders inline svg"
    );
    assert!(
        !html.contains("viewBox=\"0 0 20 20\""),
        "unchecked: no check svg yet"
    );
    // Every laid-out box is finite (NaN dimensions are the loud
    // layout refusal class — absent here by assertion).
    for debug in ["tabs-container", "tab-panel", "tab-bar", "select-box"] {
        for id in find_retained_by_debug(app.host(), debug) {
            let b = app.host().committed_box(id).expect("box laid out");
            assert!(
                b.x.is_finite() && b.y.is_finite() && b.w.is_finite() && b.h.is_finite(),
                "finite {debug}, got {b:?}"
            );
        }
    }
    let mut cpu = CpuBackend::new();
    let surf = white_surface(&mut cpu);
    let mut cursor = 0;
    assert!(
        !is_blank(&paint_now(&app, &mut cpu, surf, &mut cursor)),
        "form paints pixels"
    );
}

/// Checkbox and toggle flip through the click binding; the check
/// paints as inline SVG once checked.
#[test]
fn sink_checkbox_and_toggle_flip_through_bindings() {
    let mut app = WebApp::new_with_root("KitchenSink", (), KitchenSinkApp);
    let _ = app.html();
    let (x, y) = center(&app, "checkbox");
    let html = app.click(x, y).expect("checkbox press touches");
    let id = node_by_debug(&app, "checkbox");
    assert_eq!(
        app.host()
            .retained_semantics(id)
            .expect("semantics")
            .checked,
        Some(true),
        "press checks"
    );
    assert!(
        html.contains("M 4.5 10.5"),
        "checked: vector check path renders, {html}"
    );
    let (x, y) = center(&app, "toggle");
    app.click(x, y).expect("toggle press touches");
    let id = node_by_debug(&app, "toggle");
    assert_eq!(
        app.host()
            .retained_semantics(id)
            .expect("semantics")
            .checked,
        Some(true),
        "press flips on"
    );
}

/// Select opens through the box binding and picks through an option.
#[test]
fn sink_select_opens_and_picks_through_bindings() {
    let mut app = WebApp::new_with_root("KitchenSink", (), KitchenSinkApp);
    let _ = app.html();
    let (x, y) = center(&app, "select-box");
    let html = app.click(x, y).expect("box press opens");
    assert!(html.contains("Chocolate"), "open list renders options");
    assert_eq!(
        find_retained_by_debug(app.host(), "select-option").len(),
        3,
        "three options mount"
    );
    let mint = find_retained_by_debug(app.host(), "select-option")
        .into_iter()
        .find(|id| {
            app.host()
                .retained_semantics(*id)
                .map(|s| s.label.as_deref() == Some("Mint"))
                .unwrap_or(false)
        })
        .expect("Mint option");
    let b = app.host().committed_box(mint).expect("option laid out");
    let html = app
        .click(b.x + b.w / 2.0, b.y + b.h / 2.0)
        .expect("option press touches");
    assert!(
        find_retained_by_debug(app.host(), "select-option").is_empty(),
        "pick closes the list"
    );
    let box_id = node_by_debug(&app, "select-box");
    assert_eq!(
        app.host()
            .retained_semantics(box_id)
            .expect("semantics")
            .label
            .as_deref(),
        Some("Mint"),
        "box shows the pick"
    );
    assert!(html.contains("Mint"), "pick renders, {html}");
}

/// All four tabs render through tab presses; the modal confirms;
/// the platform tab picks and counts; every stage paints new
/// pixels (no two stages share a frame).
#[test]
fn sink_all_tabs_modal_and_platform_with_new_pixels() {
    let mut app = WebApp::new_with_root("KitchenSink", (), KitchenSinkApp);
    let _ = app.html();
    let mut cpu = CpuBackend::new();
    let surf = white_surface(&mut cpu);
    let mut cursor = 0;
    let mut frames: Vec<Vec<(u8, u8, u8, u8)>> = vec![paint_now(&app, &mut cpu, surf, &mut cursor)];

    // Layout tab (index 1): chips row + effect cards.
    let tabs = find_retained_by_debug(app.host(), "tab-item");
    assert_eq!(tabs.len(), 4, "four tab buttons mount");
    let b = app.host().committed_box(tabs[1]).expect("tab laid out");
    let html = app
        .click(b.x + b.w / 2.0, b.y + b.h / 2.0)
        .expect("layout press touches");
    assert!(
        !find_retained_by_debug(app.host(), "sink::Layout::Chips").is_empty(),
        "chips row mounts"
    );
    assert!(html.contains("Gamma"), "chip labels render");
    assert!(html.contains("soft shadow"), "effect card renders");
    frames.push(paint_now(&app, &mut cpu, surf, &mut cursor));

    // Overlays tab (index 2): dialog opens, confirms, closes.
    let tabs = find_retained_by_debug(app.host(), "tab-item");
    let (x, y) = center_of(&app, tabs[2]);
    let html = app.click(x, y).expect("overlays press touches");
    // Patch transport JSON-escapes attribute quotes (see above).
    assert!(
        html.contains("role=\\\"progressbar\\\""),
        "meter role renders"
    );
    let html = press_labeled(&mut app, "Open dialog");
    assert!(
        !find_retained_by_debug(app.host(), "modal-card").is_empty(),
        "dialog card mounts"
    );
    assert!(html.contains("Confirm settings?"), "dialog title renders");
    assert!(html.contains("role=\\\"dialog\\\""), "dialog role renders");
    frames.push(paint_now(&app, &mut cpu, surf, &mut cursor));
    let html = press_labeled(&mut app, "OK");
    assert!(
        find_retained_by_debug(app.host(), "modal-card").is_empty(),
        "confirm closes"
    );
    assert!(html.contains("confirmed: true"), "confirm callback ran");
    frames.push(paint_now(&app, &mut cpu, surf, &mut cursor));

    // Platform tab (index 3): file pick + persistent count.
    let tabs = find_retained_by_debug(app.host(), "tab-item");
    let b = app.host().committed_box(tabs[3]).expect("tab laid out");
    app.click(b.x + b.w / 2.0, b.y + b.h / 2.0)
        .expect("platform press touches");
    let html = press_labeled(&mut app, "Pick a file");
    assert!(
        html.contains("picked demo-pick.png"),
        "scripted pick renders"
    );
    let html = press_labeled(&mut app, "Count++");
    assert!(html.contains("count 1"), "counter renders, {html}");
    frames.push(paint_now(&app, &mut cpu, surf, &mut cursor));

    for (i, frame) in frames.iter().enumerate() {
        assert!(!is_blank(frame), "stage {i} paints pixels");
        if i > 0 {
            assert_ne!(&frames[i - 1], frame, "stage {i} repaints new pixels");
        }
    }
}

// ---------------------------------------------------------------------------
// Round 12.1 (decision 307): the focused field survives background
// updates through the real bindings.
// ---------------------------------------------------------------------------

fn render_patch_field_app(ctx: &oppa::Ctx, _p: &()) -> oppa::VNode {
    // Instance signals (the uncontrolled shape — no author handles;
    // the test drives everything through pids, boxes, and patches).
    let value = ctx.signal(oppa::SharedString::from(""));
    let count = ctx.signal(0i32);
    let bump_count = count.clone();
    let bump = move || bump_count.set(bump_count.get() + 1);
    oppa::Column::new().children([
        ctx.child(
            "oppa::PatchField",
            1,
            &oppa_controls::TextInputProps::new("Name", value),
            oppa_controls::TextInput,
        ),
        ctx.child(
            "oppa::PatchBump",
            2,
            &oppa_controls::ButtonProps::new("Count++", bump),
            oppa_controls::Button,
        ),
        oppa::VNode::from(oppa::Text {
            text: std::sync::Arc::from(format!("count {}", count.get())),
            style: oppa::Text::body_secondary,
        }),
    ])
}

/// End-to-end proof of the round: browser-typed text round-trips
/// with zero DOM work, and a background state update while the
/// field holds text patches everything but the field — the
/// browser keeps focus, caret, and IME composition because no op
/// ever addresses the live input.
#[test]
fn focused_field_survives_background_updates() {
    let mut app = WebApp::new_with_root("PatchField", (), render_patch_field_app);
    let _ = app.html();
    let field = find_retained_by_debug(app.host(), "text-input")
        .into_iter()
        .next()
        .expect("field retained");
    let pid = oppa_web::pid_of(field);
    // Typing commits through the U8 channel (value property syncs);
    // re-feeding the converged value touches nothing (the parity
    // rule typing depends on — no patch, no swap, no caret risk).
    let typed = app.text(&pid, "ab").expect("typing commits");
    assert!(typed.contains("ab"), "value rides the patch");
    assert!(app.text(&pid, "ab").is_none(), "converged typing is quiet");
    // Background update (counter) while the field holds text.
    let (x, y) = center(&app, "button");
    let patch = app.click(x, y).expect("counter press touches");
    assert!(patch.contains("count 1"), "new state rides the patch");
    assert!(
        patch.contains("\"full\":false"),
        "incremental, never reload"
    );
    assert!(
        !patch.contains(&format!("\"pid\":\"{pid}\"")),
        "no op targets the focused field, {patch}"
    );
}
