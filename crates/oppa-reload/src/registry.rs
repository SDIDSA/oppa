//! Swap orchestration: drain → cancel → advance → unload → rescan →
//! adopt → evict → assert → re-run (M2b §5.3, locks #14/#25).
//!
//! Keyed state (`keyed_state`), `Store`, `image_cache`, and signals are
//! core-side and hold no hot vtables, so they **survive** swaps untouched
//! — the harness never drains them. Only `OpaqueProps` cross the boundary,
//! through the outgoing/incoming manifests' typed glue (`oppa::reload`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::rc::Rc;

use oppa::{
    ComponentDesc, ComponentHost, DrainedProps, HotGeneration, OpaqueProps, Runtime, SymbolHash,
};

use crate::source::ComponentSource;

/// Why an instance did not survive a swap (restart class, §5.1 — always
/// loud, never silent reinterpretation).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EvictReason {
    /// Symbol absent from the incoming manifest (component removed).
    UnknownSymbol,
    /// Symbol present but props layout changed (type-name mismatch).
    TypeMismatch,
    /// Symbol absent from the *outgoing* manifest (harness/manifest bug —
    /// a live instance no manifest knows; draining is impossible).
    MissingDrain,
}

/// One evicted instance. A mismatched/unknown payload leaks boundedly by
/// design (one props value: its type is unknown post-unload, so it cannot
/// be freed) and is counted here — reported, never silent.
#[derive(Debug)]
pub struct Evicted {
    pub instance: u64,
    pub symbol: SymbolHash,
    pub type_name: String,
    pub reason: EvictReason,
}

/// The complete record of one swap. `ok()` (no evictions) is the
/// steady-state expectation; evictions are a loud, classified
/// restart-class signal, not a failure of the harness.
#[derive(Debug)]
pub struct ReloadReport {
    pub outgoing: HotGeneration,
    pub incoming: HotGeneration,
    pub drained: usize,
    pub adopted: usize,
    pub evicted: Vec<Evicted>,
    pub tasks_dropped: usize,
    pub effects_rerun: usize,
    /// Worker-queue deltas across the swap window (applied/discarded).
    pub worker_applied: u64,
    pub worker_discarded: u64,
    /// Retired-but-still-mapped images after this swap (M2b retire
    /// model — see `HotRegistry::reload_to` step 3).
    pub retired_images: usize,
}

impl ReloadReport {
    pub fn ok(&self) -> bool {
        self.evicted.is_empty()
    }
}

struct Drained {
    instance: u64,
    symbol: SymbolHash,
    type_name: String,
    payload: DrainedProps,
}

/// The harness: owns the host, the current manifest (+ library), an
/// optional pending swap, and the last report. Created per host; driven
/// through the RELOAD phase via [`HotRegistry::arm`] or directly in
/// tests via [`HotRegistry::reload_to`].
pub struct HotRegistry {
    host: ComponentHost,
    current: Vec<ComponentDesc>,
    current_lib: Option<libloading::Library>,
    /// Retired images: unloaded in NO case in M2b. Every value whose
    /// vtable could point into a retired image (signal/memo slot values,
    /// keyed handles, memo closures, handler entries) stays VALID as
    /// long as the image stays mapped — unloading would turn the next
    /// drop/re-run into a use-after-unload. Retiring (leaking) one image
    /// per swap is the honest M2b trade: bounded (~100s of KB per swap),
    /// counted in every report, no dangling anything. True unload needs
    /// shared-core linking (all core vtables in one never-unloaded image)
    /// and is tracked M9 product-loop work — NOT attempted here.
    retired: Vec<libloading::Library>,
    pending: Option<Box<dyn ComponentSource>>,
    last_report: Option<ReloadReport>,
}

impl HotRegistry {
    pub fn new(host: ComponentHost) -> Self {
        Self {
            host,
            current: Vec::new(),
            current_lib: None,
            retired: Vec::new(),
            pending: None,
            last_report: None,
        }
    }

    pub fn host(&self) -> &ComponentHost {
        &self.host
    }

    /// Records the currently-loaded manifest (the startup half of the
    /// protocol): the harness must know what is loaded before the first
    /// swap, or every live instance would report `MissingDrain`. No
    /// drain/advance/evict — the mounted code already runs; this only
    /// records its manifest (+ library) for the next `reload_to`.
    pub fn install(&mut self, source: Box<dyn ComponentSource>) {
        // Entries first (`Copy`), then library ownership.
        self.current = source.entries().to_vec();
        self.current_lib = source.into_library();
        self.push_render_table();
    }

    /// Points the host at the current manifest's render code (§5.3: runs
    /// resolve code by symbol, never by stale pointer). Fn pointers wrap
    /// into the host's `RenderFn` (`Rc`) here, once per swap.
    fn push_render_table(&self) {
        let table: HashMap<SymbolHash, oppa::RenderFn> = self
            .current
            .iter()
            .map(|d| {
                let render: oppa::RenderFn = std::rc::Rc::new(d.render);
                (d.symbol, render)
            })
            .collect();
        self.host.set_render_table(table);
    }

    pub fn current_symbols(&self) -> Vec<SymbolHash> {
        self.current.iter().map(|d| d.symbol).collect()
    }

    /// Looks up a scanned manifest entry by symbol (mounting discovered
    /// components, diagnostics). Entries are `Copy`; the provider stays
    /// alive in the harness.
    pub fn find_entry(&self, symbol: SymbolHash) -> Option<ComponentDesc> {
        self.current.iter().find(|d| d.symbol == symbol).copied()
    }

    pub fn last_report(&self) -> Option<&ReloadReport> {
        self.last_report.as_ref()
    }

    /// Arms the RELOAD-phase hook: a pending swap requested via
    /// [`HotRegistry::request_swap`] runs inside the phase (global apply,
    /// atomic with the swap — §5.3), single-threaded, so no dispatch ever
    /// sees a half-swapped registry.
    pub fn arm(this: &Rc<RefCell<HotRegistry>>) {
        let reg = Rc::clone(this);
        let rt = this.borrow().host.runtime();
        rt.set_reload_hook(move |_| {
            let source = reg.borrow_mut().pending.take();
            if let Some(source) = source {
                let report = reg.borrow_mut().reload_to(source);
                reg.borrow_mut().last_report = Some(report);
            }
        });
    }

    /// Queues a swap and requests the RELOAD phase. The report lands in
    /// [`HotRegistry::last_report`] after the next `run_until_idle`.
    pub fn request_swap(this: &Rc<RefCell<HotRegistry>>, source: Box<dyn ComponentSource>) {
        this.borrow_mut().pending = Some(source);
        let rt = this.borrow().host.runtime();
        rt.request_reload();
    }

    /// Performs the full swap protocol now (tests call this directly;
    /// the product loop goes through the RELOAD hook instead).
    pub fn reload_to(&mut self, source: Box<dyn ComponentSource>) -> ReloadReport {
        let rt: Runtime = self.host.runtime();
        let stats_before = rt.stats();
        let outgoing = rt.generation();

        // 1. Drain live props through the OUTGOING manifest (hot code,
        //    pre-unload — sound). Type names are copied to owned Strings;
        //    the old OpaqueProps (old vtable) drop here, pre-unload.
        let old_map: HashMap<SymbolHash, ComponentDesc> =
            self.current.iter().map(|d| (d.symbol, *d)).collect();
        let mut drained: Vec<Drained> = Vec::new();
        let mut evicted: Vec<Evicted> = Vec::new();

        for snap in self.host.reload_snapshot() {
            if !snap.has_props {
                continue;
            }
            let opaque: OpaqueProps = self
                .host
                .take_props(snap.instance)
                .expect("snapshot advertised props");
            let type_name: String = opaque.type_name().to_string();
            match old_map.get(&snap.symbol) {
                Some(desc) => {
                    // SAFETY: outgoing manifest entries are valid — the old
                    // image is retired, never unloaded (still mapped).
                    let payload = unsafe { (desc.drain_props)(&opaque) };
                    let type_name_owned = type_name.clone();
                    drop(opaque);
                    if payload.ptr.is_null() {
                        // Drain refusal (foreign/stale payload): nothing
                        // was allocated, nothing leaks — loud eviction.
                        evicted.push(Evicted {
                            instance: snap.instance,
                            symbol: snap.symbol,
                            type_name: type_name_owned,
                            reason: EvictReason::MissingDrain,
                        });
                    } else {
                        drained.push(Drained {
                            instance: snap.instance,
                            symbol: snap.symbol,
                            type_name,
                            payload,
                        });
                    }
                }
                None => {
                    drop(opaque);
                    evicted.push(Evicted {
                        instance: snap.instance,
                        symbol: snap.symbol,
                        type_name,
                        reason: EvictReason::MissingDrain,
                    });
                }
            }
        }

        // 2. Cancel-at-RELOAD: drop pending tasks of the outgoing
        //    generation (§9.6). Running tasks finish; their submits are
        //    discarded by tag at INPUT.

        let tasks_dropped = rt.drop_pending_tasks(outgoing);

        // 3. Advance the generation, then RETIRE (never unload) the old
        //    image: see the `retired` field docs. Unloading would leave
        //    hot-vtabled values (signal/memo slots, handlers, closures)
        //    dangling; retiring keeps every vtable mapped and valid.
        rt.advance_hot_generation();
        let incoming = rt.generation();
        if let Some(lib) = self.current_lib.take() {
            self.retired.push(lib);
        }

        // 4. Rescan first (entries are `Copy`), then take the new
        //    library (kept alive from here on). Re-point the host at the
        //    incoming code BEFORE any re-run can execute.
        self.current = source.entries().to_vec();
        self.current_lib = source.into_library();
        self.push_render_table();
        let new_map: HashMap<SymbolHash, ComponentDesc> =
            self.current.iter().map(|d| (d.symbol, *d)).collect();

        // 5. Adopt through the INCOMING manifest. `expected` is a
        //    core-owned CString kept alive for the call; the glue
        //    re-checks the type name before reinterpreting.
        let drained_count = drained.len();

        let mut adopted = 0usize;
        for d in drained {
            match new_map.get(&d.symbol) {
                Some(desc) => {
                    let expected =
                        CString::new(d.type_name.clone()).expect("type name holds no nul");
                    // SAFETY: incoming entries are valid — the new library
                    // was just transferred into `current_lib` above.
                    let raw = unsafe { (desc.adopt_props)(d.payload, incoming, expected.as_ptr()) };
                    if raw.is_null() {
                        // Payload leaks boundedly (one props value): its
                        // type is unknown post-unload, so it cannot be
                        // freed. Counted, never silent.
                        evicted.push(Evicted {
                            instance: d.instance,
                            symbol: d.symbol,
                            type_name: d.type_name,
                            reason: EvictReason::TypeMismatch,
                        });
                    } else {
                        // SAFETY: non-null means the glue rebuilt the value
                        // with fresh clone glue (see `oppa::reload`).
                        let props = unsafe { *Box::from_raw(raw) };
                        self.host.set_props_raw(d.instance, props);
                        adopted += 1;
                    }
                }
                None => {
                    // Same bounded leak, same reporting.
                    evicted.push(Evicted {
                        instance: d.instance,
                        symbol: d.symbol,
                        type_name: d.type_name,
                        reason: EvictReason::UnknownSymbol,
                    });
                }
            }
        }

        // 6. Evict failures (effect retired — no post-swap run can touch
        //    missing props or stale hot code).
        for e in &evicted {
            self.host.evict_instance(e.instance);
        }

        // 7. Soundness invariant (§5.1, §8.1 enforcement): no
        //    outgoing-generation props survive. Debug-only by design.
        self.host.assert_no_outgoing_props(outgoing);

        // 8. Re-run every surviving component: re-runs ARE the handler
        //    re-registration (new code re-registers the ids it serves).

        let effects_rerun = self.host.mark_all_component_effects_dirty();

        self.host.run_until_idle();

        let stats_after = rt.stats();
        ReloadReport {
            outgoing,
            incoming,
            drained: drained_count,
            adopted,
            evicted,
            tasks_dropped,
            effects_rerun,
            worker_applied: stats_after.worker_applied - stats_before.worker_applied,
            worker_discarded: stats_after.worker_discarded - stats_before.worker_discarded,
            retired_images: self.retired.len(),
        }
    }
}
