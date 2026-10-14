use super::*;

#[derive(Clone)]
struct CleanableProps {
    cleaned: Rc<Cell<bool>>,
}
impl Props for CleanableProps {}

fn cleanable_comp(ctx: &Ctx, props: &CleanableProps) -> VNode {
    let c = props.cleaned.clone();
    ctx.on_cleanup(move || {
        c.set(true);
    });
    VNode::Hole
}

#[test]
fn component_cleanup_runs_on_unmount() {
    let host = ComponentHost::new();
    let cleaned = Rc::new(Cell::new(false));

    let handle = host.mount(
        "Cleanable",
        CleanableProps {
            cleaned: cleaned.clone(),
        },
        cleanable_comp,
    );

    host.run_until_idle();
    assert!(!cleaned.get(), "cleanups do not run on initial mount");

    handle.unmount();
    assert!(cleaned.get(), "cleanups run when component is unmounted");
}

#[derive(Clone)]
struct OrderProps {
    log: Rc<RefCell<Vec<i32>>>,
}
impl Props for OrderProps {}

fn order_comp(ctx: &Ctx, props: &OrderProps) -> VNode {
    let l1 = props.log.clone();
    ctx.on_cleanup(move || l1.borrow_mut().push(1));
    let l2 = props.log.clone();
    ctx.on_cleanup(move || l2.borrow_mut().push(2));
    let l3 = props.log.clone();
    ctx.on_cleanup(move || l3.borrow_mut().push(3));
    VNode::Hole
}

#[test]
fn component_cleanups_run_in_lifo_order() {
    let host = ComponentHost::new();
    let log = Rc::new(RefCell::new(Vec::new()));

    let handle = host.mount("OrderTest", OrderProps { log: log.clone() }, order_comp);

    host.run_until_idle();
    assert!(log.borrow().is_empty());

    handle.unmount();
    assert_eq!(
        *log.borrow(),
        vec![3, 2, 1],
        "cleanups must run in reverse registration order (LIFO)"
    );
}

#[derive(Clone)]
struct RerunProps {
    runs: Rc<Cell<usize>>,
    cleanups: Rc<Cell<usize>>,
    count: u32,
}
impl Props for RerunProps {}

fn rerun_comp(ctx: &Ctx, props: &RerunProps) -> VNode {
    let _ = props.count;
    props.runs.set(props.runs.get() + 1);
    let c = props.cleanups.clone();
    ctx.on_cleanup(move || {
        c.set(c.get() + 1);
    });
    VNode::Hole
}

#[test]
fn component_cleanup_runs_before_rerun() {
    let host = ComponentHost::new();
    let runs = Rc::new(Cell::new(0));
    let cleanups = Rc::new(Cell::new(0));

    let handle = host.mount(
        "RerunTest",
        RerunProps {
            runs: runs.clone(),
            cleanups: cleanups.clone(),
            count: 0,
        },
        rerun_comp,
    );

    host.run_until_idle();
    assert_eq!(runs.get(), 1);
    assert_eq!(cleanups.get(), 0);

    handle.set_props(RerunProps {
        runs: runs.clone(),
        cleanups: cleanups.clone(),
        count: 1,
    });
    host.run_until_idle();
    assert_eq!(runs.get(), 2);
    assert_eq!(
        cleanups.get(),
        1,
        "cleanups from previous run must execute before rerun"
    );

    handle.unmount();
    assert_eq!(
        cleanups.get(),
        2,
        "cleanups from last run execute on unmount"
    );
}

#[derive(Clone)]
struct ChildProps {
    log: Rc<RefCell<Vec<&'static str>>>,
}
impl Props for ChildProps {}

fn child_comp(ctx: &Ctx, props: &ChildProps) -> VNode {
    let l = props.log.clone();
    ctx.on_cleanup(move || l.borrow_mut().push("child"));
    VNode::Hole
}

#[derive(Clone)]
struct ParentProps {
    log: Rc<RefCell<Vec<&'static str>>>,
}
impl Props for ParentProps {}

fn parent_comp(ctx: &Ctx, props: &ParentProps) -> VNode {
    let l = props.log.clone();
    ctx.on_cleanup(move || l.borrow_mut().push("parent"));
    ctx.child(
        "Child",
        1,
        &ChildProps {
            log: props.log.clone(),
        },
        child_comp,
    )
}

#[test]
fn child_cleanup_runs_recursively_on_parent_unmount() {
    let host = ComponentHost::new();
    let log = Rc::new(RefCell::new(Vec::new()));

    let handle = host.mount("Parent", ParentProps { log: log.clone() }, parent_comp);

    host.run_until_idle();
    assert!(log.borrow().is_empty());

    handle.unmount();
    assert_eq!(*log.borrow(), vec!["child", "parent"]);
}

#[derive(Clone)]
struct LifecycleProps {
    state_log: Rc<RefCell<Vec<crate::shell::AppLifecycleState>>>,
}
impl Props for LifecycleProps {}

fn lifecycle_comp(ctx: &Ctx, props: &LifecycleProps) -> VNode {
    let state = ctx.lifecycle().get();
    props.state_log.borrow_mut().push(state);
    VNode::Hole
}

#[test]
fn close_request_flag_sets_and_drains_once() {
    // Round 26.2 (decision 342): plain flag, never reactive --
    // set reads back once, the second drain is false (never a
    // stuck exit), and re-requesting re-arms.
    let host = ComponentHost::new();
    assert!(!host.take_close_request(), "unset drains false");
    host.request_close();
    assert!(host.take_close_request(), "request drains true");
    assert!(!host.take_close_request(), "drain consumes");
    host.request_close();
    assert!(host.take_close_request(), "re-request re-arms");
}

#[test]
fn ctx_request_close_forwards_to_host_flag() {
    // The component-facing half: a body calling
    // `ctx.request_close()` arms the same flag the runners
    // drain (File/Exit menu shape without a press rig).
    #[derive(Clone)]
    struct Closer;
    impl Props for Closer {}
    fn closer(ctx: &Ctx, _: &Closer) -> VNode {
        ctx.request_close();
        VNode::Hole
    }
    let host = ComponentHost::new();
    host.mount("Closer", Closer, closer);
    host.run_until_idle();
    assert!(host.take_close_request(), "ctx call arms the flag");
}

#[test]
fn system_bars_visible_defaults_hidden_and_drains_once() {
    // Android screen-mode toggle: the boot mode is hidden/immersive,
    // so the signal defaults to false; a set arms exactly one runner
    // command (last write wins, drain consumes, re-request re-arms).
    let host = ComponentHost::new();
    assert_eq!(host.take_system_bars_request(), None, "unset drains none");
    let visible = host.system_bars_visible();
    assert!(
        !crate::reactive::untrack(|| visible.get()),
        "boot mode is hidden"
    );
    host.set_system_bars_visible(true);
    assert!(
        crate::reactive::untrack(|| visible.get()),
        "signal follows the set"
    );
    assert_eq!(
        host.take_system_bars_request(),
        Some(true),
        "request drains once"
    );
    assert_eq!(host.take_system_bars_request(), None, "drain consumes");
    host.set_system_bars_visible(true);
    host.set_system_bars_visible(false);
    assert_eq!(
        host.take_system_bars_request(),
        Some(false),
        "last write wins"
    );
}

#[test]
fn selection_handles_visible_defaults_on_and_sets() {
    // Mobile affordance flag (decision 474): publication defaults
    // on (current behavior everywhere until a runner opts out);
    // desktop Windows/Linux runners set it false at startup, and
    // flips apply on the next publish with no re-render owed.
    let host = ComponentHost::new();
    assert!(
        host.selection_handles_visible(),
        "handles publish by default"
    );
    host.set_selection_handles_visible(false);
    assert!(!host.selection_handles_visible(), "runner opt-out sticks");
    host.set_selection_handles_visible(true);
    assert!(host.selection_handles_visible(), "restore works");
}

/// Decision 486: the caption hover signal defaults to no hover;
/// runner publishes follow change-only (repeats never invalidate —
/// hovering must not spin the loop).
#[test]
fn caption_hover_defaults_none_and_publishes_change_only() {
    use crate::CaptionButton;
    let host = ComponentHost::new();
    let hover = host.caption_hover();
    assert_eq!(
        crate::reactive::untrack(|| hover.get()),
        None,
        "boot hover is none"
    );
    host.set_caption_hover(Some(CaptionButton::Maximize));
    assert_eq!(
        crate::reactive::untrack(|| hover.get()),
        Some(CaptionButton::Maximize),
        "signal follows the publish"
    );
    host.set_caption_hover(None);
    assert_eq!(
        crate::reactive::untrack(|| hover.get()),
        None,
        "clearing reverts"
    );
}

/// Decision 495: the maximized signal defaults to windowed;
/// runner publishes follow change-only (repeats never invalidate —
/// polling must not spin the loop).
#[test]
fn maximized_defaults_false_and_publishes_change_only() {
    let host = ComponentHost::new();
    let maximized = host.maximized();
    assert!(
        !crate::reactive::untrack(|| maximized.get()),
        "boot state is windowed"
    );
    host.set_maximized(true);
    assert!(
        crate::reactive::untrack(|| maximized.get()),
        "signal follows the publish"
    );
    host.set_maximized(false);
    assert!(
        !crate::reactive::untrack(|| maximized.get()),
        "restoring reverts"
    );
}

#[test]
fn custom_theme_tokens_override_per_mode_and_republish() {
    // Decision 477: per-mode custom palettes — unset slots fall
    // back to built-in; publishing re-renders themed bodies in
    // place like a mode toggle (instances survive).
    use crate::{Color, ThemeMode, ThemeTokens};
    let host = ComponentHost::new();
    assert_eq!(
        host.theme_tokens(),
        ThemeTokens::of(ThemeMode::Light),
        "unset override is the built-in palette"
    );
    let custom = ThemeTokens {
        primary: Color(0x12_34_56),
        ..ThemeTokens::dark()
    };
    host.set_theme_tokens_override(ThemeMode::Dark, Some(custom));
    // Light mode is untouched by a Dark publish.
    assert_eq!(
        host.theme_tokens(),
        ThemeTokens::of(ThemeMode::Light),
        "other mode keeps built-in"
    );
    host.set_theme(ThemeMode::Dark);
    host.run_until_idle();
    assert_eq!(host.theme_tokens(), custom, "live mode reads the override");
    // Tracked republish: a body reading tokens() re-runs on publish.
    #[derive(Clone)]
    struct Inker {
        seen: crate::Signal<Color>,
    }
    impl Props for Inker {}
    fn inker(ctx: &Ctx, props: &Inker) -> VNode {
        props.seen.set(ctx.theme().tokens().primary);
        VNode::Hole
    }
    let seen = host.runtime().signal(Color(0x00_00_00));
    host.mount("Inker", Inker { seen: seen.clone() }, inker);
    host.run_until_idle();
    assert_eq!(
        crate::reactive::untrack(|| seen.get()),
        custom.primary,
        "body reads the override"
    );
    let custom2 = ThemeTokens {
        primary: Color(0x65_43_21),
        ..ThemeTokens::dark()
    };
    host.set_theme_tokens_override(ThemeMode::Dark, Some(custom2));
    host.run_until_idle();
    assert_eq!(
        crate::reactive::untrack(|| seen.get()),
        custom2.primary,
        "republish re-renders themed bodies in place"
    );
    // Clearing restores the built-in palette.
    host.set_theme_tokens_override(ThemeMode::Dark, None);
    host.run_until_idle();
    assert_eq!(
        host.theme_tokens(),
        ThemeTokens::of(ThemeMode::Dark),
        "clear restores built-in"
    );
}

#[test]
fn ctx_system_bars_toggle_forwards_to_host() {
    // The component-facing half: a body calling
    // `ctx.set_system_bars_visible(true)` writes the same signal the
    // runner drains (show-then-hide shape without a device).
    #[derive(Clone)]
    struct Shower;
    impl Props for Shower {}
    fn shower(ctx: &Ctx, _: &Shower) -> VNode {
        ctx.set_system_bars_visible(true);
        VNode::Hole
    }
    let host = ComponentHost::new();
    host.mount("Shower", Shower, shower);
    host.run_until_idle();
    assert!(crate::reactive::untrack(|| host
        .system_bars_visible()
        .get()));
    assert_eq!(
        host.take_system_bars_request(),
        Some(true),
        "ctx call arms the command"
    );
}

#[test]
fn component_lifecycle_signal_updates_and_suspends_tickers() {
    use crate::shell::AppLifecycleState;
    let host = ComponentHost::new();
    let log = Rc::new(RefCell::new(Vec::new()));

    let handle = host.mount(
        "LifecycleTester",
        LifecycleProps {
            state_log: log.clone(),
        },
        lifecycle_comp,
    );

    host.run_until_idle();
    assert_eq!(*log.borrow(), vec![AppLifecycleState::Active]);
    assert!(!host.is_lifecycle_suspended());

    // Simulate pause (APP_CMD_PAUSE)
    host.set_lifecycle(AppLifecycleState::Paused);
    host.run_until_idle();
    assert_eq!(
        *log.borrow(),
        vec![AppLifecycleState::Active, AppLifecycleState::Paused]
    );
    assert!(host.is_lifecycle_suspended());

    // While paused, tickers are suspended
    assert!(!host.tick_flings());

    // Simulate resume (APP_CMD_RESUME)
    host.set_lifecycle(AppLifecycleState::Active);
    host.run_until_idle();
    assert_eq!(
        *log.borrow(),
        vec![
            AppLifecycleState::Active,
            AppLifecycleState::Paused,
            AppLifecycleState::Active
        ]
    );
    assert!(!host.is_lifecycle_suspended());

    handle.unmount();
}

// -- Round 21.1 (decision 328): per-component timers ------------

use crate::clock::MockClock;

#[derive(Clone)]
struct TimeoutProps {
    fired: Rc<Cell<usize>>,
    slot: Rc<Cell<Option<TimerId>>>,
}
impl Props for TimeoutProps {}

fn timeout_comp(ctx: &Ctx, props: &TimeoutProps) -> VNode {
    let f = props.fired.clone();
    let id = ctx.use_timeout(100.0, move || {
        f.set(f.get() + 1);
    });
    props.slot.set(Some(id));
    VNode::Hole
}

/// `use_timeout` fires once at `now + delay_ms` and never again;
/// the wake query names the due instant; early cancel works and
/// double-cancel is quiet.
#[test]
fn timeout_fires_once_at_delay() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    let fired = Rc::new(Cell::new(0));
    let slot = Rc::new(Cell::new(None));
    let _handle = host.mount(
        "Timeout",
        TimeoutProps {
            fired: fired.clone(),
            slot: slot.clone(),
        },
        timeout_comp,
    );
    host.run_until_idle();
    assert_eq!(
        host.next_timer_due_ms(host.now_ms()),
        Some(100.0),
        "wake query names the due instant"
    );
    assert_eq!(host.tick_timers(host.now_ms()), 0, "not due at t=0");
    clock.advance(0.099);
    assert_eq!(host.tick_timers(host.now_ms()), 0, "not due at t=99ms");
    clock.advance(0.001);
    assert_eq!(host.tick_timers(host.now_ms()), 1, "fires at t=100ms");
    assert_eq!(fired.get(), 1);
    assert_eq!(
        host.next_timer_due_ms(host.now_ms()),
        None,
        "consumed one-shot leaves no horizon"
    );
    clock.advance(10.0);
    assert_eq!(host.tick_timers(host.now_ms()), 0, "one-shot never refires");
    assert_eq!(fired.get(), 1);
    let id = slot.get().expect("render published its id");
    assert!(!host.cancel_timer(id), "double-cancel after fire is quiet");
}

/// Early cancel before the due instant: the entry dies and the
/// horizon clears.
#[test]
fn timeout_early_cancel_kills_before_due() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    let fired = Rc::new(Cell::new(0));
    let slot = Rc::new(Cell::new(None));
    let _handle = host.mount(
        "Timeout",
        TimeoutProps {
            fired: fired.clone(),
            slot: slot.clone(),
        },
        timeout_comp,
    );
    host.run_until_idle();
    let id = slot.get().expect("render published its id");
    assert!(host.cancel_timer(id), "live entry cancels");
    assert_eq!(
        host.next_timer_due_ms(host.now_ms()),
        None,
        "cancelled entry leaves no horizon"
    );
    clock.advance(10.0);
    assert_eq!(host.tick_timers(host.now_ms()), 0, "cancelled never fires");
    assert_eq!(fired.get(), 0);
}

#[derive(Clone)]
struct IntervalProps {
    fired: Rc<Cell<usize>>,
}
impl Props for IntervalProps {}

fn interval_comp(ctx: &Ctx, props: &IntervalProps) -> VNode {
    let f = props.fired.clone();
    ctx.use_interval(50.0, move || {
        f.set(f.get() + 1);
    });
    VNode::Hole
}

/// `use_interval` fires at steady cadence; a long gap fires
/// once and snaps forward (no catch-up burst).
#[test]
fn interval_fires_at_cadence_without_burst() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    let fired = Rc::new(Cell::new(0));
    let _handle = host.mount(
        "Interval",
        IntervalProps {
            fired: fired.clone(),
        },
        interval_comp,
    );
    host.run_until_idle();
    clock.advance(0.05);
    assert_eq!(host.tick_timers(host.now_ms()), 1);
    assert_eq!(fired.get(), 1);
    clock.advance(0.05);
    assert_eq!(host.tick_timers(host.now_ms()), 1);
    assert_eq!(fired.get(), 2);
    // Jump 120ms past a 50ms cadence: exactly one fire, then
    // the horizon snaps to now + period (270ms), not 200ms.
    clock.advance(0.12);
    assert_eq!(
        host.tick_timers(host.now_ms()),
        1,
        "gap fires once, never bursts"
    );
    assert_eq!(fired.get(), 3);
    assert_eq!(
        host.next_timer_due_ms(host.now_ms()),
        Some(270.0),
        "cadence snaps forward past the gap"
    );
    assert_eq!(
        host.tick_timers(host.now_ms()),
        0,
        "snapped horizon is quiet"
    );
}

/// Unmounting cancels active timers immediately (the 18.2
/// cleanup path owns this — no tick after unmount ever fires).
#[test]
fn unmount_cancels_active_timers() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    let fired = Rc::new(Cell::new(0));
    let slot = Rc::new(Cell::new(None));
    let handle = host.mount(
        "Timeout",
        TimeoutProps {
            fired: fired.clone(),
            slot: slot.clone(),
        },
        timeout_comp,
    );
    host.run_until_idle();
    assert_eq!(host.next_timer_due_ms(host.now_ms()), Some(100.0));
    handle.unmount();
    assert_eq!(
        host.next_timer_due_ms(host.now_ms()),
        None,
        "unmount clears the horizon"
    );
    clock.advance(10.0);
    assert_eq!(host.tick_timers(host.now_ms()), 0, "unmounted never fires");
    assert_eq!(fired.get(), 0);
}

/// `Paused`/`Suspended` freezes timer firing and hides the wake
/// horizon; resuming fires the frozen entries (held, not dropped).
#[test]
fn suspended_lifecycle_freezes_timers() {
    use crate::shell::AppLifecycleState;
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    let fired = Rc::new(Cell::new(0));
    let slot = Rc::new(Cell::new(None));
    let _handle = host.mount(
        "Timeout",
        TimeoutProps {
            fired: fired.clone(),
            slot: slot.clone(),
        },
        timeout_comp,
    );
    host.run_until_idle();
    host.set_lifecycle(AppLifecycleState::Paused);
    host.run_until_idle();
    clock.advance(10.0);
    assert_eq!(host.tick_timers(host.now_ms()), 0, "paused fires nothing");
    assert_eq!(fired.get(), 0);
    assert_eq!(
        host.next_timer_due_ms(host.now_ms()),
        None,
        "paused hides the horizon"
    );
    host.set_lifecycle(AppLifecycleState::Suspended);
    assert_eq!(
        host.tick_timers(host.now_ms()),
        0,
        "suspended fires nothing"
    );
    host.set_lifecycle(AppLifecycleState::Active);
    host.run_until_idle();
    assert_eq!(
        host.tick_timers(host.now_ms()),
        1,
        "resume fires the frozen entry"
    );
    assert_eq!(fired.get(), 1);
}

#[derive(Clone)]
struct RerenderProps {
    fired: Rc<Cell<usize>>,
    epoch: Signal<u32>,
}
impl Props for RerenderProps {}

fn rerender_comp(ctx: &Ctx, props: &RerenderProps) -> VNode {
    let _ = props.epoch.get();
    let f = props.fired.clone();
    ctx.use_timeout(100.0, move || {
        f.set(f.get() + 1);
    });
    VNode::Hole
}

/// Re-rendering cancels the previous render's timers (per-render
/// ownership): the first render's timeout dies on the second
/// render, and only the second render's timeout fires.
#[test]
fn rerender_cancels_previous_timers() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    let fired = Rc::new(Cell::new(0));
    let epoch = host.runtime().signal(0u32);
    let _handle = host.mount(
        "Rerender",
        RerenderProps {
            fired: fired.clone(),
            epoch: epoch.clone(),
        },
        rerender_comp,
    );
    host.run_until_idle();
    // Re-render at t=50ms through the tracked epoch: render 1's
    // timeout (due 100) dies with it; render 2's is due at 150.
    clock.advance(0.05);
    epoch.set(1);
    host.run_until_idle();
    assert_eq!(
        host.next_timer_due_ms(host.now_ms()),
        Some(150.0),
        "only the latest render owns a timer"
    );
    clock.advance(0.05);
    assert_eq!(
        host.tick_timers(host.now_ms()),
        0,
        "cancelled render-1 timeout never fires at t=100"
    );
    assert_eq!(fired.get(), 0);
    clock.advance(0.05);
    assert_eq!(
        host.tick_timers(host.now_ms()),
        1,
        "render-2 timeout fires at t=150"
    );
    assert_eq!(fired.get(), 1);
}

// -- Round 23.1 (decision 333): per-key granular subscriptions --

#[derive(Clone)]
struct KeyReaderProps {
    key: String,
    runs: Rc<Cell<usize>>,
    store: Store<String, String>,
}
impl Props for KeyReaderProps {}

fn key_reader_comp(_ctx: &Ctx, props: &KeyReaderProps) -> VNode {
    props.runs.set(props.runs.get() + 1);
    let _ = props.store.get_keyed(&props.key).get();
    VNode::Hole
}

#[derive(Clone)]
struct LenReaderProps {
    runs: Rc<Cell<usize>>,
    store: Store<String, String>,
}
impl Props for LenReaderProps {}

fn len_reader_comp(_ctx: &Ctx, props: &LenReaderProps) -> VNode {
    props.runs.set(props.runs.get() + 1);
    let _ = props.store.len();
    VNode::Hole
}

/// `insert` on an existing key notifies only that key's
/// readers (no version fan-out); inserting a new key bumps
/// the version (structural readers re-run); wholesale `set`
/// re-runs everyone with synced values.
#[test]
fn store_insert_existing_notifies_only_that_key() {
    use std::collections::HashMap;
    let host = ComponentHost::new();
    let rt = host.runtime();
    let store = Store::new(
        &rt,
        vec!["a".to_string(), "b".to_string()],
        HashMap::from([
            ("a".to_string(), "A".to_string()),
            ("b".to_string(), "B".to_string()),
        ]),
    );
    let runs_a = Rc::new(Cell::new(0));
    let runs_b = Rc::new(Cell::new(0));
    let runs_len = Rc::new(Cell::new(0));
    let _ha = host.mount(
        "ReadA",
        KeyReaderProps {
            key: "a".to_string(),
            runs: runs_a.clone(),
            store: store.clone(),
        },
        key_reader_comp,
    );
    let _hb = host.mount(
        "ReadB",
        KeyReaderProps {
            key: "b".to_string(),
            runs: runs_b.clone(),
            store: store.clone(),
        },
        key_reader_comp,
    );
    let _hl = host.mount(
        "ReadLen",
        LenReaderProps {
            runs: runs_len.clone(),
            store: store.clone(),
        },
        len_reader_comp,
    );
    host.run_until_idle();
    assert_eq!((runs_a.get(), runs_b.get(), runs_len.get()), (1, 1, 1));
    // Existing-key write: only a's reader re-runs.
    store.insert("a".to_string(), "A2".to_string());
    host.run_until_idle();
    assert_eq!(
        (runs_a.get(), runs_b.get(), runs_len.get()),
        (2, 1, 1),
        "one key render, zero sibling/len re-renders"
    );
    // Unknown-key waiter: seeds None, notifies on insert.
    let runs_c = Rc::new(Cell::new(0));
    let seen_c = Rc::new(RefCell::new(Vec::new()));
    #[derive(Clone)]
    struct WaiterProps {
        runs: Rc<Cell<usize>>,
        seen: Rc<RefCell<Vec<Option<String>>>>,
        store: Store<String, String>,
    }
    impl Props for WaiterProps {}
    fn waiter_comp(_ctx: &Ctx, props: &WaiterProps) -> VNode {
        props.runs.set(props.runs.get() + 1);
        props
            .seen
            .borrow_mut()
            .push(props.store.get_keyed(&"c".to_string()).get());
        VNode::Hole
    }
    let _hw = host.mount(
        "WaitC",
        WaiterProps {
            runs: runs_c.clone(),
            seen: seen_c.clone(),
            store: store.clone(),
        },
        waiter_comp,
    );
    host.run_until_idle();
    assert_eq!(seen_c.borrow().last(), Some(&None), "unknown seeds None");
    // New-key insert: structural fan-out (len re-runs) + the
    // waiter observes; existing key readers stay quiet.
    store.insert("c".to_string(), "C".to_string());
    host.run_until_idle();
    assert_eq!(
        (runs_a.get(), runs_b.get(), runs_len.get()),
        (2, 1, 2),
        "new keys fan out structurally, never per-key"
    );
    assert_eq!(
        seen_c.borrow().last(),
        Some(&Some("C".to_string())),
        "waiter observes the insert"
    );
    // Wholesale set: everyone re-runs with synced values.
    store.set(
        vec!["a".to_string()],
        HashMap::from([("a".to_string(), "A3".to_string())]),
    );
    host.run_until_idle();
    assert_eq!(
        (runs_a.get(), runs_b.get(), runs_len.get()),
        (3, 2, 3),
        "set re-runs every reader"
    );
}

#[derive(Clone)]
struct TwoDProps;
impl Props for TwoDProps {}

/// 2D scroll area: 200×40 viewport over a 300×200 sheet (x bound
/// `[0, 100]`, y bound `[0, 160]`), owned through one 2D handle.
fn twod_comp(ctx: &Ctx, _: &TwoDProps) -> VNode {
    let _both = ctx.scroll_2d();
    crate::vnode::ScrollArea("sheet")
        .style(crate::style::Style::new().size(200, 40))
        .on_scroll(|| {})
        .child(
            crate::vnode::Div("sheet-wide")
                .style(crate::style::Style::new().size(300, 200))
                .build(),
        )
}

#[test]
fn scroll_2d_shares_state_with_the_1d_twins() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let handle = host.mount("TwoD", TwoDProps, twod_comp);
    host.run_until_idle();
    let root = handle.root_instance();
    let both = host.instance_scroll_2d(root).expect("2D handle");
    // Same signals, never forked: 1D writes read back 2D and back.
    let y = host.instance_scroll(root).expect("y twin");
    let x = host.instance_scroll_x(root).expect("x twin");
    y.set(12.0);
    x.set(34.0);
    assert_eq!(both.get(), ScrollXY { x: 34.0, y: 12.0 });
    both.set(ScrollXY { x: 1.0, y: 2.0 });
    assert_eq!((x.get(), y.get()), (1.0, 2.0));
    assert_eq!(both.x().get(), 1.0);
    assert_eq!(both.y().get(), 2.0);
}

#[test]
fn bind_scroll_2d_feeds_both_axes_and_reports_both() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let handle = host.mount("TwoD", TwoDProps, twod_comp);
    host.run_until_idle();
    let root = handle.root_instance();
    let both = host.instance_scroll_2d(root).expect("2D handle");
    let sheet = crate::find_retained_by_debug(&host, "sheet")[0];
    host.bind_scroll_2d(sheet, &both);
    assert_eq!(
        host.bound_scroll_2d(sheet),
        Some(ScrollXY { x: 0.0, y: 0.0 })
    );
    host.inject_input(crate::input::InputEvent::Scroll {
        target: sheet,
        dx: 30.0,
        dy: 50.0,
    });
    host.run_until_idle();
    assert_eq!(both.get(), ScrollXY { x: 30.0, y: 50.0 });
    assert_eq!(
        host.bound_scroll_2d(sheet),
        Some(ScrollXY { x: 30.0, y: 50.0 })
    );
    // Half-bound targets report None (a half-wired 2D area is a
    // wiring bug, never a silent half-read).
    let host2 = ComponentHost::new();
    host2.set_viewport(800.0, 600.0);
    host2.mount("TwoD", TwoDProps, twod_comp);
    host2.run_until_idle();
    let sheet2 = crate::find_retained_by_debug(&host2, "sheet")[0];
    assert_eq!(host2.bound_scroll_2d(sheet2), None);
}

#[test]
fn unbound_dx_self_wires_to_the_owner_x_offset() {
    // Round 24.2 transposed (decision 354): no `bind_scroll_x`
    // anywhere, but the target's Scroll owner holds a `scroll_x`
    // offset — `dx` feeds it, clamped to the `content_w`
    // overflow; narrow content pins at rest.
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let handle = host.mount("TwoD", TwoDProps, twod_comp);
    host.run_until_idle();
    let root = handle.root_instance();
    let both = host.instance_scroll_2d(root).expect("2D handle");
    let sheet = crate::find_retained_by_debug(&host, "sheet")[0];
    assert_eq!(host.bound_scroll_x(sheet), None, "no bound feed");
    host.inject_input(crate::input::InputEvent::Scroll {
        target: sheet,
        dx: 30.0,
        dy: 0.0,
    });
    host.run_until_idle();
    assert_eq!(both.get().x, 30.0, "owner x self-wires");
    assert_eq!(both.get().y, 0.0, "dy untouched");
    host.inject_input(crate::input::InputEvent::Scroll {
        target: sheet,
        dx: 500.0,
        dy: 0.0,
    });
    host.run_until_idle();
    assert_eq!(both.get().x, 100.0, "clamps to content_w - w");
}

/// Phase 36 PR4 (decision 359): pre-decoded pixels deposit once
/// and pull back byte-exact; short buffers, zero sizes, and
/// unknown ids refuse loudly.
#[test]
fn image_cache_pixels_deposit_once_and_pull_exact() {
    let cache = crate::ImageCache::new();
    let rgba = vec![255u8; 2 * 2 * 4];
    let id = cache.insert_pixels("dot", 2, 2, rgba.clone());
    assert_eq!(cache.pixels_of(id), Some((2, 2, rgba)));
    // Same key re-deposit replaces (last wins, same id).
    let id2 = cache.insert_pixels("dot", 1, 1, vec![0u8; 4]);
    assert_eq!(id, id2);
    assert_eq!(cache.pixels_of(id), Some((1, 1, vec![0u8; 4])));
    // Bare load() keys carry no pixels (URL path, unchanged).
    let url = cache.load("img/a.png");
    assert_eq!(cache.pixels_of(url), None);
    assert_eq!(cache.key_of(url).as_deref(), Some("img/a.png"));
}

#[test]
#[should_panic(expected = "bytes !=")]
fn image_cache_short_buffer_refuses_loudly() {
    crate::ImageCache::new().insert_pixels("short", 2, 2, vec![0u8; 15]);
}

#[test]
#[should_panic(expected = "zero size")]
fn image_cache_zero_size_refuses_loudly() {
    crate::ImageCache::new().insert_pixels("zero", 0, 2, vec![]);
}

#[derive(Clone)]
struct KidProps {
    seed: u32,
}
impl Props for KidProps {}

fn kid_comp(ctx: &Ctx, props: &KidProps) -> VNode {
    let s = ctx.signal(props.seed);
    // Publish the instance signal value into the debug label so
    // the test reads state identity off the retained tree.
    crate::vnode::Div(format!("kid-{}", s.get()).as_str()).build()
}

#[derive(Clone)]
struct AutoRootProps;
impl Props for AutoRootProps {}

fn auto_root_comp(ctx: &Ctx, _: &AutoRootProps) -> VNode {
    let _ = ctx.signal(0u32);
    crate::vnode::Div("auto-root").children([
        ctx.child_auto(&KidProps { seed: 1 }, kid_comp),
        ctx.child_auto(&KidProps { seed: 2 }, kid_comp),
        ctx.child_keyed(7, &KidProps { seed: 3 }, kid_comp),
        ctx.child_keyed(8, &KidProps { seed: 4 }, kid_comp),
    ])
}

/// Phase 37a (decision 360): `child_auto` siblings hold distinct
/// instances with isolated state (no manual strings/ordinals),
/// and `child_keyed` siblings key by the explicit key.
#[test]
fn child_auto_and_keyed_hold_distinct_state() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.mount("AutoRoot", AutoRootProps, auto_root_comp);
    host.run_until_idle();
    for want in ["kid-1", "kid-2", "kid-3", "kid-4"] {
        assert_eq!(
            crate::find_retained_by_debug(&host, want).len(),
            1,
            "one live {want}"
        );
    }
}

/// Phase 37a: auto/keyed instances survive re-renders (stable
/// call-site keying — same body, same instances, no re-seed).
#[test]
fn child_auto_instances_survive_reruns() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let handle = host.mount("AutoRoot", AutoRootProps, auto_root_comp);
    host.run_until_idle();
    let before: Vec<crate::arena::NodeId> = ["kid-1", "kid-2", "kid-3", "kid-4"]
        .iter()
        .flat_map(|d| crate::find_retained_by_debug(&host, d))
        .collect();
    assert_eq!(before.len(), 4);
    // Re-run the root (props unchanged — force via set_props).
    handle.set_props(AutoRootProps);
    host.run_until_idle();
    let after: Vec<crate::arena::NodeId> = ["kid-1", "kid-2", "kid-3", "kid-4"]
        .iter()
        .flat_map(|d| crate::find_retained_by_debug(&host, d))
        .collect();
    assert_eq!(before, after, "stable bodies re-key identically");
}

#[derive(Clone)]
struct PlainProps;
impl Props for PlainProps {}

fn simple_app(_ctx: &Ctx, _props: &PlainProps) -> VNode {
    crate::vnode::Div("main_content").build()
}

#[test]
fn debug_overlay_wraps_root_when_enabled() {
    let host = ComponentHost::new();
    host.set_presenter_name("TestPresenter");
    host.set_debug_overlay(crate::debug_overlay::DebugOverlayConfig::all());
    let _handle = host.mount("Simple", PlainProps, simple_app);
    host.run_until_idle();

    // Verify both the user root and the overlay stack are mounted
    let debug_root = crate::find_retained_by_debug(&host, "oppa_debug_root");
    let overlay = crate::find_retained_by_debug(&host, "oppa_debug_overlay");
    let main_content = crate::find_retained_by_debug(&host, "main_content");

    assert!(!debug_root.is_empty(), "debug root is inserted");
    assert!(!overlay.is_empty(), "Debug overlay badge is rendered");
    assert!(!main_content.is_empty(), "User root content is preserved");
}

#[test]
fn debug_overlay_does_not_wrap_root_when_disabled() {
    let host = ComponentHost::new();
    host.set_debug_overlay(crate::debug_overlay::DebugOverlayConfig::disabled());
    let _handle = host.mount("Simple", PlainProps, simple_app);
    host.run_until_idle();

    let debug_root = crate::find_retained_by_debug(&host, "oppa_debug_root");
    let overlay = crate::find_retained_by_debug(&host, "oppa_debug_overlay");
    let main_content = crate::find_retained_by_debug(&host, "main_content");

    assert!(
        debug_root.is_empty(),
        "No stack root inserted when disabled"
    );
    assert!(
        overlay.is_empty(),
        "No overlay badge rendered when disabled"
    );
    assert!(
        !main_content.is_empty(),
        "User root content is rendered bare"
    );
}

fn centered_app(_ctx: &Ctx, _props: &PlainProps) -> VNode {
    crate::vnode::Column::new()
        .style(crate::style::Style::new().align_items(crate::style::AlignItems::Center))
        .child(
            crate::vnode::Div("centered_plate")
                .style(crate::style::Style::new().size(100, 20))
                .build(),
        )
}

/// The overlay wrapper is layout-transparent: a centered app keeps its
/// centered boxes with the overlay on (a `Stack` wrapper would measure
/// the app intrinsically and left-align it), and the badge hugs its
/// text at the top-left instead of stretching full-width.
#[test]
fn debug_overlay_preserves_centered_app_layout() {
    fn plate_and_badge(overlay_on: bool) -> (f32, Option<(f32, f32, f32)>) {
        let host = ComponentHost::new();
        host.set_viewport(600.0, 400.0);
        if overlay_on {
            host.set_presenter_name("TestPresenter");
            host.set_debug_overlay(crate::debug_overlay::DebugOverlayConfig::all());
        } else {
            host.set_debug_overlay(crate::debug_overlay::DebugOverlayConfig::disabled());
        }
        let _handle = host.mount("Centered", PlainProps, centered_app);
        host.run_until_idle();
        let plate = crate::find_retained_by_debug(&host, "centered_plate")
            .into_iter()
            .next()
            .expect("plate retained");
        let plate_box = host.committed_box(plate).expect("plate laid out");
        let badge = crate::find_retained_by_debug(&host, "oppa_debug_overlay")
            .into_iter()
            .next()
            .and_then(|id| host.committed_box(id))
            .map(|b| (b.x, b.y, b.w));
        (plate_box.x, badge)
    }

    let (bare_x, bare_badge) = plate_and_badge(false);
    assert_eq!(bare_x, 250.0, "bare root centers the 100px plate");
    assert!(bare_badge.is_none(), "no badge when disabled");
    let (overlay_x, overlay_badge) = plate_and_badge(true);
    assert_eq!(
        overlay_x, bare_x,
        "overlay on keeps the plate centered (got {overlay_x}, bare {bare_x})"
    );
    let (bx, by, bw) = overlay_badge.expect("badge rendered when enabled");
    assert_eq!((bx, by), (8.0, 8.0), "badge floats top-left");
    assert!(bw < 100.0, "badge hugs its text, got w={bw}");
}

/// The FPS clock ticks on presents, never on reconciles: an
/// interaction after a long idle gap must not divide by the gap and
/// print 0 FPS (the benchmark-idle artifact). Reconciles record
/// commit dirt only.
#[test]
fn overlay_meter_clock_ignores_reconcile_gaps() {
    let clock = std::rc::Rc::new(crate::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_debug_overlay(crate::debug_overlay::DebugOverlayConfig::all());
    let _handle = host.mount("Simple", PlainProps, simple_app);
    host.run_until_idle();
    // Two presents 100ms apart: a ~10 FPS present cadence.
    host.note_overlay_frame();
    clock.advance(0.1);
    host.note_overlay_frame();
    let fps = host.inner.debug_overlay.borrow().fps;
    assert!(
        fps > 5.0 && fps < 40.0,
        "present cadence reads sane, got {fps}"
    );
    // A reconcile after a 13s idle gap (no presents in between)
    // records dirt but leaves the clock alone.
    clock.advance(13.0);
    host.mark_all_component_effects_dirty();
    host.run_until_idle();
    assert_eq!(
        host.inner.debug_overlay.borrow().fps,
        fps,
        "idle-gap reconcile leaves FPS alone"
    );
    assert!(
        host.inner.debug_overlay.borrow().dirty_nodes >= 1,
        "the refresh commit carries the badge text update itself"
    );
}

/// The badge text refreshes through a throttled root re-run (4Hz):
/// rapid presents only tick the clock, while a present past the
/// throttle rebuilds the badge from the live meter.
#[test]
fn overlay_badge_refresh_reruns_root_throttled() {
    let clock = std::rc::Rc::new(crate::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_presenter_name("TestPresenter");
    host.set_debug_overlay(crate::debug_overlay::DebugOverlayConfig::all());
    let _handle = host.mount("Simple", PlainProps, simple_app);
    host.run_until_idle();
    host.note_overlay_frame();
    assert_eq!(
        host.run_until_idle(),
        0,
        "no root re-run inside the throttle window"
    );
    clock.advance(0.3);
    host.note_overlay_frame();
    assert!(
        host.run_until_idle() > 0,
        "throttled present schedules the badge refresh"
    );
    let badge = host.debug_overlay_badge().expect("badge renders");
    assert!(
        badge.contains("17 FPS"),
        "badge rebuilt from present cadence (0.3s dt), got {badge:?}"
    );
    assert!(
        badge.contains("[TestPresenter]"),
        "presenter rides the refresh, got {badge:?}"
    );
}

// -- Decision 512: transition-completion observers ---------------

use crate::style::{Ease, Keyframes, Transform, Transition};

#[derive(Clone)]
struct SettleProps {
    dx: crate::reactive::Signal<f32>,
}
impl Props for SettleProps {}

fn settle_comp(_ctx: &Ctx, props: &SettleProps) -> VNode {
    let dx = props.dx.get();
    crate::vnode::Div("box")
        .style(
            Style::new()
                .size(100, 40)
                .transform(Transform::translate(dx, 0))
                .transition(Transition::new(120, Ease::Linear)),
        )
        .build()
}

fn settle_host() -> (ComponentHost, Rc<MockClock>, crate::reactive::Signal<f32>) {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let dx = host.runtime().signal(0.0f32);
    host.mount("Settle", SettleProps { dx: dx.clone() }, settle_comp);
    host.run_until_idle();
    (host, clock, dx)
}

fn box_id(host: &ComponentHost) -> crate::arena::NodeId {
    crate::find_retained_by_debug(host, "box")
        .into_iter()
        .next()
        .expect("box node retained")
}

/// Decision 512: observing a live Once track fires exactly once —
/// after the settle instant, never before, never twice.
#[test]
fn observe_settled_fires_once_after_settle() {
    let (host, clock, dx) = settle_host();
    let id = box_id(&host);
    dx.set(100.0);
    clock.advance(1.0 / 60.0);
    host.run_once();
    let fired = Rc::new(Cell::new(0));
    let f = fired.clone();
    host.observe_settled(id, move || f.set(f.get() + 1));
    // Mid-flight ticks never fire early.
    for _ in 0..5 {
        clock.advance(1.0 / 60.0);
        host.run_once();
    }
    assert_eq!(fired.get(), 0, "never fires mid-flight");
    // Past 120ms the settle tick fires exactly once.
    for _ in 0..10 {
        clock.advance(1.0 / 60.0);
        host.run_once();
    }
    assert_eq!(fired.get(), 1, "fires once after settle");
    for _ in 0..10 {
        clock.advance(1.0 / 60.0);
        host.run_once();
    }
    assert_eq!(fired.get(), 1, "one-shot never refires");
}

/// Decision 512: observing an already-settled node fires on the
/// next pump (uniform async contract — never synchronous).
#[test]
fn observe_settled_on_quiet_node_fires_next_pump() {
    let (host, clock, _dx) = settle_host();
    let id = box_id(&host);
    let fired = Rc::new(Cell::new(0));
    let f = fired.clone();
    host.observe_settled(id, move || f.set(f.get() + 1));
    assert_eq!(fired.get(), 0, "never synchronous");
    clock.advance(1.0 / 60.0);
    host.run_once();
    assert_eq!(fired.get(), 1, "fires on the next pump");
}

/// Decision 512: chains compose — a completion callback that starts
/// the next track (signal write in TIME context) drives a sequence.
#[test]
fn observe_settled_chains_drive_sequences() {
    let (host, clock, dx) = settle_host();
    let id = box_id(&host);
    let steps = Rc::new(Cell::new(0));
    dx.set(100.0);
    clock.advance(1.0 / 60.0);
    host.run_once();
    let (host2, s2) = (host.clone(), steps.clone());
    let dx2 = dx.clone();
    host.observe_settled(id, move || {
        s2.set(s2.get() + 1);
        // Chain link two starts from inside the completion (the
        // sequence pattern — TIME-context signal writes pump).
        host2.observe_settled(id, move || {});
        dx2.set(200.0);
    });
    // Settle link one (120ms), then link two starts and settles.
    for _ in 0..10 {
        clock.advance(1.0 / 60.0);
        host.run_once();
    }
    assert_eq!(steps.get(), 1, "link one completed");
    assert_eq!(dx.get(), 200.0, "link two started from the completion");
    for _ in 0..20 {
        clock.advance(1.0 / 60.0);
        host.run_once();
    }
    host.with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "chain settles to zero work");
    });
    assert!(!host.has_demand(), "quiescent after the chain");
}

// -- Decision 515: bounded runner pumps --------------------------

/// Decision 515: `pump_frames` caps under a live loop (returns the
/// cap — never a spin) and costs zero clock drift on idle trees.
#[test]
fn pump_frames_caps_under_live_loops_and_drifts_nothing_idle() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    // Idle tree: zero frames, zero drift (identical to idle).
    assert_eq!(host.pump_frames(64), 0, "idle pumps nothing");
    assert_eq!(clock.get(), 0.0, "no drift without demand");
    // Live loop: exactly the cap, then stop (the loop continues —
    // the pump returns instead of freezing). Mounts jump, so the
    // loop starts from a signal flip (the ambient-orb shape).
    #[derive(Clone)]
    struct LoopProps {
        on: crate::reactive::Signal<bool>,
    }
    impl Props for LoopProps {}
    fn loop_comp(_ctx: &Ctx, p: &LoopProps) -> VNode {
        let on = p.on.get();
        // The ambient-orb shape: a base delta plus the keyframes arm
        // (keyframes alone start no track — the delta kicks it off).
        let mut style = Style::new()
            .size(40, 40)
            .opacity(Some(if on { 1.0 } else { 0.35 }))
            .transition(Transition::new(400, Ease::InOut));
        if on {
            style = style.keyframes(Keyframes::new(vec![
                crate::style::KeyframeStop::new(200, Ease::Linear).opacity(Some(0.2)),
                crate::style::KeyframeStop::new(200, Ease::Linear).opacity(Some(1.0)),
            ]));
        }
        crate::vnode::Div("loop-box").style(style).build()
    }
    let on = host.runtime().signal(false);
    host.mount("Loop", LoopProps { on: on.clone() }, loop_comp);
    host.run_until_idle();
    on.set(true);
    clock.advance(1.0 / 60.0);
    host.run_once();
    host.with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "loop track live");
    });
    assert_eq!(host.pump_frames(10), 10, "capped, loop still live");
    host.with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "loop survives the cap");
    });
    assert!(host.has_demand(), "demand flows on");
}

/// Decision 515: `pump_frames` drains Once-only worlds early once
/// time flows past the track (fewer frames than the cap — the
/// run_until_idle shape, bounded).
#[test]
fn pump_frames_drains_once_worlds_early() {
    let (host, clock, dx) = settle_host();
    dx.set(100.0);
    // Frozen clock, live Once track: the cap bounds it (time never
    // flows here — retirement needs advancing clocks, which the
    // M8 stepping pattern owns, not the pump).
    assert_eq!(host.pump_frames(64), 64, "capped while live");
    host.with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "track still live");
    });
    // Past the 120ms tween the next pump retires and goes quiet
    // early (one retiring frame, then idle).
    clock.advance(1.0);
    let n = host.pump_frames(64);
    assert!(n < 64, "drains early once retired, ran {n}");
    host.with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "track retired");
    });
}

// ---------------------------------------------------------------------------
// Decision 519: subtree-unmount instance retirement
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RetireBranchProps {
    cleaned: Rc<Cell<bool>>,
    timeout_fired: Rc<Cell<usize>>,
    interval_fired: Rc<Cell<usize>>,
    /// The branch instance's private signal, read back every render —
    /// re-seeds to 0 exactly when the instance is fresh (evicted +
    /// re-allocated), and keeps the timeout-written 1 when it lingered.
    reseeds: Rc<Cell<u32>>,
}
impl Props for RetireBranchProps {}

fn retire_branch_comp(ctx: &Ctx, props: &RetireBranchProps) -> VNode {
    let sticky = ctx.signal(0u32);
    props.reseeds.set(sticky.get());
    let c = props.cleaned.clone();
    ctx.on_cleanup(move || {
        c.set(true);
    });
    let t = props.timeout_fired.clone();
    let tw = sticky.clone();
    ctx.use_timeout(100.0, move || {
        t.set(t.get() + 1);
        tw.set(1);
    });
    let iv = props.interval_fired.clone();
    ctx.use_interval(50.0, move || {
        iv.set(iv.get() + 1);
    });
    crate::vnode::Div("retire-branch").build()
}

#[derive(Clone)]
struct RetireToggleProps {
    show: Signal<bool>,
    branch: RetireBranchProps,
}
impl Props for RetireToggleProps {}

fn retire_toggle_comp(ctx: &Ctx, props: &RetireToggleProps) -> VNode {
    let mut kids = Vec::new();
    if props.show.get() {
        kids.push(ctx.child("RetireBranch", 1, &props.branch, retire_branch_comp));
    }
    crate::vnode::Div("retire-root").children(kids)
}

/// Hiding a subtree evicts its instances in the same cut: cleanups run
/// (cancelling per-render timers), dead timers leave no horizon and
/// never fire, and re-showing re-seeds instance state fresh.
#[test]
fn subtree_unmount_retires_instance_timers_and_cleanups() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let show = host.runtime().signal(true);
    let cleaned = Rc::new(Cell::new(false));
    let timeout_fired = Rc::new(Cell::new(0));
    let interval_fired = Rc::new(Cell::new(0));
    let reseeds = Rc::new(Cell::new(99));
    let branch = RetireBranchProps {
        cleaned: cleaned.clone(),
        timeout_fired: timeout_fired.clone(),
        interval_fired: interval_fired.clone(),
        reseeds: reseeds.clone(),
    };
    host.mount(
        "RetireToggle",
        RetireToggleProps {
            show: show.clone(),
            branch,
        },
        retire_toggle_comp,
    );
    host.run_until_idle();
    assert_eq!(reseeds.get(), 0, "fresh branch seeds zero");
    assert!(
        host.next_timer_due_ms(host.now_ms()).is_some(),
        "branch timers armed while shown"
    );
    // Prove the timers are alive before hiding (both fire while shown).
    clock.advance(0.2);
    assert!(
        host.tick_timers(host.now_ms()) >= 1,
        "timers fire while shown"
    );
    assert_eq!(timeout_fired.get(), 1, "timeout fired once while shown");
    assert!(interval_fired.get() >= 1, "interval fired while shown");
    let interval_at_hide = interval_fired.get();
    // Hide: the whole branch subtree unmounts.
    show.set(false);
    host.run_until_idle();
    assert!(cleaned.get(), "subtree removal runs branch cleanups");
    assert_eq!(
        host.next_timer_due_ms(host.now_ms()),
        None,
        "dead timers leave no wake horizon"
    );
    clock.advance(10.0);
    assert_eq!(host.tick_timers(host.now_ms()), 0, "dead timers never fire");
    assert_eq!(timeout_fired.get(), 1, "timeout never refires");
    assert_eq!(
        interval_fired.get(),
        interval_at_hide,
        "interval silenced by removal"
    );
    assert!(!host.has_demand(), "removal leaves zero scheduled work");
    host.with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "no tracks survive removal");
    });
    // Re-show: the branch owns a fresh instance (stale state gone).
    show.set(true);
    host.run_until_idle();
    assert_eq!(
        reseeds.get(),
        0,
        "re-shown branch re-seeds — the old instance was evicted"
    );
    assert!(
        host.next_timer_due_ms(host.now_ms()).is_some(),
        "re-shown branch re-arms its timers"
    );
}

#[derive(Clone)]
struct RetireMotionProps {
    show: Signal<bool>,
    lit: Signal<bool>,
}
impl Props for RetireMotionProps {}

fn retire_motion_branch(_ctx: &Ctx, props: &RetireMotionProps) -> VNode {
    // Non-black endpoints (pure 0x000000 reads TRANSPARENT and skips
    // paint — never a tween endpoint, by doctrine).
    let bg = if props.lit.get() {
        crate::style::Color(0x22_66_CC)
    } else {
        crate::style::Color(0x22_22_22)
    };
    crate::vnode::Div("retire-card")
        .style(
            Style::new()
                .size(100, 40)
                .bg(bg)
                .transition(Transition::new(400, Ease::Linear)),
        )
        .build()
}

fn retire_motion_root(ctx: &Ctx, props: &RetireMotionProps) -> VNode {
    let mut kids = Vec::new();
    if props.show.get() {
        kids.push(ctx.child("RetireCard", 7, props, retire_motion_branch));
    }
    crate::vnode::Div("retire-motion-root").children(kids)
}

/// Removing a subtree mid-flight retires its live track: the evaluator
/// drains to zero scheduled work and nothing restarts once time flows
/// past the tween (the round-1 retirement the exit-deferral round
/// builds on — proven here so a leak cannot hide behind it).
#[test]
fn subtree_unmount_retires_midflight_track() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let show = host.runtime().signal(true);
    let lit = host.runtime().signal(false);
    host.mount(
        "RetireMotion",
        RetireMotionProps {
            show: show.clone(),
            lit: lit.clone(),
        },
        retire_motion_root,
    );
    host.run_until_idle();
    let card = crate::find_retained_by_debug(&host, "retire-card")
        .into_iter()
        .next()
        .expect("card retained while shown");
    // Start a tween, then hide on the next commit with the clock frozen
    // (mid-flight — never tap/run_until_idle near live tracks).
    lit.set(true);
    host.run_once();
    host.with_evaluator(|ev| {
        assert!(!ev.is_settled(card), "tween live mid-flight");
    });
    show.set(false);
    host.run_until_idle();
    host.with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "removal retires the mid-flight track");
    });
    assert!(!host.has_demand(), "removal leaves zero scheduled work");
    assert!(
        crate::find_retained_by_debug(&host, "retire-card").is_empty(),
        "card node retired"
    );
    clock.advance(1.0);
    host.run_until_idle();
    host.with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "nothing restarts past the tween");
    });
    assert!(!host.has_demand(), "still quiet past the tween");
}

#[derive(Clone)]
struct SwapBranchProps {
    cleaned: Rc<Cell<bool>>,
    fired: Rc<Cell<usize>>,
    debug: &'static str,
}
impl Props for SwapBranchProps {}

fn swap_branch_comp(ctx: &Ctx, props: &SwapBranchProps) -> VNode {
    let c = props.cleaned.clone();
    ctx.on_cleanup(move || {
        c.set(true);
    });
    let f = props.fired.clone();
    ctx.use_timeout(100.0, move || {
        f.set(f.get() + 1);
    });
    // Same tag, no slot key on either side: the flip recycles this
    // node in place (no Remove op — ownership transfers instead).
    crate::vnode::Div(props.debug).build()
}

#[derive(Clone)]
struct SwapToggleProps {
    flipped: Signal<bool>,
    a: SwapBranchProps,
    b: SwapBranchProps,
}
impl Props for SwapToggleProps {}

fn swap_toggle_comp(ctx: &Ctx, props: &SwapToggleProps) -> VNode {
    let branch = if props.flipped.get() {
        ctx.child("SwapB", 2, &props.b, swap_branch_comp)
    } else {
        ctx.child("SwapA", 1, &props.a, swap_branch_comp)
    };
    crate::vnode::Div("swap-root").child(branch)
}

/// Slot-keyed recycling that re-homes a survivor under a new owner
/// (no `Remove` op) still retires the displaced instance: same node
/// id across the flip, old cleanups run, the old timer never fires
/// while the new owner's timer does.
#[test]
fn recycled_slot_retires_displaced_instance() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let flipped = host.runtime().signal(false);
    let a_cleaned = Rc::new(Cell::new(false));
    let a_fired = Rc::new(Cell::new(0));
    let b_cleaned = Rc::new(Cell::new(false));
    let b_fired = Rc::new(Cell::new(0));
    host.mount(
        "SwapToggle",
        SwapToggleProps {
            flipped: flipped.clone(),
            a: SwapBranchProps {
                cleaned: a_cleaned.clone(),
                fired: a_fired.clone(),
                debug: "swap-a",
            },
            b: SwapBranchProps {
                cleaned: b_cleaned.clone(),
                fired: b_fired.clone(),
                debug: "swap-b",
            },
        },
        swap_toggle_comp,
    );
    host.run_until_idle();
    let before = crate::find_retained_by_debug(&host, "swap-a")
        .into_iter()
        .next()
        .expect("side A retained");
    flipped.set(true);
    host.run_until_idle();
    let after = crate::find_retained_by_debug(&host, "swap-b")
        .into_iter()
        .next()
        .expect("side B retained");
    assert_eq!(
        before, after,
        "slot recycled in place — no remove/add across the flip"
    );
    assert!(a_cleaned.get(), "displaced instance cleanups run");
    assert!(!b_cleaned.get(), "surviving instance untouched");
    clock.advance(10.0);
    host.tick_timers(host.now_ms());
    assert_eq!(a_fired.get(), 0, "displaced timer never fires");
    assert_eq!(b_fired.get(), 1, "surviving timer fires exactly once");
    assert!(!host.has_demand(), "flip leaves zero scheduled work");
}

// ---------------------------------------------------------------------------
// Decision 520: exit-deferral (dismissals fade instead of snapping)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ExitBranchProps {
    cleaned: Rc<Cell<bool>>,
    interval_fired: Rc<Cell<usize>>,
    pressed: Rc<Cell<usize>>,
}
impl Props for ExitBranchProps {}

fn exit_branch_comp(ctx: &Ctx, props: &ExitBranchProps) -> VNode {
    let c = props.cleaned.clone();
    ctx.on_cleanup(move || {
        c.set(true);
    });
    let iv = props.interval_fired.clone();
    ctx.use_interval(50.0, move || {
        iv.set(iv.get() + 1);
    });
    let p = props.pressed.clone();
    // Non-black fill (pure 0x000000 reads TRANSPARENT and skips paint
    // — never a tween/fill endpoint, by doctrine).
    crate::vnode::Div("exit-branch")
        .style(
            Style::new()
                .size(100, 40)
                .bg(crate::style::Color(0x22_66_CC))
                .exit(Transition::exit()),
        )
        .on_press(move || {
            p.set(p.get() + 1);
        })
        .build()
}

#[derive(Clone)]
struct ExitToggleProps {
    show: Signal<bool>,
    branch: ExitBranchProps,
}
impl Props for ExitToggleProps {}

fn exit_toggle_comp(ctx: &Ctx, props: &ExitToggleProps) -> VNode {
    let mut kids = Vec::new();
    if props.show.get() {
        kids.push(ctx.child("ExitBranch", 1, &props.branch, exit_branch_comp));
    }
    crate::vnode::Div("exit-root").children(kids)
}

struct ExitRig {
    host: ComponentHost,
    clock: Rc<MockClock>,
    show: Signal<bool>,
    cleaned: Rc<Cell<bool>>,
    interval_fired: Rc<Cell<usize>>,
    pressed: Rc<Cell<usize>>,
}

fn exit_host() -> ExitRig {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let show = host.runtime().signal(true);
    let cleaned = Rc::new(Cell::new(false));
    let interval_fired = Rc::new(Cell::new(0));
    let pressed = Rc::new(Cell::new(0));
    host.mount(
        "ExitToggle",
        ExitToggleProps {
            show: show.clone(),
            branch: ExitBranchProps {
                cleaned: cleaned.clone(),
                interval_fired: interval_fired.clone(),
                pressed: pressed.clone(),
            },
        },
        exit_toggle_comp,
    );
    host.run_until_idle();
    ExitRig {
        host,
        clock,
        show,
        cleaned,
        interval_fired,
        pressed,
    }
}

/// Dismissal pins the removed subtree and fades it (mid-exit opacity
/// strictly interior on the retired id), then releases to zero work —
/// while the branch instance and its timers outlive the removal
/// through the exit (no leak through the 519 retirement) and retire
/// at exit end.
#[test]
fn exit_defers_fade_then_releases_and_sweeps() {
    let rig = exit_host();
    let (host, clock, show, cleaned, interval_fired) = (
        rig.host,
        rig.clock,
        rig.show,
        rig.cleaned,
        rig.interval_fired,
    );
    let card = crate::find_retained_by_debug(&host, "exit-branch")
        .into_iter()
        .next()
        .expect("branch retained while shown");
    show.set(false);
    host.run_once();
    // Pinned past removal: the arena dropped the node but the exit
    // tracks it.
    assert!(
        crate::find_retained_by_debug(&host, "exit-branch").is_empty(),
        "branch unmounts on hide"
    );
    host.with_evaluator(|ev| {
        assert!(ev.exit_count() >= 1, "exit pinned");
    });
    // Mid-exit: the ghost fades (frozen clock + bounded pumps only —
    // an idle pump would spin on the live exit, the M8 rule).
    clock.advance(0.09);
    host.run_once();
    let now = clock.get();
    host.with_evaluator(|ev| {
        let op = ev.resolve_opacity(card, 1.0, now);
        assert!(op > 0.0 && op < 1.0, "ghost fades mid-exit, got {op}");
    });
    // The branch instance outlives its removal through the exit (its
    // timer fires — the 519 sweep exempts deferred owners).
    assert_eq!(
        host.tick_timers(host.now_ms()),
        1,
        "branch timer fires mid-exit"
    );
    assert_eq!(interval_fired.get(), 1);
    assert!(!cleaned.get(), "cleanups wait for exit end");
    // Past the 180ms exit: release + orphan sweep to zero work.
    clock.advance(1.0);
    host.run_until_idle();
    assert!(cleaned.get(), "cleanups run at exit end");
    assert_eq!(
        host.next_timer_due_ms(host.now_ms()),
        None,
        "dead timers leave no horizon"
    );
    assert_eq!(host.tick_timers(host.now_ms()), 0, "dead timers never fire");
    assert_eq!(interval_fired.get(), 1, "interval silenced after exit");
    host.with_evaluator(|ev| {
        assert_eq!(ev.exit_count(), 0, "exits released");
        assert_eq!(ev.active_count(), 0, "zero scheduled tracks");
    });
    assert!(!host.has_demand(), "quiet after the exit");
}

/// The fading ghost takes no input: presses at its old button box
/// fall through without invoking (hit-test walks the live arena —
/// retired ids never hit, by construction).
#[test]
fn exit_ghost_takes_no_input() {
    let rig = exit_host();
    let (host, clock, show, pressed) = (rig.host, rig.clock, rig.show, rig.pressed);
    let card = crate::find_retained_by_debug(&host, "exit-branch")
        .into_iter()
        .next()
        .expect("branch retained while shown");
    let b = host.committed_box(card).expect("branch laid out");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    show.set(false);
    host.run_once();
    clock.advance(0.09);
    host.run_once();
    host.with_evaluator(|ev| {
        assert!(ev.exit_count() >= 1, "exit live");
    });
    // Press squarely where the button painted: the ghost ignores it.
    host.inject_input(crate::input::InputEvent::pointer_down(cx, cy));
    host.inject_input(crate::input::InputEvent::pointer_up(cx, cy));
    clock.advance(0.09);
    host.run_once();
    assert_eq!(pressed.get(), 0, "ghost press never invokes");
    assert!(host.focused_node().is_none(), "ghost press focuses nothing");
    // The exit still completes to quiet afterwards.
    clock.advance(1.0);
    host.run_until_idle();
    assert!(!host.has_demand(), "quiet after the exit");
}

/// Re-showing while the exit is live cancels the ghost: fresh paint
/// owns the slot (same owner, same label), the snapshot releases,
/// and the live subtree paints full opacity with its timers re-armed.
#[test]
fn exit_reshow_cancels_ghost() {
    let rig = exit_host();
    let (host, clock, show) = (rig.host, rig.clock, rig.show);
    show.set(false);
    host.run_once();
    host.with_evaluator(|ev| {
        assert!(ev.exit_count() >= 1, "exit live");
    });
    // Re-show before the fade lands: cancel, never crossfade.
    show.set(true);
    host.run_once();
    host.with_evaluator(|ev| {
        assert_eq!(ev.exit_count(), 0, "re-show releases the ghost");
        assert_eq!(ev.active_count(), 0, "no tracks left");
    });
    assert!(
        host.exit_snapshots().is_empty(),
        "snapshots released on cancel"
    );
    let card = crate::find_retained_by_debug(&host, "exit-branch")
        .into_iter()
        .next()
        .expect("branch live again");
    let now = clock.get();
    host.with_evaluator(|ev| {
        assert_eq!(
            ev.resolve_opacity(card, 0.0, now),
            1.0,
            "live subtree paints full opacity"
        );
    });
    assert!(
        host.next_timer_due_ms(host.now_ms()).is_some(),
        "re-shown branch re-arms its timer"
    );
    // Settles quiet afterwards (no half-released ghost).
    clock.advance(1.0);
    host.run_until_idle();
    assert!(!host.has_demand(), "quiet after re-show");
}

/// Loops never defer: removing a subtree with a live loop track and
/// an exit declaration refuses loudly (cancel first or drop the
/// declaration — never a silent snap).
#[test]
#[should_panic(expected = "never defers")]
fn exit_loop_refuses_loudly() {
    use crate::style::{KeyframeMode, KeyframeStop};
    use crate::vnode::Div;
    #[derive(Clone)]
    struct LoopExitProps {
        on: Signal<bool>,
        show: Signal<bool>,
    }
    impl Props for LoopExitProps {}
    fn loop_exit_branch(_ctx: &Ctx, _: &LoopExitProps) -> VNode {
        Div("loop-exit-branch").build()
    }
    fn loop_exit_root(ctx: &Ctx, props: &LoopExitProps) -> VNode {
        if !props.show.get() {
            // Incompatible root (Hole, never a diffable Element) so
            // hiding removes the whole wrapper subtree with the loop
            // track live on its declaring entry root.
            return VNode::Hole;
        }
        // Live loop (keyframes Loop mode) under an exit
        // declaration: the deferral must refuse, never fade.
        let spinning = Style::new()
            .size(40, 40)
            .opacity(Some(if props.on.get() { 1.0 } else { 0.35 }))
            .transition(Transition::new(400, Ease::InOut))
            .keyframes(
                Keyframes::new(vec![
                    KeyframeStop::new(200, Ease::Linear).opacity(Some(0.2)),
                    KeyframeStop::new(200, Ease::Linear).opacity(Some(1.0)),
                ])
                .mode(KeyframeMode::Loop),
            )
            .exit(Transition::exit());
        // The style rides the wrapper (the removed entry root
        // declares the exit): render it around the child.
        let inner = ctx.child(
            "LoopExit",
            3,
            &LoopExitProps {
                on: props.on.clone(),
                show: props.show.clone(),
            },
            loop_exit_branch,
        );
        Div("loop-exit-wrap").style(spinning).child(inner)
    }
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let on = host.runtime().signal(false);
    let show = host.runtime().signal(true);
    host.mount(
        "LoopExit",
        LoopExitProps {
            on: on.clone(),
            show: show.clone(),
        },
        loop_exit_root,
    );
    host.run_until_idle();
    on.set(true);
    clock.advance(1.0 / 60.0);
    host.run_once();
    host.with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "loop track live");
    });
    // Dismiss with the loop running: loud refusal, never a fade.
    show.set(false);
    host.run_once();
}

// ---------------------------------------------------------------------------
// Decisions 521-522: layout animation (FLIP for position, fence for size)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FenceCardProps {
    wide: Signal<bool>,
}
impl Props for FenceCardProps {}

fn fence_card_comp(ctx: &Ctx, props: &FenceCardProps) -> VNode {
    let _ = ctx.signal(0u32);
    // Non-black fill (pure 0x000000 reads TRANSPARENT and skips paint
    // — never a fill endpoint, by doctrine).
    let w = if props.wide.get() { 200.0 } else { 100.0 };
    crate::vnode::Div("fence-card")
        .style(
            Style::new()
                .size(w, 50.0)
                .bg(crate::style::Color(0x22_66_CC))
                .layout_anim(Transition::new(200, Ease::Linear)),
        )
        .build()
}

/// Fence interpolates explicit-px size mid-flight, lands exact on
/// the committed target, and retires to zero work (no new demand
/// source — quiescence reads the one number).
#[test]
fn fence_interpolates_size_then_settles_quiet() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let wide = host.runtime().signal(false);
    host.mount(
        "Fence",
        FenceCardProps { wide: wide.clone() },
        fence_card_comp,
    );
    host.run_until_idle();
    let card = crate::find_retained_by_debug(&host, "fence-card")
        .into_iter()
        .next()
        .expect("card retained");
    wide.set(true);
    host.run_once();
    // Committed box jumps at once (engine truth); the fence lags it.
    host.with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "fence track live");
    });
    clock.advance(0.1);
    host.run_once();
    let now = clock.get();
    let mid = host
        .effective_box(card, now)
        .expect("fence interpolates mid-flight");
    assert!(
        mid.w > 100.0 && mid.w < 200.0,
        "fence runs mid-flight, got {}",
        mid.w
    );
    assert!(
        host.committed_box(card).expect("committed").w == 200.0,
        "committed box already at target"
    );
    clock.advance(1.0);
    host.run_until_idle();
    host.with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "fence retires to zero work");
    });
    assert!(!host.has_demand(), "quiet after the fence");
}

#[derive(Clone)]
struct FlipListProps {
    extra: Signal<bool>,
}
impl Props for FlipListProps {}

fn flip_list_comp(ctx: &Ctx, props: &FlipListProps) -> VNode {
    let _ = ctx.signal(0u32);
    let anim = Transition::new(200, Ease::Linear);
    let mut kids = Vec::new();
    if props.extra.get() {
        kids.push(
            crate::vnode::Div("flip-extra")
                .style(Style::new().size(100.0, 20.0))
                .build(),
        );
    }
    for (i, name) in ["flip-a", "flip-b"].into_iter().enumerate() {
        kids.push(
            crate::vnode::Div(name)
                .key(i as u64)
                .style(Style::new().size(100.0, 20.0).layout_anim(anim))
                .build(),
        );
    }
    crate::vnode::Column::new().children(kids)
}

/// Inserting a sibling shifts flagged rows down through FLIP
/// (mid-flight offsets strictly interior), landing exact and quiet.
#[test]
fn flip_moves_shifted_rows_then_settles_quiet() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let extra = host.runtime().signal(false);
    host.mount(
        "Flip",
        FlipListProps {
            extra: extra.clone(),
        },
        flip_list_comp,
    );
    host.run_until_idle();
    let a = crate::find_retained_by_debug(&host, "flip-a")
        .into_iter()
        .next()
        .expect("row a retained");
    let y0 = host.committed_box(a).expect("row laid out").y;
    extra.set(true);
    host.run_once();
    let y1 = host.committed_box(a).expect("row shifted").y;
    assert!(y1 > y0, "insert shifts the row down");
    // Mid-flight: the row paints above its committed box (offset
    // decays to zero — frozen clock + bounded pumps only).
    clock.advance(0.1);
    host.run_once();
    let now = clock.get();
    let dy = host.with_evaluator(|ev| ev.resolve_transform(a, None, now).dy.get());
    assert!(
        dy < 0.0 && dy > y0 - y1,
        "flip offsets mid-flight, got {dy} for a {}px move",
        y1 - y0
    );
    clock.advance(1.0);
    host.run_until_idle();
    host.with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "flip retires to zero work");
    });
    assert!(!host.has_demand(), "quiet after the flip");
}

/// A child riding its moving parent records zero (transforms
/// compose down every backend — absolute deltas would double-apply).
/// Exactly one flip track exists (the parent's); the child resolves
/// identity while painting parent-composed.
#[test]
fn flip_parent_relative_records_zero_for_riding_child() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let dx = host.runtime().signal(0.0f32);
    #[derive(Clone)]
    struct ShiftProps {
        dx: Signal<f32>,
    }
    impl Props for ShiftProps {}
    fn shift_comp(ctx: &Ctx, props: &ShiftProps) -> VNode {
        let _ = ctx.signal(0u32);
        let anim = Transition::new(200, Ease::Linear);
        let inner = ctx.child(
            "ShiftKid",
            1,
            &ShiftProps {
                dx: props.dx.clone(),
            },
            shift_kid_comp,
        );
        // Static root (roots pin at the origin — the moving wrap
        // nests one level down so `.x()` actually displaces it).
        let wrap = crate::vnode::Div("shift-wrap")
            .style(Style::new().x(props.dx.get()).layout_anim(anim))
            .child(inner);
        crate::vnode::Div("shift-root").child(wrap)
    }
    fn shift_kid_comp(ctx: &Ctx, _: &ShiftProps) -> VNode {
        let _ = ctx.signal(0u32);
        crate::vnode::Div("shift-kid")
            .style(
                Style::new()
                    .size(40.0, 20.0)
                    .layout_anim(Transition::new(200, Ease::Linear)),
            )
            .build()
    }
    host.mount("Shift", ShiftProps { dx: dx.clone() }, shift_comp);
    host.run_until_idle();
    let kid = crate::find_retained_by_debug(&host, "shift-kid")
        .into_iter()
        .next()
        .expect("kid retained");
    dx.set(50.0);
    host.run_once();
    // Exactly the parent's track: the riding child records zero
    // (zero offsets snap — no track).
    host.with_evaluator(|ev| {
        let live = ev.live_nodes();
        assert_eq!(live.len(), 1, "one flip track, got {live:?}");
    });
    clock.advance(0.1);
    host.run_once();
    let now = clock.get();
    host.with_evaluator(|ev| {
        assert!(
            ev.resolve_transform(kid, None, now).is_identity(),
            "child carries no offset of its own"
        );
    });
    clock.advance(1.0);
    host.run_until_idle();
    assert!(!host.has_demand(), "quiet after the shift");
}

#[derive(Clone)]
struct ScrollRowProps {
    off: Option<crate::ScrollOffset>,
}
impl Props for ScrollRowProps {}

fn scroll_area_comp(ctx: &Ctx, _: &ScrollRowProps) -> VNode {
    let _ = ctx.signal(0u32);
    // Owner offset (the VirtualList shape — the body owns the
    // offset, rows position from it, no explicit bind).
    let off = ctx.scroll_offset();
    let row = ctx.child(
        "ScrollRow",
        1,
        &ScrollRowProps {
            off: Some(off.clone()),
        },
        scroll_row_comp,
    );
    crate::vnode::ScrollArea("scroll-box")
        .style(Style::new().size(200.0, 200.0))
        .on_scroll(|| {})
        .child(row)
}

fn scroll_row_comp(ctx: &Ctx, props: &ScrollRowProps) -> VNode {
    let _ = ctx.signal(0u32);
    let off = props.off.clone().expect("container passes its offset");
    crate::vnode::Div("scroll-row")
        .style(
            Style::new()
                .size(180.0, 30.0)
                .absolute_y(10.0 - off.get())
                .layout_anim(Transition::new(200, Ease::Linear)),
        )
        .build()
}

/// Scroll-driven moves never ease (decision 521): the flagged row
/// jumps with the offset while recording nothing.
#[test]
fn flip_scroll_driven_moves_suppress() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    host.mount("ScrollAnim", ScrollRowProps { off: None }, scroll_area_comp);
    host.run_until_idle();
    let row = crate::find_retained_by_debug(&host, "scroll-row")
        .into_iter()
        .next()
        .expect("row retained");
    let y0 = host.committed_box(row).expect("row laid out").y;
    // Drive the owner offset directly (same value change a wheel
    // feed produces — the suppression keys on change, not source).
    host.with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "idle has no tracks");
    });
    // Reach the owner offset through the instance (the row reads the
    // container's own signal — bump it like a feed would).
    let bumped = host.instance_scroll(1).expect("owner offset exists");
    bumped.set(100.0);
    host.run_once();
    let y1 = host.committed_box(row).expect("row scrolled").y;
    assert!(
        (y0 - y1 - 100.0).abs() < 1e-3,
        "offset moves the row, {y0} -> {y1}"
    );
    host.with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "scroll moves record nothing");
    });
    assert!(!host.has_demand(), "quiet after the scroll");
}

/// Mid-flight fences retarget from the live box (decision 522 —
/// restart-from-current, never cancel, never jump).
#[test]
fn fence_mid_flight_retargets_from_live() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let wide = host.runtime().signal(false);
    host.mount(
        "Fence2",
        FenceCardProps { wide: wide.clone() },
        fence_card_comp,
    );
    host.run_until_idle();
    let card = crate::find_retained_by_debug(&host, "fence-card")
        .into_iter()
        .next()
        .expect("card retained");
    wide.set(true);
    host.run_once();
    clock.advance(0.1);
    host.run_once();
    let now = clock.get();
    let mid = host.effective_box(card, now).expect("fence live").w;
    assert!(mid > 100.0 && mid < 200.0, "mid-flight, got {mid}");
    // Retarget mid-flight (same signal shape, new target): shrink
    // back while the grow is live — continuity, no jump.
    wide.set(false);
    host.run_once();
    clock.advance(0.05);
    host.run_once();
    let now = clock.get();
    let after = host.effective_box(card, now).expect("retarget live").w;
    assert!(
        after < mid,
        "retarget heads back from the live box, {mid} -> {after}"
    );
    clock.advance(1.0);
    host.run_until_idle();
    assert!(!host.has_demand(), "quiet after the retarget");
    assert_eq!(
        host.committed_box(card).expect("card").w,
        100.0,
        "lands on the latest target"
    );
}

/// Mutating a sibling mid-flight retargets without dirt leakage
/// (decision 522): generation bumps stay bounded to genuine commits
/// (no per-frame layout churn), the fence lands exact, and the tree
/// goes quiet.
#[test]
fn fence_sibling_mutation_stays_dirt_bounded() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    #[derive(Clone)]
    struct PairProps {
        wide_card: Signal<bool>,
        wide_sib: Signal<bool>,
    }
    impl Props for PairProps {}
    fn pair_comp(ctx: &Ctx, props: &PairProps) -> VNode {
        let _ = ctx.signal(0u32);
        let anim = Transition::new(200, Ease::Linear);
        let card = crate::vnode::Div("dirt-card")
            .style(
                Style::new()
                    .size(if props.wide_card.get() { 200.0 } else { 100.0 }, 50.0)
                    .layout_anim(anim),
            )
            .build();
        let sib = crate::vnode::Div("dirt-sib")
            .style(Style::new().size(if props.wide_sib.get() { 150.0 } else { 80.0 }, 50.0))
            .build();
        crate::vnode::Column::new().children(vec![card, sib])
    }
    let wide_card = host.runtime().signal(false);
    let wide_sib = host.runtime().signal(false);
    host.mount(
        "Pair",
        PairProps {
            wide_card: wide_card.clone(),
            wide_sib: wide_sib.clone(),
        },
        pair_comp,
    );
    host.run_until_idle();
    let card = crate::find_retained_by_debug(&host, "dirt-card")
        .into_iter()
        .next()
        .expect("card retained");
    let gen0 = host.layout_generation();
    wide_card.set(true);
    host.run_once();
    let gen1 = host.layout_generation();
    assert_eq!(gen1, gen0 + 1, "expand commits once");
    // Idle frames bump nothing (no per-frame layout churn).
    for _ in 0..3 {
        clock.advance(1.0 / 60.0);
        host.run_once();
    }
    assert_eq!(
        host.layout_generation(),
        gen1,
        "mid-flight frames run no layout"
    );
    // Sibling mutates mid-flight: one more commit, then quiet.
    wide_sib.set(true);
    host.run_once();
    assert_eq!(host.layout_generation(), gen1 + 1, "sibling commits once");
    clock.advance(1.0);
    host.run_until_idle();
    assert!(!host.has_demand(), "quiet after both settle");
    assert_eq!(
        host.committed_box(card).expect("card").w,
        200.0,
        "fence lands on its target"
    );
}

/// An auto-sized box that changes size under a layout_anim
/// declaration refuses loudly (no defined endpoint rule — the
/// clamp-panic precedent, judged on final spans).
#[test]
#[should_panic(expected = "without explicit h")]
fn fence_auto_height_refuses_loudly() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let tall = host.runtime().signal(false);
    #[derive(Clone)]
    struct AutoProps {
        tall: Signal<bool>,
    }
    impl Props for AutoProps {}
    fn auto_comp(ctx: &Ctx, props: &AutoProps) -> VNode {
        let _ = ctx.signal(0u32);
        // The kid snaps (explicit, undeclared); the inner auto
        // height follows content — the refusal case. (Nested: mount
        // roots take the viewport height, so the auto box nests one
        // level down.)
        let kid = crate::vnode::Div("auto-kid")
            .style(Style::new().size(50.0, if props.tall.get() { 60.0 } else { 30.0 }))
            .build();
        let inner = crate::vnode::Div("auto-box")
            .style(
                Style::new()
                    .w(200.0)
                    .layout_anim(Transition::new(200, Ease::Linear)),
            )
            .child(kid);
        crate::vnode::Div("auto-root")
            .style(Style::new().size(400.0, 400.0))
            .child(inner)
    }
    host.mount("Auto", AutoProps { tall: tall.clone() }, auto_comp);
    host.run_until_idle();
    tall.set(true);
    host.run_once();
}

/// A size change on a refused node jumps entirely (decision 521 —
/// mixed move+resize on a refused node never half-animates). This
/// test documents the jump through an undeclared twin: same tree
/// without the declaration snaps to the same end state.
#[test]
fn mixed_move_resize_without_declaration_snaps() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let wide = host.runtime().signal(false);
    #[derive(Clone)]
    struct PlainProps {
        wide: Signal<bool>,
    }
    impl Props for PlainProps {}
    fn plain_comp(ctx: &Ctx, props: &PlainProps) -> VNode {
        let _ = ctx.signal(0u32);
        // No layout_anim anywhere: every layout change applies
        // instantly, exactly as before decisions 521-522.
        let w = if props.wide.get() { 200.0 } else { 100.0 };
        crate::vnode::Div("plain-card")
            .style(Style::new().size(w, 50.0))
            .build()
    }
    host.mount("Plain", PlainProps { wide: wide.clone() }, plain_comp);
    host.run_until_idle();
    let card = crate::find_retained_by_debug(&host, "plain-card")
        .into_iter()
        .next()
        .expect("card retained");
    wide.set(true);
    host.run_once();
    host.with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "undeclared moves create no tracks");
    });
    assert_eq!(
        host.committed_box(card).expect("card").w,
        200.0,
        "undeclared resize snaps to target"
    );
    assert!(!host.has_demand(), "quiet");
    let _ = clock;
}

/// Stamped commits snap layout too (Q5 covers layout like exits
/// and tweens): a binding edge plus a box move in one commit
/// creates no tracks — values jump.
#[test]
fn stamped_layout_moves_snap() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let wide = host.runtime().signal(false);
    #[derive(Clone)]
    struct StampProps {
        wide: Signal<bool>,
    }
    impl Props for StampProps {}
    fn stamp_comp(ctx: &Ctx, props: &StampProps) -> VNode {
        let _ = ctx.signal(0u32);
        // Binding edge on the same signal that grows the card: one
        // commit carries both the stamp and the move. The memo must
        // be READ (unevaluated memos record no dependency edges, so
        // an unread binding never fires — pull semantics, §9.1).
        let wide = props.wide.clone();
        let _ = ctx.binding(move || wide.get()).read();
        let w = if props.wide.get() { 200.0 } else { 100.0 };
        crate::vnode::Div("stamp-card")
            .style(
                Style::new()
                    .size(w, 50.0)
                    .layout_anim(Transition::new(200, Ease::Linear)),
            )
            .build()
    }
    host.mount("Stamp", StampProps { wide: wide.clone() }, stamp_comp);
    host.run_until_idle();
    let card = crate::find_retained_by_debug(&host, "stamp-card")
        .into_iter()
        .next()
        .expect("card retained");
    wide.set(true);
    host.run_once();
    host.with_evaluator(|ev| {
        assert_eq!(ev.active_count(), 0, "stamped moves create no tracks");
    });
    assert_eq!(
        host.committed_box(card).expect("card").w,
        200.0,
        "stamped resize jumps to target"
    );
    assert!(!host.has_demand(), "quiet");
    let _ = clock;
}
