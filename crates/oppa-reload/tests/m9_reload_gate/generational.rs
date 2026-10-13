use super::*;

// ---------------------------------------------------------------------------
// Mechanical proof: the generational check fires (fuzzer silence is earned)
// ---------------------------------------------------------------------------

/// Formats a caught panic payload (same shape as the arena unit tests).
fn panic_text(payload: &Box<dyn std::any::Any + Send>) -> String {
    (**payload)
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| (**payload).downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

#[test]
fn m9_generational_proof() {
    // GenArena: retired → Retired; reused → StaleGeneration; double
    // retire → AlreadyRetired; OOB → OutOfBounds; live get works.
    let mut arena: oppa::GenArena<u32> = oppa::GenArena::new();
    let a = arena.alloc(1u32);
    assert!(arena.is_alive(a));
    arena.retire(a).expect("retire live");
    assert!(!arena.is_alive(a));
    assert_eq!(
        arena.try_get(a),
        Err(oppa::SlotError::Retired { index: a.index() })
    );
    let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = arena.get(a);
    }))
    .unwrap_err();
    let text = panic_text(&payload);
    assert!(text.contains("retired"), "loud panic was: {text}");
    // Reuse bumps the generation: the old handle is stale, never an alias.
    let b = arena.alloc(2u32);
    assert_eq!(b.index(), a.index());
    assert_eq!(b.generation(), a.generation() + 1);
    assert!(matches!(
        arena.try_get(a),
        Err(oppa::SlotError::StaleGeneration { .. })
    ));
    assert_eq!(*arena.get(b), 2);
    // Double-retire and OOB are loud too.
    arena.retire(b).expect("retire b");
    assert!(matches!(
        arena.retire(b),
        Err(oppa::SlotError::AlreadyRetired { .. })
    ));
    assert!(matches!(
        arena.try_get(oppa::GenerationalId::new(9999, 0)),
        Err(oppa::SlotError::OutOfBounds { .. })
    ));

    // Runtime signal: retiring the slot makes reads panic loudly.
    let rt = ComponentHost::new().runtime();
    let sig = rt.signal(7u32);
    assert_eq!(sig.get(), 7);
    rt.retire_signal(&sig);
    let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = sig.get();
    }))
    .unwrap_err();
    let text = panic_text(&payload);
    assert!(
        text.contains("retired") || text.contains("stale"),
        "retired signal read must panic loudly, got: {text}"
    );

    // NodeId arena: same generation discipline as retained nodes.
    let mut nodes: oppa::NodeArena<u32> = oppa::NodeArena::new();
    let n = nodes.alloc(9);
    let id = oppa::NodeId::from_gen(n);
    assert_eq!(*nodes.get(id.gen()), 9);
    nodes.retire(id.gen()).expect("retire node");
    assert!(nodes.try_get(id.gen()).is_err());

    eprintln!("m9[generational-proof] retired/stale/OOB/double-retire all loud — checks fire");
}
