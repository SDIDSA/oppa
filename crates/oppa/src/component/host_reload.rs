// ---------------------------------------------------------------------------
// Hot-reload orchestration (M2b §5.3)
// ---------------------------------------------------------------------------

use super::*;

impl ComponentHost {
    /// Snapshot of every live component instance for the drain phase: the
    /// harness drains props through the *outgoing* manifest before unload.
    pub fn reload_snapshot(&self) -> Vec<InstanceSnapshot> {
        self.inner
            .instances
            .borrow()
            .iter()
            .map(|(instance, rec)| InstanceSnapshot {
                instance: *instance,
                symbol: rec.component,
                has_props: rec.props.is_some(),
                props_generation: rec.props.as_ref().map(|p| p.generation()),
            })
            .collect()
    }

    /// Removes and returns an instance's props without running any hot
    /// code (the drain half: the harness re-wraps the payload through the
    /// outgoing manifest's typed drain glue *before* unload).
    pub fn take_props(&self, instance: u64) -> Option<OpaqueProps> {
        self.inner
            .instances
            .borrow_mut()
            .get_mut(&instance)
            .and_then(|rec| rec.props.take())
    }

    /// Installs adopted props (already re-wrapped through the *incoming*
    /// manifest's glue, post-load). Does not schedule: the harness marks
    /// every component effect dirty once the whole swap is adopted.
    pub fn set_props_raw(&self, instance: u64, props: OpaqueProps) {
        let mut instances = self.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&instance)
            .expect("adopt props on a live instance");
        rec.props = Some(props);
    }

    /// Re-runs every component after a swap (M2b §5.3: re-runs *are* the
    /// handler re-registration — new code re-registers the ids it serves
    /// during its first post-swap run, inside the RELOAD phase, so no
    /// dispatch ever sees a half-swapped registry). Returns the marked
    /// count. Evicted instances are gone from the map (see
    /// `evict_instance`), so every remaining tracked effect has props.
    pub fn mark_all_component_effects_dirty(&self) -> usize {
        let effects: Vec<crate::reactive::Effect> =
            self.inner.effects.borrow().values().cloned().collect();
        for effect in &effects {
            self.inner.rt.mark_effect_dirty(effect.id());
        }
        effects.len()
    }

    /// Runs cleanups registered on `instance` in reverse order (LIFO, Round 18.2).
    pub fn run_instance_cleanups(&self, instance: u64) {
        let cleanups: Vec<Box<dyn FnOnce()>> = {
            let mut instances = self.inner.instances.borrow_mut();
            if let Some(rec) = instances.get_mut(&instance) {
                std::mem::take(&mut rec.cleanups)
            } else {
                Vec::new()
            }
        };
        for cleanup in cleanups.into_iter().rev() {
            cleanup();
        }
    }

    /// Recursively runs cleanups on child instances and then on the instance itself.
    pub fn cleanup_instance(&self, instance: u64) {
        let children: Vec<u64> = {
            self.inner
                .instances
                .borrow()
                .get(&instance)
                .map(|rec| rec.children.values().copied().collect())
                .unwrap_or_default()
        };
        for child_id in children {
            if let Some(effect) = self.inner.effects.borrow_mut().remove(&child_id) {
                self.inner.rt.retire_effect(&effect);
            }
            self.cleanup_instance(child_id);
            self.inner.instances.borrow_mut().remove(&child_id);
        }
        self.run_instance_cleanups(instance);
    }

    /// Unmounts an instance, running all cleanups and retiring any associated effect.
    pub fn unmount(&self, instance: u64) -> bool {
        self.evict_instance(instance)
    }

    /// Evicts a component instance whose symbol (or props layout) did not
    /// survive the swap (restart class, §5.1): drops its record and
    /// retires its effect so no post-swap run can touch missing props or
    /// stale hot code. Returns false if the instance was already gone.
    pub fn evict_instance(&self, instance: u64) -> bool {
        let effect = self.inner.effects.borrow_mut().remove(&instance);
        if let Some(effect) = &effect {
            self.inner.rt.retire_effect(effect);
        }
        if !self.inner.instances.borrow().contains_key(&instance) && effect.is_none() {
            return false;
        }
        self.cleanup_instance(instance);
        self.inner.instances.borrow_mut().remove(&instance);
        // Scrub child references (roots have no parent; harmless anyway).
        for rec in self.inner.instances.borrow_mut().values_mut() {
            rec.children.retain(|_, child| *child != instance);
        }
        true
    }

    /// Soundness invariant (§5.1, §8.1 enforcement): after a swap, no
    /// instance may hold props tagged with the retired generation — their
    /// clone/drop glue points into the unloaded dylib. Debug-only by
    /// design: the harness calls this post-adopt; a violation is a harness
    /// bug (drain/adopt not total), never a runtime cost in release.
    pub fn assert_no_outgoing_props(&self, retired: HotGeneration) {
        debug_assert!(
            {
                let instances = self.inner.instances.borrow();
                instances
                    .values()
                    .all(|rec| rec.props.as_ref().is_none_or(|p| p.generation() != retired))
            },
            "reload left props of retired generation {retired} alive — \
             drain/adopt was not total (hot vtables would dangle past unload)"
        );
    }

    // -- instance-scoped storage (Ctx back-ends) -------------------------

    pub(crate) fn instance_signal<T: Clone + 'static>(
        &self,
        instance: u64,
        key: SiteKey,
        init: T,
    ) -> Signal<T> {
        let mut instances = self.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&instance)
            .expect("signal on dead instance");
        if let Some(any) = rec.signals.get(&key) {
            return any
                .downcast_ref::<Signal<T>>()
                .expect(
                    "component signal type changed at an unchanged call site — \
                     hot-reload shape change is the restart class (§5.1)",
                )
                .clone();
        }
        let sig = self.inner.rt.signal(init);
        rec.signals.insert(key, Box::new(sig.clone()));
        sig
    }

    pub(crate) fn instance_memo<T: PartialEq + 'static>(
        &self,
        instance: u64,
        key: SiteKey,
        f: impl FnMut() -> T + 'static,
    ) -> Memo<T> {
        let mut instances = self.inner.instances.borrow_mut();
        let rec = instances.get_mut(&instance).expect("memo on dead instance");
        if let Some(any) = rec.memos.get(&key) {
            return any
                .downcast_ref::<Memo<T>>()
                .expect("component memo type changed at an unchanged call site (§5.1)")
                .clone();
        }
        let memo = self.inner.rt.memo(f);
        rec.memos.insert(key, Box::new(memo.clone()));
        memo
    }

    pub(crate) fn child_instance(&self, parent: u64, name: &str, key: u64) -> u64 {
        if let Some(id) = self.inner.instances.borrow()[&parent].children.get(&key) {
            return *id;
        }
        let component = SymbolHash::of(name);
        let id = self.alloc_instance(component, Some(key), Some(parent));
        self.inner
            .instances
            .borrow_mut()
            .get_mut(&parent)
            .expect("parent instance")
            .children
            .insert(key, id);
        id
    }
}
