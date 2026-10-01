//! Blessed fetch→render (G7 — decisions 220–221).
//!
//! The executor (`ctx.spawn`, generation-tagged submits, INPUT drain)
//! existed, but only as tribal knowledge in a test comment
//! (`reload_cycle.rs`: "tasks cannot capture signals (`!Send`) —
//! rendezvous through `keyed_state`"). This module turns that into
//! the blessed app-level async pattern: one state shape plus one
//! driver, shared by native and web.
//!
//! Design (see decisions 220–221 in `docs/04-planning/state.md`):
//!
//! - **One state shape.** [`FetchState<T>`] (`Idle/Loading/Ready/
//!   Failed`) lives in a keyed signal ([`Ctx::fetch_state`]) —
//!   components render it like any other state (match + VNode).
//! - **Native driver.** [`Ctx::spawn_fetch`] sets `Loading`
//!   synchronously, runs `fetch` on the executor thread, and submits
//!   `Ready/Failed` through the `keyed_state` rendezvous (the only
//!   `Send`-safe path to UI state — signals never cross threads,
//!   locked by doctest). Generation tags ride along, so a result
//!   landing after a swap is discarded, never applied half-swapped
//!   (§9.6, proven by M9). [`Ctx::spawn_fetch_with_retry`] (Round
//!   13.1) adds a retry budget: up to N total tries in one task run,
//!   exhaustion surfacing a distinct `Failed("<last> (retry budget
//!   exhausted after N attempts)")`.
//! - **Preparation stages.** Every submit — task or fetch — walks
//!   `Queued → Prepared → Ready → Done` (Round 13.1, decision 308):
//!   [`Ctx::prepare_task`] parks bodies with dependencies until
//!   every dep id reads `Done`, then promotes and schedules on the
//!   single worker thread (FIFO, id-ordered promotion — the walk is
//!   total, therefore testable). Deps gate order, never success.
//! - **Wasm driver.** No threads on wasm — `spawn_task`/`spawn_fetch`
//!   refuse loudly there (explicit panic, not the cryptic OS stub).
//!   The platform binding resolves the promise and writes the *same*
//!   keyed signal from the UI thread, then requests a frame: same
//!   shape, mechanism-only difference (the verdict-(b) split applied
//!   to async).
//! - **Key namespacing.** Keyed state is one global u64 namespace per
//!   runtime — [`fetch_key`] hashes a `"route:name"` string (FNV-1a,
//!   the handler-id hash), so two features never collide silently.
//!
//! Out of scope: fetch backends themselves (OQ-G5-5 owns network);
//! cancellation tokens (generation discard already drops retired
//! results — explicit cancel is OQ-G7-2); progress reporting
//! (OQ-G7-3); a reload-harness example (G14 owns app/reload wiring).

use crate::hash::fnv1a64;

/// Remote-data lifecycle for one fetch (clone it in/out of its keyed
/// signal — the stack never schedules by itself).
#[derive(Clone, Debug, PartialEq)]
pub enum FetchState<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

/// Namespaces a fetch key: `fetch_key("settings:avatar")` (FNV-1a over
/// the string — deterministic across runs, same hash as handler ids).
pub fn fetch_key(name: &str) -> u64 {
    fnv1a64(name.as_bytes())
}

/// Page-state key for a paged collection load (Round 13.3,
/// decision 310): `FetchState<Vec<T>>` lives here (Loading →
/// Ready(page rows) / Failed), so each page tracks independently
/// while rows stream into the shared collection.
pub fn page_key(collection_key: u64, page: usize) -> u64 {
    fetch_key(&format!("page:{collection_key}:{page}"))
}

/// Load-generation key for a paged load (Round 13.3): a keyed u64
/// bumped per load; stale applies discard against it (the
/// start/resolve guard shape, per page instead of per field).
pub fn page_gen_key(collection_key: u64, page: usize) -> u64 {
    fetch_key(&format!("page-gen:{collection_key}:{page}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{ComponentHost, Ctx, Props};
    use crate::reactive::Signal;
    use crate::vnode::{Div, VNode};
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
}
