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
    AlignItems, Color, Column, Ctx, Div, FetchState, FileDialog, FilePickerOptions, FlexWrap,
    JustifyContent, Row, ScriptedDialog, SharedString, Signal, Style, Text, ThemeMode, ThemeTokens,
    VNode,
};

use crate::{
    Action, Badge, BadgeProps, BadgeVariant, Button, ButtonProps, Checkbox, CheckboxProps, Modal,
    ModalProps, ProgressBar, ProgressBarProps, RadioGroup, RadioGroupProps, RadioOption, Select,
    SelectItem, SelectProps, Slider, SliderProps, TabItem, Tabs, TabsProps, TextArea,
    TextAreaProps, TextInput, TextInputProps, Toggle, ToggleProps,
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
}

impl SinkTab {
    fn label(&self) -> &'static str {
        match self {
            SinkTab::Form => "Form",
            SinkTab::Layout => "Layout",
            SinkTab::Overlays => "Overlays",
            SinkTab::Platform => "Platform",
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
        Rc::new(move |ctx: &Ctx| {
            // Theme contract round: the showcase's Dark toggle owns
            // the host palette (previously a dead switch — it showed
            // state but moved nothing). The toggle stays the bound
            // `dark` signal; the callback maps it onto the host
            // theme, and every themed surface re-derives in place.
            let theme_host = ctx.host().clone();
            Column::new().gap(12).children([
                ctx.child(
                    "sink::Form::Name",
                    1,
                    &TextInputProps::new("Name", name.clone()).placeholder("Type your name..."),
                    TextInput,
                ),
                ctx.child(
                    "sink::Form::Bio",
                    2,
                    &TextAreaProps::new("Bio", bio.clone())
                        .placeholder("Multi-line bio...")
                        .width(280.0),
                    TextArea,
                ),
                ctx.child(
                    "sink::Form::Volume",
                    3,
                    &SliderProps {
                        label: SharedString::from("Volume"),
                        value: volume.clone(),
                        min: 0.0,
                        max: 100.0,
                        step: 5.0,
                        enabled: true,
                        on_change: None,
                    },
                    Slider,
                ),
                ctx.child(
                    "sink::Form::Dark",
                    4,
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
                    },
                    Toggle,
                ),
                ctx.child(
                    "sink::Form::Accept",
                    5,
                    &CheckboxProps {
                        label: SharedString::from("Accept terms"),
                        checked: accept.clone(),
                        enabled: true,
                        on_change: None,
                    },
                    Checkbox,
                ),
                ctx.child(
                    "sink::Form::Flavor",
                    6,
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
                ctx.child(
                    "sink::Form::Size",
                    7,
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
                    },
                    RadioGroup::<Size>,
                ),
            ])
        })
    };
    let layout = Rc::new(|ctx: &Ctx| {
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
        ])
    });
    let overlays = {
        let (dialog_open, confirmed, download) =
            (dialog_open.clone(), confirmed.clone(), download.clone());
        Rc::new(move |ctx: &Ctx| {
            let open_button = dialog_open.clone();
            let dialog_state = dialog_open.clone();
            let dialog_confirmed = confirmed.clone();
            Column::new().gap(12).children([
                ctx.child(
                    "sink::Overlays::Open",
                    8,
                    &ButtonProps {
                        debug: SharedString::from("sink::Overlays::Open"),
                        ..ButtonProps::new("Open dialog", move || open_button.set(true))
                    },
                    Button,
                ),
                ctx.child(
                    "sink::Overlays::Dialog",
                    9,
                    &ModalProps::new("Confirm settings?", dialog_state)
                        .on_confirm(move || dialog_confirmed.set(true)),
                    Modal,
                ),
                // Live progress: the slider drives the meter
                // (determinate — value text announces the percent).
                ctx.child(
                    "sink::Overlays::Drive",
                    11,
                    &SliderProps {
                        label: SharedString::from("Download control"),
                        value: download.clone(),
                        min: 0.0,
                        max: 100.0,
                        step: 5.0,
                        enabled: true,
                        on_change: None,
                    },
                    Slider,
                ),
                ctx.child(
                    "sink::Overlays::Progress",
                    10,
                    &ProgressBarProps {
                        label: Some(SharedString::from("Download")),
                        ..ProgressBarProps::new(download.get() / 100.0)
                    },
                    ProgressBar,
                ),
                Row("sink::Overlays::Badges")
                    .style(Style::new().gap(8))
                    .children([
                        ctx.child(
                            "sink::Overlays::BadgePrimary",
                            12,
                            &BadgeProps::new("Primary").variant(BadgeVariant::Primary),
                            Badge,
                        ),
                        ctx.child(
                            "sink::Overlays::BadgeSuccess",
                            13,
                            &BadgeProps::new("Success").variant(BadgeVariant::Success),
                            Badge,
                        ),
                        ctx.child(
                            "sink::Overlays::BadgeDim",
                            14,
                            &BadgeProps::new("Dim").variant(BadgeVariant::Dim),
                            Badge,
                        ),
                    ]),
                VNode::from(Text {
                    text: SharedString::from(format!("confirmed: {}", confirmed.get())),
                    style: Text::body_secondary,
                }),
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
                ctx.child(
                    "sink::Platform::Pick",
                    15,
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
                ctx.child(
                    "sink::Platform::Count",
                    16,
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
                        ctx.child(
                            "sink::Platform::FetchOk",
                            17,
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
                        ctx.child(
                            "sink::Platform::FetchErr",
                            18,
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
            ctx.child(
                "sink::Tabs",
                19,
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
                    ],
                    active: tab,
                    enabled: true,
                },
                Tabs::<SinkTab>,
            ),
        ])
}
