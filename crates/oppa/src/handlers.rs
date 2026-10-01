use crate::hash::SymbolHash;
use std::collections::HashMap;

/// Stable id for an event handler: the hash of its call-site symbol
/// (DESIGN §5.3, locked #11). Ids are stable across hot swaps — after a swap
/// the registry re-resolves the *same* hash to the new function pointer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct HandlerId(u64);

impl HandlerId {
    pub const fn from_hash(hash: SymbolHash) -> Self {
        Self(hash.bits())
    }

    pub fn from_symbol(symbol: &str) -> Self {
        Self::from_hash(SymbolHash::of(symbol))
    }

    pub const fn bits(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for HandlerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HandlerId({:#018x})", self.0)
    }
}

pub type HandlerFn = Box<dyn Fn() + 'static>;

/// `HandlerId` -> callable registry, keyed by stable symbol hash.
///
/// The whole table can be flipped atomically (`swap`) — §5.3 requires the
/// `HandlerId` → function mapping to flip in one step inside the RELOAD
/// phase, with no in-flight dispatch seeing a half-swapped registry. A miss
/// (dispatching an id the current table does not resolve) fails loudly: a
/// hot swap that fails to re-register a handler is a bug, not silent no-op.
#[derive(Default)]
pub struct HandlerRegistry {
    table: HashMap<HandlerId, HandlerFn>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, id: HandlerId, f: HandlerFn) {
        self.table.insert(id, f);
    }

    /// Atomic whole-registry flip: the old table is replaced in one step.
    pub fn swap(&mut self, table: HashMap<HandlerId, HandlerFn>) {
        self.table = table;
    }

    pub fn rebind(&mut self, id: HandlerId, f: HandlerFn) {
        self.table.insert(id, f);
    }

    pub fn contains(&self, id: HandlerId) -> bool {
        self.table.contains_key(&id)
    }

    /// Takes the handler out of the table for invocation (re-entrancy-safe
    /// dispatch: the closure may itself dispatch other handlers without a
    /// double-borrow).
    pub fn take(&mut self, id: HandlerId) -> Option<HandlerFn> {
        self.table.remove(&id)
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    pub fn try_call(&mut self, id: HandlerId) -> Result<(), String> {
        match self.table.get_mut(&id) {
            Some(f) => {
                f();
                Ok(())
            }
            None => Err(format!(
                "handler registry miss: {id} does not resolve in the current table — \
                 a hot swap must re-register every handler id it still serves (§5.3)"
            )),
        }
    }

    pub fn call(&mut self, id: HandlerId) {
        if let Err(e) = self.try_call(id) {
            panic!("{e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silence() {
        std::panic::set_hook(Box::new(|_| {}));
    }

    fn restore() {
        let _ = std::panic::take_hook();
    }

    fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
        (**payload)
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| (**payload).downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    #[test]
    fn register_dispatch_roundtrip() {
        let hit = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let mut reg = HandlerRegistry::new();
        let id = HandlerId::from_symbol("toggle.on_press");
        reg.register(id, {
            let hit = hit.clone();
            Box::new(move || hit.set(hit.get() + 1))
        });
        reg.call(id);
        reg.call(id);
        assert_eq!(hit.get(), 2);
    }

    #[test]
    fn unknown_hash_is_a_loud_miss() {
        let mut reg = HandlerRegistry::new();
        let err = reg
            .try_call(HandlerId::from_symbol("never.registered"))
            .unwrap_err();
        assert!(err.contains("registry miss"), "{err}");
        silence();
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            reg.call(HandlerId::from_symbol("never.registered"));
        }))
        .unwrap_err();
        restore();
        let text = panic_message(&payload);
        assert!(text.contains("registry miss"), "panic message was: {text}");
    }

    #[test]
    fn swap_flips_atomically_and_re_resolves_by_hash() {
        let hit = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let mut reg = HandlerRegistry::new();
        let id = HandlerId::from_symbol("view.render");
        let dropped = HandlerId::from_symbol("view.only_in_old_table");
        reg.register(id, {
            let hit = hit.clone();
            Box::new(move || hit.set(hit.get() + 1))
        });
        reg.register(dropped, Box::new(|| {}));
        reg.call(id);
        assert_eq!(hit.get(), 1, "old function ran once before the swap");
        let mut next = HashMap::new();
        next.insert(id, {
            let hit = hit.clone();
            Box::new(move || hit.set(hit.get() * 10)) as HandlerFn
        });
        reg.swap(next);
        reg.call(id);
        assert_eq!(hit.get(), 10, "same id resolves to the swapped-in function");
        assert!(
            !reg.contains(dropped),
            "unre-registered id must not survive a swap"
        );
        silence();
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            reg.call(dropped);
        }))
        .unwrap_err();
        restore();
        let text = panic_message(&payload);
        assert!(text.contains("registry miss"), "panic message was: {text}");
    }
}
