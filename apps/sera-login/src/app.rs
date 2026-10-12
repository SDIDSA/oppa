//! Sera-style dental sign-in page — the app, written once.
//!
//! A two-pane sign-in card over a dark page: saved-login shortcuts on
//! the left, the email/password form on the right. Only `oppa` +
//! `oppa-controls` vocabulary (signals, `Row`/`Column`/`Div`/`Text`,
//! `TextInput` with validation marks, `Checkbox`, `Button`). No `cfg`,
//! no shell/renderer/file/clipboard imports, so this file compiles
//! unchanged into every runner:
//!
//! - Desktop (Windows + Linux): `src/main.rs` mounts `sera_root`
//!   via `run_desktop` — run with `cargo run`.
//! - Web: `src/lib.rs` exports `sera_root` via `export_app!` — run
//!   with `cargo oppa serve`.
//! - Android: mount `sera_root` through the shared runner (see the
//!   `login` app README for the recipe; needs a device).
//!
//! Fidelity notes (deliberate approximations, never silent): all
//! eight icons are Remix Icon v4.9.1 `line` glyphs (© Remix Design,
//! Apache-2.0 — see the Icons section below for the file list) as
//! filled `Path` leaves. Note the GPU backend tessellates paths at
//! `sample_count: 1` (no MSAA), so filled 18–20px glyphs read clean
//! while hairline strokes would crunch — the pack's filled outlines
//! are exactly the right shape for it. The sign-in button is a
//! hand-rolled press `Row` (dark ink on blue needs app colors the
//! themed `Button` cannot take). The `Checkbox` keeps themed fills (a
//! half shade off the reference — accepted).

// Control components are `CamelCase` functions by framework convention
// (`#[component] fn Name` — M2 authoring surface).
#![allow(non_snake_case)]

use oppa::{
    AlignItems, Color, Column, Ctx, CursorIcon, Div, ImageCache, JustifyContent, Path, Portal,
    Props, Row, Semantics, SharedString, Signal, Style, Text, ThemeMode, VNode,
};

use oppa_controls::{with_ink, Checkbox, CheckboxProps, TextInput, TextInputProps};

// ---------------------------------------------------------------------------
// Palette — every literal sampled off the reference screenshot (median
// over solid fills, brightest-pixel for text, darkest for inks; see
// the round notes in docs/STATE.md). The reference page is one fixed
// dark design, so page/card/row/input chrome all ride exact colors;
// only the `Checkbox` keeps themed fills (its 20px box sits a half
// shade deeper than the reference — accepted, stated here).
// ---------------------------------------------------------------------------

const PAGE: Color = Color(0xFF_191D23);
const CARD: Color = Color(0xFF_25272C);
const EDGE: Color = Color(0xFF_34383E);
const DIVIDER: Color = Color(0xFF_34383E);
const ROW_BG: Color = Color(0xFF_171A1E);
const ROW_EDGE: Color = Color(0xFF_3C3F44);
const AVATAR_BLUE: Color = Color(0xFF_6BA3FF);
const AVATAR_PINK: Color = Color(0xFF_FF7F7F);
const AVATAR_INK: Color = Color(0xFF_171A1E);
const LINK: Color = Color(0xFF_6BA3FF);
const WHITE: Color = Color(0xFF_FF_FF);
/// Subtitles, field labels, checkbox-adjacent copy.
const TEXT_DIM: Color = Color(0xFF_D7D7D7);
/// Roles, footer, trash mark.
const TEXT_FAINT: Color = Color(0xFF_919191);
const INPUT_BG: Color = Color(0xFF_171A1E);
const INPUT_EDGE: Color = Color(0xFF_2E3238);
const INPUT_FOCUS: Color = Color(0xFF_548DEB);
const BUTTON_BG: Color = Color(0xFF_6BA3FF);
const BUTTON_PRESS: Color = Color(0xFF_548DEB);
const BUTTON_INK: Color = Color(0xFF_171A1E);

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

/// One saved-login shortcut row.
#[derive(Clone)]
pub struct SavedLogin {
    pub id: u64,
    pub name: String,
    pub role: String,
    pub email: String,
    pub initials: String,
    pub blue: bool,
}

/// Author-owned page state (controls stay controlled — signals in,
/// nothing retained).
#[derive(Clone)]
pub struct SeraState {
    pub email: Signal<SharedString>,
    pub password: Signal<SharedString>,
    pub save: Signal<bool>,
    pub tried: Signal<bool>,
    pub status: Signal<SharedString>,
    pub logins: Signal<Vec<SavedLogin>>,
    pub icons: Icons,
    pub email_focused: Signal<bool>,
    pub password_focused: Signal<bool>,
}

impl Props for SeraState {}

pub fn default_logins() -> Vec<SavedLogin> {
    vec![
        SavedLogin {
            id: 1,
            name: "Dr. Karim Benali".to_string(),
            role: "AdminDentist".to_string(),
            email: "karim@clinic.com".to_string(),
            initials: "DKB".to_string(),
            blue: true,
        },
        SavedLogin {
            id: 2,
            name: "Dr. Karim Benali".to_string(),
            role: "Admin".to_string(),
            email: "karim@clinic.com".to_string(),
            initials: "DKB".to_string(),
            blue: true,
        },
        SavedLogin {
            id: 3,
            name: "Visual Tester".to_string(),
            role: "Admin".to_string(),
            email: "tester@clinic.com".to_string(),
            initials: "VT".to_string(),
            blue: false,
        },
    ]
}

fn label(text: &str, size: u16, color: Color) -> VNode {
    with_ink(VNode::from(Text::new(text).size(size).bold()), color)
}

fn caption(text: &str) -> VNode {
    with_ink(VNode::from(Text::new(text).size(13)), TEXT_DIM)
}

// ---------------------------------------------------------------------------
// Icons — Remix Icon v4.9.1 (© Remix Design, Apache-2.0;
// https://github.com/Remix-Design/remixicon/blob/master/LICENSE).
// Rendered as native `Path` leaves (decision 291) at exact 1× target sizes
// (20px top-bar/inputs, 18px submit/trash). Vector paths are rendered
// directly by each backend without minification aliasing: inline <svg>
// on Web, tiny-skia on CPU, and Lyon tessellation on GPU.
// ---------------------------------------------------------------------------

/// Specification for a single native vector icon.
#[derive(Clone, Debug, PartialEq)]
pub struct IconSpec {
    pub debug: &'static str,
    pub data: SharedString,
    pub color: Color,
    pub size: f32,
}

impl IconSpec {
    /// Constructs an `IconSpec` by parsing SVG markup (e.g. from `include_str!("../icons/foo.svg")`),
    /// extracting its `<path d="...">` and `viewBox`, and scaling the vector coordinates
    /// to the requested `target_size`.
    pub fn from_svg(debug: &'static str, svg: &str, target_size: f32, color: Color) -> Self {
        let (view_w, path_d) = extract_svg_path(svg)
            .unwrap_or_else(|| panic!("failed to extract path from SVG for {debug}"));
        let scale = if view_w > 0.0 {
            target_size / view_w
        } else {
            1.0
        };
        let scaled = scale_svg_path(&path_d, scale);
        Self {
            debug,
            data: SharedString::from(scaled),
            color,
            size: target_size,
        }
    }

    /// Constructs an `IconSpec` from an SVG file path on disk (native desktop/test targets).
    #[allow(dead_code)]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_file(debug: &'static str, file_path: &str, target_size: f32, color: Color) -> Self {
        let content = std::fs::read_to_string(file_path)
            .unwrap_or_else(|e| panic!("failed to read SVG file at {file_path} for {debug}: {e}"));
        Self::from_svg(debug, &content, target_size, color)
    }
}

/// Helper to extract viewBox width and path data `d` from single-path SVG markup.
fn extract_svg_path(svg: &str) -> Option<(f32, String)> {
    let view_w = if let Some(vb_start) = svg.find("viewBox=\"") {
        let vb_val = &svg[vb_start + 9..];
        if let Some(vb_end) = vb_val.find('"') {
            let parts: Vec<&str> = vb_val[..vb_end].split_whitespace().collect();
            if parts.len() == 4 {
                parts[2].parse::<f32>().ok().unwrap_or(24.0)
            } else {
                24.0
            }
        } else {
            24.0
        }
    } else {
        24.0
    };

    let d_start = svg.find(" d=\"")?;
    let d_val = &svg[d_start + 4..];
    let d_end = d_val.find('"')?;
    let path_d = d_val[..d_end].to_string();

    Some((view_w, path_d))
}

/// Scales SVG path coordinates by `scale`.
fn scale_svg_path(d: &str, scale: f32) -> String {
    let bytes = d.as_bytes();
    let mut out = String::with_capacity(d.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() || b == b',' {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
                i += 1;
            }
        } else if b.is_ascii_alphabetic() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push(b as char);
            out.push(' ');
            i += 1;
        } else if b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'+' {
            let start = i;
            if b == b'-' || b == b'+' {
                i += 1;
            }
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let s = std::str::from_utf8(&bytes[start..i]).unwrap_or("0");
            let val: f32 = s.parse().unwrap_or(0.0);
            let scaled = val * scale;
            let formatted = format!("{scaled:.4}");
            let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(trimmed);
        } else {
            i += 1;
        }
    }
    out.trim().to_string()
}

/// Native vector icon set.
#[derive(Clone, Debug, PartialEq)]
pub struct Icons {
    pub mail: IconSpec,
    pub lock: IconSpec,
    pub trash: IconSpec,
    pub login: IconSpec,
    pub bell: IconSpec,
    pub info: IconSpec,
    pub cloud: IconSpec,
    pub more: IconSpec,
}

/// Returns the native vector icons parsed and scaled from the vendored SVG files.
pub fn default_icons() -> Icons {
    Icons {
        mail: IconSpec::from_svg(
            "sera-icon-mail",
            include_str!("../icons/mail.svg"),
            20.0,
            TEXT_FAINT,
        ),
        lock: IconSpec::from_svg(
            "sera-icon-lock",
            include_str!("../icons/lock.svg"),
            20.0,
            TEXT_FAINT,
        ),
        trash: IconSpec::from_svg(
            "sera-icon-trash",
            include_str!("../icons/trash.svg"),
            18.0,
            TEXT_FAINT,
        ),
        login: IconSpec::from_svg(
            "sera-icon-login",
            include_str!("../icons/login.svg"),
            18.0,
            BUTTON_INK,
        ),
        bell: IconSpec::from_svg(
            "sera-icon-bell",
            include_str!("../icons/bell.svg"),
            20.0,
            TEXT_DIM,
        ),
        info: IconSpec::from_svg(
            "sera-icon-info",
            include_str!("../icons/info.svg"),
            20.0,
            TEXT_DIM,
        ),
        cloud: IconSpec::from_svg(
            "sera-icon-cloud",
            include_str!("../icons/cloud.svg"),
            20.0,
            TEXT_DIM,
        ),
        more: IconSpec::from_svg(
            "sera-icon-more",
            include_str!("../icons/more.svg"),
            20.0,
            TEXT_DIM,
        ),
    }
}

/// Compatibility deposit hook (preserved for runners and harness calls —
/// vector leaves require no cache deposits, so this returns `default_icons()`).
pub fn deposit_icons(_images: &ImageCache) -> Icons {
    default_icons()
}

/// One vector icon as a native `Path` leaf (decision 291).
fn icon(_ctx: &Ctx, spec: &IconSpec, alt: &str, size: f32) -> VNode {
    let mut vnode = Path::new(spec.debug)
        .data(&spec.data)
        .fill(spec.color)
        .size(size, size)
        .build();
    if let VNode::Element(ref mut el) = vnode {
        if !alt.is_empty() {
            el.semantics = Some(Semantics::default().label(alt));
        }
    }
    vnode
}

/// Top-bar icon box: 30px bordered square centering one baked glyph.
fn sys_box(debug: &str, glyph: VNode) -> VNode {
    Row(debug)
        .style(
            Style::new()
                .size(30, 30)
                .radius(8)
                .border(1, EDGE)
                .align_items(AlignItems::Center)
                .justify_content(JustifyContent::Center),
        )
        .child(glyph)
}

/// Exact sign-in button: blue fill, radius 10, dark label + exit icon.
/// A hand-rolled press `Row` (not the themed `Button` control) because
/// the reference ink is near-black on blue while the control paints
/// themed secondary. Measured layout: the icon sits at 17px left
/// padding while the label centers on the FULL button width — built
/// with symmetric 35px insets (17 pad + 18 icon left, 18 spacer + 17
/// pad right) so the middle column lands exactly on center. Press
/// ownership still yields the Tab stop and Enter/Space activation with
/// `button` semantics; the fill deepens while held.
fn signin_button(ctx: &Ctx, glyph: VNode, on_press: impl Fn() + 'static) -> VNode {
    let pressed = ctx.pressed().get();
    Row("sera-submit")
        .style(
            Style::new()
                .fill_width()
                .h(44)
                .bg(if pressed { BUTTON_PRESS } else { BUTTON_BG })
                .radius(10)
                .cursor(CursorIcon::Pointer)
                .pad_x(17)
                .align_items(AlignItems::Center),
        )
        .semantics(Semantics::button().label("Sign in"))
        .on_press(on_press)
        .children([
            glyph,
            Column::new()
                .style(Style::new().flex_grow(1.0).align_items(AlignItems::Center))
                .children([with_ink(
                    VNode::from(Text::new("Sign in").size(15).bold()),
                    BUTTON_INK,
                )]),
            Div("sera-submit-pad")
                .style(Style::new().size(18, 18))
                .build(),
        ])
}

/// Exact input chrome: a near-black 48px rounded box holding the icon
/// slot and a borderless editing field (the `DatePicker` composite
/// precedent — the field renders seamlessly, the container owns the
/// box). The chrome owns the invalid edge (error-red while an error
/// shows, hairline blue-gray otherwise); the message renders under the
/// box, never inside it — a footer inside the horizontal chrome would
/// flex-squeeze beside the field. So `TextInputProps.error_message`
/// stays `None` while `invalid`/`required` still announce through the
/// field's own semantics.
#[allow(clippy::too_many_arguments)]
fn credential_field(
    ctx: &Ctx,
    debug: &str,
    label_text: &str,
    icon: VNode,
    field: VNode,
    error: Option<SharedString>,
    margin_top: u16,
    is_focused: bool,
) -> VNode {
    let t = ctx.theme().tokens();
    // Fixed 22px icon slot so both fields' text starts at the same x.
    let slot = Row(format!("{debug}-slot").as_str())
        .style(
            Style::new()
                .size(22, 30)
                .align_items(AlignItems::Center)
                .justify_content(JustifyContent::Center),
        )
        .child(icon);
    let edge_color = if is_focused {
        INPUT_FOCUS
    } else if error.is_some() {
        t.error
    } else {
        INPUT_EDGE
    };
    let chrome = Row(debug)
        .style(
            Style::new()
                .fill_width()
                .h(48)
                .bg(INPUT_BG)
                .radius(12)
                .border(if is_focused { 1.5 } else { 1.0 }, edge_color)
                .pad_x(12)
                .gap(10)
                .align_items(AlignItems::Center),
        )
        .children([slot, field]);
    Column::new()
        .style(Style::new().fill_width().gap(14).margin_top(margin_top))
        .children(
            [label(label_text, 13, TEXT_DIM), chrome]
                .into_iter()
                .chain(error.map(|msg| with_ink(VNode::from(Text::new(msg).size(12)), t.error))),
        )
}

/// One saved-login row: avatar monogram + name/role + delete mark. The
/// row selects (fills the form email); the mark deletes the row. Both
/// are plain press handlers — no component state, so rows compose
/// inline and need no `ctx.child` identity. The row root and every
/// centering box are `Row`s, never `Div`s: `Div` is block-lite
/// (children stack vertically and flex centering does not apply), so a
/// `Div` root renders this row as a vertical stack with overflowing
/// glyphs instead of the horizontal reference row.
fn saved_row(ctx: &Ctx, state: &SeraState, login: &SavedLogin) -> VNode {
    let email = state.email.clone();
    let status = state.status.clone();
    let fill = login.email.clone();
    let id = login.id;
    let avatar_bg = if login.blue { AVATAR_BLUE } else { AVATAR_PINK };
    let initials = login.initials.clone();
    let delete_logins = state.logins.clone();
    Row(format!("saved-{id}").as_str())
        .style(
            Style::new()
                .bg(ROW_BG)
                .radius(12)
                .border(1, ROW_EDGE)
                .pad_x(14)
                .pad_y(15)
                .gap(12)
                .fill_width()
                .align_items(AlignItems::Center),
        )
        .on_press(move || {
            email.set(SharedString::from(fill.clone()));
            status.set(SharedString::from(""));
        })
        .children([
            Row(format!("avatar-{id}").as_str())
                .style(
                    Style::new()
                        .size(42, 42)
                        .bg(avatar_bg)
                        .radius(10)
                        .align_items(AlignItems::Center)
                        .justify_content(JustifyContent::Center),
                )
                .child(with_ink(
                    VNode::from(Text::new(initials).size(14).bold()),
                    AVATAR_INK,
                )),
            Column::new()
                .style(Style::new().flex_grow(1.0).gap(2))
                .children([
                    with_ink(
                        VNode::from(Text::new(login.name.as_str()).size(14).bold()),
                        WHITE,
                    ),
                    with_ink(
                        VNode::from(Text::new(login.role.as_str()).size(12)),
                        TEXT_FAINT,
                    ),
                ]),
            Row(format!("delete-{id}").as_str())
                .style(
                    Style::new()
                        .size(30, 30)
                        .radius(8)
                        .align_items(AlignItems::Center)
                        .justify_content(JustifyContent::Center),
                )
                .on_press(move || {
                    let kept: Vec<SavedLogin> = delete_logins
                        .get()
                        .into_iter()
                        .filter(|l| l.id != id)
                        .collect();
                    delete_logins.set(kept);
                })
                .child(icon(ctx, &state.icons.trash, "Remove saved login", 18.0)),
        ])
}

/// The two-pane page over the dark background.
pub fn SeraApp(ctx: &Ctx, state: &SeraState) -> VNode {
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

    let mut email_props = TextInputProps::new("Email address", state.email.clone())
        .placeholder("you@clinic.com")
        .borderless(true)
        .focused(state.email_focused.clone());
    email_props.width = 344.0;
    email_props.height = 30.0;
    email_props.debug = SharedString::from("sera-email");
    email_props.required = true;
    email_props.invalid = show_email_err.is_some();
    let mut pass_props = TextInputProps::new("Password", state.password.clone())
        .placeholder("Enter your password")
        .masked(true)
        .borderless(true)
        .focused(state.password_focused.clone());
    pass_props.width = 344.0;
    pass_props.height = 30.0;
    pass_props.debug = SharedString::from("sera-password");
    pass_props.required = true;
    pass_props.invalid = show_pass_err.is_some();

    let status_text = state.status.get();
    let status_clone = status_text.clone();
    let new_clinic = state.status.clone();
    let logins = state.logins.get();

    let mut rows: Vec<VNode> = Vec::with_capacity(logins.len());
    for login in &logins {
        rows.push(saved_row(ctx, state, login));
    }

    // Top bar: wordmark left, Remix icon cluster right.
    let top = Row("sera-top")
        .style(
            Style::new()
                .fill_width()
                .pad_x(20)
                .pad_y(14)
                .justify_content(JustifyContent::SpaceBetween)
                .align_items(AlignItems::Center),
        )
        .children([
            Row("sera-brand")
                .style(Style::new().gap(10).align_items(AlignItems::Center))
                .children([
                    Row("sera-mark")
                        .style(
                            Style::new()
                                .size(26, 26)
                                .circle()
                                .bg(WHITE)
                                .align_items(AlignItems::Center)
                                .justify_content(JustifyContent::Center),
                        )
                        .child(with_ink(VNode::from(Text::new("S").size(13).bold()), PAGE)),
                    label("Sera", 16, WHITE),
                ]),
            Row("sera-sys")
                .style(Style::new().gap(8).align_items(AlignItems::Center))
                .children([
                    sys_box(
                        "sys-bell",
                        icon(ctx, &state.icons.bell, "Notifications", 20.0),
                    ),
                    sys_box("sys-info", icon(ctx, &state.icons.info, "About", 20.0)),
                    sys_box(
                        "sys-cloud",
                        icon(ctx, &state.icons.cloud, "Sync status", 20.0),
                    ),
                    sys_box(
                        "sys-more",
                        icon(ctx, &state.icons.more, "More options", 20.0),
                    ),
                ]),
        ]);

    // Split card: saved logins | divider | sign-in form. Vertical
    // rhythm is measured off the reference (title→subtitle 18,
    // subtitle→rows 27 with 10px row gaps; head 0, email 28, password
    // 21, checkbox 30, button 21, link 22, status 10).
    let card = Row("sera-card")
        .style(Style::new().bg(CARD).radius(16).border(1, EDGE))
        .children([
            Column::new()
                .style(Style::new().w(460).pad_x(30).pad_y(32).gap(27))
                .children([
                    Column::new()
                        .style(Style::new().fill_width().gap(18))
                        .children([
                            label("Saved Logins", 18, WHITE),
                            caption("Choose from your previously saved login credentials"),
                        ]),
                    Column::new()
                        .style(Style::new().fill_width().gap(10))
                        .children(rows),
                ]),
            Div("sera-divider")
                .style(Style::new().w(1).fill_height().bg(DIVIDER).margin_y(30))
                .build(),
            Column::new()
                .style(Style::new().w(460).pad_x(30).pad_y(32).gap(0))
                .children([
                    Column::new()
                        .style(Style::new().fill_width().gap(18))
                        .children([
                            label("Sign in", 18, WHITE),
                            caption("Enter your credentials"),
                        ]),
                    credential_field(
                        ctx,
                        "sera-email-chrome",
                        "Email address",
                        icon(ctx, &state.icons.mail, "Email", 20.0),
                        ctx.child_auto(&email_props, TextInput),
                        show_email_err,
                        28,
                        state.email_focused.get(),
                    ),
                    credential_field(
                        ctx,
                        "sera-password-chrome",
                        "Password",
                        icon(ctx, &state.icons.lock, "Password", 20.0),
                        ctx.child_auto(&pass_props, TextInput),
                        show_pass_err,
                        21,
                        state.password_focused.get(),
                    ),
                    Div("sera-save-wrap")
                        .style(Style::new().fill_width().margin_top(30))
                        .child(ctx.child_auto(
                            &CheckboxProps {
                                label: SharedString::from("Save login for future use"),
                                checked: state.save.clone(),
                                enabled: true,
                                on_change: None,
                                invalid: false,
                                required: false,
                                error_message: None,
                                helper_text: None,
                            },
                            Checkbox,
                        )),
                    Div("sera-submit-wrap")
                        .style(Style::new().fill_width().margin_top(21))
                        .child(signin_button(
                            ctx,
                            icon(ctx, &state.icons.login, "Sign in", 18.0),
                            on_sign_in,
                        )),
                    Div("sera-new")
                        .style(Style::new().fill_width().margin_top(22))
                        .on_press(move || {
                            new_clinic.set(SharedString::from(
                                "Clinic creation is not part of this demo",
                            ));
                        })
                        .child(with_ink(
                            VNode::from(Text::new("New clinic? Create one here").size(13)),
                            LINK,
                        )),
                    Div("sera-status-wrap")
                        .style(Style::new().fill_width().margin_top(10))
                        .child(with_ink(
                            VNode::from(Text::new(status_clone).size(13)),
                            TEXT_DIM,
                        )),
                ]),
        ]);

    let foot_row = Row("sera-foot")
        .style(
            Style::new()
                .fill_width()
                .pad_y(22)
                .justify_content(JustifyContent::Center),
        )
        .children([with_ink(
            VNode::from(
                Text::new("© 2025 Sera. Professional dental practice management.").size(12),
            ),
            TEXT_FAINT,
        )]);

    // Page chrome: the top bar (14px pads + 30px boxes = 58px) and
    // the footer (22px pads + 14px caption = 58px) are equal height
    // by construction, and the whole page rides ONE viewport-tall
    // column inside a `Portal`: portal children lay out against the
    // live viewport height (decision 296), so `SpaceBetween` pins the
    // bar top, the caption bottom, and the card on the TRUE viewport
    // center at any window size — plain flow cannot do this (flex
    // shares and justification only engage under an explicit height,
    // and the engine hands the root a `None` height hint, so a flow
    // middle never grows and the card packs under the bar). The
    // reference holds its card within 1px of center with the caption
    // ~27px above the bottom edge, which equal chrome reproduces.
    // Overflow packs top (negative leftover clamps to zero gaps).
    Portal("sera-page").child(
        Column::new()
            .style(
                Style::new()
                    .bg(PAGE)
                    .fill_width()
                    .fill_height()
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::SpaceBetween)
                    .pad_x(24),
            )
            .children([top, card, foot_row]),
    )
}

/// Self-owned root for real runners (`()` props): desktop
/// `run_desktop`, web `export_app!`, Android `mount_app` all mount
/// exactly this. Pins the dark palette (the reference is one fixed
/// dark design): the conditional set converges after one re-render —
/// a tracked read with no write once dark.
pub fn sera_root(ctx: &Ctx, _props: &()) -> VNode {
    if ctx.host().theme().mode() != ThemeMode::Dark {
        ctx.host().set_theme(ThemeMode::Dark);
    }
    let state = SeraState {
        email: ctx.signal(SharedString::from("")),
        password: ctx.signal(SharedString::from("")),
        save: ctx.signal(true),
        tried: ctx.signal(false),
        status: ctx.signal(SharedString::from("")),
        logins: ctx.signal(default_logins()),
        icons: deposit_icons(&ctx.images()),
        email_focused: ctx.signal(false),
        password_focused: ctx.signal(false),
    };
    ctx.child_auto(&state, SeraApp)
}
