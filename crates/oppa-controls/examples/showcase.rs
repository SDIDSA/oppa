//! Interactive controls showcase (decisions 242 + 244 + 245 + 247 +
//! 251): all eleven shipped controls in three tabs, live in a desktop
//! window.
//!
//! Run: `cargo run -p oppa-controls --example showcase`
//! (Windows/Linux desktops; needs a display server).
//!
//! Typing into the name field works through the runner's focused
//! session (decision 243). Tab contents are factory closures sharing
//! the Tabs instance namespace — child `(name, key)` pairs stay
//! unique across all tabs (documented on `TabItem`).

use std::rc::Rc;

use oppa::{AlignItems, Column, Ctx, Row, SharedString, Style, Text, VNode};
use oppa_app::{run_desktop, WindowOptions};
use oppa_controls::{
    Badge, BadgeProps, BadgeVariant, Button, ButtonProps, Checkbox, CheckboxProps, Modal,
    ModalProps, ProgressBar, ProgressBarProps, RadioGroup, RadioGroupProps, RadioOption, Select,
    SelectItem, SelectProps, Slider, SliderProps, TabItem, Tabs, TabsProps, TextInput,
    TextInputProps, Toggle, ToggleProps,
};

/// Showcase plan choice (decision 244): mutually exclusive options
/// over one signal — the `RadioGroup` proof target.
#[derive(Clone, PartialEq)]
enum Plan {
    Free,
    Pro,
}

impl Plan {
    fn label(&self) -> &'static str {
        match self {
            Plan::Free => "Free",
            Plan::Pro => "Pro",
        }
    }
}

/// Showcase theme choice (decision 247): the `Select` proof target —
/// a controlled value + open pair over one signal pair.
#[derive(Clone, PartialEq)]
enum Theme {
    Light,
    Dark,
    System,
}

impl Theme {
    fn label(&self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::System => "System",
        }
    }
}

/// Showcase pages (decision 245): the tab values. Content factories
/// close over root-owned signals, so every tab reads live state.
#[derive(Clone, PartialEq)]
enum Page {
    Profile,
    Preferences,
    Actions,
}

impl Page {
    fn label(&self) -> &'static str {
        match self {
            Page::Profile => "Profile",
            Page::Preferences => "Preferences",
            Page::Actions => "Actions",
        }
    }
}

/// Settings/profile app: header, three tabs, live footer. All state
/// is root-owned via `ctx.signal` (the linux-demo scene precedent);
/// every stateful control mounts through `ctx.child` (own flags per
/// instance — the M8/F6 rule).
fn showcase(ctx: &Ctx, _props: &()) -> VNode {
    let name = ctx.signal(SharedString::from(""));
    let dark = ctx.signal(false);
    let notes = ctx.signal(true);
    let accept = ctx.signal(false);
    let volume = ctx.signal(50.0f32);
    let plan = ctx.signal(Plan::Free);
    let open = ctx.signal(false);
    let confirmed = ctx.signal(false);
    let page = ctx.signal(Page::Profile);
    let theme = ctx.signal(Theme::System);
    let theme_open = ctx.signal(false);

    let profile = {
        let name = name.clone();
        let plan = plan.clone();
        Rc::new(move |ctx: &Ctx| {
            // Account status badge follows the plan (decision 251):
            // Pro reads PRO/Primary, Free reads ACTIVE/Success.
            let (badge_label, badge_variant) = match plan.get() {
                Plan::Pro => ("PRO", BadgeVariant::Primary),
                Plan::Free => ("ACTIVE", BadgeVariant::Success),
            };
            let account = format!("{} account", plan.get().label());
            Column::new().gap(16).children([
                Row("showcase::StatusRow")
                    .style(Style::new().gap(8).align_items(AlignItems::Center))
                    .children([
                        ctx.child(
                            "showcase::Badge",
                            11,
                            &BadgeProps::new(badge_label).variant(badge_variant),
                            Badge,
                        ),
                        VNode::from(Text {
                            text: SharedString::from(account),
                            style: Text::body_secondary,
                        }),
                    ]),
                ctx.child(
                    "showcase::Name",
                    1,
                    &TextInputProps::new("Name", name.clone()).placeholder("Enter your name..."),
                    TextInput,
                ),
                ctx.child(
                    "showcase::Plan",
                    2,
                    &RadioGroupProps {
                        options: vec![
                            RadioOption {
                                value: Plan::Free,
                                label: SharedString::from(Plan::Free.label()),
                            },
                            RadioOption {
                                value: Plan::Pro,
                                label: SharedString::from(Plan::Pro.label()),
                            },
                        ],
                        selected: plan.clone(),
                        enabled: true,
                    },
                    RadioGroup::<Plan>,
                ),
                // Storage quota meter (decision 251): fixed 68% fill
                // with its own descriptive text (the bar's caption +
                // value text already announce the percentage).
                ctx.child(
                    "showcase::Quota",
                    12,
                    &ProgressBarProps {
                        label: Some(SharedString::from("Storage Quota")),
                        ..ProgressBarProps::new(0.68)
                    },
                    ProgressBar,
                ),
                VNode::from(Text {
                    text: SharedString::from("6.8 GB of 10 GB used"),
                    style: Text::body_secondary,
                }),
            ])
        })
    };
    let preferences = {
        let (dark, notes, accept, volume, theme, theme_open) = (
            dark.clone(),
            notes.clone(),
            accept.clone(),
            volume.clone(),
            theme.clone(),
            theme_open.clone(),
        );
        Rc::new(move |ctx: &Ctx| {
            Column::new().gap(16).children([
                ctx.child(
                    "showcase::Dark",
                    3,
                    &ToggleProps {
                        label: SharedString::from("Dark Mode"),
                        on: dark.clone(),
                        enabled: true,
                        on_change: None,
                    },
                    Toggle,
                ),
                ctx.child(
                    "showcase::Theme",
                    10,
                    &SelectProps::new(
                        vec![
                            SelectItem {
                                value: Theme::Light,
                                label: SharedString::from(Theme::Light.label()),
                            },
                            SelectItem {
                                value: Theme::Dark,
                                label: SharedString::from(Theme::Dark.label()),
                            },
                            SelectItem {
                                value: Theme::System,
                                label: SharedString::from(Theme::System.label()),
                            },
                        ],
                        theme.clone(),
                        theme_open.clone(),
                    ),
                    Select::<Theme>,
                ),
                ctx.child(
                    "showcase::Notes",
                    4,
                    &ToggleProps {
                        label: SharedString::from("Notifications"),
                        on: notes.clone(),
                        enabled: true,
                        on_change: None,
                    },
                    Toggle,
                ),
                ctx.child(
                    "showcase::Accept",
                    5,
                    &CheckboxProps {
                        label: SharedString::from("Accept Terms"),
                        checked: accept.clone(),
                        enabled: true,
                        on_change: None,
                    },
                    Checkbox,
                ),
                ctx.child(
                    "showcase::Volume",
                    6,
                    &SliderProps {
                        label: SharedString::from("Volume"),
                        value: volume.clone(),
                        min: 0.0,
                        max: 100.0,
                        step: 10.0,
                        enabled: true,
                        on_change: None,
                    },
                    Slider,
                ),
            ])
        })
    };
    let actions = {
        let (open, confirmed) = (open.clone(), confirmed.clone());
        Rc::new(move |ctx: &Ctx| {
            let open_button = open.clone();
            let dialog_open = open.clone();
            let dialog_confirmed = confirmed.clone();
            Column::new().gap(16).children([
                ctx.child(
                    "showcase::Open",
                    7,
                    &ButtonProps::new("Open Dialog", move || open_button.set(true)),
                    Button,
                ),
                ctx.child(
                    "showcase::Dialog",
                    8,
                    &ModalProps::new("Confirm settings?", dialog_open)
                        .on_confirm(move || dialog_confirmed.set(true)),
                    Modal,
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
            VNode::from(Text::new("Oppa Controls Showcase").size(22).bold()),
            ctx.child(
                "showcase::Tabs",
                9,
                &TabsProps {
                    tabs: vec![
                        TabItem {
                            value: Page::Profile,
                            label: SharedString::from(Page::Profile.label()),
                            content: profile,
                        },
                        TabItem {
                            value: Page::Preferences,
                            label: SharedString::from(Page::Preferences.label()),
                            content: preferences,
                        },
                        TabItem {
                            value: Page::Actions,
                            label: SharedString::from(Page::Actions.label()),
                            content: actions,
                        },
                    ],
                    active: page,
                    enabled: true,
                },
                Tabs::<Page>,
            ),
            VNode::from(Text {
                text: SharedString::from(format!(
                    "Name: {} | Dark: {} | Notes: {} | Terms: {} | Volume: {} | Plan: {} | Theme: {} | Saved: {}",
                    name.get(),
                    dark.get(),
                    notes.get(),
                    accept.get(),
                    volume.get(),
                    plan.get().label(),
                    theme.get().label(),
                    confirmed.get(),
                )),
                style: Text::body_secondary,
            }),
        ])
}

fn main() {
    if let Err(e) = run_desktop(
        WindowOptions::new("Oppa Controls Showcase", 500, 600),
        (),
        showcase,
    ) {
        eprintln!("showcase: FATAL: {e}");
        std::process::exit(1);
    }
}
