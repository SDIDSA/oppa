use super::*;
use crate::component::{ComponentHost, Ctx, Props};
use crate::reactive::Signal;
use crate::vnode::{Div, VNode};
use crate::worker::{TaskId, TaskStage};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

#[test]
fn fetch_key_is_deterministic_and_scoped() {
    assert_eq!(fetch_key("settings:avatar"), fetch_key("settings:avatar"));
    assert_ne!(fetch_key("settings:avatar"), fetch_key("settings:cover"));
}

type FetchSignal = Signal<FetchState<String>>;

#[derive(Clone)]
struct ProbeProps {
    stash: Rc<RefCell<Option<FetchSignal>>>,
    fired: Rc<Cell<bool>>,
    fail: bool,
}
impl Props for ProbeProps {}

fn probe_render(ctx: &Ctx, p: &ProbeProps) -> VNode {
    let key = ctx.fetch_key("test:probe");
    let state = ctx.fetch_state::<String>(key);
    *p.stash.borrow_mut() = Some(state);
    if !p.fired.get() {
        p.fired.set(true);
        if p.fail {
            ctx.spawn_fetch(key, || Err::<String, _>("dns".to_string()));
        } else {
            ctx.spawn_fetch(key, || Ok::<_, String>("hello".to_string()));
        }
    }
    Div("probe").build()
}

fn stashed(props: &ProbeProps) -> FetchState<String> {
    props
        .stash
        .borrow()
        .as_ref()
        .expect("probe stashed its signal")
        .get()
}

#[test]
fn spawn_fetch_drives_loading_then_ready() {
    let host = ComponentHost::new();
    let props = ProbeProps {
        stash: Rc::new(RefCell::new(None)),
        fired: Rc::new(Cell::new(false)),
        fail: false,
    };
    host.mount("Probe", props.clone(), probe_render);
    // Mount runs the effect synchronously: Loading is already set,
    // before any frame (and before the worker can answer).
    assert_eq!(
        stashed(&props),
        FetchState::Loading,
        "Loading is synchronous -- first paint already shows it"
    );
    host.run_until_idle();
    wait_for_tasks(&host.runtime(), 1);
    host.run_until_idle();
    assert_eq!(stashed(&props), FetchState::Ready("hello".to_string()));
}

#[test]
fn spawn_fetch_reports_failure_as_state_not_panic() {
    let host = ComponentHost::new();
    let props = ProbeProps {
        stash: Rc::new(RefCell::new(None)),
        fired: Rc::new(Cell::new(false)),
        fail: true,
    };
    host.mount("ProbeFail", props.clone(), probe_render);
    host.run_until_idle();
    wait_for_tasks(&host.runtime(), 1);
    host.run_until_idle();
    assert_eq!(stashed(&props), FetchState::Failed("dns".to_string()));
}

fn wait_for_tasks(rt: &crate::reactive::Runtime, n: u64) {
    let mut waited = 0;
    loop {
        let stats = rt.stats();
        if stats.tasks_done + stats.tasks_dropped >= n {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "executor task never resolved");
    }
}

/// Round 13.1 (decision 308): preparation stages walk in strict
/// order. B parks in `Queued` (its `Prepared` logs strictly
/// after A's `Done` — promotion cannot precede completion), and
/// each id walks `Queued → Prepared → Ready → Done` exactly
/// once (single worker thread, FIFO queue, id-ordered
/// promotion). Asserted as interleave-proof invariants (per-id
/// subsequences + positions — never wall-clock races), because
/// the worker runs concurrently with the asserting thread.
#[test]
fn prep_stages_walk_in_strict_order() {
    use crate::worker::{TaskId, TaskStage};
    use std::sync::Mutex;
    let host = ComponentHost::new();
    let rt = host.runtime();
    // Bodies must be `Send` — the marker log crosses threads
    // through a mutex, never a signal (the §9.6 residence rule).
    let marker: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
    let m = marker.clone();
    let a: TaskId = rt.spawn_task(move |_scope| {
        m.lock().expect("marker log").push("A-ran");
    });
    let m = marker.clone();
    let b: TaskId = rt.prepare_task(&[a], move |_scope| {
        m.lock().expect("marker log").push("B-ran");
    });
    wait_for_tasks(&rt, 2);
    host.run_until_idle();
    // Bodies ran in dependency order (B cannot run before its
    // promotion; promotion cannot precede A's completion;
    // completion follows A's body — transitively airtight).
    assert_eq!(*marker.lock().expect("marker log"), vec!["A-ran", "B-ran"]);
    assert_eq!(rt.task_stage(a), Some(TaskStage::Done));
    assert_eq!(rt.task_stage(b), Some(TaskStage::Done));
    // Per-id stage walks are exact (each id's own entries are
    // totally ordered: submit logs Queued[/Prepared/Ready]
    // synchronously, the worker logs the rest in sequence).
    let log = rt.task_transitions();
    for id in [a, b] {
        let walk: Vec<TaskStage> = log
            .iter()
            .filter(|(i, _)| *i == id)
            .map(|(_, s)| *s)
            .collect();
        assert_eq!(
            walk,
            vec![
                TaskStage::Queued,
                TaskStage::Prepared,
                TaskStage::Ready,
                TaskStage::Done
            ],
            "{id:?} must walk Queued → Prepared → Ready → Done exactly once"
        );
    }
    // Parking proof: B's promotion lands strictly after A's
    // completion (complete() logs Done before scanning — same
    // thread, so the positions cannot invert).
    let pos = |id: TaskId, s: TaskStage| {
        log.iter()
            .position(|(i, t)| *i == id && *t == s)
            .unwrap_or_else(|| panic!("{id:?} never reached {s:?}: {log:?}"))
    };
    assert!(
        pos(b, TaskStage::Prepared) > pos(a, TaskStage::Done),
        "B promotes only after A completes: {log:?}"
    );
}

/// Round 13.1 (decision 308): an exhausted retry budget
/// surfaces a distinct terminal error — greppable, never
/// confusable with a first-try failure. All three tries run
/// inside one task run (one `tasks_done` count).
#[test]
fn retry_budget_exhausts_with_distinct_error() {
    use std::sync::atomic::{AtomicU32, Ordering};
    let host = ComponentHost::new();
    let tries = Arc::new(AtomicU32::new(0));
    let key = fetch_key("test:retry-exhaust");
    let signal = host
        .runtime()
        .keyed_state::<FetchState<String>>(key, || FetchState::Idle);
    // Drive through a component body (the blessed call path —
    // direct `Ctx` construction is not a thing).
    #[derive(Clone)]
    struct RetryProbe {
        key: u64,
        tries: Arc<AtomicU32>,
    }
    impl Props for RetryProbe {}
    fn retry_render(ctx: &Ctx, p: &RetryProbe) -> VNode {
        let tries = p.tries.clone();
        // `fetch_add` through `Arc` — the closure never mutates
        // captures, so it satisfies `FnMut` cleanly.
        ctx.spawn_fetch_with_retry(p.key, 3, move || {
            tries.fetch_add(1, Ordering::SeqCst);
            Err::<String, _>("dns".to_string())
        });
        Div("probe").build()
    }
    let tries_probe = tries.clone();
    host.mount(
        "Retry",
        RetryProbe {
            key,
            tries: tries_probe,
        },
        retry_render,
    );
    host.run_until_idle();
    wait_for_tasks(&host.runtime(), 1);
    host.run_until_idle();
    assert_eq!(tries.load(Ordering::SeqCst), 3, "all three tries ran");
    assert_eq!(
        signal.get(),
        FetchState::Failed("dns (retry budget exhausted after 3 attempts)".to_string()),
        "terminal error names the exhaustion distinctly"
    );
}

/// Round 13.1: a late success still wins — the budget is a
/// ceiling, not a quota.
#[test]
fn retry_succeeds_before_budget_runs_out() {
    use std::sync::atomic::{AtomicU32, Ordering};
    let host = ComponentHost::new();
    let key = fetch_key("test:retry-flaky");
    let signal = host
        .runtime()
        .keyed_state::<FetchState<String>>(key, || FetchState::Idle);
    let tries = Arc::new(AtomicU32::new(0));
    #[derive(Clone)]
    struct FlakyProbe {
        key: u64,
        tries: Arc<AtomicU32>,
    }
    impl Props for FlakyProbe {}
    fn flaky_render(ctx: &Ctx, p: &FlakyProbe) -> VNode {
        let tries = p.tries.clone();
        ctx.spawn_fetch_with_retry(p.key, 3, move || {
            let n = tries.fetch_add(1, Ordering::SeqCst) + 1;
            if n < 3 {
                Err::<String, _>("flaky".to_string())
            } else {
                Ok("steady".to_string())
            }
        });
        Div("probe").build()
    }
    let tries_probe = tries.clone();
    host.mount(
        "Flaky",
        FlakyProbe {
            key,
            tries: tries_probe,
        },
        flaky_render,
    );
    host.run_until_idle();
    wait_for_tasks(&host.runtime(), 1);
    host.run_until_idle();
    assert_eq!(tries.load(Ordering::SeqCst), 3, "two failures then success");
    assert_eq!(signal.get(), FetchState::Ready("steady".to_string()));
}

/// Round 13.1: a zero-try budget is an authoring bug — loud
/// refusal, never a silent no-op fetch.
#[test]
#[should_panic(expected = "budget must be")]
fn retry_zero_budget_panics() {
    let host = ComponentHost::new();
    #[derive(Clone)]
    struct ZeroProbe {
        key: u64,
    }
    impl Props for ZeroProbe {}
    fn zero_render(ctx: &Ctx, p: &ZeroProbe) -> VNode {
        ctx.spawn_fetch_with_retry(p.key, 0, || Ok::<String, String>("x".to_string()));
        Div("probe").build()
    }
    host.mount(
        "Zero",
        ZeroProbe {
            key: fetch_key("test:retry-zero"),
        },
        zero_render,
    );
    host.run_until_idle();
}

/// Round 4.1 (web fetch driver): start sets Loading + mints
/// generations, stale resolves discard, unknown keys never
/// invent state, both result shapes apply.
#[test]
fn platform_fetch_start_resolve_and_generation_discard() {
    let host = ComponentHost::new();
    let key = fetch_key("web:quote");
    assert_eq!(host.fetch_snapshot(key), FetchState::Idle);
    let g1 = host.start_fetch(key);
    assert_eq!(host.fetch_snapshot(key), FetchState::Loading);
    let g2 = host.start_fetch(key);
    assert!(g2 > g1, "re-fetch mints a new generation");
    assert!(
        !host.resolve_fetch(key, g1, Ok("stale".to_string())),
        "stale result discards"
    );
    assert_eq!(
        host.fetch_snapshot(key),
        FetchState::Loading,
        "discard leaves Loading"
    );
    assert!(host.resolve_fetch(key, g2, Ok("hi".to_string())));
    assert_eq!(
        host.fetch_snapshot(key),
        FetchState::Ready("hi".to_string())
    );
    assert!(
        !host.resolve_fetch(0xDEAD, 1, Ok("x".to_string())),
        "unknown keys discard"
    );
    assert_eq!(
        host.fetch_snapshot(0xDEAD),
        FetchState::Idle,
        "discard never invents Loading"
    );
    let g3 = host.start_fetch(key);
    assert!(host.resolve_fetch(key, g3, Err("http 404".to_string())));
    assert_eq!(
        host.fetch_snapshot(key),
        FetchState::Failed("http 404".to_string())
    );
}

/// Round 13.3 (decision 310): paged loads track per-page
/// state and stream rows into the shared collection — ids mint
/// in stage order across pages.
#[test]
fn fetch_page_tracks_state_and_ingests() {
    use crate::store::Collection;
    let host = ComponentHost::new();
    let coll_key = fetch_key("test:pages");
    let coll: Collection<String> = Collection::new(&host.runtime(), coll_key);
    #[derive(Clone)]
    struct PageProbe {
        coll_key: u64,
        fired: Rc<Cell<bool>>,
    }
    impl Props for PageProbe {}
    fn page_render(ctx: &Ctx, p: &PageProbe) -> VNode {
        if !p.fired.get() {
            p.fired.set(true);
            ctx.spawn_fetch_page(p.coll_key, 0, 10, 3, |page, per_page| {
                assert_eq!((page, per_page), (0, 10), "page args thread through");
                Ok(vec!["a".to_string(), "b".to_string(), "c".to_string()])
            });
            ctx.spawn_fetch_page(p.coll_key, 1, 10, 3, |page, _| {
                Ok(vec![format!("p{page}-x"), format!("p{page}-y")])
            });
        }
        Div("probe").build()
    }
    host.mount(
        "Pages",
        PageProbe {
            coll_key,
            fired: Rc::new(Cell::new(false)),
        },
        page_render,
    );
    // Mount runs bodies synchronously: Loading is already set,
    // before any frame (and before the worker can answer — no
    // drain has run, so Ready is unreachable here).
    let state0 = host
        .runtime()
        .keyed_state::<FetchState<Vec<String>>>(page_key(coll_key, 0), || FetchState::Idle)
        .get();
    assert_eq!(state0, FetchState::Loading, "page 0 Loading first paint");
    host.run_until_idle();
    wait_for_tasks(&host.runtime(), 2);
    host.run_until_idle();
    let state0 = host
        .runtime()
        .keyed_state::<FetchState<Vec<String>>>(page_key(coll_key, 0), || FetchState::Idle)
        .get();
    assert_eq!(
        state0,
        FetchState::Ready(vec!["a".to_string(), "b".to_string(), "c".to_string()])
    );
    // Pages accumulate streaming-style, ids in stage order.
    assert_eq!(
        coll.rows()
            .iter()
            .map(|r| (r.id, r.value.clone()))
            .collect::<Vec<_>>(),
        vec![
            (crate::store::RowId(0), "a".to_string()),
            (crate::store::RowId(1), "b".to_string()),
            (crate::store::RowId(2), "c".to_string()),
            (crate::store::RowId(3), "p1-x".to_string()),
            (crate::store::RowId(4), "p1-y".to_string()),
        ]
    );
}

/// Round 13.3: a superseded load discards — the stale apply
/// never half-applies over the fresh query. Deterministic under
/// any thread interleave (single FIFO worker: either A spins
/// while B waits, or B applies before A even starts — both end
/// with only B committed).
#[test]
fn fetch_page_superseded_load_discards() {
    use crate::store::Collection;
    let host = ComponentHost::new();
    let coll_key = fetch_key("test:pages-super");
    let coll: Collection<String> = Collection::new(&host.runtime(), coll_key);
    let gate: Arc<std::sync::Mutex<bool>> = Arc::new(std::sync::Mutex::new(false));
    #[derive(Clone)]
    struct SuperProbe {
        coll_key: u64,
        fired: Rc<Cell<bool>>,
        gate: Arc<std::sync::Mutex<bool>>,
    }
    impl Props for SuperProbe {}
    fn super_render(ctx: &Ctx, p: &SuperProbe) -> VNode {
        if !p.fired.get() {
            p.fired.set(true);
            // Load A spins until the test opens the gate.
            let gate = p.gate.clone();
            ctx.spawn_fetch_page(p.coll_key, 0, 10, 1, move |_, _| {
                while !*gate.lock().expect("gate") {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Ok(vec!["stale".to_string()])
            });
            // Load B supersedes (same page, newer generation).
            ctx.spawn_fetch_page(p.coll_key, 0, 10, 1, |_, _| Ok(vec!["fresh".to_string()]));
        }
        Div("probe").build()
    }
    host.mount(
        "Super",
        SuperProbe {
            coll_key,
            fired: Rc::new(Cell::new(false)),
            gate: gate.clone(),
        },
        super_render,
    );
    // No idle pump here: mount already ran the bodies
    // synchronously (both loads submitted), and `run_until_idle`
    // would spin while A holds worker demand — demand clears
    // only when tasks complete, which needs the gate open.
    // Nothing applied yet (A spins on the closed gate, B waits
    // behind it in FIFO order — either way the collection is
    // untouched, deterministically).
    assert!(coll.is_empty(), "no page lands before the gate opens");
    *gate.lock().expect("gate") = true;
    wait_for_tasks(&host.runtime(), 2);
    host.run_until_idle();
    // Only the fresh load committed; the stale apply discarded.
    assert_eq!(
        coll.rows()
            .iter()
            .map(|r| r.value.clone())
            .collect::<Vec<_>>(),
        vec!["fresh".to_string()]
    );
    let state0 = host
        .runtime()
        .keyed_state::<FetchState<Vec<String>>>(page_key(coll_key, 0), || FetchState::Idle)
        .get();
    assert_eq!(state0, FetchState::Ready(vec!["fresh".to_string()]));
}

/// Round 13.3: zero-budget page loads refuse loudly (same class
/// as the unpaged retry refusal).
#[test]
#[should_panic(expected = "budget must be")]
fn fetch_page_zero_budget_panics() {
    let host = ComponentHost::new();
    #[derive(Clone)]
    struct ZeroPageProbe {
        coll_key: u64,
    }
    impl Props for ZeroPageProbe {}
    fn zero_page_render(ctx: &Ctx, p: &ZeroPageProbe) -> VNode {
        ctx.spawn_fetch_page(p.coll_key, 0, 10, 0, |_, _| {
            Ok::<Vec<String>, String>(Vec::new())
        });
        Div("probe").build()
    }
    host.mount(
        "ZeroPage",
        ZeroPageProbe {
            coll_key: fetch_key("test:pages-zero"),
        },
        zero_page_render,
    );
    host.run_until_idle();
}

/// The resolve schedules dependents (a component reading the
/// signal re-renders with the result — the binding's sync
/// then picks up DOM changes the same frame).
#[test]
fn resolve_schedules_readers() {
    #[derive(Clone)]
    struct RProps {
        renders: Rc<Cell<usize>>,
        seen: Rc<RefCell<Vec<FetchState<String>>>>,
    }
    impl Props for RProps {}
    fn render(ctx: &Ctx, p: &RProps) -> VNode {
        p.renders.set(p.renders.get() + 1);
        let state = ctx.fetch_state::<String>(ctx.fetch_key("web:probe"));
        p.seen.borrow_mut().push(state.get());
        Div("probe").build()
    }
    let host = ComponentHost::new();
    let props = RProps {
        renders: Rc::new(Cell::new(0)),
        seen: Rc::new(RefCell::new(Vec::new())),
    };
    host.mount("Probe", props.clone(), render);
    host.run_until_idle();
    let key = fetch_key("web:probe");
    let renders_before = props.renders.get();
    let g = host.start_fetch(key);
    host.run_until_idle();
    assert!(
        props.renders.get() > renders_before,
        "Loading re-renders readers"
    );
    assert!(host.resolve_fetch(key, g, Ok("hi".to_string())));
    host.run_until_idle();
    let seen = props.seen.borrow();
    assert_eq!(seen.last(), Some(&FetchState::Ready("hi".to_string())));
}

/// Phase 37b (decision 362 — G16): cancelling a parked task
/// removes it before running (never runs, stage `Cancelled`),
/// and its dependents still unblock (dep-gating is order, and
/// a cancelled dep orders like a done one).
#[test]
fn cancel_parked_task_never_runs_and_unblocks_dependents() {
    use std::sync::Mutex;
    let host = ComponentHost::new();
    let rt = host.runtime();
    let ran = Arc::new(Mutex::new(Vec::new()));
    // `dep` parks forever (depends on an id that never completes).
    let never = TaskId(u64::MAX - 1);
    let m = ran.clone();
    let dep: TaskId = rt.prepare_task(&[never], move |_scope| {
        m.lock().expect("marker").push("dep-ran");
    });
    assert_eq!(rt.task_stage(dep), Some(TaskStage::Queued));
    let m = ran.clone();
    let after: TaskId = rt.prepare_task(&[dep], move |_scope| {
        m.lock().expect("marker").push("after-ran");
    });
    assert!(rt.cancel_task(dep), "parked cancel reports true");
    assert_eq!(rt.task_stage(dep), Some(TaskStage::Cancelled));
    assert!(ran.lock().expect("marker").is_empty(), "never ran");
    // Dependent unblocks off the cancellation (Cancelled orders
    // like Done for dep purposes).
    wait_for_tasks(&rt, 1);
    host.run_until_idle();
    assert_eq!(*ran.lock().expect("marker"), vec!["after-ran"]);
    assert_eq!(rt.task_stage(after), Some(TaskStage::Done));
    // Idempotent re-cancel, and unknown/completed report false.
    assert!(rt.cancel_task(dep), "re-cancel stays true");
    assert!(!rt.cancel_task(never), "unknown ids report false");
    assert!(!rt.cancel_task(after), "completed ids report false");
    let log = rt.task_transitions();
    let walk: Vec<TaskStage> = log
        .iter()
        .filter(|(i, _)| *i == dep)
        .map(|(_, s)| *s)
        .collect();
    assert_eq!(walk, vec![TaskStage::Queued, TaskStage::Cancelled]);
}

/// Phase 37b: cancelling a running task sets the cooperative
/// token (the body observes it and stops early — preemption is
/// impossible on a thread boundary, stated).
#[test]
fn cancel_running_task_sets_the_cooperative_token() {
    use std::sync::mpsc::channel;
    let host = ComponentHost::new();
    let rt = host.runtime();
    let (tx, rx) = channel::<()>();
    // `started` proves the body runs (a queued task reports the
    // same `Ready` stage as a running one — the flag, not the
    // stage, makes "running" observable, so the cancel
    // deterministically lands mid-run).
    let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let st = started.clone();
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let s = stopped.clone();
    let id: TaskId = rt.spawn_task(move |scope| {
        st.store(true, std::sync::atomic::Ordering::SeqCst);
        rx.recv().expect("gate");
        if scope.is_cancelled() {
            s.store(true, std::sync::atomic::Ordering::SeqCst);
            return;
        }
        panic!("cancelled body must observe the token");
    });
    let mut waited = 0;
    while !started.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "task never started");
    }
    // The body blocks on the gate (cannot proceed without the
    // send), so this cancel deterministically lands mid-run.
    assert!(rt.cancel_task(id), "running cancel reports true");
    tx.send(()).expect("unblock");
    wait_for_tasks(&rt, 1);
    assert!(
        stopped.load(std::sync::atomic::Ordering::SeqCst),
        "body observed the token"
    );
    assert!(rt.is_task_cancelled(id), "flag stays honest");
}

/// Phase 37b: a queued fetch cancelled behind a gate never runs
/// (no body, no submit — the submit path is unreachable), and a
/// running fetch cancelled mid-flight resets to `Idle` through
/// `cancel_fetch` (never a stuck `Loading`, never a late
/// `Ready` over the reset).
#[test]
fn cancel_queued_fetch_never_runs_and_resets_idle() {
    use std::sync::mpsc::channel;
    let host = ComponentHost::new();
    let rt = host.runtime();
    // Gate the worker (queued fetches park behind it — submit
    // logs `Ready` synchronously, so the queued state is
    // deterministic, never a race). The `gated` flag proves the
    // gate holds the worker (spawn-then-cancel cannot slip
    // ahead of it).
    let (gate_tx, gate_rx) = channel::<()>();
    let gated = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let g = gated.clone();
    rt.spawn_task(move |_scope| {
        g.store(true, std::sync::atomic::Ordering::SeqCst);
        gate_rx.recv().expect("gate");
    });
    let mut waited = 0;
    while !gated.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "gate never engaged");
    }
    let key = fetch_key("test:cancel-queued");
    let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let r = ran.clone();
    #[derive(Clone)]
    struct QueueProbe {
        key: u64,
        id_slot: Rc<Cell<Option<TaskId>>>,
        ran: Arc<std::sync::atomic::AtomicBool>,
    }
    impl Props for QueueProbe {}
    fn queue_render(ctx: &Ctx, p: &QueueProbe) -> VNode {
        // Spawn once (the id slot gates re-spawns — bodies
        // re-run, spawns do not).
        if p.id_slot.get().is_none() {
            let r = p.ran.clone();
            let id = ctx.spawn_fetch(p.key, move || {
                r.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok::<String, _>("late".to_string())
            });
            p.id_slot.set(Some(id));
        }
        Div("probe").build()
    }
    let id_slot = Rc::new(Cell::new(None));
    host.mount(
        "Queue",
        QueueProbe {
            key,
            id_slot: id_slot.clone(),
            ran: r,
        },
        queue_render,
    );
    // Single frames only (never `run_until_idle` — a task parked
    // behind the gate is permanent frame demand by design: task
    // traffic wakes the loop for its results, so idling past an
    // unreleased gate would spin forever, correctly).
    for _ in 0..100 {
        host.run_once();
        if id_slot.get().is_some() {
            break;
        }
    }
    let id = id_slot.get().expect("spawned id");
    assert_eq!(
        rt.task_stage(id),
        Some(TaskStage::Ready),
        "fetch queues behind the gate"
    );
    // Cancel pre-run (runtime-level — the `cancel_fetch` recipe
    // below covers the state half through a body).
    assert!(rt.cancel_task(id), "queued cancel reports true");
    assert_eq!(rt.task_stage(id), Some(TaskStage::Cancelled));
    gate_tx.send(()).expect("unblock");
    wait_for_tasks(&rt, 1);
    host.run_until_idle();
    assert!(
        !ran.load(std::sync::atomic::Ordering::SeqCst),
        "cancelled body never ran"
    );
    assert_eq!(
        rt.keyed_state::<FetchState<String>>(key, || FetchState::Idle)
            .get(),
        FetchState::Loading,
        "no submit ever ran (the state half resets explicitly)"
    );
}

/// Phase 37b: `cancel_fetch` cancels mid-flight and resets to
/// `Idle` synchronously (the recipe — `cancel_task` plus the
/// state reset, one call).
#[test]
fn cancel_fetch_resets_midflight_to_idle() {
    use std::sync::mpsc::channel;
    use std::sync::Mutex;
    let host = ComponentHost::new();
    let key = fetch_key("test:cancel-midflight");
    let (tx, rx) = channel::<()>();
    let rx = Arc::new(Mutex::new(Some(rx)));
    #[derive(Clone)]
    struct MidProbe {
        key: u64,
        gate: Arc<Mutex<Option<std::sync::mpsc::Receiver<()>>>>,
        id_slot: Rc<Cell<Option<TaskId>>>,
        cancel_now: Rc<Cell<bool>>,
    }
    impl Props for MidProbe {}
    fn mid_render(ctx: &Ctx, p: &MidProbe) -> VNode {
        if p.id_slot.get().is_none() {
            let gate = p.gate.clone();
            let id = ctx.spawn_fetch(p.key, move || {
                // Block until the test releases (cancel lands
                // mid-flight deterministically — drops release
                // too, and the token decides, never the gate).
                let rx = gate.lock().expect("gate").take().expect("gate once");
                let _ = rx.recv();
                Ok::<String, _>("late".to_string())
            });
            p.id_slot.set(Some(id));
        } else if p.cancel_now.get() {
            p.cancel_now.set(false);
            assert!(
                ctx.cancel_fetch::<String>(p.key, p.id_slot.get().expect("id")),
                "mid-flight cancel reports true"
            );
        }
        Div("probe").build()
    }
    let props = MidProbe {
        key,
        gate: rx,
        id_slot: Rc::new(Cell::new(None)),
        cancel_now: Rc::new(Cell::new(false)),
    };
    let handle = host.mount("Mid", props.clone(), mid_render);
    host.run_until_idle();
    // Fetch is blocked in-flight (Loading shows); re-run with
    // the cancel flag (same body, second pass cancels).
    props.cancel_now.set(true);
    handle.set_props(props.clone());
    host.run_until_idle();
    assert_eq!(
        host.runtime()
            .keyed_state::<FetchState<String>>(key, || FetchState::Idle)
            .get(),
        FetchState::Idle,
        "cancel_fetch resets synchronously"
    );
    // Release the gate (drop wakes `recv` with `Err` in both
    // worlds — queued-cancel never took the receiver, mid-flight
    // observes the token; the token decides, never the gate).
    drop(tx);
    wait_for_tasks(&host.runtime(), 1);
    host.run_until_idle();
    assert_eq!(
        host.runtime()
            .keyed_state::<FetchState<String>>(key, || FetchState::Idle)
            .get(),
        FetchState::Idle,
        "no late Ready over the reset"
    );
}

/// Phase 37b (decision 362): the `Fetcher` seam — scripted
/// doubles serve routes and refuse unscripted URLs loudly;
/// `spawn_fetch_with` drives `FetchState` like `spawn_fetch`.
#[test]
fn fetcher_seam_scripts_and_drives_state() {
    let fetcher = ScriptedFetcher::new();
    assert_eq!(fetcher.name(), "scripted");
    fetcher.script("https://a.test/x", Ok("body-x".to_string()));
    assert_eq!(fetcher.fetch("https://a.test/x"), Ok("body-x".to_string()));
    assert!(
        fetcher
            .fetch("https://a.test/missing")
            .unwrap_err()
            .contains("unscripted url"),
        "misses refuse loudly"
    );
    // Closure backends name themselves (the native hook).
    let answering = ClosureFetcher("native-test", |url: &str| Ok(format!("got:{url}")));
    assert_eq!(answering.name(), "native-test");
    assert_eq!(
        answering.fetch("u").expect("closure fetch"),
        "got:u".to_string()
    );
    // The driver serves scripted bytes through FetchState.
    let host = ComponentHost::new();
    let key = fetch_key("test:fetcher-drive");
    let signal = host
        .runtime()
        .keyed_state::<FetchState<String>>(key, || FetchState::Idle);
    #[derive(Clone)]
    struct DriveProbe {
        key: u64,
        fetcher: ScriptedFetcher,
    }
    impl Props for DriveProbe {}
    fn drive_render(ctx: &Ctx, p: &DriveProbe) -> VNode {
        let f = std::sync::Arc::new(p.fetcher.clone());
        ctx.spawn_fetch_with(f, p.key, "https://a.test/x");
        Div("probe").build()
    }
    fetcher.script("https://a.test/x", Ok("driven".to_string()));
    host.mount(
        "Drive",
        DriveProbe {
            key,
            fetcher: fetcher.clone(),
        },
        drive_render,
    );
    host.run_until_idle();
    wait_for_tasks(&host.runtime(), 1);
    host.run_until_idle();
    assert_eq!(
        signal.get(),
        FetchState::Ready("driven".to_string()),
        "scripted bytes drive state"
    );
}
