use super::*;

/// Core-owned reactive collection (§9.6 residence rule): the shape the §4.2
/// trace assumes — generational, reached through an id-handle passed as a
/// prop or captured by handlers. `Id` is the stable item identity (slot
/// rebinding swaps *which* id a slot shows); `V` is the item payload.
///
/// Honest arity delta (M2 ROUNDS entry): DESIGN writes `Store<ContactId>`
/// with both `store.get(index) -> ContactId` and `store.lookup(id) ->
/// Contact` — one type parameter cannot express both. `Store<Id, V>` is the
/// minimal shape that types both call sites.
///
/// Tracking is coarse in M2 (one version signal: any mutation invalidates
/// all readers; memo equality gates dedup downstream). Per-key granular
/// subscriptions ride `get_keyed` (Round 23.1, decision 333 — lazy
/// per-key signal slots; `insert` on an existing key notifies only
/// that key's readers, structural writes fan out).
#[derive(Clone)]
pub struct Store<Id, V> {
    rt: Runtime,
    inner: Rc<RefCell<StoreInner<Id, V>>>,
    version: Signal<u64>,
}

struct StoreInner<Id, V> {
    ids: Vec<Id>,
    values: HashMap<Id, V>,
    /// Per-key notify slots (Round 23.1, decision 333): lazy
    /// `Signal<Option<V>>` per key (`None` = absent — unknown keys
    /// and removals read live instead of refusing). Plain data
    /// residence like `values` (writes never fan out by
    /// themselves — only `slot.set` notifies, precisely).
    slots: HashMap<Id, Signal<Option<V>>>,
}

impl<Id, V> Store<Id, V>
where
    Id: Clone + Eq + std::hash::Hash + 'static,
    V: Clone + 'static,
{
    pub fn new(rt: &Runtime, ids: Vec<Id>, values: HashMap<Id, V>) -> Self {
        Self {
            rt: rt.clone(),
            inner: Rc::new(RefCell::new(StoreInner {
                ids,
                values,
                slots: HashMap::new(),
            })),
            version: rt.signal(0),
        }
    }

    pub fn len(&self) -> usize {
        self.version.get();
        self.inner.borrow().ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Id at a position (the slot→item binding source, §4.2).
    pub fn get(&self, index: usize) -> Option<Id> {
        self.version.get();
        self.inner.borrow().ids.get(index).cloned()
    }

    /// Payload for an id (the row's re-derivation source).
    pub fn lookup(&self, id: &Id) -> Option<V> {
        self.version.get();
        self.inner.borrow().values.get(id).cloned()
    }

    /// Replace the contents; bumps the version (one invalidation fan-out).
    pub fn set(&self, ids: Vec<Id>, values: HashMap<Id, V>) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.ids = ids;
            inner.values = values;
            // Sync every live slot so holders never read stale
            // (present keys update, vanished keys go `None` — all
            // precisely notified, none silently dropped).
            for (key, slot) in inner.slots.iter() {
                slot.set(inner.values.get(key).cloned());
            }
        }
        self.version.update(|v| v + 1);
    }

    /// Upserts one key (Round 23.1, decision 333): mutating an
    /// EXISTING key writes the value and notifies only that key's
    /// `get_keyed` readers (no version bump — structural readers
    /// stay quiet); inserting a NEW key also appends the id and
    /// bumps the version (shape changed — structural fan-out, plus
    /// any `None`-seeded waiter on that key).
    pub fn insert(&self, key: Id, value: V) {
        let is_new = !self.inner.borrow().values.contains_key(&key);
        {
            let mut inner = self.inner.borrow_mut();
            if is_new {
                inner.ids.push(key.clone());
            }
            inner.values.insert(key.clone(), value.clone());
            if let Some(slot) = inner.slots.get(&key) {
                slot.set(Some(value));
            }
        }
        if is_new {
            self.version.update(|v| v + 1);
        }
    }

    /// Per-key tracked read (Round 23.1, decision 333): a lazy
    /// `Signal<Option<V>>` for exactly this key (`None` when
    /// absent — unknown keys seed `None` and notify when the key
    /// later inserts). Subscribers re-run only on this key's
    /// writes (plus wholesale `set`, which syncs every slot);
    /// structural reads (`len`/`get`/`lookup`) stay version-gated.
    pub fn get_keyed(&self, key: &Id) -> Signal<Option<V>> {
        if let Some(slot) = self.inner.borrow().slots.get(key).cloned() {
            return slot;
        }
        let seed = self.inner.borrow().values.get(key).cloned();
        let slot = self.rt.signal(seed);
        self.inner
            .borrow_mut()
            .slots
            .insert(key.clone(), slot.clone());
        slot
    }

    /// The runtime this store notifies through (wiring aid for tests).
    pub fn runtime(&self) -> Runtime {
        self.rt.clone()
    }
}
