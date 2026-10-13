//! Sera-style dental sign-in page — the app, written once.
//!
//! A centered two-pane sign-in card over a dark page: saved-login
//! shortcuts on the left, the email/password form on the right, a
//! copyright line under the card. Only `oppa` + `oppa-controls`
//! vocabulary (signals, `Row`/`Column`/`Div`/`Text`, `ScrollArea`,
//! `TextInput` in shared [`FieldChrome`](oppa_controls::FieldChrome),
//! `Checkbox`, branded [`Button`](oppa_controls::Button)), no `cfg`,
//! no shell/renderer/file/clipboard imports, so this file compiles
//! unchanged into every runner:
//!
//! - Desktop (Windows + Linux): `src/main.rs` mounts
//!   `sera_extended_root` via `run_desktop_with` under the
//!   `Extended` window style (bare native frame — no in-app bar, so
//!   the page is flexible space, card, and footer) — run with
//!   `cargo run`.
//! - Web: `src/lib.rs` exports `sera_root` via `export_app!` — run
//!   with `cargo oppa serve`.
//! - Android: mount `sera_root` through the shared runner (see the
//!   `login` app README for the recipe; needs a device).
//!
//! Copy keys (`login_*`) resolve to inline English — no i18n infra
//! exists yet: `login_saved_logins` / `login_saved_logins_extended`
//! (left head), `login_sign_in` / `login_sign_in_extended` (right
//! head), `login_save_login` (remember-me), `login_signup_link`
//! ("New clinic? …"), `login_delete` ("Delete" trash tooltip),
//! `login_copyright` + `APP_NAME` (footer).
//!
//! Fidelity notes (deliberate approximations, never silent): the
//! four Remix Icon v4.9.1 `line` glyphs (© Remix Design, Apache-2.0)
//! plus the vendored dentacore tooth mark render as filled `Path`
//! leaves through the shared [`VectorIcon`](oppa_controls::VectorIcon)
//! helper (parsed + scaled by [`Path::from_svg`](oppa::Path::from_svg)).
//! Note the GPU backend tessellates paths with 4x MSAA, so the filled
//! outlines read clean. The sign-in button is the themed `Button`
//! control with brand overrides (periwinkle fill, dark-navy ink,
//! 12px radius, full-width label centering). The `Checkbox` keeps
//! themed fills (a half shade off the reference — accepted, stated
//! here). The trash tooltip is the `Tooltip` control (`Above`,
//! `instant`, error-red card).

// Control components are `CamelCase` functions by framework convention
// (`#[component] fn Name` — M2 authoring surface).
#![allow(non_snake_case)]

use oppa::{
    AlignItems, Color, Column, Ctx, CursorIcon, Div, FontWeight, ImageCache, JustifyContent, Path,
    Portal, Props, Row, ScrollArea, Semantics, SharedString, Signal, Style, Text, TextClass,
    ThemeMode, VNode,
};

use oppa_controls::{
    Button, ButtonProps, CaptionOutbox, Checkbox, CheckboxProps, FieldChrome, HeaderBar,
    HeaderBarProps, Link, LinkProps, TextInput, TextInputProps, Tooltip, TooltipProps, VectorIcon,
};

// ---------------------------------------------------------------------------
// Palette — every literal sampled off the reference screenshot (median
// over solid fills, brightest-pixel for text, darkest for inks; see
// the round notes in docs/STATE.md). The reference page is one fixed
// dark design, so page/card/row/input chrome all ride exact colors;
// only the `Checkbox` keeps themed fills (its 20px box sits a half
// shade deeper than the reference — accepted, stated here).
// ---------------------------------------------------------------------------

/// Base sheet, opaque: #0d0d0d blended 8% toward #8FB0E6. This is
/// what you mostly see — and both gradient rims dissolve into it.
const PAGE: Color = Color(0xFF_171A1E);
/// Card fill (the layering reads window < card < rows/fields).
const CARD: Color = Color(0xFF_26272C);
/// Default border color: every outline (card, rows, fields, divider)
/// rides 1.3px of this.
const EDGE: Color = Color(0xFF_3A3B42);
/// Saved-row + input fill (one token for both — rows and fields
/// share the primary surface).
const ROW_BG: Color = Color(0xFF_17181C);
/// Saved-row hover: `ROW_BG` tinted 5% toward the primary accent.
const ROW_HOVER: Color = Color(0xFF_1B1F27);
const AVATAR_BLUE: Color = Color(0xFF_6BA3FF);
const AVATAR_PINK: Color = Color(0xFF_FF7F7F);
const AVATAR_INK: Color = Color(0xFF_171A1E);
const WHITE: Color = Color(0xFF_FF_FF);
/// Secondary text: subtitles, field labels, footer.
const TEXT_DIM: Color = Color(0xFF_D0D0D0);
/// Muted text: roles, trash mark.
const ROLE_TEXT: Color = Color(0xFF_B8B8B8);
/// Tertiary text + icons: the top-bar wordmark and cluster.
const TERTIARY: Color = Color(0xFF_9AA0A8);
/// Synced-state network mark (soft teal-green).
const CLOUD_GREEN: Color = Color(0xFF_5AC8A0);
/// Trash idle mark.
const TEXT_FAINT: Color = Color(0xFF_919191);
const INPUT_BG: Color = Color(0xFF_17181C);
const INPUT_FOCUS: Color = Color(0xFF_548DEB);
/// Leading field icons (envelope / lock).
const FIELD_ICON: Color = Color(0xFF_B0B6BE);
/// Placeholder ink: primary white dimmed to 30% — faint hint
/// text, clearly below every other ink on the form.
const PLACEHOLDER: Color = Color(0x4D_FF_FF_FF);
const BUTTON_BG: Color = Color(0xFF_6EA0FF);
const BUTTON_PRESS: Color = Color(0xFF_548DEB);
const BUTTON_INK: Color = Color(0xFF_182338);
/// Bottom-left glow, opaque center: normalized primary #6BA3FF
/// (#2974EE after dark-mode normalization) washed 93% toward the
/// base. Painted with the base as its rim stop — over the flat base
/// that composites exactly like a feather into transparency.
const CLOUD_PRIMARY: Color = Color(0xFF_1D242E);
/// Top-right glow, opaque center: derived accent #FDC76F (the
/// complement #FFB133 of the primary, normalized again) washed 93%
/// toward the base. Same base-rim treatment as above.
const CLOUD_ACCENT: Color = Color(0xFF_272624);

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

/// App name (`APP_NAME` in the copy keys — the footer composes
/// `login_copyright` around it).
pub const APP_NAME: &str = "Sera";

/// App typography scale (one-liners over the `Text` builder — size +
/// weight + ink chain inline, no wrapper helper needed).
fn label(text: &str, size: u16, color: Color) -> VNode {
    Text::new(text).size(size).bold().ink(color).into()
}

/// Medium label (row names, roles, field labels —
/// `FontWeight::MEDIUM`, the spec weight between regular and bold).
fn label_med(text: &str, size: u16, color: Color) -> VNode {
    Text::new(text)
        .size(size)
        .weight(FontWeight::MEDIUM)
        .ink(color)
        .into()
}

/// Semi-bold label (field labels, saved-row names, button text —
/// `FontWeight::SEMI_BOLD`; the framework has no fractional sizes,
/// so 13.5px specs land on 14px).
fn label_sb(text: &str, size: u16, color: Color) -> VNode {
    Text::new(text)
        .size(size)
        .weight(FontWeight::SEMI_BOLD)
        .ink(color)
        .into()
}

fn caption(text: &str) -> VNode {
    Text::new(text).size(14).ink(TEXT_DIM).into()
}

// ---------------------------------------------------------------------------
// Icons — Remix Icon v4.9.1 (© Remix Design, Apache-2.0;
// https://github.com/Remix-Design/remixicon/blob/master/LICENSE).
// Rendered as native `Path` leaves through the shared `VectorIcon`
// helper (single-path SVG parsed + scaled by `Path::from_svg`).
// ---------------------------------------------------------------------------

/// Native vector icon set.
#[derive(Clone, Debug, PartialEq)]
pub struct Icons {
    pub mail: VectorIcon,
    pub lock: VectorIcon,
    pub trash: VectorIcon,
    pub login: VectorIcon,
    pub bell: VectorIcon,
    pub info: VectorIcon,
    pub cloud: VectorIcon,
    pub more: VectorIcon,
    pub logo: VectorIcon,
}

/// Returns the native vector icons parsed and scaled from the vendored SVG files.
/// Field glyphs ride 18px boxes (~15px ink); trash and the button
/// glyph ride 20px (~17px ink); the top-bar cluster rides 16px boxes
/// (~14px ink) in tertiary, except the network mark which keeps its
/// synced-state green; the dentacore tooth rides 20px in tertiary. (Remix glyph ink fills ~5/6 of the 24px
/// viewBox, so ink reads ~3px smaller than the box — boxes are sized
/// up to compensate.)
pub fn default_icons() -> Icons {
    Icons {
        mail: VectorIcon::from_svg(
            "sera-icon-mail",
            include_str!("../icons/mail.svg"),
            18.0,
            FIELD_ICON,
        ),
        lock: VectorIcon::from_svg(
            "sera-icon-lock",
            include_str!("../icons/lock.svg"),
            18.0,
            FIELD_ICON,
        ),
        trash: VectorIcon::from_svg(
            "sera-icon-trash",
            include_str!("../icons/trash.svg"),
            20.0,
            TEXT_FAINT,
        ),
        login: VectorIcon::from_svg(
            "sera-icon-login",
            include_str!("../icons/login.svg"),
            20.0,
            BUTTON_INK,
        ),
        bell: VectorIcon::from_svg(
            "sera-icon-bell",
            include_str!("../icons/bell.svg"),
            16.0,
            TERTIARY,
        ),
        info: VectorIcon::from_svg(
            "sera-icon-info",
            include_str!("../icons/info.svg"),
            16.0,
            TERTIARY,
        ),
        cloud: VectorIcon::from_svg(
            "sera-icon-cloud",
            include_str!("../icons/cloud.svg"),
            16.0,
            CLOUD_GREEN,
        ),
        more: VectorIcon::from_svg(
            "sera-icon-more",
            include_str!("../icons/more.svg"),
            16.0,
            TERTIARY,
        ),
        // Dentacore tooth mark (vendored from the dentacore app —
        // group/path transforms flattened into the coordinates, so
        // the loader's viewBox scaling lands it exactly).
        logo: VectorIcon::from_svg(
            "sera-icon-logo",
            include_str!("../icons/logo.svg"),
            20.0,
            TERTIARY,
        ),
    }
}

/// Compatibility deposit hook (preserved for runners and harness calls —
/// vector leaves require no cache deposits, so this returns `default_icons()`).
pub fn deposit_icons(_images: &ImageCache) -> Icons {
    default_icons()
}

/// The dentacore tooth mark in the bar: the vendored `icons/logo.svg`
/// (transforms flattened into the path at vendor time) rendered as a
/// native 20px `Path` leaf in the tertiary bar ink.
fn logo_mark(icons: &Icons) -> VNode {
    Row("sera-bar-mark")
        .style(Style::new().square(20).center())
        .child(icons.logo.leaf("Sera logo"))
}

/// Top header bar (shared by both window frames): a thin transparent
/// strip pinned to the very top, full window width — the blurred page
/// background shows through it. Left group (7 top / 8 right / 6
/// bottom / 8 left pads, 5px gap): the 20px dentacore tooth mark
/// plus the 16px SemiBold tertiary app name (capitalized `Sera`);
/// right group pinned top-right (2px top/right pads, 10px gaps):
/// bell, info, network status, overflow, plus a 0-wide end spacer.
/// Everything handlerless (click-through), so the whole strip drags
/// the OS window under the `Extended` frame; native buttons stay
/// collapsed (height 0 — this strip alone is the chrome).
fn sera_bar(icons: &Icons) -> VNode {
    Row("sera-bar")
        .style(
            Style::new()
                .fill_width()
                .align_items(AlignItems::Center)
                .justify_content(JustifyContent::SpaceBetween),
        )
        .children([
            Row("sera-bar-brand")
                .style(
                    Style::new()
                        .pad_trbl(7, 8, 6, 8)
                        .gap(5)
                        .align_items(AlignItems::Center),
                )
                .children([logo_mark(icons), label_sb("Sera", 16, TERTIARY)]),
            Row("sera-bar-sys")
                .style(
                    Style::new()
                        .pad_top(2)
                        .pad_right(2)
                        .gap(10)
                        .align_items(AlignItems::Center),
                )
                .children([
                    Row("sys-bell").child(icons.bell.leaf("Notifications")),
                    Row("sys-info").child(icons.info.leaf("About")),
                    Row("sys-cloud").child(icons.cloud.leaf("Sync status")),
                    Row("sys-more").child(icons.more.leaf("More options")),
                    Div("sera-sys-end").style(Style::new().size(0, 16)).build(),
                ]),
        ])
}

/// Extended-mode props: the caption outbox plus whether native
/// buttons exist (desktop `main` drains the outbox through
/// `run_desktop_with`; web/Android mount `sera_root` with
/// `Decorated` chrome and never see this).
#[derive(Clone)]
pub struct SeraExtendedProps {
    pub hooks: CaptionOutbox,
    /// Native caption buttons declared (default true — the OS
    /// answers first on Windows, the same buttons bridge on Linux;
    /// false draws no buttons at all).
    pub native: bool,
}

impl Props for SeraExtendedProps {}

/// Exact sign-in button: periwinkle-blue fill, radius 12, dark-navy
/// label + enter icon. The themed `Button` control with brand
/// overrides (the reference ink is dark on blue while the default
/// control paints themed secondary): full-width 44px box, 12px
/// content pads, label centered on the FULL width (icon left, equal
/// spacer right), fill deepening while held. Press ownership still
/// yields the Tab stop and Enter/Space activation.
fn signin_button(ctx: &Ctx, icons: &Icons, on_press: impl Fn() + 'static) -> VNode {
    ctx.child_auto(
        &ButtonProps::new("Sign in", on_press)
            .debug("sera-submit")
            .full_width()
            .height(44.0)
            .content_pad(12.0)
            .fill(BUTTON_BG)
            .pressed_fill(BUTTON_PRESS)
            .ink(BUTTON_INK)
            .radius(12.0)
            .text_style(TextClass::Custom {
                size_px: 15,
                weight: FontWeight::SEMI_BOLD,
            })
            .vector_icon(&icons.login.data, BUTTON_INK, 20.0)
            .full_width_label(),
        Button,
    )
}

/// One saved-login row: avatar monogram + name/role + delete mark.
/// A real component (not inline) so `ctx.hovered()` is per-row:
/// hovering tints the fill 5% toward the primary accent and swaps
/// the outline to primary. The row selects (fills the form email);
/// the mark deletes the row (its own component, so its hover stays
/// its own, wrapped in an immediate above-placed error-red
/// `Tooltip`). Roots are `Row`s, never `Div`s: `Div`
/// is block-lite (children stack vertically and flex centering does
/// not apply), so a `Div` root renders this row as a vertical stack
/// with overflowing glyphs instead of the horizontal reference row.
#[derive(Clone)]
pub struct SavedRowProps {
    pub login: SavedLogin,
    pub state: SeraState,
}

impl Props for SavedRowProps {}

fn saved_row_view(ctx: &Ctx, props: &SavedRowProps) -> VNode {
    let login = &props.login;
    let email = props.state.email.clone();
    let status = props.state.status.clone();
    let fill = login.email.clone();
    let id = login.id;
    let avatar_bg = if login.blue { AVATAR_BLUE } else { AVATAR_PINK };
    let initials = login.initials.clone();
    let hovered = ctx.hovered().get();
    let t = ctx.theme().tokens();
    Row(format!("saved-{id}").as_str())
        .style(
            Style::new()
                .bg(if hovered { ROW_HOVER } else { ROW_BG })
                .radius(12)
                .border(1.3, if hovered { BUTTON_BG } else { EDGE })
                .pad_x(15)
                .pad_y(15)
                .gap(15)
                .fill_width()
                .align_items(AlignItems::Center)
                .cursor(CursorIcon::Pointer),
        )
        .on_press(move || {
            email.set(SharedString::from(fill.clone()));
            status.set(SharedString::from(""));
        })
        .children([
            Row(format!("avatar-{id}").as_str())
                .style(Style::new().square(42).bg(avatar_bg).radius(10).center())
                .child(Text::new(initials).size(15).bold().ink(AVATAR_INK).into()),
            Column::new()
                .style(Style::new().flex_grow(1.0).gap(3))
                .children([
                    label_med(login.name.as_str(), 16, WHITE),
                    Text::new(login.role.as_str())
                        .size(12)
                        .weight(FontWeight::MEDIUM)
                        .ink(ROLE_TEXT)
                        .into(),
                ]),
            ctx.child_auto(
                &TooltipProps::new(
                    "Delete",
                    trash_button,
                    TrashButtonProps {
                        icons: props.state.icons.clone(),
                        id,
                        logins: props.state.logins.clone(),
                    },
                )
                .above()
                .instant()
                .fill(t.error)
                .ink(WHITE)
                .debug(format!("trash-tip-{id}")),
                Tooltip,
            ),
        ])
}

/// The delete mark: its own component so its hover stays its own —
/// hovering the 20px trash turns it error-red (the row tint is the
/// row's business). Wrapped by the row in an immediate above-placed
/// `Tooltip`, so no per-row tip signals travel the page.
#[derive(Clone)]
struct TrashButtonProps {
    icons: Icons,
    id: u64,
    logins: Signal<Vec<SavedLogin>>,
}

impl Props for TrashButtonProps {}

fn trash_button(ctx: &Ctx, props: &TrashButtonProps) -> VNode {
    let t = ctx.theme().tokens();
    let hovered = ctx.hovered().get();
    let id = props.id;
    let delete_logins = props.logins.clone();
    let mut glyph = Path::new("sera-icon-trash")
        .data(&props.icons.trash.data)
        .fill(if hovered { t.error } else { TEXT_FAINT })
        .size(20.0, 20.0)
        .build();
    if let VNode::Element(ref mut el) = glyph {
        el.semantics = Some(Semantics::default().label("Remove saved login"));
    }
    Row(format!("delete-{id}").as_str())
        .style(
            Style::new()
                .square(30)
                .radius(8)
                .center()
                .cursor(CursorIcon::Pointer),
        )
        .on_press(move || {
            let kept: Vec<SavedLogin> = delete_logins
                .get()
                .into_iter()
                .filter(|l| l.id != id)
                .collect();
            delete_logins.set(kept);
        })
        .child(glyph)
}

/// Split card (shared by the portable `SeraApp` page and the
/// `Extended` root — one card definition, two window frames).
/// Split card: saved logins | divider | sign-in form. One
/// horizontal row, centered content: two fixed 450px panels with
/// 25px inner padding, 12px between stacked elements, top-aligned;
/// the 1.3px divider runs full height between them. The left list
/// scrolls internally (236px viewport = exactly three rows) so the
/// card follows the right side. Right rhythm: head, 3px, zero-gap
/// input stack, flexible spacer, button, link. (Submit feedback
/// lives in the per-field error slots — no bottom status line.)
fn sera_card(ctx: &Ctx, state: &SeraState) -> VNode {
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

    let email_props = TextInputProps::new("Email address", state.email.clone())
        .placeholder("you@clinic.com")
        .placeholder_ink(PLACEHOLDER)
        .borderless(true)
        .style(TextClass::Custom {
            size_px: 16,
            weight: FontWeight::NORMAL,
        })
        .focused(state.email_focused.clone())
        .size(344.0, 20.0)
        .debug("sera-email")
        .required(true)
        .invalid(show_email_err.is_some());
    let pass_props = TextInputProps::new("Password", state.password.clone())
        .placeholder("Enter your password")
        .placeholder_ink(PLACEHOLDER)
        .masked(true)
        .borderless(true)
        .style(TextClass::Custom {
            size_px: 16,
            weight: FontWeight::NORMAL,
        })
        .focused(state.password_focused.clone())
        .size(344.0, 20.0)
        .debug("sera-password")
        .required(true)
        .invalid(show_pass_err.is_some());

    let new_clinic = state.status.clone();
    let logins = state.logins.get();

    let rows: Vec<VNode> = logins
        .iter()
        .map(|login| {
            ctx.child_auto(
                &SavedRowProps {
                    login: login.clone(),
                    state: state.clone(),
                },
                saved_row_view,
            )
        })
        .collect();

    let list = ScrollArea("sera-saved-list")
        .style(Style::new().fill_width().h(236).gap(10))
        .on_scroll(|| {})
        .children(rows);
    Row("sera-card")
        .style(
            Style::new()
                .bg(CARD)
                .radius(20)
                .border(1.3, EDGE)
                .align_items(AlignItems::Start)
                .justify_content(JustifyContent::Center),
        )
        .children([
            Column::new()
                .style(
                    Style::new()
                        .w(450)
                        .pad_x(25)
                        .pad_top(25)
                        .pad_bottom(25)
                        .gap(12),
                )
                .children([
                    Column::new()
                        .style(Style::new().fill_width().gap(12))
                        .children([
                            label("Saved Logins", 18, WHITE),
                            caption("Choose from your previously saved login credentials"),
                        ]),
                    Div("sera-list-fill")
                        .style(Style::new().fill_width().flex_grow(1.0))
                        .build(),
                    list,
                ]),
            Div("sera-divider")
                .style(Style::new().w(1.3).fill_height().bg(EDGE))
                .build(),
            Column::new()
                .style(
                    Style::new()
                        .w(450)
                        .pad_x(25)
                        .pad_top(25)
                        .pad_bottom(25)
                        .gap(12),
                )
                .children([
                    Column::new()
                        .style(Style::new().fill_width().gap(12))
                        .children([
                            label("Sign in", 18, WHITE),
                            caption("Enter your credentials"),
                        ]),
                    // The 3px strip proper: with the panel's
                    // 12px rhythm above and below, the
                    // subtitle floats exactly 27px over the
                    // email label on bare secondary.
                    Div("sera-head-strip")
                        .style(Style::new().fill_width().h(3))
                        .build(),
                    Column::new()
                        .style(Style::new().fill_width().gap(0))
                        .children([
                            FieldChrome::new("sera-email-chrome", "Email address")
                                .label_ink(TEXT_DIM)
                                .chrome_bg(INPUT_BG)
                                .edge(EDGE)
                                .edge_focused(INPUT_FOCUS)
                                .build(
                                    ctx,
                                    state.icons.mail.leaf("Email"),
                                    ctx.child_auto(&email_props, TextInput),
                                    show_email_err,
                                    state.email_focused.get(),
                                ),
                            FieldChrome::new("sera-password-chrome", "Password")
                                .label_ink(TEXT_DIM)
                                .chrome_bg(INPUT_BG)
                                .edge(EDGE)
                                .edge_focused(INPUT_FOCUS)
                                .build(
                                    ctx,
                                    state.icons.lock.leaf("Password"),
                                    ctx.child_auto(&pass_props, TextInput),
                                    show_pass_err,
                                    state.password_focused.get(),
                                ),
                            ctx.child_auto(
                                &CheckboxProps {
                                    label: SharedString::from("Save login for future use"),
                                    checked: state.save.clone(),
                                    enabled: true,
                                    on_change: None,
                                    invalid: false,
                                    required: false,
                                    error_message: None,
                                    helper_text: None,
                                    box_px: 18.0,
                                    box_radius: 5.0,
                                    label_style: TextClass::Custom {
                                        size_px: 14,
                                        weight: FontWeight::NORMAL,
                                    },
                                    label_ink: Some(TEXT_DIM),
                                    check_ink: Some(PAGE),
                                    check_scale: 0.85,
                                },
                                Checkbox,
                            ),
                        ]),
                    Div("sera-form-fill")
                        .style(Style::new().fill_width().flex_grow(1.0))
                        .build(),
                    signin_button(ctx, &state.icons, on_sign_in),
                    // Indented 5px like the field labels, with a little
                    // extra air below the button (12 rhythm + 6).
                    Row("sera-new-row")
                        .style(Style::new().fill_width().pad_left(5).margin_top(6))
                        .child(
                            ctx.child_auto(
                                &LinkProps::new("New clinic? Create one here", move || {
                                    new_clinic.set(SharedString::from(
                                        "Clinic creation is not part of this demo",
                                    ));
                                })
                                .style(TextClass::Custom {
                                    size_px: 13,
                                    weight: FontWeight::NORMAL,
                                })
                                // Dark-mode normalized primary — vivid on
                                // dark, exactly as specified.
                                .ink(Color(0xFF_6BA3FF))
                                .debug("sera-new"),
                                Link,
                            ),
                        ),
                ]),
        ])
}

/// The centered page (shared by the portable `SeraApp` and the
/// `Extended` root — one layout, two chromes). One viewport-tall
/// column inside a `Portal` (portal children lay out against the
/// live viewport height, decision 296): equal flex fillers above and
/// below push the card to the middle at any window size, the footer
/// rides under the card with 20px to the bottom edge. Plain flow
/// cannot do this (flex shares only engage under an explicit height,
/// and the engine hands the root a `None` height hint). Overflow
/// packs top.
fn sera_page(ctx: &Ctx, header: VNode, card: VNode) -> VNode {
    let mut children = sera_hazes(ctx);
    children.extend([
        header,
        Div("sera-fill-top")
            .style(Style::new().fill_width().flex_grow(1.0))
            .build(),
        card,
        Div("sera-fill-bottom")
            .style(Style::new().fill_width().flex_grow(1.0))
            .build(),
        sera_foot(),
    ]);
    Portal("sera-page").child(
        Column::new()
            .style(
                Style::new()
                    .bg(PAGE)
                    .fill_width()
                    .fill_height()
                    .align_items(AlignItems::Center),
            )
            .children(children),
    )
}

/// Portable page (system window chrome outside, no in-app bar — web,
/// Android, and headless all mount exactly this).
pub fn SeraApp(ctx: &Ctx, state: &SeraState) -> VNode {
    sera_page(ctx, sera_bar(&state.icons), sera_card(ctx, state))
}

/// Full-window backdrop clouds: one viewport-sized radial fill per
/// corner, sized from the live viewport (radius = 0.7 × shortest
/// side, so ~539px on the 1400×770 window) — a true gradient from
/// the washed center out to the base, expanded by the shared builder
/// into solid-circle steps (native `radial-gradient` on DOM). Both
/// boxes are handlerless out-of-flow (`x`/`absolute_y` directly under
/// the page column, whose content origin is the viewport origin), so
/// they never disturb layout and every click falls through to the
/// app. They paint first (below the bar, the card, and any toast).
/// The falloff completes far past every window edge (centers sit
/// 30px in with r ≈ 539), so the surface edge never cuts it.
fn sera_hazes(ctx: &Ctx) -> Vec<VNode> {
    let (vw, vh) = ctx.host().viewport_size();
    let r = 0.7 * vw.min(vh);
    vec![
        Div("sera-haze-blue")
            .style(Style::new().size(vw, vh).x(0.0).absolute_y(0.0).bg_radial(
                30.0,
                vh - 30.0,
                r,
                CLOUD_PRIMARY,
                PAGE,
            ))
            .build(),
        Div("sera-haze-olive")
            .style(Style::new().size(vw, vh).x(0.0).absolute_y(0.0).bg_radial(
                vw - 30.0,
                30.0,
                r,
                CLOUD_ACCENT,
                PAGE,
            ))
            .build(),
    ]
}

/// Page footer caption (shared by both window frames):
/// `login_copyright` + `APP_NAME` in small secondary text, centred,
/// riding 12px under the card with 20px to the window bottom.
fn sera_foot() -> VNode {
    Row("sera-foot")
        .style(
            Style::new()
                .fill_width()
                .margin_top(12)
                .pad_bottom(20)
                .justify_content(JustifyContent::Center),
        )
        .child(
            Text::new(format!(
                "© 2025 {APP_NAME}. Professional dental practice management."
            ))
            .size(13)
            .ink(TEXT_DIM)
            .into(),
        )
}

/// The `Extended` desktop root (`SeraExtendedProps` props): the bare
/// native frame with the custom top strip as its only chrome — the
/// strip identifies as the `HeaderBar` drag trait (transparent,
/// click-through, full window width) with no native buttons declared
/// (height 0). The page below is flexible space, the centered card,
/// and the footer (see `sera_page` for the layout contract). The
/// caption outbox is still published (the desktop poll hook in
/// `main.rs` drains it), but with no buttons declared the signals
/// never arm. Web/Android keep mounting `sera_root` (`Decorated`,
/// browser/OS chrome, no in-app header).
pub fn sera_extended_root(ctx: &Ctx, props: &SeraExtendedProps) -> VNode {
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
    // Level-triggered caption requests (drained exactly once by the
    // desktop poll hook in `main.rs` — the shared `CaptionOutbox`
    // shape). Retained for the bridge shape, though nothing arms them
    // on this barless design.
    let minimize = ctx.signal(false);
    let maximize = ctx.signal(false);
    let close = ctx.signal(false);
    props
        .hooks
        .publish(minimize.clone(), maximize.clone(), close.clone());
    let bar_icons = state.icons.clone();
    let header_props = HeaderBarProps::new().content(move |_ctx| sera_bar(&bar_icons));
    // Same centered layout as the portable page (see `sera_page`).
    sera_page(
        ctx,
        ctx.child_auto(&header_props, HeaderBar),
        sera_card(ctx, &state),
    )
}

/// Self-owned portable root (`()` props): web `export_app!`,
/// Android `mount_app`, and headless all mount exactly this
/// (`Decorated` frame — system chrome outside, `sera_top_bar` app
/// bar inside). The desktop binary mounts `sera_extended_root`
/// instead. Pins the dark palette (the reference is one fixed
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
