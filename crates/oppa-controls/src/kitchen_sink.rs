//! Unified kitchen sink (Phase 7, decision 276): one reference
//! application exercising every framework capability closed across
//! decisions 238–275 — form controls, flex layout, visual effects,
//! overlays, file picking, persistent storage, and async fetch —
//! mountable on every target (Desktop `run_desktop`, Web
//! `new_with_root`, Android `mount_app`).
//!
//! State is root-owned via `ctx.signal` (the showcase precedent);
//! every stateful control mounts through `ctx.child` with a
//! tab-unique `(name, key)` pair (the M8/F6 rule — pairs are unique
//! across ALL tabs since factories share the Tabs namespace).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use oppa::store::KvStore;
use oppa::{
    AlignItems, CanvasOp, Color, Column, Ctx, Div, Ease, FetchState, FileDialog, FilePickerOptions,
    FlexWrap, FontWeight, GridTrack, ImageCache, JustifyContent, KeyframeMode, KeyframeStop,
    Keyframes, Px, Row, ScriptedDialog, SharedString, Signal, Style, Text, TextSpan, ThemeMode,
    ThemeTokens, VNode,
};

use crate::{
    Action, Badge, BadgeProps, BadgeVariant, BarItem, Button, ButtonProps, CanvasView,
    CanvasViewProps, Checkbox, CheckboxProps, Date, DatePicker, DatePickerProps, FilePicker,
    FilePickerMode, FilePickerProps, ImageView, ImageViewProps, MenuItemProps, MenuTitle, Menubar,
    MenubarProps, Modal, ModalProps, NavHost, NavHostProps, ProgressBar, ProgressBarProps,
    RadioGroup, RadioGroupProps, RadioOption, RichTextView, RichTextViewProps, RouteView, Select,
    SelectItem, SelectProps, Slider, SliderProps, Splitter, SplitterAxis, SplitterProps, TabItem,
    Tabs, TabsProps, TextArea, TextAreaProps, TextInput, TextInputProps, Toggle, ToggleProps,
    Toolbar, ToolbarProps, Tree, TreeNode, TreeProps,
};

/// Unit props: `KitchenSinkApp` takes `&KitchenSinkProps` while the
/// runners pass `()` — the alias unifies both spellings (`()`
/// satisfies `Props`, the showcase precedent), so the brief's
/// `run_desktop(..., (), KitchenSinkApp)` and
/// `fn KitchenSinkApp(ctx, &KitchenSinkProps)` are one signature.
pub type KitchenSinkProps = ();

/// Tab identity (the `Tabs` proof target — exactly one panel shows).
#[derive(Clone, PartialEq)]
pub enum SinkTab {
    Form,
    Layout,
    Overlays,
    Platform,
    Views,
}

impl SinkTab {
    fn label(&self) -> &'static str {
        match self {
            SinkTab::Form => "Form",
            SinkTab::Layout => "Layout",
            SinkTab::Overlays => "Overlays",
            SinkTab::Platform => "Platform",
            SinkTab::Views => "Views",
        }
    }
}

/// Size choice (the `RadioGroup` proof target).
#[derive(Clone, PartialEq)]
enum Size {
    Small,
    Medium,
    Large,
}

impl Size {
    fn label(&self) -> &'static str {
        match self {
            Size::Small => "Small",
            Size::Medium => "Medium",
            Size::Large => "Large",
        }
    }
}

/// Flavor choice (the `Select` proof target).
#[derive(Clone, PartialEq)]
enum Flavor {
    Vanilla,
    Chocolate,
    Mint,
}

impl Flavor {
    fn label(&self) -> &'static str {
        match self {
            Flavor::Vanilla => "Vanilla",
            Flavor::Chocolate => "Chocolate",
            Flavor::Mint => "Mint",
        }
    }
}

/// One wrap-demo chip (plain `Div` — no instance state, so no
/// `ctx.child` needed). The fill is the theme's `disabled` wash
/// (theme contract round): Light's wash IS the pre-contract
/// `0xEE_EE_EE`, so the catalog pixels don't move; Dark gets a
/// deep wash the themed default ink reads on.
fn chip(t: ThemeTokens, index: usize, label: &str) -> VNode {
    Div(&format!("sink::Layout::Chip{index}"))
        .style(
            Style::new()
                .size(72, 28)
                .radius(14)
                .bg(t.disabled)
                .align_items(AlignItems::Center)
                .justify_content(JustifyContent::Center),
        )
        .child(VNode::from(Text::new(label).size(12)))
}

/// NavHost demo routes (Phase 38c, G22): two mini-screens over the
/// sink-owned stack (pushes flow through `NavStack::push` —
/// runners own system back, stated in `NavHost`).
fn sink_home(_ctx: &Ctx) -> VNode {
    Div("sink-views-home").child(VNode::from(Text::new("home route")))
}

fn sink_settings(_ctx: &Ctx) -> VNode {
    Div("sink-views-settings").child(VNode::from(Text::new("settings route")))
}

/// Captioned card helper (explicit size + caption text).
fn card(debug: String, style: Style, caption: &str) -> VNode {
    Div(&debug).style(style).child(VNode::from(Text {
        text: SharedString::from(caption),
        style: Text::body_secondary,
    }))
}

/// The unified showcase: four tabs over root-owned signals. See the
/// module docs for the state/identity rules.
pub fn KitchenSinkApp(ctx: &Ctx, _props: &KitchenSinkProps) -> VNode {
    // ---- Form state (Tab 1) ----
    let name = ctx.signal(SharedString::from(""));
    let bio = ctx.signal(SharedString::from(""));
    let volume = ctx.signal(50.0f32);
    let dark = ctx.signal(false);
    let accept = ctx.signal(false);
    let flavor = ctx.signal(Flavor::Vanilla);
    let flavor_open = ctx.signal(false);
    let size = ctx.signal(Size::Medium);
    // Phase 38a validation demo (G7): an always-invalid email shows
    // the error edge + message (validators stay app-side — the
    // signal below is the stub validator's verdict, announced).
    let email = ctx.signal(SharedString::from("not-an-email"));
    // Phase 38b date demo (G14): controlled date + popup.
    let when = ctx.signal(Date::new(2026, 10, 1));
    let when_open = ctx.signal(false);
    // ---- Views state (Tab 5, Phase 38b–38c) ----
    // Tree hierarchy (G12) over an author-owned collection.
    let tree_coll = oppa::Collection::new(&ctx.host().runtime(), oppa::fetch_key("sink:tree"));
    if tree_coll.is_empty() {
        tree_coll.ingest(vec![
            TreeNode::root("src", "src"),
            TreeNode::child("lib", "lib.rs", "src"),
            TreeNode::child("main", "main.rs", "src"),
            TreeNode::root("docs", "docs"),
        ]);
    }
    let tree_expanded = ctx.signal(vec![SharedString::from("src")]);
    let tree_selected = ctx.signal(None::<SharedString>);
    // Splitter fraction (G13), menubar + file state (G21), nav
    // stack (G22), and the demo image cache (G22 pixels).
    let split_frac = ctx.signal(0.5f32);
    let menu_open = ctx.signal(None::<usize>);
    let picked = ctx.signal(None::<SharedString>);
    let nav_stack = ctx.signal(oppa::NavStack::new(
        oppa::Route::new("home").expect("route names parse"),
    ));
    let img_cache = ctx.signal(ImageCache::new());
    // Bar feedback (G21): toolbar/menubar actions land here.
    let bar_note = ctx.signal(SharedString::from("no command yet"));
    // ---- Overlay state (Tab 3) ----
    let dialog_open = ctx.signal(false);
    let confirmed = ctx.signal(false);
    let download = ctx.signal(35.0f32);
    // ---- Platform state (Tab 4) ----
    let file_status = ctx.signal(SharedString::from("no file picked"));
    let dialog: Signal<Rc<RefCell<ScriptedDialog>>> =
        ctx.signal(Rc::new(RefCell::new(ScriptedDialog::new())));
    let count = ctx.signal(0u32);
    let kv = ctx.signal(oppa::InMemoryKv::new());
    // ---- Tab identity ----
    let tab = ctx.signal(SinkTab::Form);

    let form = {
        let (name, bio, volume, dark, accept, flavor, flavor_open, size) = (
            name.clone(),
            bio.clone(),
            volume.clone(),
            dark.clone(),
            accept.clone(),
            flavor.clone(),
            flavor_open.clone(),
            size.clone(),
        );
        let (email, when, when_open) = (email.clone(), when.clone(), when_open.clone());
        Rc::new(move |ctx: &Ctx| {
            // Theme contract round: the showcase's Dark toggle owns
            // the host palette (previously a dead switch — it showed
            // state but moved nothing). The toggle stays the bound
            // `dark` signal; the callback maps it onto the host
            // theme, and every themed surface re-derives in place.
            let theme_host = ctx.host().clone();
            Column::new().gap(12).children([
                ctx.child_auto(
                    &TextInputProps::new("Name", name.clone()).placeholder("Type your name..."),
                    TextInput,
                ),
                ctx.child_auto(
                    &TextAreaProps::new("Bio", bio.clone())
                        .placeholder("Multi-line bio...")
                        .width(280.0),
                    TextArea,
                ),
                ctx.child_auto(
                    &SliderProps {
                        label: SharedString::from("Volume"),
                        value: volume.clone(),
                        min: 0.0,
                        max: 100.0,
                        step: 5.0,
                        enabled: true,
                        on_change: None,
                        invalid: false,
                        required: false,
                        error_message: None,
                        helper_text: None,
                    },
                    Slider,
                ),
                ctx.child_auto(
                    &ToggleProps {
                        label: SharedString::from("Dark mode"),
                        on: dark.clone(),
                        enabled: true,
                        on_change: Some(Rc::new(move |on: bool| {
                            theme_host.set_theme(if on {
                                ThemeMode::Dark
                            } else {
                                ThemeMode::Light
                            });
                        })),
                        invalid: false,
                        required: false,
                        error_message: None,
                        helper_text: None,
                    },
                    Toggle,
                ),
                ctx.child_auto(
                    &CheckboxProps {
                        label: SharedString::from("Accept terms"),
                        checked: accept.clone(),
                        enabled: true,
                        on_change: None,
                        invalid: false,
                        required: false,
                        error_message: None,
                        helper_text: None,
                    },
                    Checkbox,
                ),
                ctx.child_auto(
                    &SelectProps::new(
                        vec![
                            SelectItem {
                                value: Flavor::Vanilla,
                                label: SharedString::from(Flavor::Vanilla.label()),
                            },
                            SelectItem {
                                value: Flavor::Chocolate,
                                label: SharedString::from(Flavor::Chocolate.label()),
                            },
                            SelectItem {
                                value: Flavor::Mint,
                                label: SharedString::from(Flavor::Mint.label()),
                            },
                        ],
                        flavor.clone(),
                        flavor_open.clone(),
                    ),
                    Select::<Flavor>,
                ),
                ctx.child_auto(
                    &RadioGroupProps {
                        options: vec![
                            RadioOption {
                                value: Size::Small,
                                label: SharedString::from(Size::Small.label()),
                            },
                            RadioOption {
                                value: Size::Medium,
                                label: SharedString::from(Size::Medium.label()),
                            },
                            RadioOption {
                                value: Size::Large,
                                label: SharedString::from(Size::Large.label()),
                            },
                        ],
                        selected: size.clone(),
                        enabled: true,
                        invalid: false,
                        required: false,
                        error_message: None,
                        helper_text: None,
                    },
                    RadioGroup::<Size>,
                ),
                // Phase 38a validation demo (G7): the stub validator
                // rejects this value — the error edge + message
                // announce, the helper shows when valid (edit the
                // signal to see both; valid trees stay identical).
                ctx.child_auto(
                    &TextInputProps {
                        label: SharedString::from("Email"),
                        value: email.clone(),
                        placeholder: Some(SharedString::from("you@example.com")),
                        enabled: true,
                        width: 280.0,
                        height: 32.0,
                        style: Text::body_secondary,
                        debug: SharedString::from("sink-email"),
                        on_change: None,
                        masked: false,
                        invalid: true,
                        required: true,
                        error_message: Some(SharedString::from("Enter a valid email")),
                        helper_text: Some(SharedString::from("We never share it")),
                    },
                    TextInput,
                ),
                // Phase 38b date demo (G14): popup month grid + the
                // `YYYY-MM-DD` parse bridge in one box.
                ctx.child_auto(
                    &DatePickerProps {
                        value: when.clone(),
                        open: when_open.clone(),
                        min: Some(Date::new(2026, 1, 1)),
                        max: Some(Date::new(2026, 12, 31)),
                        enabled: true,
                        width: 240.0,
                        label: SharedString::from("When"),
                    },
                    DatePicker,
                ),
            ])
        })
    };
    let layout = {
        let split_frac = split_frac.clone();
        Rc::new(move |ctx: &Ctx| {
            // Theme contract round: page furniture resolves from the
            // host theme (tracked — toggling re-renders this panel).
            // Light tokens reproduce the pre-contract literals exactly
            // (`surface` IS `0xFF_FF_FF`, `disabled` IS `0xEE_EE_EE`);
            // the gradient card + swatch cells stay deliberate fixed
            // literals (they demo literal styling, and carry no text).
            let t = ctx.theme().tokens();
            Column::new().gap(12).children([
                // Wrapping chip row (decision 253): constrained width
                // forces greedy line-breaking; intrinsic width would
                // (statedly) refuse to wrap.
                Row("sink::Layout::Chips")
                    .style(
                        Style::new()
                            .size(300, 96)
                            .flex_wrap(FlexWrap::Wrap)
                            .gap(8)
                            .align_items(AlignItems::Center),
                    )
                    .children(
                        ["Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta"]
                            .iter()
                            .enumerate()
                            .map(|(i, label)| chip(t, i, label)),
                    ),
                // Soft shadow card (decision 254: `.shadow` then
                // `.shadow_blur` — blur without a shadow panics).
                card(
                    "sink::Layout::ShadowCard".to_string(),
                    Style::new()
                        .size(280, 48)
                        .bg(t.surface)
                        .shadow(2, 4, Color(0x88_88_88))
                        .shadow_blur(8)
                        .pad_x(12)
                        .pad_y(8)
                        .build(),
                    "soft shadow",
                ),
                // Asymmetric borders (decision 254: no radius here —
                // sharp bands vs round shapes refuse loudly).
                card(
                    "sink::Layout::BorderCard".to_string(),
                    Style::new()
                        .size(280, 48)
                        .bg(t.surface)
                        .border_edges(4, 1, 4, 1, Color(0x22_66_CC))
                        .pad_x(12)
                        .pad_y(8)
                        .build(),
                    "asymmetric borders",
                ),
                // Vertical gradient card (decision 254: replaces `bg`,
                // no radius — stated, never silent).
                card(
                    "sink::Layout::GradientCard".to_string(),
                    Style::new()
                        .size(280, 48)
                        .bg_gradient(Color(0x22_66_CC), Color(0x99_CC_FF))
                        .pad_x(12)
                        .pad_y(8)
                        .build(),
                    "linear gradient",
                ),
                // Nested rows/columns with padding and margins
                // (decision 249).
                Column::new()
                    .style(Style::new().pad_x(8).pad_y(8).gap(8))
                    .children([Row("sink::Layout::NestedRow")
                        .style(Style::new().gap(8).margin(4, 2))
                        .children([
                            Div("sink::Layout::CellA")
                                .style(Style::new().size(64, 32).bg(Color(0xDD_DD_DD)))
                                .build(),
                            Div("sink::Layout::CellB")
                                .style(Style::new().size(64, 32).bg(Color(0xCC_CC_CC)))
                                .build(),
                        ])]),
                // Minimal grid (Phase 36 PR2a, decision 353): two
                // `Px` columns over auto rows, one cell spanning both
                // columns (spans only — no explicit placement).
                oppa::Grid("sink::Layout::Grid")
                    .style(
                        Style::new()
                            .grid_cols(vec![
                                GridTrack::Px(Px::of(136.0)),
                                GridTrack::Px(Px::of(136.0)),
                            ])
                            .gap(8),
                    )
                    .children([
                        Div("sink::Layout::G00")
                            .style(Style::new().size(136, 32).bg(t.disabled))
                            .build(),
                        Div("sink::Layout::G01")
                            .style(Style::new().size(136, 32).bg(t.disabled))
                            .build(),
                        Div("sink::Layout::GSpan")
                            .style(
                                Style::new()
                                    .size(280, 32)
                                    .bg(t.surface)
                                    .border(1, t.border)
                                    .col_span(2),
                            )
                            .build(),
                    ]),
                // Splitter (Phase 38b, G13): the fraction signal owns
                // the proportions — drag the divider or arrow it.
                ctx.child_auto(
                    &SplitterProps {
                        fraction: split_frac.clone(),
                        axis: SplitterAxis::Vertical,
                        width: 280.0,
                        height: 120.0,
                        divider_px: 8.0,
                        min_first_px: 48.0,
                        min_second_px: 48.0,
                        enabled: true,
                        debug: SharedString::from("sink-split"),
                        first: Rc::new(|_: &Ctx| {
                            VNode::from(Text::new(SharedString::from("first")))
                        }),
                        second: Rc::new(|_: &Ctx| {
                            VNode::from(Text::new(SharedString::from("second")))
                        }),
                        on_change: None,
                    },
                    Splitter,
                ),
            ])
        })
    };
    let overlays = {
        let (dialog_open, confirmed, download) =
            (dialog_open.clone(), confirmed.clone(), download.clone());
        // Pulse replay gate (Phase 39b): the base wash alternates on
        // press — a real style delta on the keyed card starts a real
        // track (production pumps it; no test presses it, so frozen
        // headless clocks never see a live track).
        let pulse_alt = ctx.signal(false);
        Rc::new(move |ctx: &Ctx| {
            let open_button = dialog_open.clone();
            let dialog_state = dialog_open.clone();
            let dialog_confirmed = confirmed.clone();
            // Keyframe card below resolves from the host theme
            // (tracked — toggling re-renders it in place).
            let t = ctx.theme().tokens();
            Column::new().gap(12).children([
                ctx.child_auto(
                    &ButtonProps {
                        debug: SharedString::from("sink::Overlays::Open"),
                        ..ButtonProps::new("Open dialog", move || open_button.set(true))
                    },
                    Button,
                ),
                ctx.child_auto(
                    &ModalProps::new("Confirm settings?", dialog_state)
                        .on_confirm(move || dialog_confirmed.set(true)),
                    Modal,
                ),
                // Live progress: the slider drives the meter
                // (determinate — value text announces the percent).
                ctx.child_auto(
                    &SliderProps {
                        label: SharedString::from("Download control"),
                        value: download.clone(),
                        min: 0.0,
                        max: 100.0,
                        step: 5.0,
                        enabled: true,
                        on_change: None,
                        invalid: false,
                        required: false,
                        error_message: None,
                        helper_text: None,
                    },
                    Slider,
                ),
                ctx.child_auto(
                    &ProgressBarProps {
                        label: Some(SharedString::from("Download")),
                        ..ProgressBarProps::new(download.get() / 100.0)
                    },
                    ProgressBar,
                ),
                Row("sink::Overlays::Badges")
                    .style(Style::new().gap(8))
                    .children([
                        ctx.child_auto(
                            &BadgeProps::new("Primary").variant(BadgeVariant::Primary),
                            Badge,
                        ),
                        ctx.child_auto(
                            &BadgeProps::new("Success").variant(BadgeVariant::Success),
                            Badge,
                        ),
                        ctx.child_auto(&BadgeProps::new("Dim").variant(BadgeVariant::Dim), Badge),
                    ]),
                VNode::from(Text {
                    text: SharedString::from(format!("confirmed: {}", confirmed.get())),
                    style: Text::body_secondary,
                }),
                // Keyframe pulse (Phase 36 PR4, decision 357): a
                // two-stop `Once` run over per-segment ease. The card
                // carries a stable `.key()` (slot-keyed reuse, lock
                // #13): tab switches Add/Remove it instead of
                // order-pairing it against other tabs' content, so no
                // cross-tab style delta ever spawns a foreign track —
                // without the key, the inherited `prev` bg starts a
                // real track every switch, and frozen-clock harnesses
                // (`MockClock` web hosts) can never settle it. Genuine
                // tracks start only through Replay (a real delta on the
                // card itself); no suite presses it.
                ctx.child_auto(
                    &ButtonProps {
                        debug: SharedString::from("sink::Overlays::Replay"),
                        ..ButtonProps::new("Replay pulse", {
                            let pulse_alt = pulse_alt.clone();
                            move || pulse_alt.set(!pulse_alt.get())
                        })
                    },
                    Button,
                ),
                Div("sink::Overlays::Pulse")
                    .key(0xB16B_00B5)
                    .style(
                        Style::new()
                            .size(280, 32)
                            .radius(16)
                            .bg(if pulse_alt.get() {
                                t.surface
                            } else {
                                t.disabled
                            })
                            .keyframes(Keyframes {
                                stops: vec![
                                    KeyframeStop::new(250, Ease::InOut).bg(t.primary),
                                    KeyframeStop::new(250, Ease::InOut).bg(t.disabled),
                                ],
                                mode: KeyframeMode::Once,
                            }),
                    )
                    .child(VNode::from(Text {
                        text: SharedString::from("replay-gated keyframes"),
                        style: Text::body_secondary,
                    })),
            ])
        })
    };
    let platform = {
        let (file_status, dialog, count, kv) = (
            file_status.clone(),
            dialog.clone(),
            count.clone(),
            kv.clone(),
        );
        let picked = picked.clone();
        Rc::new(move |ctx: &Ctx| {
            // File picker trigger (decision 230): scripted dialog
            // stands in for the OS backend headlessly; each press
            // scripts one response, then requests + polls it.
            let pick_dialog = dialog.clone();
            let pick_status = file_status.clone();
            let pick = Rc::new(move || {
                let handle = pick_dialog.get();
                let mut dlg = handle.borrow_mut();
                dlg.push_response(vec![PathBuf::from("demo-pick.png")]);
                dlg.request_open(FilePickerOptions::default());
                let out = dlg.poll_open();
                drop(dlg);
                pick_status.set(match out {
                    Some(Ok(paths)) if paths.is_empty() => SharedString::from("dismissed"),
                    Some(Ok(paths)) => SharedString::from(format!("picked {}", paths[0].display())),
                    Some(Err(e)) => SharedString::from(format!("picker failed: {e}")),
                    None => SharedString::from("still open..."),
                });
            });
            // Persistent counter (decision 216): author-owned signal
            // mirrored into the `KvStore` on every press.
            let bump_count = count.clone();
            let bump_kv = kv.clone();
            let bump: Action = Rc::new(move || {
                let next = bump_count.get() + 1;
                bump_count.set(next);
                let mut store = bump_kv.get();
                store
                    .set("sink:count", next.to_string().into_bytes())
                    .expect("memory kv sets");
                bump_kv.set(store);
            });
            let stored = kv
                .get()
                .get("sink:count")
                .expect("memory kv reads")
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default();
            // Async fetch mock (decision 220, String rendezvous):
            // the host start/resolve pair is the exact call shape
            // the web binding makes around `fetch()` — Loading
            // shows synchronously, resolve settles Ready/Failed.
            let host = ctx.host();
            let fkey = ctx.fetch_key("kitchen:quote");
            let quote = ctx.fetch_state::<String>(fkey);
            let fetch_status = match quote.get() {
                FetchState::Idle => "tap Fetch for a quote".to_string(),
                FetchState::Loading => "loading...".to_string(),
                FetchState::Ready(text) => text,
                FetchState::Failed(e) => e,
            };
            let ok_host = host.clone();
            let err_host = host.clone();
            Column::new().gap(12).children([
                ctx.child_auto(
                    &ButtonProps {
                        debug: SharedString::from("sink::Platform::Pick"),
                        ..ButtonProps::new("Pick a file", move || pick())
                    },
                    Button,
                ),
                VNode::from(Text {
                    text: SharedString::from(format!("file: {}", file_status.get())),
                    style: Text::body_secondary,
                }),
                ctx.child_auto(
                    &ButtonProps {
                        debug: SharedString::from("sink::Platform::Count"),
                        ..ButtonProps::new("Count++", move || bump())
                    },
                    Button,
                ),
                VNode::from(Text {
                    text: SharedString::from(format!("count {} stored {stored}", count.get())),
                    style: Text::body_secondary,
                }),
                Row("sink::Platform::FetchRow")
                    .style(Style::new().gap(8))
                    .children([
                        ctx.child_auto(
                            &ButtonProps {
                                debug: SharedString::from("sink::Platform::FetchOk"),
                                ..ButtonProps::new("Fetch quote", move || {
                                    let gen = ok_host.start_fetch(fkey);
                                    let _ = ok_host.resolve_fetch(
                                        fkey,
                                        gen,
                                        Ok("DejaVu shapes everywhere".to_string()),
                                    );
                                })
                            },
                            Button,
                        ),
                        ctx.child_auto(
                            &ButtonProps {
                                debug: SharedString::from("sink::Platform::FetchErr"),
                                ..ButtonProps::new("Fetch fails", move || {
                                    let gen = err_host.start_fetch(fkey);
                                    let _ =
                                        err_host.resolve_fetch(fkey, gen, Err("dns".to_string()));
                                })
                            },
                            Button,
                        ),
                    ]),
                VNode::from(Text {
                    text: SharedString::from(fetch_status),
                    style: Text::body_secondary,
                }),
                // FilePicker control (Phase 38c, G21): the same
                // scripted backend behind one trigger — pick sets
                // the controlled path, dismissal keeps it.
                ctx.child_auto(
                    &{
                        let mut fp = FilePickerProps::new(picked.clone(), FilePickerMode::Open);
                        fp.open_dialog = Some(dialog.get());
                        fp.debug = SharedString::from("sink::Platform::Files");
                        fp
                    },
                    FilePicker,
                ),
            ])
        })
    };
    // ---- Views tab (Phase 38b–38c: Tree, bars, G22 leaves) ----
    let views = {
        let (tree_coll, tree_expanded, tree_selected, menu_open, nav_stack, img_cache, bar_note) = (
            tree_coll.clone(),
            tree_expanded.clone(),
            tree_selected.clone(),
            menu_open.clone(),
            nav_stack.clone(),
            img_cache.clone(),
            bar_note.clone(),
        );
        Rc::new(move |ctx: &Ctx| {
            // Demo image deposit (G22): 2×2 logo, one deposit —
            // backends serve from here (the cache stays app-owned).
            let logo = {
                let cache = img_cache.get();
                cache.insert_pixels(
                    "sink-logo",
                    2,
                    2,
                    vec![
                        0xCC, 0x22, 0x22, 0xFF, 0x22, 0xCC, 0x22, 0xFF, 0x22, 0x22, 0xCC, 0xFF,
                        0xFF, 0xFF, 0xFF, 0xFF,
                    ],
                );
                cache.load("sink-logo")
            };
            let say = |note: &'static str| {
                let bar_note = bar_note.clone();
                let text = SharedString::from(note);
                move || bar_note.set(text.clone())
            };
            Column::new().gap(12).children([
                // Tree (G12): collection hierarchy, chevrons,
                // arrows, Tree/TreeItem roles.
                ctx.child_auto(
                    &TreeProps {
                        nodes: tree_coll.clone(),
                        expanded: tree_expanded.clone(),
                        selected: tree_selected.clone(),
                        label: SharedString::from("Files"),
                        width: 280.0,
                        height: 140.0,
                        row_height: 28.0,
                        overscan: 2,
                        enabled: true,
                        debug: SharedString::from("sink-tree"),
                    },
                    Tree,
                ),
                // Toolbar (G21): one tab stop, arrows rove.
                ctx.child_auto(
                    &ToolbarProps::new(vec![
                        BarItem::new("Cut", say("cut")),
                        BarItem::new("Copy", say("copy")),
                        BarItem::separator(),
                        BarItem::new("Paste", say("paste")).disabled(),
                    ]),
                    Toolbar,
                ),
                // Menubar (G21): titles open standalone Menus.
                ctx.child_auto(
                    &MenubarProps::new(
                        vec![
                            MenuTitle::new(
                                "File",
                                vec![
                                    MenuItemProps::new("New", say("new file")),
                                    MenuItemProps::new("Open", say("open file")),
                                ],
                            ),
                            MenuTitle::new("Edit", vec![MenuItemProps::new("Undo", say("undo"))]),
                        ],
                        menu_open.clone(),
                    ),
                    Menubar,
                ),
                VNode::from(Text {
                    text: SharedString::from(format!("command: {}", bar_note.get())),
                    style: Text::body_secondary,
                }),
                // RichText display (G22): two spans, one size.
                ctx.child_auto(
                    &RichTextViewProps {
                        spans: vec![
                            TextSpan::new("Hello, "),
                            TextSpan::new("Oppa")
                                .weight(FontWeight::BOLD)
                                .ink(Color(0x22_66_CC)),
                        ],
                        style: Text::body_secondary,
                        label: Some(SharedString::from("Greeting")),
                        debug: SharedString::from("sink-rich"),
                    },
                    RichTextView,
                ),
                // Canvas surface (G22): rect + rounded rect ops.
                ctx.child_auto(
                    &CanvasViewProps {
                        ops: vec![
                            CanvasOp::Rect {
                                x: Px::of(4.0),
                                y: Px::of(4.0),
                                w: Px::of(48.0),
                                h: Px::of(24.0),
                                color: Color(0x22_66_CC),
                            },
                            CanvasOp::RRect {
                                x: Px::of(60.0),
                                y: Px::of(4.0),
                                w: Px::of(48.0),
                                h: Px::of(24.0),
                                radius: Px::of(6.0),
                                color: Color(0x88_88_88),
                            },
                        ],
                        width: 120.0,
                        height: 32.0,
                        label: None,
                        debug: SharedString::from("sink-plot"),
                    },
                    CanvasView,
                ),
                // Static image (G22): the deposit above, alt-named.
                ctx.child_auto(
                    &ImageViewProps {
                        image: logo,
                        size: 32.0,
                        radius: 4.0,
                        alt: SharedString::from("Demo logo"),
                    },
                    ImageView,
                ),
                // Declarative router (G22): stack-driven views with
                // push buttons (system back stays runner-owned).
                Row("sink::Views::NavRow").gap(8).children([
                    ctx.child_auto(
                        &ButtonProps {
                            debug: SharedString::from("sink::Views::GoHome"),
                            ..ButtonProps::new("Home", {
                                let nav_stack = nav_stack.clone();
                                move || {
                                    let mut stack = nav_stack.get();
                                    stack
                                        .push(oppa::Route::new("home").expect("route names parse"));
                                    nav_stack.set(stack);
                                }
                            })
                        },
                        Button,
                    ),
                    ctx.child_auto(
                        &ButtonProps {
                            debug: SharedString::from("sink::Views::GoSettings"),
                            ..ButtonProps::new("Settings", {
                                let nav_stack = nav_stack.clone();
                                move || {
                                    let mut stack = nav_stack.get();
                                    stack.push(
                                        oppa::Route::new("settings").expect("route names parse"),
                                    );
                                    nav_stack.set(stack);
                                }
                            })
                        },
                        Button,
                    ),
                ]),
                ctx.child_auto(
                    &NavHostProps {
                        stack: nav_stack.clone(),
                        routes: vec![
                            RouteView {
                                name: SharedString::from("home"),
                                content: Rc::new(sink_home),
                            },
                            RouteView {
                                name: SharedString::from("settings"),
                                content: Rc::new(sink_settings),
                            },
                        ],
                    },
                    NavHost,
                ),
            ])
        })
    };
    Column::new()
        .style(
            Style::new()
                .align_items(AlignItems::Center)
                .pad_x(24)
                .pad_y(20)
                .gap(16),
        )
        .children([
            VNode::from(Text::new("Oppa Kitchen Sink").size(22).bold()),
            VNode::from(Text {
                text: SharedString::from(format!(
                    "name: {} | volume: {} | dark: {} | size: {} | flavor: {}",
                    name.get(),
                    volume.get(),
                    dark.get(),
                    size.get().label(),
                    flavor.get().label(),
                )),
                style: Text::body_secondary,
            }),
            ctx.child_auto(
                &TabsProps {
                    tabs: vec![
                        TabItem {
                            value: SinkTab::Form,
                            label: SharedString::from(SinkTab::Form.label()),
                            content: form,
                        },
                        TabItem {
                            value: SinkTab::Layout,
                            label: SharedString::from(SinkTab::Layout.label()),
                            content: layout,
                        },
                        TabItem {
                            value: SinkTab::Overlays,
                            label: SharedString::from(SinkTab::Overlays.label()),
                            content: overlays,
                        },
                        TabItem {
                            value: SinkTab::Platform,
                            label: SharedString::from(SinkTab::Platform.label()),
                            content: platform,
                        },
                        TabItem {
                            value: SinkTab::Views,
                            label: SharedString::from(SinkTab::Views.label()),
                            content: views,
                        },
                    ],
                    active: tab,
                    enabled: true,
                },
                Tabs::<SinkTab>,
            ),
        ])
}
