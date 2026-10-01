//! Runtime-level service proofs: the handler registry round-trips through
//! event dispatch (handler-as-id, locked #11's capture shape), a swap
//! re-resolves the SAME stable symbol hash to new code (§5.3 — the key
//! scheme is load-bearing for hot reload), misses are loud, and reload is
//! an atomic within-phase apply.

use std::cell::Cell;
use std::panic::catch_unwind;
use std::rc::Rc;

use oppa::{Event, EventKind, HandlerFn, HandlerId, Runtime};

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

fn expect_panic(f: impl FnOnce()) -> String {
    let payload = catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_err();
    panic_message(&payload)
}

#[test]
fn handler_id_is_the_stable_symbol_hash() {
    let a = HandlerId::from_symbol("toggle.on_press");
    let b = HandlerId::from_symbol("toggle.on_press");
    assert_eq!(a, b, "same symbol = same id, across runs and hot swaps");
    assert_ne!(a, HandlerId::from_symbol("toggle.on_release"));
    let built = HandlerId::from_hash(oppa::SymbolHash::of("toggle.on_press"));
    assert_eq!(a, built, "HandlerId::from_hash agrees with from_symbol");
}

#[test]
fn dispatch_roundtrip_through_event_queue_and_frame() {
    let rt = Runtime::new();
    let hits = Rc::new(Cell::new(0u32));
    let id = HandlerId::from_symbol("btn.press");
    rt.register_handler(id, {
        let hits = hits.clone();
        move || hits.set(hits.get() + 1)
    });
    rt.push_event(Event {
        kind: EventKind::Press,
        handler: id,
    });
    rt.push_event(Event {
        kind: EventKind::Release,
        handler: id,
    });
    rt.run_until_idle();
    assert_eq!(
        hits.get(),
        2,
        "each event dispatched its handler exactly once"
    );
}

#[test]
fn registry_swap_re_resolves_the_same_hash_to_new_code() {
    let rt = Runtime::new();
    let hits = Rc::new(Cell::new(0u32));
    let id = HandlerId::from_symbol("view.render");
    rt.register_handler(id, {
        let hits = hits.clone();
        move || hits.set(hits.get() + 1)
    });
    rt.dispatch(Event {
        kind: EventKind::Press,
        handler: id,
    });
    assert_eq!(hits.get(), 1);

    // Simulated hot swap: new code table, same symbol hash.
    let mut next = std::collections::HashMap::new();
    next.insert(id, {
        let hits = hits.clone();
        Box::new(move || hits.set(hits.get() * 10)) as HandlerFn
    });
    rt.swap_handlers(next);

    rt.dispatch(Event {
        kind: EventKind::Press,
        handler: id,
    });
    assert_eq!(
        hits.get(),
        10,
        "the same id now resolves to the swapped-in function"
    );
}

#[test]
fn dispatch_of_unresolved_id_is_a_loud_miss() {
    let rt = Runtime::new();
    let id = HandlerId::from_symbol("never.registered");
    let _hook = SilenceHook::set();
    let msg = expect_panic({
        let rt = rt.clone();
        move || {
            rt.dispatch(Event {
                kind: EventKind::Press,
                handler: id,
            });
        }
    });
    assert!(msg.contains("registry miss"), "{msg}");
}

#[test]
fn swap_without_re_register_refuses_the_dropped_handler() {
    let rt = Runtime::new();
    let served = HandlerId::from_symbol("app.served");
    let dropped = HandlerId::from_symbol("app.dropped");
    rt.register_handler(served, || {});
    rt.register_handler(dropped, || {});
    rt.swap_handlers(std::collections::HashMap::new()); // empty new table
    let _hook = SilenceHook::set();
    let msg = expect_panic({
        let rt = rt.clone();
        move || {
            rt.dispatch(Event {
                kind: EventKind::Press,
                handler: dropped,
            });
        }
    });
    assert!(msg.contains("registry miss"), "{msg}");
    assert!(msg.contains("re-register"), "{msg}");
    assert!(
        !rt.has_demand(),
        "the failed dispatch left no phantom demand"
    );
}

#[test]
fn handler_can_dispatch_another_handler() {
    let rt = Runtime::new();
    let inner = HandlerId::from_symbol("inner");
    let outer = HandlerId::from_symbol("outer");
    let hits = Rc::new(Cell::new(0u32));
    rt.register_handler(inner, {
        let hits = hits.clone();
        move || hits.set(hits.get() + 1)
    });
    rt.register_handler(outer, {
        let rt = rt.clone();
        move || {
            rt.dispatch(Event {
                kind: EventKind::Press,
                handler: inner,
            });
        }
    });
    rt.dispatch(Event {
        kind: EventKind::Press,
        handler: outer,
    });
    assert_eq!(
        hits.get(),
        1,
        "nested dispatch works (registry take/put-back discipline)"
    );
}

#[test]
fn reload_hook_can_flip_registry_atomically_inside_the_phase() {
    let rt = Runtime::new();
    let hits = Rc::new(Cell::new(0u32));
    let id = HandlerId::from_symbol("post.reload.handler");
    rt.register_handler(id, {
        let hits = hits.clone();
        move || hits.set(hits.get() + 1)
    });

    let hits_after = hits.clone();
    let new_id = HandlerId::from_symbol("post.reload.v2");
    rt.set_reload_hook(move |rt: &Runtime| {
        let mut table = std::collections::HashMap::new();
        table.insert(new_id, {
            let hits = hits.clone();
            Box::new(move || hits.set(hits.get() + 100)) as HandlerFn
        });
        // Atomic flip inside RELOAD (§5.3: registry re-resolution is atomic
        // with the swap).
        rt.swap_handlers(table);
    });
    rt.request_reload();
    assert!(rt.run_once());
    rt.dispatch(Event {
        kind: EventKind::Press,
        handler: new_id,
    });
    assert_eq!(
        hits_after.get(),
        100,
        "the re-registered handler resolves immediately after the phase"
    );
}

#[test]
fn two_runtimes_are_independent_cores() {
    let a = Runtime::new();
    let b = Runtime::new();
    let sa = a.signal(1i32);
    let sb = b.signal(2i32);
    let seen_a = Rc::new(Cell::new(0i32));
    let seen_b = Rc::new(Cell::new(0i32));
    a.effect_named("ea", {
        let sa = sa.clone();
        let seen = seen_a.clone();
        move || {
            seen.set(sa.get());
        }
    });
    b.effect_named("eb", {
        let sb = sb.clone();
        let seen = seen_b.clone();
        move || {
            seen.set(sb.get());
        }
    });
    sa.set(10);
    a.run_until_idle();
    assert_eq!(seen_a.get(), 10);
    assert_eq!(seen_b.get(), 2, "b's core untouched");
    let _ = (seen_a, seen_b, sb);
}
