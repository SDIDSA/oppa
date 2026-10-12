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
