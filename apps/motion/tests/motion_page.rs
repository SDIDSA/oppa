//! Headless proof of the motion lab (`MotionApp`): the same tree
//! `run_desktop_with`, `export_app!`, and the Android runner mount —
//! no window, no platform code. Round 2 scope: tab switching, param
//! steppers, geometry sanity, the live transforms room (loop play
//! keeps frame demand flowing; boxes never move — paint-only), and
//! quiescence of the idle tree.

use motion::{MotionApp, MotionState, DUR_DEFAULT_MS, DUR_STEP_MS, EASE_NAMES, TABS};
use oppa::SharedString;
use oppa_testkit::Harness;

fn mount_motion() -> (Harness, MotionState) {
    let app = Harness::new();
    let state = MotionState {
        tab: app.host().runtime().signal(0usize),
        duration_ms: app.host().runtime().signal(DUR_DEFAULT_MS),
        ease_idx: app.host().runtime().signal(0usize),
        spin: app.host().runtime().signal(false),
        zoom: app.host().runtime().signal(false),
        twist: app.host().runtime().signal(false),
    };
    app.mount("Motion", state.clone(), MotionApp);
    (app, state)
}

#[test]
fn motion_boots_on_entrance_tab() {
    let (app, _) = mount_motion();
    app.node("motion-room-entrance");
    for (key, _) in TABS.iter().skip(1) {
        assert!(
            oppa::find_retained_by_debug(app.host(), &format!("motion-room-{key}")).is_empty(),
            "only the selected room mounts, got {key}"
        );
    }
}

#[test]
fn motion_tab_switch_shows_each_room() {
    let (app, state) = mount_motion();
    for (i, (key, _)) in TABS.iter().enumerate() {
        app.tap(&format!("motion-tab-{key}"));
        assert_eq!(state.tab.get(), i, "tab tap selects room {key}");
        app.node(&format!("motion-room-{key}"));
    }
}

#[test]
fn motion_duration_steppers_clamp_at_ends() {
    let (app, state) = mount_motion();
    app.tap("motion-dur-plus");
    app.tap("motion-dur-plus");
    assert_eq!(
        state.duration_ms.get(),
        DUR_DEFAULT_MS + 2.0 * DUR_STEP_MS,
        "steppers add exact steps"
    );
    for _ in 0..60 {
        app.tap("motion-dur-minus");
    }
    assert_eq!(
        state.duration_ms.get(),
        0.0,
        "bottom clamps, never negative"
    );
    for _ in 0..60 {
        app.tap("motion-dur-plus");
    }
    assert_eq!(state.duration_ms.get(), 2000.0, "top clamps at max");
}

#[test]
fn motion_ease_cycles_through_all_names() {
    let (app, state) = mount_motion();
    for i in 1..=EASE_NAMES.len() {
        app.tap("motion-ease-cycle");
        assert_eq!(state.ease_idx.get(), i, "ease index advances");
    }
    assert_eq!(
        state.ease_idx.get() % EASE_NAMES.len(),
        0,
        "full cycle returns to the first ease"
    );
}

#[test]
fn motion_idle_tree_holds_no_live_tracks() {
    let (app, _) = Harness::with_clock();
    let state = MotionState {
        tab: app.host().runtime().signal(0usize),
        duration_ms: app.host().runtime().signal(DUR_DEFAULT_MS),
        ease_idx: app.host().runtime().signal(0usize),
        spin: app.host().runtime().signal(false),
        zoom: app.host().runtime().signal(false),
        twist: app.host().runtime().signal(false),
    };
    app.mount("Motion", state, MotionApp);
    // Bounded presses (taps would idle-pump under the hover tweens
    // each press starts on a frozen clock — the M8 pattern).
    for (key, _) in TABS {
        press(&app, &format!("motion-tab-{key}"));
        app.advance_frames(2.0 / 60.0);
    }
    // The clock jumps past every micro tween; the idle pump retires
    // them on its first frames and goes quiet.
    app.advance(5.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "idle tree owns no live tracks");
    });
    assert!(
        !app.host().has_demand(),
        "settled tree leaves zero scheduled work"
    );
}

#[test]
fn motion_tabbar_sits_above_the_room() {
    let (app, _) = mount_motion();
    let bar = app
        .host()
        .committed_box(app.node("motion-tabbar"))
        .expect("tabbar laid out");
    let room = app
        .host()
        .committed_box(app.node("motion-room-entrance"))
        .expect("room laid out");
    assert!(bar.w > 0.0 && bar.h > 0.0, "tabbar has area");
    assert!(room.w > 0.0 && room.h > 0.0, "room has area");
    assert!(
        room.y >= bar.y + bar.h,
        "room starts below the tab bar ({room:?} vs {bar:?})"
    );
}

#[test]
fn motion_pure_helpers_cover_all_rooms() {
    assert_eq!(TABS.len(), 7, "seven rooms — six motion goals plus layout animation");
    assert_eq!(EASE_NAMES.len(), 5, "four cubics plus the spring");
    assert_eq!(
        SharedString::from(motion::room_title("springs")),
        SharedString::from("Springs")
    );
}

fn mount_clocked() -> (Harness, MotionState) {
    let (app, _) = Harness::with_clock();
    let state = MotionState {
        tab: app.host().runtime().signal(0usize),
        duration_ms: app.host().runtime().signal(DUR_DEFAULT_MS),
        ease_idx: app.host().runtime().signal(0usize),
        spin: app.host().runtime().signal(false),
        zoom: app.host().runtime().signal(false),
        twist: app.host().runtime().signal(false),
    };
    app.mount("Motion", state.clone(), MotionApp);
    (app, state)
}

fn committed(app: &Harness, debug: &str) -> oppa::LayoutBox {
    app.host()
        .committed_box(app.node(debug))
        .unwrap_or_else(|| panic!("{debug} has no committed box — pump first"))
}

/// Down+up without the idle pump (`tap` would spin forever once the
/// press starts a live track — callers step `advance_frames`
/// explicitly, the M8 pattern).
fn press(app: &Harness, debug: &str) {
    let b = committed(app, debug);
    let (x, y) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    app.host()
        .inject_input(oppa::InputEvent::pointer_down(x, y));
    app.host().inject_input(oppa::InputEvent::pointer_up(x, y));
}

fn down_at(app: &Harness, x: f32, y: f32) {
    app.host()
        .inject_input(oppa::InputEvent::pointer_down(x, y));
}

fn up_at(app: &Harness, x: f32, y: f32) {
    app.host().inject_input(oppa::InputEvent::pointer_up(x, y));
}

fn hover_at(app: &Harness, x: f32, y: f32) {
    app.host().inject_input(oppa::InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Move,
        x,
        y,
        modifiers: oppa::Modifiers::NONE,
    });
}

fn box_center(app: &Harness, debug: &str) -> (f32, f32) {
    let b = committed(app, debug);
    (b.x + b.w / 2.0, b.y + b.h / 2.0)
}

/// Menu row ids top-down (the fade probe needs retired ids —
/// positions alone cannot resolve evaluator values past removal).
fn menu_row_ids(app: &Harness) -> Vec<oppa::NodeId> {
    let mut ids = Vec::new();
    app.host().with_retained_mut(|rec, _| {
        for id in rec.retained_ids() {
            if rec
                .get(id)
                .is_some_and(|n| n.debug.starts_with("menu-item-"))
            {
                ids.push(id);
            }
        }
    });
    ids.sort_by_cached_key(|id| {
        app.host()
            .committed_box(*id)
            .map(|b| (b.y * 1_000_000.0) as i64)
            .unwrap_or(i64::MAX)
    });
    ids
}

/// Menu row centers top-down (decision 514): rows carry
/// instance-scoped labels, so discover by prefix + committed boxes.
fn menu_row_centers(app: &Harness) -> Vec<(f32, f32)> {
    let mut ids = Vec::new();
    app.host().with_retained_mut(|rec, _| {
        for id in rec.retained_ids() {
            if rec
                .get(id)
                .is_some_and(|n| n.debug.starts_with("menu-item-"))
            {
                ids.push(id);
            }
        }
    });
    let mut rows: Vec<(f32, (f32, f32))> = ids
        .into_iter()
        .filter_map(|id| {
            app.host()
                .committed_box(id)
                .map(|b| (b.y, (b.x + b.w / 2.0, b.y + b.h / 2.0)))
        })
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("finite boxes"));
    rows.into_iter().map(|(_, p)| p).collect()
}

/// Decision 511: playing the loops starts live tracks (frame demand
/// flows while they run) while every committed box holds exactly
/// still — transforms are paint-only, layout-untouched.
#[test]
fn motion_loop_play_runs_and_boxes_hold() {
    let (app, _) = mount_clocked();
    // Bounded tab switch (never `tap` here: if rooms ever recycled
    // nodes again, the cross-room tracks would spin an idle pump
    // forever — press + frames reports loudly instead).
    press(&app, "motion-tab-transforms");
    app.advance_frames(2.0 / 60.0);
    // Tab switches replace rooms (tab-keyed mounts): the room leaves
    // mount fresh with no tracks of their own (the tab buttons'
    // selection tweens may live — assert per-node, never global).
    for label in [
        "motion-xf-loop",
        "motion-xf-text",
        "motion-xf-vector",
        "motion-xf-zoom-rect",
        "motion-xf-spin-rect",
        "motion-xf-twist-rect",
    ] {
        let id = app.node(label);
        app.host().with_evaluator(|ev| {
            assert!(ev.is_settled(id), "{label} mounts without tracks");
        });
    }
    let before = committed(&app, "motion-xf-loop");
    let before_text = committed(&app, "motion-xf-text");
    let before_vector = committed(&app, "motion-xf-vector");
    press(&app, "motion-xf-play");
    // One frame dispatches the press and commits the delta (the
    // assert below must see pumped state — `press` never pumps).
    app.advance_frames(1.0 / 60.0);
    app.host().with_evaluator(|ev| {
        assert!(ev.created() >= 1, "play starts loop tracks");
    });
    // Default 150ms legs: 0.2s in, the loops are mid-flight
    // (`advance_frames` — the M8 pattern: `advance` would spin
    // forever under a live track).
    app.advance_frames(0.2);
    app.host().with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "loops stay live until stopped");
    });
    assert!(
        app.host().has_demand(),
        "live loops keep frame demand flowing"
    );
    assert_eq!(
        committed(&app, "motion-xf-loop"),
        before,
        "layout never moves under a transform"
    );
    assert_eq!(
        committed(&app, "motion-xf-text"),
        before_text,
        "text wrapper holds still too"
    );
    assert_eq!(
        committed(&app, "motion-xf-vector"),
        before_vector,
        "vector wrapper holds still too"
    );
    // Stop: the closing tweens settle back to zero scheduled work.
    press(&app, "motion-xf-play");
    app.advance_frames(5.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "stopped loops settle");
    });
    assert!(
        !app.host().has_demand(),
        "settled room leaves zero scheduled work"
    );
    assert_eq!(
        committed(&app, "motion-xf-loop"),
        before,
        "boxes still hold"
    );
}

/// Decision 511: the zoom one-shot tweens to its scale target and
/// settles exact, boxes unmoved throughout.
#[test]
fn motion_zoom_one_shot_settles_exact() {
    let (app, _) = mount_clocked();
    press(&app, "motion-tab-transforms");
    app.advance_frames(2.0 / 60.0);
    let id = app.node("motion-xf-zoom-rect");
    let before = app.host().committed_box(id).expect("zoom box laid out");
    press(&app, "motion-xf-zoom");
    app.advance_frames(DUR_DEFAULT_MS as f64 / 2000.0);
    // Node-specific liveness (the zoom button's own hover/press
    // tweens share the global count — never assert it mid-flight).
    app.host().with_evaluator(|ev| {
        assert!(!ev.is_settled(id), "one-shot tween in flight");
    });
    assert_eq!(
        app.host().committed_box(id).expect("zoom box laid out"),
        before,
        "layout never moves under a transform"
    );
    app.advance_frames(5.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "one-shot settles");
        let xf = ev.resolve_transform(id, None, 5.0);
        assert_eq!(
            (xf.sx.get(), xf.sy.get()),
            (1.6, 1.6),
            "settles exact on target"
        );
    });
}

/// Decision 517: the rotation loop runs while playing (sampled
/// rotation leaves zero and boxes hold), and stopping settles it back
/// to identity with zero scheduled work.
#[test]
fn motion_rotate_loop_runs_and_boxes_hold() {
    let (app, _) = mount_clocked();
    press(&app, "motion-tab-transforms");
    app.advance_frames(2.0 / 60.0);
    let id = app.node("motion-xf-spin-rect");
    let before = committed(&app, "motion-xf-spin-rect");
    press(&app, "motion-xf-play");
    // One frame dispatches the press and commits the delta.
    app.advance_frames(1.0 / 60.0);
    // Default 150ms legs: sweep the flight for nonzero rotation (the
    // ±45° oscillation crosses zero twice per cycle — one instant
    // would be phase luck).
    let mut deg_max = 0.0f32;
    for _ in 0..10 {
        app.advance_frames(0.03);
        let now = app.host().now_ms() / 1000.0;
        let deg = app
            .host()
            .with_evaluator(|ev| ev.resolve_transform(id, None, now).rotate_deg.get());
        deg_max = deg_max.max(deg.abs());
    }
    app.host().with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "rotation loop stays live");
    });
    assert!(
        deg_max > 20.0,
        "rotation loop travels toward ±45°, got max {deg_max}"
    );
    assert!(
        app.host().has_demand(),
        "live loops keep frame demand flowing"
    );
    assert_eq!(
        committed(&app, "motion-xf-spin-rect"),
        before,
        "layout never moves under rotation"
    );
    // Stop: the closing tween settles back to zero scheduled work.
    press(&app, "motion-xf-play");
    app.advance_frames(5.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "stopped loops settle");
        assert!(
            ev.resolve_transform(id, None, 10.0).is_identity(),
            "rotation closes exact on identity"
        );
    });
    assert!(
        !app.host().has_demand(),
        "settled room leaves zero scheduled work"
    );
}

/// Decision 517: the twist one-shot tweens to 90° and settles exact,
/// boxes unmoved throughout.
#[test]
fn motion_twist_one_shot_settles_exact() {
    let (app, _) = mount_clocked();
    press(&app, "motion-tab-transforms");
    app.advance_frames(2.0 / 60.0);
    let id = app.node("motion-xf-twist-rect");
    let before = app.host().committed_box(id).expect("twist box laid out");
    press(&app, "motion-xf-twist");
    app.advance_frames(DUR_DEFAULT_MS as f64 / 2000.0);
    app.host().with_evaluator(|ev| {
        assert!(!ev.is_settled(id), "one-shot tween in flight");
    });
    let now = app.host().now_ms() / 1000.0;
    let mid = app
        .host()
        .with_evaluator(|ev| ev.resolve_transform(id, None, now).rotate_deg.get());
    assert!(
        mid > 0.0 && mid < 90.0,
        "twist travels 0→90 strictly interior, got {mid}"
    );
    assert_eq!(
        app.host().committed_box(id).expect("twist box laid out"),
        before,
        "layout never moves under rotation"
    );
    app.advance_frames(5.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "one-shot settles");
        assert_eq!(
            ev.resolve_transform(id, None, 10.0).rotate_deg.get(),
            90.0,
            "settles exact on target"
        );
    });
}

/// Fires due card timeouts at the current mock instant (headless
/// pumps never tick timers by themselves — runners do it per pump).
fn tick(app: &Harness) -> usize {
    let now = app.host().now_ms();
    app.host().tick_timers(now)
}

fn card_opacity(app: &Harness, i: usize, now: f64) -> f32 {
    let id = app.node(&format!("motion-enter-{i}"));
    app.host()
        .with_evaluator(|ev| ev.resolve_opacity(id, 0.0, now))
}

/// Decision 512: entrance choreography — cards mount hidden with
/// zero tracks, flip through one armed timeout each, then stagger
/// home (card 3 still at entry while card 0 is mid-flight), and
/// settle exact with boxes unmoved and zero scheduled work left.
#[test]
fn motion_entrance_staggers_and_settles() {
    let (app, _) = mount_clocked();
    for i in 0..4usize {
        assert_eq!(card_opacity(&app, i, 0.0), 0.0, "card {i} mounts hidden");
    }
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.created(), 0, "mounts jump — no tracks yet");
    });
    let before: Vec<_> = (0..4usize)
        .map(|i| committed(&app, &format!("motion-enter-{i}")))
        .collect();
    // Past the 30ms flip: all four timeouts fire in one tick.
    app.advance_frames(0.05);
    assert_eq!(tick(&app), 4, "one flip per card");
    app.advance_frames(1.0 / 60.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.created(), 4, "one entrance track per card");
        assert!(ev.active_count() >= 1, "stagger in flight");
    });
    // Default 150ms legs, 70ms stagger: card 0 is mid-flight
    // while card 3 (delay 210ms) still holds entry — sampled at the
    // live clock, never a hand-computed instant.
    app.advance_frames(0.05);
    let now = app.host().now_ms() / 1000.0;
    let o0 = card_opacity(&app, 0, now);
    assert!(o0 > 0.0 && o0 < 1.0, "card 0 mid-flight, got {o0}");
    assert_eq!(
        card_opacity(&app, 3, now),
        0.0,
        "card 3 still at entry (stagger)"
    );
    assert!(app.host().has_demand(), "stagger keeps demand flowing");
    for (i, b) in before.iter().enumerate() {
        assert_eq!(
            committed(&app, &format!("motion-enter-{i}")),
            *b,
            "card {i} box never moves"
        );
    }
    // Past every delay + leg: settled exact, quiescent.
    app.advance_frames(3.0);
    for (i, b) in before.iter().enumerate() {
        assert_eq!(card_opacity(&app, i, 10.0), 1.0, "card {i} lands exact");
        let id = app.node(&format!("motion-enter-{i}"));
        app.host().with_evaluator(|ev| {
            assert!(ev.resolve_transform(id, None, 10.0).is_identity());
        });
        assert_eq!(
            committed(&app, &format!("motion-enter-{i}")),
            *b,
            "card {i} box still holds"
        );
    }
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "entrance retires");
    });
    assert!(!app.host().has_demand(), "zero scheduled work after");
}

/// Decision 512: Replay re-keys the cards so the stagger runs
/// again — the replay commit tweens back toward hidden (entry 1.0
/// is correct pre-start behavior), fresh flips re-arm, and the
/// second entrance lands exact with zero work left.
#[test]
fn motion_entrance_replay_restarts_the_stagger() {
    let (app, _) = mount_clocked();
    app.advance_frames(0.05);
    assert_eq!(tick(&app), 4);
    app.advance_frames(3.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "first run settled");
    });
    let created = app.host().with_evaluator(|ev| ev.created());
    assert_eq!(created, 4);
    // Bounded press (tap's idle pump would spin forever under the
    // live hide-tweens — the M8 pattern).
    press(&app, "motion-enter-replay");
    // Two frames: the first commits the replay delta, the second lets
    // the hide-tweens travel (sampling mid-commit reads entry).
    app.advance_frames(2.0 / 60.0);
    let now = app.host().now_ms() / 1000.0;
    // The replay commit tweens back toward hidden: card 0 (no
    // delay) is already leaving 1.0 behind while card 3 (210ms
    // delay) still holds entry — the stagger in reverse.
    let o0 = card_opacity(&app, 0, now);
    assert!(o0 < 1.0, "card 0 hiding already, got {o0}");
    assert_eq!(
        card_opacity(&app, 3, now),
        1.0,
        "card 3 still at entry (stagger)"
    );
    app.host().with_evaluator(|ev| {
        // Four hide-tweens plus the replay button's own press/hover
        // tweens (never an exact count where buttons are involved —
        // assert the cards hide instead).
        assert!(ev.created() >= created + 4, "hide-tweens started");
        assert!(ev.active_count() >= 1, "hiding in flight");
    });
    let hide0 = app.node("motion-enter-0");
    app.host().with_evaluator(|ev| {
        assert!(!ev.is_settled(hide0), "card 0 tween live");
    });
    app.advance_frames(0.05);
    assert_eq!(tick(&app), 4, "re-armed flips fire again");
    app.advance_frames(3.0);
    for i in 0..4usize {
        assert_eq!(
            card_opacity(&app, i, 10.0),
            1.0,
            "card {i} lands exact again"
        );
    }
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "second run settles too");
    });
    assert!(!app.host().has_demand());
}

fn goto_micro(app: &Harness) {
    press(app, "motion-tab-micro");
    app.advance_frames(2.0 / 60.0);
}

fn goto_overlays(app: &Harness) {
    press(app, "motion-tab-overlays");
    app.advance_frames(2.0 / 60.0);
}

fn goto_layout(app: &Harness) {
    press(app, "motion-tab-layout");
    app.advance_frames(2.0 / 60.0);
}

/// Decision 514: holding a Button tweens pressed-fill in and back
/// out, boxes unmoved, quiescent at rest.
#[test]
fn motion_button_hold_tweens_and_releases() {
    let (app, _) = mount_clocked();
    goto_micro(&app);
    let id = app.node("motion-micro-press");
    let before = app.host().committed_box(id).expect("button laid out");
    let (x, y) = (before.x + before.w / 2.0, before.y + before.h / 2.0);
    down_at(&app, x, y);
    app.advance_frames(2.0 / 60.0);
    app.host().with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "pressed tween live while held");
    });
    app.advance_frames(0.06);
    let mid = app.host().with_evaluator(|ev| {
        ev.resolve_bg(id, None, app.host().now_ms() / 1000.0)
            .expect("tracked")
    });
    let r = (mid.0 >> 16) & 0xFF;
    // Light theme: primary 0x22 → pressed 0x1B, mid-flight between.
    assert!(
        (0x1B..=0x22).contains(&r),
        "pressed tween travels primary→pressed, got {r:02x}"
    );
    assert_eq!(
        app.host().committed_box(id).expect("button laid out"),
        before,
        "layout never moves under a press"
    );
    up_at(&app, x, y);
    app.advance_frames(1.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "release settles");
        // The pointer still hovers the button, so the wash persists
        // (correct!) — hover away to see bare primary again.
        assert_eq!(
            ev.resolve_bg(id, None, 10.0),
            Some(oppa::Color(0x3D_78_D2)),
            "settles exact on the hover wash"
        );
    });
    hover_at(&app, 750.0, 550.0);
    app.advance_frames(1.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(
            ev.resolve_bg(id, None, 10.0),
            Some(oppa::Color(0x22_66_CC)),
            "unhovered settles exact on primary"
        );
    });
    assert!(!app.host().has_demand());
    assert_eq!(
        app.host().committed_box(id).expect("button laid out"),
        before
    );
}

/// Decision 514: toggling a Checkbox tweens the box fill, boxes
/// unmoved, quiescent at rest.
#[test]
fn motion_checkbox_toggle_tweens() {
    let (app, _) = mount_clocked();
    goto_micro(&app);
    let id = app.node("checkbox-box");
    press(&app, "checkbox");
    app.advance_frames(0.06);
    app.host().with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "check tween live");
    });
    let now = app.host().now_ms() / 1000.0;
    let mid = app
        .host()
        .with_evaluator(|ev| ev.resolve_bg(id, None, now).expect("tracked"));
    let b = mid.0 & 0xFF;
    // Surface white → primary blue: mid-flight blue channel between.
    assert!(
        (0xCC..=0xFF).contains(&b),
        "check tween travels surface→primary, got {b:02x}"
    );
    app.advance_frames(1.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "toggle settles");
        // Pointer still hovers the box: the wash persists over the
        // checked fill (correct) — hover away for the bare value.
        assert_eq!(
            ev.resolve_bg(id, None, 10.0),
            Some(oppa::Color(0x3D_78_D2)),
            "settles exact on hovered primary"
        );
    });
    hover_at(&app, 750.0, 550.0);
    app.advance_frames(1.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(
            ev.resolve_bg(id, None, 10.0),
            Some(oppa::Color(0x22_66_CC)),
            "unhovered settles exact on primary"
        );
    });
    assert!(!app.host().has_demand());
}

/// Decision 514: menu rows tween on hover highlight, then tap
/// invokes through settled geometry and closes.
#[test]
fn motion_menu_row_hover_tap_invokes_and_closes() {
    let (app, _) = mount_clocked();
    goto_micro(&app);
    press(&app, "motion-micro-menu-open");
    app.advance_frames(2.0 / 60.0);
    // Fire flips without counting: unmounted rooms leave stale
    // one-shots behind (instances linger — see the blocked row), so
    // only the behavior below is asserted, never the fired count.
    tick(&app);
    app.advance_frames(0.3);
    let rows = menu_row_centers(&app);
    assert_eq!(rows.len(), 2, "two rows laid out");
    hover_at(&app, rows[1].0, rows[1].1);
    app.advance_frames(2.0 / 60.0);
    app.host().with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "highlight tween live");
    });
    app.advance_frames(0.5);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "highlight settles");
    });
    let (x, y) = rows[1];
    down_at(&app, x, y);
    up_at(&app, x, y);
    // Settle past every micro tween (the pointer move unhovers the
    // open button along the way).
    app.advance_frames(0.5);
    assert!(menu_row_centers(&app).is_empty(), "invoke closes the menu");
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "invoke settles to zero work");
    });
    assert!(!app.host().has_demand());
}

/// Decision 514: an instant Tooltip fades in on hover and dismisses
/// on leave.
#[test]
fn motion_tooltip_fades_on_hover() {
    let (app, _) = mount_clocked();
    goto_micro(&app);
    let (x, y) = box_center(&app, "motion-micro-tip-anchor");
    hover_at(&app, x, y);
    app.advance_frames(2.0 / 60.0);
    let id = app.node("tooltip-card");
    tick(&app);
    app.advance_frames(2.0 / 60.0);
    app.host().with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "fade track live");
    });
    app.advance_frames(0.05);
    let now = app.host().now_ms() / 1000.0;
    let mid = app
        .host()
        .with_evaluator(|ev| ev.resolve_opacity(id, 0.0, now));
    assert!(mid > 0.0 && mid < 1.0, "fade travels 0→1, got {mid}");
    app.advance_frames(1.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "fade settles");
        assert_eq!(ev.resolve_opacity(id, 0.0, 10.0), 1.0);
    });
    assert!(!app.host().has_demand());
    hover_at(&app, 750.0, 550.0);
    app.advance_frames(2.0 / 60.0);
    assert!(
        oppa::find_retained_by_debug(app.host(), "tooltip-card").is_empty(),
        "leave dismisses"
    );
}

/// Decision 514: the modal backdrop fades while the card fades +
/// Decision 520: the modal enters (fade + rise), settles exact,
/// and dismisses through a fade (no snap) — the backdrop tap keeps
/// the ghost past removal while its exit track runs, then releases
/// to zero work.
#[test]
fn motion_modal_enters_and_dismisses() {
    let (app, _) = mount_clocked();
    goto_overlays(&app);
    press(&app, "motion-ov-modal-open");
    app.advance_frames(2.0 / 60.0);
    // Fire flips without counting (stale one-shots from unmounted
    // rooms share the table — see the micro menu test).
    tick(&app);
    app.advance_frames(0.09);
    let card = app.node("modal-card");
    let back = app.node("modal-backdrop");
    let now = app.host().now_ms() / 1000.0;
    let (card_xf, back_op) = app.host().with_evaluator(|ev| {
        (
            ev.resolve_transform(card, None, now),
            ev.resolve_opacity(back, 0.0, now),
        )
    });
    assert!(
        card_xf.dy.get() > 0.0,
        "card rises from below, got {}",
        card_xf.dy.get()
    );
    assert!(
        back_op > 0.0 && back_op < 0.5,
        "scrim fades toward 0.5, got {back_op}"
    );
    app.advance_frames(1.0);
    app.host().with_evaluator(|ev| {
        assert!(ev.resolve_transform(card, None, 10.0).is_identity());
        assert_eq!(ev.resolve_opacity(back, 0.0, 10.0), 0.5);
    });
    // Backdrop tap outside the card dismisses (decision 520: the
    // dialog fades instead of snapping — the ghost paints past
    // removal while its exit runs).
    let bb = committed(&app, "modal-backdrop");
    let cb = committed(&app, "modal-card");
    assert!(cb.x > bb.x + 5.0, "tap lands outside the card");
    down_at(&app, bb.x + 5.0, bb.y + bb.h / 2.0);
    up_at(&app, bb.x + 5.0, bb.y + bb.h / 2.0);
    // Mid-exit: the scrim fades from its 0.5 veil on the retired id.
    app.advance_frames(0.09);
    let now = app.host().now_ms() / 1000.0;
    let mid = app
        .host()
        .with_evaluator(|ev| ev.resolve_opacity(back, 0.0, now));
    assert!(
        mid > 0.0 && mid < 0.5,
        "scrim fades mid-exit, got {mid}"
    );
    // Settle past every micro tween (the pointer move unhovers the
    // open button — its wash tween must retire too) and the exit.
    app.advance_frames(0.5);
    assert!(
        oppa::find_retained_by_debug(app.host(), "modal-card").is_empty(),
        "dialog unmounts on dismiss"
    );
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "exit retires to zero work");
    });
    assert!(!app.host().has_demand());
}

/// Decision 520: the toast card fades + rises, settles exact, and
/// the dismiss button fades it out (no snap) before releasing to
/// zero work.
#[test]
fn motion_toast_enters_and_dismisses() {
    let (app, _) = mount_clocked();
    goto_overlays(&app);
    press(&app, "motion-ov-toast-open");
    app.advance_frames(2.0 / 60.0);
    tick(&app);
    app.advance_frames(0.09);
    let id = app.node("toast-card");
    let now = app.host().now_ms() / 1000.0;
    let (xf, op) = app.host().with_evaluator(|ev| {
        (
            ev.resolve_transform(id, None, now),
            ev.resolve_opacity(id, 0.0, now),
        )
    });
    assert!(xf.dy.get() > 0.0, "card rises, got {}", xf.dy.get());
    assert!(op > 0.0 && op < 1.0, "card fades, got {op}");
    app.advance_frames(1.0);
    app.host().with_evaluator(|ev| {
        assert!(ev.resolve_transform(id, None, 10.0).is_identity());
        assert_eq!(ev.resolve_opacity(id, 0.0, 10.0), 1.0);
    });
    press(&app, "toast-dismiss");
    // Mid-exit: the card fades from 1 on the retired id.
    app.advance_frames(0.09);
    let now = app.host().now_ms() / 1000.0;
    let mid = app
        .host()
        .with_evaluator(|ev| ev.resolve_opacity(id, 0.0, now));
    assert!(mid > 0.0 && mid < 1.0, "card fades mid-exit, got {mid}");
    // Settle past every micro tween (pointer moves unhover buttons
    // along the way) and the exit.
    app.advance_frames(0.5);
    assert!(
        oppa::find_retained_by_debug(app.host(), "toast-card").is_empty(),
        "dismiss closes"
    );
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "exit retires to zero work");
    });
    assert!(!app.host().has_demand());
}

/// Decision 520: the overlay menu plate fades + rises to its anchor,
/// settles exact, and invoking fades it out (no snap) before
/// settling to zero work.
#[test]
fn motion_overlay_menu_enters_settled() {
    let (app, _) = mount_clocked();
    goto_overlays(&app);
    press(&app, "motion-ov-menu-open");
    app.advance_frames(2.0 / 60.0);
    tick(&app);
    app.advance_frames(0.09);
    let rows = menu_row_centers(&app);
    assert_eq!(rows.len(), 1, "one row laid out");
    app.advance_frames(1.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "entrance settles");
    });
    assert!(!app.host().has_demand());
    let (x, y) = rows[0];
    let row_id = menu_row_ids(&app)[0];
    down_at(&app, x, y);
    up_at(&app, x, y);
    // Mid-exit: the invoked row fades on its retired id.
    app.advance_frames(0.09);
    let now = app.host().now_ms() / 1000.0;
    let mid = app
        .host()
        .with_evaluator(|ev| ev.resolve_opacity(row_id, 0.0, now));
    assert!(mid > 0.0 && mid < 1.0, "row fades mid-exit, got {mid}");
    app.advance_frames(0.5);
    assert!(menu_row_centers(&app).is_empty(), "invoke closes the menu");
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "exit retires to zero work");
    });
    assert!(!app.host().has_demand());
}

fn goto_springs(app: &Harness) {
    press(app, "motion-tab-springs");
    app.advance_frames(2.0 / 60.0);
}

fn goto_ambient(app: &Harness) {
    press(app, "motion-tab-ambient");
    app.advance_frames(2.0 / 60.0);
}

/// Decision 513: firing springs overshoots the translate target
/// mid-flight (the motion), clamps the opacity spring into gamut,
/// and settles everything exact with boxes unmoved.
#[test]
fn motion_springs_overshoot_and_settle_exact() {
    let (app, _) = mount_clocked();
    goto_springs(&app);
    let move_id = app.node("motion-spring-move");
    let grow_id = app.node("motion-spring-grow");
    let fade_id = app.node("motion-spring-fade");
    let before: Vec<_> = [
        "motion-spring-move",
        "motion-spring-grow",
        "motion-spring-fade",
    ]
    .into_iter()
    .map(|d| committed(&app, d))
    .collect();
    press(&app, "motion-spring-fire");
    app.advance_frames(2.0 / 60.0);
    app.host().with_evaluator(|ev| {
        assert!(ev.active_count() >= 3, "three spring tracks live");
    });
    // Default 150ms spring: sweep samples through the flight for
    // the overshoot (the peak is narrow — one instant would be
    // alignment luck).
    let mut dx_max = 0.0f32;
    let mut sx_max = 0.0f32;
    for _ in 0..10 {
        app.advance_frames(0.02);
        let now = app.host().now_ms() / 1000.0;
        let (dx, sx, op) = app.host().with_evaluator(|ev| {
            (
                ev.resolve_transform(move_id, None, now).dx.get(),
                ev.resolve_transform(grow_id, None, now).sx.get(),
                ev.resolve_opacity(fade_id, 1.0, now),
            )
        });
        dx_max = dx_max.max(dx);
        sx_max = sx_max.max(sx);
        // Opacity spring 1→0.4 clamps into range at every sample
        // (never past either end — 1 ulp of float dust allowed).
        assert!(
            (0.4 - 1e-6..=1.0).contains(&op),
            "opacity spring stays in gamut, got {op}"
        );
    }
    assert!(
        dx_max > 120.0,
        "translate spring overshoots its 120 target, got max {dx_max}"
    );
    assert!(
        sx_max > 1.6,
        "scale spring overshoots 1.6, got max {sx_max}"
    );
    for (i, d) in [
        "motion-spring-move",
        "motion-spring-grow",
        "motion-spring-fade",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(committed(&app, d), before[i], "box {d} never moves");
    }
    app.advance_frames(2.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "springs settle");
        assert_eq!(
            ev.resolve_transform(move_id, None, 10.0).dx.get(),
            120.0,
            "translate lands exact"
        );
        let g = ev.resolve_transform(grow_id, None, 10.0);
        assert_eq!((g.sx.get(), g.sy.get()), (1.6, 1.6), "scale lands exact");
        assert_eq!(ev.resolve_opacity(fade_id, 1.0, 10.0), 0.4);
    });
    assert!(!app.host().has_demand());
    for (i, d) in [
        "motion-spring-move",
        "motion-spring-grow",
        "motion-spring-fade",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(committed(&app, d), before[i], "boxes still hold");
    }
}

/// Decisions 512–513: the ambient orb pings until unmounted (loop
/// demand flows), while the chained sequence fires link by link
/// from completion signals and lands exact with the ambient left
/// running.
#[test]
fn motion_ambient_loops_and_chain_completes() {
    let (app, _) = mount_clocked();
    goto_ambient(&app);
    // The ambient loop needs one flip to start (mounts parked).
    app.host().tick_timers(app.host().now_ms());
    app.advance_frames(0.2);
    let orb = app.node("motion-amb-orb");
    app.host().with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "ambient loop alive");
    });
    assert!(app.host().has_demand(), "loops keep demand flowing");
    // Play the chain: three links, each starting from the previous
    // link's settle (the observe_settled contract from app code).
    press(&app, "motion-amb-play");
    // Mid-chain: link 1 lit while link 3 is still dim (links fire
    // in sequence — per-step bg discriminates; opacity is shared
    // once the chain starts, so it cannot).
    app.advance_frames(0.25);
    let now = app.host().now_ms() / 1000.0;
    let s1 = app.node("motion-amb-step1");
    let s3 = app.node("motion-amb-step3");
    let (b1, b3) = app
        .host()
        .with_evaluator(|ev| (ev.resolve_bg(s1, None, now), ev.resolve_bg(s3, None, now)));
    assert_eq!(b1, Some(oppa::Color(0x22_66_CC)), "link 1 lit first");
    assert_ne!(
        b3,
        Some(oppa::Color(0x22_66_CC)),
        "link 3 still dim, got {b3:?}"
    );
    app.advance_frames(2.0);
    for i in 1..=3usize {
        let id = app.node(&format!("motion-amb-step{i}"));
        let op = app
            .host()
            .with_evaluator(|ev| ev.resolve_opacity(id, 0.0, 10.0));
        assert_eq!(op, 1.0, "step {i} landed (chain link fired)");
    }
    // The chain settled; only the ambient loop still runs.
    app.host().with_evaluator(|ev| {
        assert!(!ev.is_settled(orb), "orb still looping");
        for i in 1..=3usize {
            let id = app.node(&format!("motion-amb-step{i}"));
            assert!(ev.is_settled(id), "step {i} settled");
        }
    });
    // Leaving the tab prunes the loop: demand drains to zero (loops
    // never leak the pump).
    press(&app, "motion-tab-entrance");
    app.advance_frames(0.5);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "unmount retires the loop");
    });
    assert!(!app.host().has_demand());
}

/// Decisions 521-522: expanding the card runs the fence (interior
/// height mid-flight, committed target already applied), lands
/// exact, and retires to zero work.
#[test]
fn motion_layout_expand_fences_then_settles() {
    let (app, _) = mount_clocked();
    goto_layout(&app);
    let card = app.node("motion-layout-card");
    press(&app, "motion-layout-expand");
    app.advance_frames(2.0 / 60.0);
    let target = committed(&app, "motion-layout-card");
    assert_eq!((target.w, target.h), (160.0, 96.0), "expand commits");
    // Mid-flight (default 150ms Out): strictly interior.
    app.advance_frames(0.06);
    let now = app.host().now_ms() / 1000.0;
    let mid = app
        .host()
        .effective_box(card, now)
        .expect("fence interpolates");
    assert!(
        mid.h > 48.0 && mid.h < 96.0,
        "fence runs mid-flight, got {}",
        mid.h
    );
    app.advance_frames(1.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "fence retires to zero work");
    });
    assert_eq!(
        committed(&app, "motion-layout-card").h,
        96.0,
        "settles exact on target"
    );
    assert!(!app.host().has_demand(), "quiet after the fence");
}

/// Decisions 521-522 shared timeline: the expanding card (fence)
/// and the sibling below (FLIP, same spec, same commit) track each
/// other every frame — the card's bottom edge never crosses the
/// sibling's visual top.
#[test]
fn motion_layout_shared_timeline_never_overlaps() {
    let (app, _) = mount_clocked();
    goto_layout(&app);
    let card = app.node("motion-layout-card");
    let sib = app.node("motion-layout-sib");
    press(&app, "motion-layout-expand");
    for _ in 0..12 {
        app.advance_frames(1.0 / 60.0);
        let now = app.host().now_ms() / 1000.0;
        // Settled fences read None (the committed fallback already
        // agrees) — the invariant holds through settle either way.
        let card_box = app
            .host()
            .effective_box(card, now)
            .or_else(|| app.host().committed_box(card))
            .expect("card has a box");
        let sib_box = committed(&app, "motion-layout-sib");
        let flip_dy = app
            .host()
            .with_evaluator(|ev| ev.resolve_transform(sib, None, now).dy.get());
        let card_bottom = card_box.y + card_box.h;
        let sib_visual_top = sib_box.y + flip_dy;
        assert!(
            card_bottom <= sib_visual_top + 1.0,
            "shared timeline holds (bottom {card_bottom} vs top {sib_visual_top})"
        );
    }
    app.advance_frames(1.0);
    assert!(!app.host().has_demand(), "quiet after the shared run");
}

/// Decision 521: inserting a row shifts every flagged row below it
/// through FLIP (mid-flight offsets strictly interior), landing
/// exact and quiet.
#[test]
fn motion_layout_insert_flips_rows_then_settles() {
    let (app, _) = mount_clocked();
    goto_layout(&app);
    let r0 = app.node("motion-layout-r0");
    let y0 = committed(&app, "motion-layout-r0").y;
    press(&app, "motion-layout-add");
    app.advance_frames(2.0 / 60.0);
    let y1 = committed(&app, "motion-layout-r0").y;
    assert!(y1 > y0, "insert shifts rows down");
    app.advance_frames(0.06);
    let now = app.host().now_ms() / 1000.0;
    let dy = app
        .host()
        .with_evaluator(|ev| ev.resolve_transform(r0, None, now).dy.get());
    assert!(
        dy < 0.0 && dy > y0 - y1,
        "flip offsets mid-flight, got {dy}"
    );
    app.advance_frames(1.0);
    app.host().with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "flip retires to zero work");
    });
    assert!(!app.host().has_demand(), "quiet after the flip");
}
