//! Motion lab — the app, written once.
//!
//! A six-room showcase of the framework's animation/transition system:
//! entrance choreography, transform playground, springs,
//! micro-interactions, overlay motion, and ambient loops + chained
//! sequences. Only `oppa` + `oppa-controls` vocabulary (signals,
//! `Row`/`Column`/`Div`/`Text`, `Button`), no `cfg`, no
//! shell/renderer imports, so this file compiles unchanged into
//! every runner:
//!
//! - Desktop (Windows + Linux): `src/main.rs` mounts `motion_root`
//!   via `run_desktop_with` with the FPS counter armed (the
//!   frame-cost readout) — run with `cargo run`.
//! - Web: `src/lib.rs` exports `motion_root` via `export_app!` — run
//!   with `cargo oppa serve`.
//! - Android: mount `motion_root` through the shared runner (see the
//!   `login` app README for the recipe; needs a device).
//!
//! Round 2 lights up the transforms room (decision 511): looping
//! translate tracks on rect + text + vector leaves plus a scale
//! one-shot, all driven by the shared duration/easing params. The
//! other five stubs still name their missing primitives — nothing
//! here fakes motion the framework cannot do yet (no prefabricated
//! "animated page": primitives land in `crates/`, app design stays
//! here).

// Control components are `CamelCase` functions by framework convention
// (`#[component] fn Name` — M2 authoring surface).
#![allow(non_snake_case)]

use oppa::{
    AlignItems, Color, Column, Ctx, Div, Ease, KeyframeMode, KeyframeStop, Keyframes, Path, Props,
    Row, SharedString, Signal, Style, StyleBuilder, Text, Transform, Transition, VNode,
};

use oppa_controls::{Button, ButtonProps, Checkbox, Tooltip};

/// Room tabs in bar order: `(key, label)`.
pub const TABS: [(&str, &str); 6] = [
    ("entrance", "Entrance"),
    ("transforms", "Transforms"),
    ("springs", "Springs"),
    ("micro", "Micro"),
    ("overlays", "Overlays"),
    ("ambient", "Ambient"),
];

/// Easing names in cycle order (maps through [`ease_of`]).
pub const EASE_NAMES: [&str; 5] = ["Out", "In", "InOut", "Linear", "Spring"];

/// Duration stepper step (ms).
pub const DUR_STEP_MS: f32 = 50.0;
/// Slowest / fastest tweakable durations (ms).
pub const DUR_MIN_MS: f32 = 0.0;
pub const DUR_MAX_MS: f32 = 2000.0;
/// Default duration: the standard control-tween feel (T3 target).
pub const DUR_DEFAULT_MS: f32 = 150.0;

/// Cycle index → framework easing (total — every index names an ease).
pub fn ease_of(idx: usize) -> Ease {
    match idx % EASE_NAMES.len() {
        0 => Ease::Out,
        1 => Ease::In,
        2 => Ease::InOut,
        3 => Ease::Linear,
        _ => Ease::Spring,
    }
}

/// Author-owned lab state (controls stay controlled — signals in,
/// nothing retained).
#[derive(Clone)]
pub struct MotionState {
    pub tab: Signal<usize>,
    pub duration_ms: Signal<f32>,
    pub ease_idx: Signal<usize>,
    /// Loop play switch (transforms room).
    pub spin: Signal<bool>,
    /// Scale one-shot switch (transforms room).
    pub zoom: Signal<bool>,
    /// Rotate one-shot switch (transforms room, decision 517).
    pub twist: Signal<bool>,
}

impl Props for MotionState {}

/// What each static stub is waiting for (track order: T1 → T2 → T3).
pub fn room_need(key: &str) -> &'static str {
    match key {
        "entrance" => {
            "Staggered card entrances — live since decision 512 (fade + rise, per-card delay)."
        }
        "transforms" => {
            "Translate/scale/rotate loops + one-shots — live since decisions 511 + 517."
        }
        "springs" => "Spring overshoot + settle — live since decision 513 (canonical underdamped curve).",
        "micro" => "Press/hover tweens — live since decision 514 (Button, Checkbox, menu rows, tooltip fade).",
        "overlays" => "Modal/Toast/Menu entrances — live since decision 514 (fade + rise; exits snap by contract).",
        "ambient" => "Loop ambient + chained sequence — live since decisions 512–513 (ping-pong loop, completion-driven chain).",
        _ => "Unknown room — refused by omission, never a silent stub.",
    }
}

pub fn room_title(key: &str) -> &'static str {
    match key {
        "entrance" => "Entrance choreography",
        "transforms" => "Transform playground",
        "springs" => "Springs",
        "micro" => "Micro-interactions",
        "overlays" => "Overlay motion",
        "ambient" => "Ambient + completion",
        _ => "Unknown room",
    }
}

/// One static room stub: title + the primitive it waits for.
fn room_stub(key: &str) -> VNode {
    Div(format!("motion-room-{key}").as_str())
        .style(
            Style::new()
                .fill_width()
                .fill_height()
                .pad_x(24)
                .pad_y(24)
                .gap(8),
        )
        .children([
            VNode::from(Text::new(room_title(key)).size(22).bold().build()),
            VNode::from(Text {
                text: SharedString::from(room_need(key)),
                style: Text::body_secondary,
            }),
        ])
}

/// Conditional keyframes arm (a missing track keeps the tween —
/// the play/stop shape for loops, the mount shape for ambient).
fn with_keyframes(base: StyleBuilder, kf: &Option<Keyframes>) -> StyleBuilder {
    match kf {
        Some(k) => base.keyframes(k.clone()),
        None => base,
    }
}

/// Live transforms room (decisions 511 + 517): looping translate
/// tracks on a rect leaf plus text + vector leaves (wrappers carry
/// the shared track — proving accumulation), a rotation loop on a
/// third rect, and scale + rotate one-shots. All durations/easings
/// ride the shared params; committed boxes never move (paint-only —
/// the headless suite pins it).
fn TransformsRoom(ctx: &Ctx, state: &MotionState) -> VNode {
    let dur = state.duration_ms.get().max(0.0) as u32;
    let ease = ease_of(state.ease_idx.get() % EASE_NAMES.len());
    let spinning = state.spin.get();
    let leg = dur.max(1);
    // Loops oscillate ±40px (two stops + the closing leg); stopping
    // drops the keyframes so the same delta closes as a tween.
    let loop_kf = || {
        Keyframes::new(vec![
            KeyframeStop::new(leg, ease).transform(Transform::translate(-40, 0)),
            KeyframeStop::new(leg, ease).transform(Transform::translate(40, 0)),
        ])
        .mode(KeyframeMode::Loop)
    };
    let loop_target = if spinning {
        Transform::translate(40, 0)
    } else {
        Transform::identity()
    };
    let loop_kfs = if spinning { Some(loop_kf()) } else { None };
    let loop_style = || {
        with_keyframes(
            Style::new()
                .transform(loop_target)
                .transition(Transition::new(dur, ease)),
            &loop_kfs,
        )
    };
    let t = ctx.theme().tokens();
    let spin_sig = state.spin.clone();
    let play = ButtonProps::new(
        if spinning { "Stop loops" } else { "Play loops" },
        move || {
            spin_sig.set(!spin_sig.get());
        },
    )
    .size(120.0, 40.0)
    .debug("motion-xf-play");
    let zoom_sig = state.zoom.clone();
    let zoomed = state.zoom.get();
    let zoom_btn = ButtonProps::new(if zoomed { "Zoom 1×" } else { "Zoom 1.6×" }, move || {
        zoom_sig.set(!zoom_sig.get());
    })
    .size(120.0, 40.0)
    .debug("motion-xf-zoom");
    let zoom_xf = if zoomed {
        Transform::scale(1.6, 1.6)
    } else {
        Transform::identity()
    };
    // Rotation loop (decision 517): oscillates ±45° while the loops
    // play (same play switch, same legs — rotation is a first-class
    // track now, raw-lerped with no shortest-arc).
    let rot_kf = || {
        Keyframes::new(vec![
            KeyframeStop::new(leg, ease).transform(Transform::rotate(-45)),
            KeyframeStop::new(leg, ease).transform(Transform::rotate(45)),
        ])
        .mode(KeyframeMode::Loop)
    };
    let rot_target = if spinning {
        Transform::rotate(45)
    } else {
        Transform::identity()
    };
    let rot_kfs = if spinning { Some(rot_kf()) } else { None };
    let twist_sig = state.twist.clone();
    let twisted = state.twist.get();
    let twist_btn = ButtonProps::new(
        if twisted { "Twist 0°" } else { "Twist 90°" },
        move || {
            twist_sig.set(!twist_sig.get());
        },
    )
    .size(120.0, 40.0)
    .debug("motion-xf-twist");
    let twist_xf = if twisted {
        Transform::rotate(90)
    } else {
        Transform::identity()
    };
    Div("motion-room-transforms")
        // Tab-unique VNode key (instance keys never reach the
        // reconciler's child matching — without this the room output
        // position-diffs into the previous tab's leaves and spawns
        // cross-room delta tracks).
        .key(1)
        .style(
            Style::new()
                .fill_width()
                .fill_height()
                .pad_x(24)
                .pad_y(24)
                .gap(12),
        )
        .children([
            VNode::from(Text::new(room_title("transforms")).size(22).bold().build()),
            Row("motion-xf-row")
                .style(Style::new().gap(24).align_items(AlignItems::Center))
                .children([
                    Div("motion-xf-loop")
                        .style(with_keyframes(
                            Style::new()
                                .square(60)
                                .bg(t.primary)
                                .radius(12)
                                .transform(loop_target)
                                .transition(Transition::new(dur, ease)),
                            &loop_kfs,
                        ))
                        .build(),
                    Div("motion-xf-text")
                        .style(loop_style())
                        .child(VNode::from(Text::new("Motion").size(28).bold().build())),
                    Div("motion-xf-vector").style(loop_style()).child(
                        Path::new("motion-tri")
                            .data("M 0 0 L 60 0 L 30 52 Z")
                            .fill(t.primary)
                            .size(60, 52)
                            .build(),
                    ),
                    Div("motion-xf-zoom-rect")
                        .style(
                            Style::new()
                                .square(60)
                                .bg(t.primary_pressed)
                                .radius(12)
                                .transform(zoom_xf)
                                .transition(Transition::new(dur, ease)),
                        )
                        .build(),
                    Div("motion-xf-spin-rect")
                        .style(with_keyframes(
                            Style::new()
                                .square(60)
                                .bg(t.primary)
                                .radius(12)
                                .transform(rot_target)
                                .transition(Transition::new(dur, ease)),
                            &rot_kfs,
                        ))
                        .build(),
                    Div("motion-xf-twist-rect")
                        .style(
                            Style::new()
                                .square(60)
                                .bg(t.primary_pressed)
                                .radius(12)
                                .transform(twist_xf)
                                .transition(Transition::new(dur, ease)),
                        )
                        .build(),
                ]),
            Row("motion-xf-controls")
                .style(Style::new().gap(8))
                .children([
                    ctx.child_auto(&play, Button),
                    ctx.child_auto(&zoom_btn, Button),
                    ctx.child_auto(&twist_btn, Button),
                ]),
        ])
}

/// Portable motion lab over [`MotionState`].
pub fn MotionApp(ctx: &Ctx, state: &MotionState) -> VNode {
    let tab = state.tab.get();
    let dur = state.duration_ms.get();
    let ease = state.ease_idx.get() % EASE_NAMES.len();
    // Single source of truth: the readout names the framework ease
    // value the index maps to (T2 wires this into real tweens).
    let ease_dbg = format!("{:?}", ease_of(ease));

    let t = ctx.theme().tokens();
    let (key, _) = TABS[tab % TABS.len()];

    let mut tabs: Vec<VNode> = Vec::with_capacity(TABS.len());
    for (i, (tkey, label)) in TABS.iter().enumerate() {
        let selected = i == tab % TABS.len();
        let tab_sig = state.tab.clone();
        let fill = if selected { t.primary } else { t.surface };
        let ink = if selected {
            Color(0xFF_FF_FF)
        } else {
            t.text_primary
        };
        tabs.push(
            ctx.child_keyed(
                i as u64,
                &ButtonProps::new(label, move || tab_sig.set(i))
                    .size(120.0, 40.0)
                    .debug(format!("motion-tab-{tkey}").as_str())
                    .fill(fill)
                    .ink(ink),
                Button,
            ),
        );
    }

    let dur_sig = state.duration_ms.clone();
    let dur_minus = ButtonProps::new("−50ms", move || {
        dur_sig.set((dur_sig.get() - DUR_STEP_MS).max(DUR_MIN_MS));
    })
    .size(100.0, 40.0)
    .debug("motion-dur-minus");
    let dur_sig = state.duration_ms.clone();
    let dur_plus = ButtonProps::new("+50ms", move || {
        dur_sig.set((dur_sig.get() + DUR_STEP_MS).min(DUR_MAX_MS));
    })
    .size(100.0, 40.0)
    .debug("motion-dur-plus");
    let ease_sig = state.ease_idx.clone();
    let ease_btn = ButtonProps::new(EASE_NAMES[ease], move || {
        ease_sig.set(ease_sig.get() + 1);
    })
    .size(100.0, 40.0)
    .debug("motion-ease-cycle");

    let params = Row("motion-param-row")
        .style(Style::new().gap(8).align_items(AlignItems::Center))
        .children([
            ctx.child_auto(&dur_minus, Button),
            ctx.child_auto(&dur_plus, Button),
            ctx.child_auto(&ease_btn, Button),
            VNode::from(Text {
                text: SharedString::from(format!("{dur:.0}ms · {ease_dbg}")),
                style: Text::body_secondary,
            }),
        ]);

    Column::new()
        .style(
            Style::new()
                .bg(t.background)
                .fill_width()
                .fill_height()
                .pad_x(16)
                .pad_y(16)
                .gap(12),
        )
        .children([
            Div("motion-tabbar")
                .style(Style::new().gap(8))
                .children(tabs),
            Div("motion-params").child(params),
            Div("motion-readout").child(VNode::from(Text {
                text: SharedString::from(format!(
                    "Room {} of {} · frame cost on the desktop FPS badge",
                    tab % TABS.len() + 1,
                    TABS.len()
                )),
                style: Text::body_secondary,
            })),
            room_for(ctx, state, tab % TABS.len(), key),
        ])
}

/// Room dispatch: live rooms mount keyed by tab (different keys
/// force unmount+mount — never positional recycling, which would
/// diff one room's leaves into another's and spawn cross-room delta
/// tracks); the rest stay honest stubs.
fn room_for(ctx: &Ctx, state: &MotionState, tab: usize, key: &str) -> VNode {
    match key {
        "entrance" => ctx.child_keyed(
            tab as u64,
            &EntranceRoomProps {
                duration_ms: state.duration_ms.clone(),
                ease_idx: state.ease_idx.clone(),
            },
            EntranceRoom,
        ),
        "transforms" => ctx.child_keyed(tab as u64, state, TransformsRoom),
        "springs" => ctx.child_keyed(tab as u64, state, SpringsRoom),
        "micro" => ctx.child_keyed(tab as u64, &(), MicroRoom),
        "overlays" => ctx.child_keyed(tab as u64, &(), OverlayRoom),
        "ambient" => ctx.child_keyed(tab as u64, &(), AmbientRoom),
        _ => room_stub(key),
    }
}

/// Stagger step between entrance cards (ms).
pub const ENTER_STAGGER_MS: u32 = 70;
/// Entrance rise distance (px).
pub const ENTER_RISE_PX: f32 = 18.0;

/// Entrance card props: per-card delay index + shared timing.
#[derive(Clone)]
pub struct EntranceCardProps {
    pub index: usize,
    pub dur_ms: u32,
    pub ease: Ease,
}

impl Props for EntranceCardProps {}

/// One entrance card (decision 512): mounts hidden (opacity 0,
/// risen), flips visible through a single armed timeout, and tweens
/// home with its index stagger. Fresh mounts replay automatically
/// (boot + every tab switch); the room's Replay key remounts
/// without touching tab state.
fn EntranceCard(ctx: &Ctx, props: &EntranceCardProps) -> VNode {
    let shown = ctx.signal(false);
    if !shown.get() {
        // Flip through one timeout per render (the `use_timeout`
        // contract: timers are per-render, so re-renders cancel and
        // re-arm — harmless here because the shared flip delay is
        // zero (any tick fires it); no signal writes here, so
        // nothing self-cancels.
        // (oppa_controls::motion::ENTER_FLIP_MS duplicated as a
        // literal — apps own their choreography numbers.)
        let shown_sig = shown.clone();
        ctx.use_timeout(0.0, move || shown_sig.set(true));
    }
    let t = ctx.theme().tokens();
    let live = shown.get();
    Div(format!("motion-enter-{}", props.index).as_str())
        .style(
            Style::new()
                .size(140, 90)
                .bg(if props.index.is_multiple_of(2) {
                    t.surface
                } else {
                    t.primary
                })
                .radius(12)
                .opacity(Some(if live { 1.0 } else { 0.0 }))
                .transform(if live {
                    Transform::identity()
                } else {
                    Transform::translate(0, ENTER_RISE_PX)
                })
                .transition(
                    Transition::new(props.dur_ms, props.ease)
                        .delayed(props.index as u32 * ENTER_STAGGER_MS),
                ),
        )
        .child(VNode::from(
            Text::new(format!("Card {}", props.index + 1).as_str())
                .size(16)
                .bold()
                .build(),
        ))
}

/// Live entrance room (decision 512): staggered card entrances
/// (fade + rise) on boot and on every tab switch, all durations and
/// easings riding the shared params. Committed boxes never move
/// (paint-only — the headless suite pins it).
#[derive(Clone)]
pub struct EntranceRoomProps {
    pub duration_ms: Signal<f32>,
    pub ease_idx: Signal<usize>,
}

impl Props for EntranceRoomProps {}

fn EntranceRoom(ctx: &Ctx, props: &EntranceRoomProps) -> VNode {
    let gen = ctx.signal(0u64);
    let gen_sig = gen.clone();
    let replay = ButtonProps::new("Replay entrance", move || {
        gen_sig.set(gen_sig.get() + 1);
    })
    .size(150.0, 40.0)
    .debug("motion-enter-replay");
    let g = gen.get();
    let dur = props.duration_ms.get().max(0.0) as u32;
    let ease = ease_of(props.ease_idx.get() % EASE_NAMES.len());
    let mut cards: Vec<VNode> = Vec::with_capacity(4);
    for i in 0..4usize {
        cards.push(ctx.child_keyed(
            i as u64 * 1000 + g,
            &EntranceCardProps {
                index: i,
                dur_ms: dur,
                ease,
            },
            EntranceCard,
        ));
    }
    Div("motion-room-entrance")
        // Tab-unique VNode key (see the transforms room).
        .key(0)
        .style(
            Style::new()
                .fill_width()
                .fill_height()
                .pad_x(24)
                .pad_y(24)
                .gap(12),
        )
        .children([
            VNode::from(Text::new(room_title("entrance")).size(22).bold().build()),
            Row("motion-enter-row")
                .style(Style::new().gap(16))
                .children(cards),
            ctx.child_auto(&replay, Button),
        ])
}

/// Live springs room (decision 513): spring one-shots — translate
/// overshoot, scale overshoot, and a clamped opacity spring — all on
/// the canonical underdamped curve with the shared duration. Boxes
/// never move (paint-only — the headless suite pins it).
fn SpringsRoom(ctx: &Ctx, state: &MotionState) -> VNode {
    let dur = state.duration_ms.get().max(0.0) as u32;
    let sprung = ctx.signal(false);
    let sprung_sig = sprung.clone();
    let fire = ButtonProps::new("Fire springs", move || sprung_sig.set(!sprung_sig.get()))
        .size(120.0, 40.0)
        .debug("motion-spring-fire");
    let on = sprung.get();
    let t = ctx.theme().tokens();
    let target = |dx: f32| {
        if on {
            Transform::translate(dx, 0.0)
        } else {
            Transform::identity()
        }
    };
    Div("motion-room-springs")
        .key(2)
        .style(
            Style::new()
                .fill_width()
                .fill_height()
                .pad_x(24)
                .pad_y(24)
                .gap(12),
        )
        .children([
            VNode::from(Text::new(room_title("springs")).size(22).bold().build()),
            Row("motion-spring-row")
                .style(Style::new().gap(24).align_items(AlignItems::Center))
                .children([
                    Div("motion-spring-move")
                        .style(
                            Style::new()
                                .square(60)
                                .bg(t.primary)
                                .radius(12)
                                .transform(target(120.0))
                                .transition(Transition::new(dur, Ease::Spring)),
                        )
                        .build(),
                    Div("motion-spring-grow")
                        .style(
                            Style::new()
                                .square(60)
                                .bg(t.primary_pressed)
                                .radius(12)
                                .transform(if on {
                                    Transform::scale(1.6, 1.6)
                                } else {
                                    Transform::identity()
                                })
                                .transition(Transition::new(dur, Ease::Spring)),
                        )
                        .build(),
                    Div("motion-spring-fade")
                        .style(
                            Style::new()
                                .square(60)
                                .bg(t.primary)
                                .radius(12)
                                .opacity(Some(if on { 0.4 } else { 1.0 }))
                                .transition(Transition::new(dur, Ease::Spring)),
                        )
                        .build(),
                ]),
            ctx.child_auto(&fire, Button),
        ])
}

/// Live ambient + completion room (decisions 512–513): a ping-pong
/// ambient loop (opacity + drift, runs while mounted) plus a
/// three-step chained sequence driven by completion observers (each
/// link starts from the previous link's settle — the `observe_settled`
/// contract end to end from app code).
fn AmbientRoom(ctx: &Ctx, _props: &()) -> VNode {
    let live = ctx.signal(false);
    if !live.get() {
        let live_sig = live.clone();
        ctx.use_timeout(0.0, move || live_sig.set(true));
    }
    let on = live.get();
    let t = ctx.theme().tokens();
    // Ambient loop: ping-pong drift + breathe, closing on the live
    // target so it runs until unmounted.
    let amb_kf = || {
        Keyframes::new(vec![
            KeyframeStop::new(400, Ease::InOut)
                .opacity(Some(0.35))
                .transform(Transform::translate(-24, 0)),
            KeyframeStop::new(400, Ease::InOut)
                .opacity(Some(1.0))
                .transform(Transform::translate(24, 0)),
        ])
        .mode(KeyframeMode::PingPong)
    };
    let amb_on = on;
    // Chained sequence state + completion driver (app-side use of
    // `observe_settled` — node ids resolve by debug label, loud when
    // renamed).
    let chain = ctx.signal(0usize);
    let watching = ctx.signal(false);
    let host = ctx.host();
    // Link 1 observes from render (never from the press closure — a
    // press-time observe can fire before the link's own commit
    // exists; render-time observes always postdate it). Later links
    // nest in the completions (the commit lands later the same pump
    // there — proven in the round-3 host tests).
    if chain.get() >= 1 && !watching.get() {
        watching.set(true);
        let chain_sig = chain.clone();
        let host2 = host.clone();
        let n1 = expect_node(&host, "motion-amb-step1");
        host.observe_settled(n1, move || {
            chain_sig.set(2);
            let n2 = expect_node(&host2, "motion-amb-step2");
            let c2 = chain_sig.clone();
            host2.observe_settled(n2, move || {
                c2.set(3);
            });
        });
    }
    let play = {
        let chain_sig = chain.clone();
        ButtonProps::new("Play chain", move || {
            chain_sig.set(1);
        })
        .size(120.0, 40.0)
        .debug("motion-amb-play")
    };
    let reset = {
        let chain_sig = chain.clone();
        let watching_sig = watching.clone();
        ButtonProps::new("Reset chain", move || {
            chain_sig.set(0);
            watching_sig.set(false);
        })
        .size(120.0, 40.0)
        .debug("motion-amb-reset")
    };
    let step = chain.get();
    let step_style = |i: usize| {
        let lit = step >= i && step > 0;
        Style::new()
            .square(44)
            .bg(if lit { t.primary } else { t.surface })
            .radius(22)
            .opacity(Some(if step == 0 { 0.35 } else { 1.0 }))
            .transform(if step == 0 {
                Transform::translate(0, 10)
            } else {
                Transform::identity()
            })
            .transition(Transition::new(150, Ease::Out))
    };
    Div("motion-room-ambient")
        .key(5)
        .style(
            Style::new()
                .fill_width()
                .fill_height()
                .pad_x(24)
                .pad_y(24)
                .gap(12),
        )
        .children([
            VNode::from(Text::new(room_title("ambient")).size(22).bold().build()),
            Row("motion-amb-row")
                .style(Style::new().gap(24).align_items(AlignItems::Center))
                .children([
                    Div("motion-amb-orb")
                        .style(with_keyframes(
                            Style::new()
                                .square(60)
                                .bg(t.primary)
                                .circle()
                                .opacity(Some(if amb_on { 1.0 } else { 0.35 }))
                                .transform(if amb_on {
                                    Transform::translate(24, 0)
                                } else {
                                    Transform::translate(-24, 0)
                                })
                                .transition(Transition::new(400, Ease::InOut)),
                            &if amb_on { Some(amb_kf()) } else { None },
                        ))
                        .build(),
                    Div("motion-amb-step1").style(step_style(1)).build(),
                    Div("motion-amb-step2").style(step_style(2)).build(),
                    Div("motion-amb-step3").style(step_style(3)).build(),
                ]),
            Row("motion-amb-controls")
                .style(Style::new().gap(8))
                .children([
                    ctx.child_auto(&play, Button),
                    ctx.child_auto(&reset, Button),
                ]),
        ])
}

/// Retained node by debug label (loud on miss — a renamed label is
/// an app wiring bug, the testkit rule applied to app code).
fn expect_node(host: &oppa::ComponentHost, debug: &str) -> oppa::NodeId {
    oppa::find_retained_by_debug(host, debug)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("motion: no retained node {debug:?}"))
}

/// Self-owned root for real runners (`()` props): desktop
/// `run_desktop_with`, web `export_app!`, Android `mount_app` all
/// mount exactly this.
pub fn motion_root(ctx: &Ctx, _props: &()) -> VNode {
    let state = MotionState {
        tab: ctx.signal(0usize),
        duration_ms: ctx.signal(DUR_DEFAULT_MS),
        ease_idx: ctx.signal(0usize),
        spin: ctx.signal(false),
        zoom: ctx.signal(false),
        twist: ctx.signal(false),
    };
    ctx.child_auto(&state, MotionApp)
}

/// Anchor leaf for the micro tooltip (decision 514).
fn tip_anchor(_ctx: &Ctx, _: &()) -> VNode {
    Div("motion-micro-tip-anchor")
        .style(Style::new().size(140, 32).bg(Color(0x22_66_CC)).radius(6))
        .build()
}

/// Live micro-interactions room (decision 514): press-and-hold on a
/// Button, a Checkbox toggle, a standalone Menu (open, hover a row,
/// tap it), and an instant Tooltip on hover — every swap tweening
/// through the shared micro policy. All state room-local.
fn MicroRoom(ctx: &Ctx, _props: &()) -> VNode {
    let tapped = ctx.signal(false);
    let tapped_sig = tapped.clone();
    let press_btn = ButtonProps::new("Hold me", move || tapped_sig.set(true))
        .size(120.0, 40.0)
        .debug("motion-micro-press");
    let checked = ctx.signal(false);
    let check_props = oppa_controls::CheckboxProps {
        label: SharedString::from("Check me"),
        checked: checked.clone(),
        enabled: true,
        on_change: None,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
        box_px: 20.0,
        box_radius: 4.0,
        label_style: Text::body_secondary,
        label_ink: None,
        check_ink: None,
        check_scale: 1.0,
    };
    let menu_open = ctx.signal(false);
    let menu_open_sig = menu_open.clone();
    let open_btn = ButtonProps::new("Open menu", move || menu_open_sig.set(true))
        .size(120.0, 40.0)
        .debug("motion-micro-menu-open");
    let picked = ctx.signal(SharedString::from(""));
    let picked_sig = picked.clone();
    let menu_props = oppa_controls::MenuProps {
        items: vec![
            oppa_controls::MenuItemProps::new("Alpha", {
                let picked_sig = picked_sig.clone();
                move || picked_sig.set(SharedString::from("Alpha"))
            }),
            oppa_controls::MenuItemProps::new("Beta", {
                let picked_sig = picked_sig.clone();
                move || picked_sig.set(SharedString::from("Beta"))
            }),
        ],
        open: menu_open.clone(),
        anchor: (24.0, 220.0),
        width: 160.0,
        highlight: None,
        anchor_focus: None,
    };
    let tip_props = oppa_controls::TooltipProps::new("Micro tip", tip_anchor, ())
        .instant()
        .debug("motion-micro-tip");
    Div("motion-room-micro")
        .key(3)
        .style(
            Style::new()
                .fill_width()
                .fill_height()
                .pad_x(24)
                .pad_y(24)
                .gap(12),
        )
        .children([
            VNode::from(Text::new(room_title("micro")).size(22).bold().build()),
            Row("motion-micro-row")
                .style(Style::new().gap(16).align_items(AlignItems::Center))
                .children([
                    ctx.child_auto(&press_btn, Button),
                    ctx.child_auto(&check_props, Checkbox),
                    ctx.child_auto(&open_btn, Button),
                    ctx.child_auto(&tip_props, Tooltip),
                ]),
            ctx.child_auto(&menu_props, oppa_controls::Menu),
            VNode::from(Text {
                text: SharedString::from(format!(
                    "tapped={} checked={} picked={}",
                    tapped.get(),
                    checked.get(),
                    picked.get()
                )),
                style: Text::body_secondary,
            }),
        ])
}

/// Live overlay room (decision 514): Modal, Toast, and Menu
/// entrances (fade + rise through the shared enter policy) with
/// dismiss paths. Exits snap by specified contract (appear-first).
fn OverlayRoom(ctx: &Ctx, _props: &()) -> VNode {
    let modal_open = ctx.signal(false);
    let toast_open = ctx.signal(false);
    let menu_open = ctx.signal(false);
    let modal_sig = modal_open.clone();
    let toast_sig = toast_open.clone();
    let menu_sig = menu_open.clone();
    let picked = ctx.signal(SharedString::from(""));
    let picked_sig = picked.clone();
    Div("motion-room-overlays")
        .key(4)
        .style(
            Style::new()
                .fill_width()
                .fill_height()
                .pad_x(24)
                .pad_y(24)
                .gap(12),
        )
        .children([
            VNode::from(Text::new(room_title("overlays")).size(22).bold().build()),
            Row("motion-overlays-row")
                .style(Style::new().gap(16))
                .children([
                    ctx.child_auto(
                        &ButtonProps::new("Open modal", move || modal_sig.set(true))
                            .size(120.0, 40.0)
                            .debug("motion-ov-modal-open"),
                        Button,
                    ),
                    ctx.child_auto(
                        &ButtonProps::new("Show toast", move || toast_sig.set(true))
                            .size(120.0, 40.0)
                            .debug("motion-ov-toast-open"),
                        Button,
                    ),
                    ctx.child_auto(
                        &ButtonProps::new("Open menu", move || menu_sig.set(true))
                            .size(120.0, 40.0)
                            .debug("motion-ov-menu-open"),
                        Button,
                    ),
                ]),
            ctx.child_auto(
                &oppa_controls::ModalProps::new("Confirm?", modal_open.clone()),
                oppa_controls::Modal,
            ),
            ctx.child_auto(
                &oppa_controls::ToastProps::new("Saved", toast_open.clone()),
                oppa_controls::Toast,
            ),
            ctx.child_auto(
                &oppa_controls::MenuProps {
                    items: vec![oppa_controls::MenuItemProps::new("Go", {
                        let picked_sig = picked_sig.clone();
                        move || picked_sig.set(SharedString::from("Go"))
                    })],
                    open: menu_open.clone(),
                    anchor: (24.0, 220.0),
                    width: 160.0,
                    highlight: None,
                    anchor_focus: None,
                },
                oppa_controls::Menu,
            ),
            VNode::from(Text {
                text: SharedString::from(format!("picked={}", picked.get())),
                style: Text::body_secondary,
            }),
        ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_of_covers_every_index() {
        assert_eq!(
            [ease_of(0), ease_of(1), ease_of(2), ease_of(3), ease_of(4)],
            [Ease::Out, Ease::In, Ease::InOut, Ease::Linear, Ease::Spring]
        );
        assert_eq!(ease_of(5), Ease::Out, "cycles, never panics");
        for (i, name) in EASE_NAMES.iter().enumerate() {
            assert_eq!(&format!("{:?}", ease_of(i)), name, "readout names agree");
        }
    }

    #[test]
    fn room_tables_cover_every_tab() {
        for (key, _) in TABS {
            assert!(!room_title(key).is_empty());
            assert!(!room_need(key).starts_with("Unknown"));
        }
        assert_eq!(
            room_need("nope"),
            "Unknown room — refused by omission, never a silent stub."
        );
    }
}
