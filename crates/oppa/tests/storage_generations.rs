//! Milestone #11 proofs: generation checks are mechanical. Retiring a
//! generation makes every access through it fail loudly — panic with the
//! refusing reason — never silently serving stale data. Covers the generic
//! arena, the `NodeId` retained-node arena, and reactive signal/memo slots
//! (the mechanism §9.6's hot-reload identity-churn safety argument depends
//! on).

use std::panic::catch_unwind;

use oppa::{GenArena, NodeArena, NodeId, Runtime, SlotError};

struct SilenceHook;

impl SilenceHook {
    fn set() -> Self {
        std::panic::set_hook(Box::new(|_| {}));
        Self
    }
}

impl Drop for SilenceHook {
    fn drop(&mut self) {
        let _ = std::panic::take_hook();
    }
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    (**payload)
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| (**payload).downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

fn assert_panics_with(f: impl FnOnce(), needle: &str) {
    let payload = catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_err();
    let msg = panic_message(&payload);
    assert!(
        msg.contains(needle),
        "expected panic containing {needle:?}, got: {msg}"
    );
}

#[test]
fn arena_retire_then_get_panics_loudly() {
    let mut arena: GenArena<u64> = GenArena::new();
    let id = arena.alloc(1234);
    arena.retire(id).unwrap();
    assert!(!arena.is_alive(id));
    let _hook = SilenceHook::set();
    assert_panics_with(
        move || {
            arena.get(id);
        },
        "retired",
    );
}

#[test]
fn arena_handle_of_reused_slot_never_aliases_new_data() {
    let mut arena: NodeArena<&'static str> = NodeArena::new();
    let old = arena.alloc("OLD");
    arena.retire(old).unwrap();
    let new = arena.alloc("NEW");
    assert_eq!(new.index(), old.index(), "same slot index");
    assert_ne!(
        new.generation(),
        old.generation(),
        "generation bumped on reuse"
    );
    assert_eq!(
        *arena.get(new),
        "NEW",
        "the live handle sees the new occupant"
    );
    // Retire-by-stale-handle is refused too:
    assert!(matches!(
        arena.retire(old),
        Err(SlotError::StaleGeneration { .. })
    ));
    // And the new occupant is untouched:
    assert_eq!(*arena.get(new), "NEW");
    let _hook = SilenceHook::set();
    assert_panics_with(
        move || {
            let _ = arena.get(old);
        },
        "stale identity",
    );
}

#[test]
fn node_arena_roundtrip_and_loud_retirement() {
    let mut arena: NodeArena<u32> = NodeArena::new();
    let unused = NodeId::new(0, 0);
    let _ = unused;
    let node = arena.alloc(77);
    assert_eq!(*arena.get(node), 77);
    arena.retire(node).unwrap();
    let _hook = SilenceHook::set();
    assert_panics_with(
        move || {
            arena.get(node);
        },
        "retired",
    );
}

#[test]
fn retired_signal_refuses_reads_and_writes() {
    let rt = Runtime::new();
    let s = rt.signal(5i32);
    assert_eq!(s.get(), 5);
    rt.retire_signal(&s);
    let _hook = SilenceHook::set();
    assert_panics_with(
        {
            let s = s.clone();
            move || {
                s.get();
            }
        },
        "retired",
    );
    assert_panics_with(
        {
            let s = s.clone();
            move || {
                s.set(6);
            }
        },
        "retired",
    );
    assert_panics_with(
        {
            let s = s.clone();
            move || {
                s.get_arc();
            }
        },
        "retired",
    );
    // Double-retire is refused loudly:
    assert_panics_with(
        {
            let rt = rt.clone();
            move || {
                rt.retire_signal(&s);
            }
        },
        "already retired",
    );
}

#[test]
fn retired_memo_refuses_reads() {
    let rt = Runtime::new();
    let m = rt.memo(|| 7i32);
    assert_eq!(m.read(), 7);
    rt.retire_memo(&m);
    let _hook = SilenceHook::set();
    assert_panics_with(
        {
            let m = m.clone();
            move || {
                m.read();
            }
        },
        "retired",
    );
}

#[test]
fn memo_depending_on_retired_signal_fails_loud_not_stale() {
    let rt = Runtime::new();
    let s = rt.signal(1i32);
    let m = rt.memo_named("m", {
        let s = s.clone();
        move || s.get() + 1
    });
    assert_eq!(m.read(), 2);
    // Retire the slot, then reuse it: a fresh alloc takes the same index
    // under a new generation, so the memo's old dep handle is now a
    // stale-generation identity.
    rt.retire_signal(&s);
    let fresh = rt.signal(999i32);
    assert_eq!(fresh.get(), 999);
    // Any read through the memo must fail loudly rather than serve the
    // stale pre-retirement value:
    let _hook = SilenceHook::set();
    assert_panics_with(
        {
            let m = m.clone();
            move || {
                m.read();
            }
        },
        "retired",
    );
}
