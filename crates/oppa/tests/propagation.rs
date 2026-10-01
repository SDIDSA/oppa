//! Milestone #19 proofs: the propagation contract is executable —
//! topological-by-depth order, one run per node per pass, fold-in vs
//! re-entry, the 3-pass budget with a cycle-printing assert, the
//! structural-equality gate, pull-recompute, and memos-never-write.

use std::cell::Cell;
use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use oppa::{untrack, Runtime};

fn counter() -> Rc<Cell<u32>> {
    Rc::new(Cell::new(0))
}

fn log() -> Rc<RefCell<Vec<String>>> {
    Rc::new(RefCell::new(Vec::new()))
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    (**payload)
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| (**payload).downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

/// Diamond: S -> {memo A, memo C} -> effect E. One write must give exactly
/// one E run that sees BOTH settled values (no glitch, no re-entry).
#[test]
fn diamond_runs_effect_once_with_settled_values() {
    let rt = Runtime::new();
    let s = rt.signal(1i32);
    let a = rt.memo_named("a", {
        let s = s.clone();
        move || s.get() * 2
    });
    let c = rt.memo_named("c", {
        let s = s.clone();
        move || s.get() + 10
    });
    let runs = counter();
    let seen = Rc::new(RefCell::new((0i32, 0i32)));
    let e = rt.effect_named("e", {
        let a = a.clone();
        let c = c.clone();
        let runs = runs.clone();
        let seen = seen.clone();
        move || {
            runs.set(runs.get() + 1);
            *seen.borrow_mut() = (a.read(), c.read());
        }
    });
    assert_eq!(runs.get(), 1, "initial run at creation");
    s.set(5);
    assert_eq!(rt.run_until_idle(), 1);
    assert_eq!(runs.get(), 2, "exactly one re-run for one write");
    assert_eq!(*seen.borrow(), (10, 15), "downstream sees settled values");
    assert_eq!(rt.stats().passes_last_frame, 1, "single pass, no re-entry");
    let _ = e;
}

/// A memo chain settles top-down: the effect reading the LAST memo runs
/// once, seeing the fully settled chain; each memo recomputes exactly once.
#[test]
fn deep_chain_is_topological_and_single_run() {
    let rt = Runtime::new();
    let s = rt.signal(1i32);
    let m1_runs = counter();
    let m2_runs = counter();
    let m3_runs = counter();
    let m1 = rt.memo_named("m1", {
        let s = s.clone();
        let runs = m1_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            s.get() + 1
        }
    });
    let m2 = rt.memo_named("m2", {
        let m1 = m1.clone();
        let runs = m2_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            m1.read() * 2
        }
    });
    let m3 = rt.memo_named("m3", {
        let m2 = m2.clone();
        let runs = m3_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            m2.read() + 100
        }
    });
    let runs = counter();
    let seen = Rc::new(RefCell::new(0i32));
    rt.effect_named("e", {
        let m3 = m3.clone();
        let runs = runs.clone();
        let seen = seen.clone();
        move || {
            runs.set(runs.get() + 1);
            *seen.borrow_mut() = m3.read();
        }
    });
    assert_eq!(*seen.borrow(), 104, "initial run sees settled chain");
    s.set(2);
    rt.run_until_idle();
    assert_eq!(m1_runs.get(), 2);
    assert_eq!(m2_runs.get(), 2);
    assert_eq!(m3_runs.get(), 2, "each memo ran at most once per pass");
    assert_eq!(runs.get(), 2);
    assert_eq!(*seen.borrow(), 106);
    assert_eq!(rt.stats().passes_last_frame, 1);
}

/// Same depth: tie order is call-site-stable creation order, every pass.
#[test]
fn ties_break_by_creation_order() {
    let rt = Runtime::new();
    let s = rt.signal(0u8);
    let log = log();
    let a = rt.effect_named("a", {
        let s = s.clone();
        let log = log.clone();
        move || {
            s.get();
            log.borrow_mut().push("a".to_string());
        }
    });
    let b = rt.effect_named("b", {
        let s = s.clone();
        let log = log.clone();
        move || {
            s.get();
            log.borrow_mut().push("b".to_string());
        }
    });
    let c = rt.effect_named("c", {
        let s = s.clone();
        let log = log.clone();
        move || {
            s.get();
            log.borrow_mut().push("c".to_string());
        }
    });
    assert_eq!(
        *log.borrow(),
        vec!["a", "b", "c"],
        "creation order, initial runs"
    );
    log.borrow_mut().clear();
    s.set(1);
    rt.run_until_idle();
    assert_eq!(*log.borrow(), vec!["a", "b", "c"]);
    log.borrow_mut().clear();
    s.set(2);
    rt.run_until_idle();
    assert_eq!(
        *log.borrow(),
        vec!["a", "b", "c"],
        "order stable across passes"
    );
    let _ = (a, b, c);
}

/// Writes during a run fold in when downstream has not yet run: one pass,
/// downstream still sees the value written mid-pass.
#[test]
fn mid_pass_write_folds_in_downstream() {
    let rt = Runtime::new();
    let s1 = rt.signal(0i32);
    let s2 = rt.signal(0i32);
    let m = rt.memo_named("m", {
        let s1 = s1.clone();
        move || s1.get() * 2
    });
    let writer_runs = counter();
    rt.effect_named("writer", {
        let m = m.clone();
        let s2 = s2.clone();
        let runs = writer_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            s2.set(m.read());
        }
    });
    let downstream_runs = counter();
    let downstream_seen = Rc::new(RefCell::new(0i32));
    rt.effect_named("downstream", {
        let s2 = s2.clone();
        let runs = downstream_runs.clone();
        let seen = downstream_seen.clone();
        move || {
            runs.set(runs.get() + 1);
            *seen.borrow_mut() = s2.get();
        }
    });
    assert_eq!(*downstream_seen.borrow(), 0);
    s1.set(3);
    rt.run_until_idle();
    assert_eq!(
        *downstream_seen.borrow(),
        6,
        "folded write reached downstream"
    );
    assert_eq!(writer_runs.get(), 2);
    assert_eq!(downstream_runs.get(), 2);
    assert_eq!(rt.stats().passes_last_frame, 1, "no re-entry needed");
}

/// A write whose downstream already ran schedules a re-entry pass instead
/// of reordering live: 2 passes, downstream runs twice, writer once.
#[test]
fn late_write_schedules_re_entry_pass() {
    let rt = Runtime::new();
    let s1 = rt.signal(0i32);
    let s2 = rt.signal(0i32);
    let early_runs = counter();
    let early_seen = Rc::new(RefCell::new(Vec::new()));
    rt.effect_named("early", {
        let s1 = s1.clone();
        let runs = early_runs.clone();
        let seen = early_seen.clone();
        move || {
            runs.set(runs.get() + 1);
            seen.borrow_mut().push(s1.get());
        }
    });
    let late_runs = counter();
    let run_no = Rc::new(Cell::new(0u32));
    rt.effect_named("late", {
        let s2 = s2.clone();
        let s1 = s1.clone();
        let runs = late_runs.clone();
        let run_no = run_no.clone();
        move || {
            runs.set(runs.get() + 1);
            s2.get();
            // Write exactly once, during the first scheduled pass, so the
            // write's downstream (already run) must re-enter.
            let n = run_no.get();
            run_no.set(n + 1);
            if n == 1 {
                s1.set(s1.get() + 1);
            }
        }
    });
    assert_eq!(*early_seen.borrow(), vec![0], "initial run");
    s1.set(10);
    s2.set(1);
    rt.run_until_idle();
    assert_eq!(
        *early_seen.borrow(),
        vec![0, 10, 11],
        "first pass sees the input write; re-entry sees the late write"
    );
    assert_eq!(early_runs.get(), 3);
    assert_eq!(
        late_runs.get(),
        3,
        "writer ran once (pass one) plus its re-entry"
    );
    assert_eq!(rt.stats().passes_last_frame, 2, "one re-entry pass");
}

/// The budget actually bounds re-entry: a write-back loop is caught on the
/// 4th pass attempt, and the debug assert prints the dependency cycle.
#[test]
#[cfg(debug_assertions)]
fn budget_exceeded_debug_asserts_with_cycle_path() {
    let rt = Runtime::new();
    let s = rt.signal_named("counter", 0i32);
    let runs = counter();
    let e = rt.effect_named("cycler", {
        let s = s.clone();
        let runs = runs.clone();
        move || {
            runs.set(runs.get() + 1);
            let v = s.get();
            s.set(v + 1);
        }
    });
    assert_eq!(runs.get(), 1, "initial run at creation");
    // The initial run's own write had no edges yet (correctly): make the
    // write-back loop live with one external write, then the loop must be
    // caught on the 4th pass.
    s.set(5);
    let caught = catch_unwind(AssertUnwindSafe(|| rt.run_until_idle()));
    let payload = match caught {
        Ok(frames) => panic!("cyclic write-back did not assert; ran {frames} frames"),
        Err(payload) => payload,
    };
    let msg = panic_message(&payload);
    assert!(
        msg.contains("cycle:"),
        "message must identify the cycle: {msg}"
    );
    assert!(
        msg.contains("cycler"),
        "message must name the looping node: {msg}"
    );
    assert!(
        msg.contains("counter"),
        "message must name the signal in the loop: {msg}"
    );
    assert!(msg.contains("->"), "message must print the chain: {msg}");
    assert!(
        msg.contains("3 passes/frame"),
        "message must state the budget: {msg}"
    );
    assert_eq!(
        runs.get(),
        4,
        "initial run + exactly three passes before the assert"
    );
    let _ = e;
}

/// Ping-pong write-back between two effects: bounded at 3 passes, then the
/// assert fires with the cycle through both nodes.
#[test]
#[cfg(debug_assertions)]
fn ping_pong_cycle_hits_budget_with_two_node_chain() {
    let rt = Runtime::new();
    let s1 = rt.signal_named("a_state", 0i32);
    let s2 = rt.signal_named("b_state", 0i32);
    let a_runs = counter();
    let b_runs = counter();
    let a = rt.effect_named("a_eff", {
        let s1 = s1.clone();
        let s2 = s2.clone();
        let runs = a_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            s2.set(s1.get() + 1);
        }
    });
    let b = rt.effect_named("b_eff", {
        let s2 = s2.clone();
        let s1 = s1.clone();
        let runs = b_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            s1.set(s2.get() + 1);
        }
    });
    let _ = (a, b);
    // b_eff's initial run writes s1, whose dependent a_eff is already
    // registered: the ping-pong loop is live from the start.
    let caught = catch_unwind(AssertUnwindSafe(|| rt.run_until_idle()));
    let payload = match caught {
        Ok(frames) => panic!("ping-pong cycle did not assert; ran {frames} frames"),
        Err(payload) => payload,
    };
    let msg = panic_message(&payload);
    assert!(
        msg.contains("a_eff") && msg.contains("b_eff"),
        "both nodes named: {msg}"
    );
    assert!(
        msg.contains("a_state") || msg.contains("b_state"),
        "signals named: {msg}"
    );
    assert_eq!(a_runs.get(), 4);
    assert_eq!(b_runs.get(), 4);
}

/// A guarded write-back settles naturally inside the budget when the loop
/// closes before pass 4: writes during a run are legal, not a bug.
#[test]
fn guarded_write_back_settles_within_budget() {
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let runs = counter();
    rt.effect_named("bounded", {
        let s = s.clone();
        let runs = runs.clone();
        move || {
            runs.set(runs.get() + 1);
            let v = s.get();
            if v < 2 {
                s.set(v + 1);
            }
        }
    });
    // The initial run's write to its own dependency already schedules the
    // next run (immediate edge registration): 0->1 settles, pass2 sees 2.
    rt.run_until_idle();
    assert_eq!(
        runs.get(),
        3,
        "0->1 in the initial run, then 1->2, then sees 2"
    );
    assert_eq!(
        rt.stats().passes_last_frame,
        2,
        "used re-entry but stayed inside the budget"
    );
    rt.run_until_idle();
    assert_eq!(runs.get(), 3, "settled: no further wake, no livelock");
}

/// The structural-equality gate: a memo whose recomputed value equals the
/// old one does not invalidate dependents.
#[test]
fn equality_gate_blocks_downstream_invalidation() {
    let rt = Runtime::new();
    let s = rt.signal(1i32);
    let m_runs = counter();
    let m = rt.memo_named("const_zero", {
        let s = s.clone();
        let runs = m_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            let _ = s.get();
            0i32
        }
    });
    let e_runs = counter();
    let e = rt.effect_named("e", {
        let m = m.clone();
        let runs = e_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            m.read();
        }
    });
    assert_eq!(e_runs.get(), 1);
    s.set(2);
    s.set(3);
    s.set(4);
    rt.run_until_idle();
    assert_eq!(m_runs.get(), 2, "memo recomputed once despite three writes");
    assert_eq!(
        e_runs.get(),
        1,
        "equal recomputed value did not re-run the effect"
    );
    assert_eq!(rt.stats().passes_last_frame, 1);
    let _ = e;
}

/// A memo that CHANGES still propagates: the gate is per-recompute, not a
/// blanket suppression.
#[test]
fn changing_memo_still_propagates() {
    let rt = Runtime::new();
    let s = rt.signal(1i32);
    let m = rt.memo_named("double", {
        let s = s.clone();
        move || s.get() * 2
    });
    let e_runs = counter();
    let seen = Rc::new(RefCell::new(0i32));
    rt.effect_named("e", {
        let m = m.clone();
        let runs = e_runs.clone();
        let seen = seen.clone();
        move || {
            runs.set(runs.get() + 1);
            *seen.borrow_mut() = m.read();
        }
    });
    s.set(7);
    rt.run_until_idle();
    assert_eq!(*seen.borrow(), 14);
    assert_eq!(e_runs.get(), 2);
}

/// `memo_with_eq`: the per-memo comparator override.
#[test]
fn memo_with_eq_custom_comparator() {
    let rt = Runtime::new();
    let s = rt.signal(0u32);
    let m = rt.memo_with_eq(|a: &u32, b: &u32| a % 2 == b % 2, {
        let s = s.clone();
        move || s.get()
    });
    let e_runs = counter();
    let e = rt.effect_named("e", {
        let m = m.clone();
        let runs = e_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            m.read();
        }
    });
    assert_eq!(e_runs.get(), 1);
    s.set(2);
    rt.run_until_idle();
    assert_eq!(
        e_runs.get(),
        1,
        "same parity: custom cmp says equal, no propagation"
    );
    s.set(3);
    rt.run_until_idle();
    assert_eq!(e_runs.get(), 2, "parity flip: custom cmp says changed");
    let _ = e;
}

/// The hard invariant: memos never write signals. Asserted, not documented.
#[test]
fn memo_writing_a_signal_is_a_loud_failure() {
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let m = rt.memo_named("writer", {
        let s = s.clone();
        move || {
            let v = s.get();
            s.set(v + 1);
            v
        }
    });
    // Memos are lazy: the violation fires when the memo is computed.
    let caught = catch_unwind(AssertUnwindSafe(|| {
        let _ = m.read();
    }));
    let payload = match caught {
        Ok(_) => panic!("a memo's signal write did not fail loudly"),
        Err(payload) => payload,
    };
    let msg = panic_message(&payload);
    assert!(
        msg.contains("memos never write signals"),
        "invariant message must be explicit: {msg}"
    );
}

/// The twin escape: creating an effect inside a memo is refused too — the
/// effect's initial run would perform the memo's writes for it.
#[test]
fn memo_creating_an_effect_is_a_loud_failure() {
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let rt_for_memo = rt.clone();
    let m = rt_for_memo.memo_named("spawner", {
        let s = s.clone();
        move || {
            rt.effect({
                let s = s.clone();
                move || {
                    s.set(1);
                }
            });
            0i32
        }
    });
    let caught = catch_unwind(AssertUnwindSafe(|| {
        let _ = m.read();
    }));
    let payload = match caught {
        Ok(_) => panic!("a memo creating an effect did not fail loudly"),
        Err(payload) => payload,
    };
    let msg = panic_message(&payload);
    assert!(msg.contains("memo tried to create an effect"), "{msg}");
}

/// Nested batches merge: one invalidation application at the outermost end.
#[test]
fn batch_and_nested_batch_apply_once() {
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let runs = counter();
    let seen = Rc::new(RefCell::new(Vec::new()));
    rt.effect_named("e", {
        let s = s.clone();
        let runs = runs.clone();
        let seen = seen.clone();
        move || {
            runs.set(runs.get() + 1);
            seen.borrow_mut().push(s.get());
        }
    });
    assert_eq!(*seen.borrow(), vec![0]);
    {
        let _outer = rt.batch();
        s.set(1);
        {
            let _inner = rt.batch();
            s.set(2);
        }
        s.set(3);
    }
    assert_eq!(runs.get(), 1, "no propagation inside the batch");
    rt.run_until_idle();
    assert_eq!(runs.get(), 2);
    assert_eq!(*seen.borrow(), vec![0, 3], "final value, one run");
    assert_eq!(rt.stats().passes_last_frame, 1);
}

/// A batch opened in INPUT closes before EFFECTS of the same frame:
/// handler write -> settled visuals within one frame.
#[test]
fn input_batch_completes_within_the_same_frame() {
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let runs = counter();
    let seen = Rc::new(RefCell::new(0i32));
    rt.effect_named("e", {
        let s = s.clone();
        let runs = runs.clone();
        let seen = seen.clone();
        move || {
            runs.set(runs.get() + 1);
            *seen.borrow_mut() = s.get();
        }
    });
    let id = oppa::HandlerId::from_symbol("btn.press");
    rt.register_handler(id, {
        let s = s.clone();
        move || {
            s.set(1);
            s.set(2);
            s.set(3);
        }
    });
    rt.push_event(oppa::Event {
        kind: oppa::EventKind::Press,
        handler: id,
    });
    assert!(rt.run_once());
    assert_eq!(*seen.borrow(), 3, "input -> visual within one frame");
    assert_eq!(runs.get(), 2);
    assert_eq!(rt.stats().passes_last_frame, 1, "batched writes = one pass");
}

/// Pull-recompute: reading a dirty memo outside any frame recomputes it on
/// demand, tracked normally, without waking a frame — and the later
/// EFFECTS pass does not recompute it a second time.
#[test]
fn pull_recompute_outside_frames_is_on_demand_and_tracked() {
    let rt = Runtime::new();
    let s = rt.signal(1i32);
    let m_runs = counter();
    let m = rt.memo_named("double", {
        let s = s.clone();
        let runs = m_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            s.get() * 2
        }
    });
    assert_eq!(m.read(), 2, "initial pull");
    assert_eq!(m_runs.get(), 1);
    s.set(5);
    assert_eq!(rt.stats().frames, 0, "a set alone does not run a frame");
    assert_eq!(m.read(), 10, "dirty memo pull-recomputes on read");
    assert_eq!(m_runs.get(), 2);
    assert_eq!(rt.stats().frames, 0, "pull is not a frame");
    let e_runs = counter();
    let seen = Rc::new(RefCell::new(0i32));
    rt.effect_named("e", {
        let m = m.clone();
        let runs = e_runs.clone();
        let seen = seen.clone();
        move || {
            runs.set(runs.get() + 1);
            *seen.borrow_mut() = m.read();
        }
    });
    assert_eq!(e_runs.get(), 1);
    assert_eq!(
        *seen.borrow(),
        10,
        "effect's initial run sees the pulled value"
    );
    assert_eq!(
        rt.run_until_idle(),
        1,
        "the set's dirt wakes exactly one frame"
    );
    assert_eq!(
        m_runs.get(),
        2,
        "memo was NOT recomputed again in EFFECTS (pull settled it)"
    );
    assert_eq!(e_runs.get(), 1);
}

/// A memo read mid-pass by an effect that has just written its dependency
/// pulls fresh (fold/re-entry machinery) and claims its single run for the
/// pass.
#[test]
fn memo_pull_mid_pass_claims_its_single_run() {
    let rt = Runtime::new();
    let src = rt.signal(0i32);
    let other = rt.signal(0i32);
    let m_runs = counter();
    let m = rt.memo_named("m", {
        let src = src.clone();
        let runs = m_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            src.get() + 1
        }
    });
    let reader_runs = counter();
    let reader_seen = Rc::new(RefCell::new(Vec::new()));
    let wrote = Rc::new(Cell::new(false));
    rt.effect_named("reader", {
        let m = m.clone();
        let other = other.clone();
        let src = src.clone();
        let runs = reader_runs.clone();
        let seen = reader_seen.clone();
        let wrote = wrote.clone();
        move || {
            runs.set(runs.get() + 1);
            // Write exactly once, so the loop settles within the budget.
            if !wrote.get() {
                wrote.set(true);
                src.set(src.get() + 1);
            }
            other.get();
            seen.borrow_mut().push(m.read());
        }
    });
    // Initial run: the reader writes src (0->1), then pulls m, which
    // pull-recomputes on demand from the settled value.
    assert_eq!(*reader_seen.borrow(), vec![2], "initial run");
    src.set(10);
    other.set(1);
    rt.run_until_idle();
    assert_eq!(m_runs.get(), 2, "memo computed once per pass, not per read");
    assert_eq!(
        *reader_seen.borrow().last().unwrap(),
        11,
        "pull saw the folded write"
    );
    assert_eq!(reader_runs.get(), 2);
    assert_eq!(
        rt.stats().passes_last_frame,
        1,
        "the reader runs after the memo: pure fold"
    );
}

/// untrack: reads inside the closure are not dependencies.
#[test]
fn untracked_reads_do_not_invalidate() {
    let rt = Runtime::new();
    let s = rt.signal(1i32);
    let runs = counter();
    let last_seen = Rc::new(RefCell::new(0i32));
    rt.effect_named("untracked", {
        let s = s.clone();
        let runs = runs.clone();
        let seen = last_seen.clone();
        move || {
            runs.set(runs.get() + 1);
            *seen.borrow_mut() = untrack(|| s.get());
        }
    });
    assert_eq!(runs.get(), 1);
    assert_eq!(*last_seen.borrow(), 1);
    s.set(2);
    rt.run_until_idle();
    assert_eq!(runs.get(), 1, "untracked read created no dependency");
    assert_eq!(untrack(|| s.get()), 2, "the value itself is still fresh");
}

/// Release-build deferral: past the budget the unsettled dirt is deferred
/// once to the next frame's EFFECTS (same budget), then parked — the
/// subtree stops updating with a logged reason, never a silent livelock.
#[test]
#[cfg(not(debug_assertions))]
fn budget_exceeded_release_defers_then_parks() {
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let runs = counter();
    rt.effect_named("cycler", {
        let s = s.clone();
        let runs = runs.clone();
        move || {
            runs.set(runs.get() + 1);
            let v = s.get();
            s.set(v + 1);
        }
    });
    let frames = rt.run_until_idle();
    assert_eq!(
        frames, 2,
        "one frame attempts, one frame retries, then park"
    );
    let after_first = runs.get();
    rt.run_until_idle();
    assert_eq!(runs.get(), after_first, "parked subtree stops updating");
}
