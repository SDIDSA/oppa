use std::error::Error;
use std::fmt;

/// Generational handle into a [`GenArena`]. `generation` increments each time
/// the slot is reused, so a handle to a retired generation can never alias a
/// newer occupant (DESIGN §3.1, §9.6/#25: stale access fails loudly, never
/// silently returns data).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct GenerationalId {
    index: u32,
    generation: u32,
}

impl GenerationalId {
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// Why a slot access was refused. Every variant is loud: [`GenArena::get`]
/// panics with these; `try_get` returns them.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SlotError {
    /// The slot was retired and not yet reused.
    Retired { index: u32 },
    /// The slot was retired and reused; the handle predates the new occupant.
    StaleGeneration {
        index: u32,
        requested: u32,
        current: u32,
    },
    /// The handle was double-retired.
    AlreadyRetired { index: u32 },
    /// The index is out of bounds.
    OutOfBounds { index: u32 },
}

impl fmt::Display for SlotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SlotError::Retired { index } => {
                write!(f, "generational slot access refused: slot {index} is retired (no live generation)")
            }
            SlotError::StaleGeneration {
                index,
                requested,
                current,
            } => write!(
                f,
                "generational slot access refused: slot {index} holds generation {current}, \
                 handle requests retired generation {requested} — stale identity, refusing"
            ),
            SlotError::AlreadyRetired { index } => {
                write!(
                    f,
                    "generational slot double-retire: slot {index} is already retired"
                )
            }
            SlotError::OutOfBounds { index } => {
                write!(
                    f,
                    "generational slot access refused: index {index} out of bounds"
                )
            }
        }
    }
}

impl Error for SlotError {}

struct Slot<T> {
    value: Option<T>,
    generation: u32,
}

/// Generational slot arena: the storage primitive behind signals, reactive
/// nodes, and the `NodeId` retained-node arena (DESIGN §3.1, locked #11).
///
/// Contract: any access through a retired generation must fail loudly —
/// panicking on `get`/`get_mut`, `Err` on `try_get` — never returning stale
/// data. This is the mechanism the hot-reload identity-churn safety argument
/// (§9.6, #25) depends on.
pub struct GenArena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
}

impl<T> Default for GenArena<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> GenArena<T> {
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    /// Inserts a value, reusing a retired slot (bumping its generation) when
    /// possible.
    pub fn alloc(&mut self, value: T) -> GenerationalId {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.generation += 1;
            slot.value = Some(value);
            GenerationalId::new(index, slot.generation)
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(Slot {
                value: Some(value),
                generation: 0,
            });
            GenerationalId::new(index, 0)
        }
    }

    /// Retires the slot: value is dropped and returned, the slot becomes free
    /// for reuse with a bumped generation.
    pub fn retire(&mut self, id: GenerationalId) -> Result<T, SlotError> {
        let index = id.index as usize;
        if index >= self.slots.len() {
            return Err(SlotError::OutOfBounds { index: id.index });
        }
        let slot = &mut self.slots[index];
        if slot.generation != id.generation {
            return Err(SlotError::StaleGeneration {
                index: id.index,
                requested: id.generation,
                current: slot.generation,
            });
        }
        match slot.value.take() {
            Some(value) => {
                self.free.push(id.index);
                Ok(value)
            }
            None => Err(SlotError::AlreadyRetired { index: id.index }),
        }
    }

    pub fn is_alive(&self, id: GenerationalId) -> bool {
        self.try_get(id).is_ok()
    }

    pub fn try_get(&self, id: GenerationalId) -> Result<&T, SlotError> {
        let index = id.index as usize;
        let slot = self
            .slots
            .get(index)
            .ok_or(SlotError::OutOfBounds { index: id.index })?;
        if slot.generation != id.generation {
            return Err(SlotError::StaleGeneration {
                index: id.index,
                requested: id.generation,
                current: slot.generation,
            });
        }
        slot.value
            .as_ref()
            .ok_or(SlotError::Retired { index: id.index })
    }

    pub fn try_get_mut(&mut self, id: GenerationalId) -> Result<&mut T, SlotError> {
        let index = id.index as usize;
        let slot = self
            .slots
            .get_mut(index)
            .ok_or(SlotError::OutOfBounds { index: id.index })?;
        if slot.generation != id.generation {
            let (requested, current) = (id.generation, slot.generation);
            return Err(SlotError::StaleGeneration {
                index: id.index,
                requested,
                current,
            });
        }
        slot.value
            .as_mut()
            .ok_or(SlotError::Retired { index: id.index })
    }

    /// Loud access: retired/stale generations panic with the full reason,
    /// they never alias live or stale data.
    pub fn get(&self, id: GenerationalId) -> &T {
        match self.try_get(id) {
            Ok(value) => value,
            Err(e) => panic!("{e}"),
        }
    }

    pub fn get_mut(&mut self, id: GenerationalId) -> &mut T {
        match self.try_get_mut(id) {
            Ok(value) => value,
            Err(e) => panic!("{e}"),
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter_alive(&self) -> impl Iterator<Item = (GenerationalId, &T)> {
        let slots = &self.slots;
        (0..slots.len() as u32).filter_map(move |index| {
            let slot = &slots[index as usize];
            slot.value
                .as_ref()
                .map(|v| (GenerationalId::new(index, slot.generation), v))
        })
    }
}

/// Handle into a retained-node arena. The real `RetainedNode` payload lands
/// with the reconciler (M2); the arena + generation checks are M0 and must
/// already be mechanical.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct NodeId(GenerationalId);

impl NodeId {
    pub const fn from_gen(id: GenerationalId) -> Self {
        Self(id)
    }

    pub const fn new(index: u32, generation: u32) -> Self {
        Self(GenerationalId::new(index, generation))
    }

    pub const fn gen(self) -> GenerationalId {
        self.0
    }

    pub const fn index(self) -> u32 {
        self.0.index()
    }

    pub const fn generation(self) -> u32 {
        self.0.generation()
    }
}

/// Retained-node arena: `NodeId` handles with generation checks.
pub type NodeArena<T> = GenArena<T>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_get_roundtrip() {
        let mut a = GenArena::new();
        let x = a.alloc(41u32);
        assert_eq!(*a.get(x), 41);
        assert!(a.is_alive(x));
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn retire_then_get_panics_loudly() {
        let mut a = GenArena::new();
        let x = a.alloc(7u32);
        a.retire(x).unwrap();
        assert!(!a.is_alive(x));
        let err = a.try_get(x).unwrap_err();
        assert!(matches!(err, SlotError::Retired { .. }));
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = a.get(x);
        }))
        .unwrap_err();
        let text = panic_message(&payload);
        assert!(text.contains("retired"), "panic message was: {text}");
    }

    #[test]
    fn stale_handle_after_reuse_panics_with_generations() {
        let mut a = GenArena::new();
        let old = a.alloc(1u32);
        a.retire(old).unwrap();
        let new = a.alloc(2u32);
        assert_eq!(new.index(), old.index(), "slot reused");
        assert_eq!(
            new.generation(),
            old.generation() + 1,
            "generation bumped on reuse"
        );
        assert_eq!(*a.get(new), 2);
        let err = a.try_get(old).unwrap_err();
        assert!(matches!(err, SlotError::StaleGeneration { .. }));
        assert!(err.to_string().contains("stale identity"));
    }

    #[test]
    fn retire_wrong_generation_is_refused() {
        let mut a = GenArena::new();
        let old = a.alloc(1u32);
        a.retire(old).unwrap();
        let new = a.alloc(2u32);
        let err = a.retire(old).unwrap_err();
        assert!(matches!(err, SlotError::StaleGeneration { .. }));
        a.retire(new).unwrap();
        let err = a.retire(new).unwrap_err();
        assert!(matches!(err, SlotError::AlreadyRetired { .. }));
    }

    #[test]
    fn iter_alive_skips_retired() {
        let mut a = GenArena::new();
        let x = a.alloc(1u32);
        let y = a.alloc(2u32);
        a.retire(x).unwrap();
        let alive: Vec<u32> = a.iter_alive().map(|(_, v)| *v).collect();
        assert_eq!(alive, vec![2]);
        let _ = y;
    }

    fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
        (**payload)
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| (**payload).downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default()
    }
}
