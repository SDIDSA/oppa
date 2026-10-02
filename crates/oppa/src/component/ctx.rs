use super::*;

/// VNode`. Created fresh per run (call-site ordinals restart every run, so
/// stable bodies re-key identically); instance state persists in the host.
pub struct Ctx {
    pub(crate) host: ComponentHost,
    pub(crate) rt: Runtime,
    pub(crate) instance: u64,
    pub(crate) sites: RefCell<HashMap<u64, u32>>,
}

impl Ctx {
    fn next_site(&self, base: u64) -> SiteKey {
        let mut sites = self.sites.borrow_mut();
        let n = sites.entry(base).or_insert(0);
        let ordinal = *n;
        *n += 1;
        SiteKey {
            hash: base,
            ordinal,
        }
    }

    /// Registers a cleanup closure for this component instance (Round 18.2, decision 321).
    ///
    /// Cleanups run:
    /// - Before the component instance re-renders (in reverse registration order, LIFO).
    /// - When the component instance is unmounted or evicted.
    /// - When the component host drops.
    pub fn on_cleanup(&self, f: impl FnOnce() + 'static) {
        let mut instances = self.host.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&self.instance)
            .expect("on_cleanup on dead instance");
        rec.cleanups.push(Box::new(f));
    }

    /// Fires `callback` once at `now + delay_ms` (Round 21.1,
    /// decision 328 — the per-component timeout hook closing the
    /// Round 17.2 timer gap): `delay_ms` is host-clock ms off
    /// [`ComponentHost::now_ms`]. The entry is owned by this
    /// component instance and cancels automatically on re-render
    /// or unmount (the 18.2 cleanup registered here — timers are
    /// per-render: a body that re-renders every frame starves its
    /// own timeouts, stated); `cancel_timer` cancels early.
    /// `delay_ms` must be finite and `>= 0` (loud panic otherwise —
    /// a negative delay is an authoring bug, never a clamp).
    pub fn use_timeout(&self, delay_ms: f64, callback: impl Fn() + 'static) -> TimerId {
        assert!(
            delay_ms.is_finite() && delay_ms >= 0.0,
            "use_timeout: delay must be finite and >= 0, got {delay_ms}"
        );
        let due = self.host.now_ms() + delay_ms;
        let id = self
            .host
            .push_timer(self.instance, due, None, Rc::new(callback));
        let host = self.host.clone();
        self.on_cleanup(move || {
            host.cancel_timer(id);
        });
        id
    }

    /// Fires `callback` every `period_ms` (Round 21.1, decision 328
    /// — the per-component interval twin of [`Ctx::use_timeout`]):
    /// first fire at `now + period`, then steady cadence through
    /// `tick_timers` (missed periods snap forward — no burst).
    /// Same ownership and auto-cancel rules as `use_timeout`;
    /// `period_ms` must be finite and `> 0` (a zero period would
    /// self-demand every pump — refused loudly, never a spin).
    pub fn use_interval(&self, period_ms: f64, callback: impl Fn() + 'static) -> TimerId {
        assert!(
            period_ms.is_finite() && period_ms > 0.0,
            "use_interval: period must be finite and > 0, got {period_ms}"
        );
        let due = self.host.now_ms() + period_ms;
        let id = self
            .host
            .push_timer(self.instance, due, Some(period_ms), Rc::new(callback));
        let host = self.host.clone();
        self.on_cleanup(move || {
            host.cancel_timer(id);
        });
        id
    }

    /// Per-instance signal, seeded from props on first run (§4.1
    /// `ctx.signal(props.initial)`). Keyed by call-site source hash +
    /// ordinal: a body edit inserting a signal above shifts later sites →
    /// re-seed, never shuffle (§5.1).
    #[track_caller]
    pub fn signal<T: Clone + 'static>(&self, init: T) -> Signal<T> {
        let base = call_site_hash!();
        let key = self.next_site(base);
        self.host.instance_signal(self.instance, key, init)
    }

    /// Per-instance memo (re-derivation, §4.2 `contact`/`avatar`/`is_sel`).
    #[track_caller]
    pub fn memo<T: PartialEq + 'static>(&self, f: impl FnMut() -> T + 'static) -> Memo<T> {
        let base = call_site_hash!();
        let key = self.next_site(base);
        self.host.instance_memo(self.instance, key, f)
    }

    /// Binding edge (§9.4): a memo variant whose *value change* is an
    /// identity event. The scheduler raises the per-commit stamp; the
    /// reconciler carries `suppress_transitions` for exactly that commit.
    #[track_caller]
    pub fn binding<T: PartialEq + 'static>(&self, f: impl FnMut() -> T + 'static) -> Memo<T> {
        let base = call_site_hash!();
        let key = self.next_site(base);
        let hit = self
            .host
            .inner
            .instances
            .borrow()
            .get(&self.instance)
            .and_then(|rec| rec.memos.get(&key))
            .and_then(|any| any.downcast_ref::<Memo<T>>().cloned());
        if let Some(memo) = hit {
            return memo;
        }
        let memo = self.host.inner.rt.memo(f);
        self.host.inner.rt.mark_memo_binding(&memo);
        self.host
            .inner
            .instances
            .borrow_mut()
            .get_mut(&self.instance)
            .expect("live instance")
            .memos
            .insert(key, Box::new(memo.clone()));
        memo
    }

    /// Keyed side-state (§2.2 escape hatch, "the one new concept recycling
    /// asks of component authors"): core-side, keyed by item identity, LRU-
    /// evicted — distinct from per-instance signals, which re-seed on
    /// rebind by construction.
    pub fn keyed_state<T: Clone + 'static>(&self, key: u64, init: impl FnOnce() -> T) -> Signal<T> {
        self.rt.keyed_state(key, init)
    }

    /// Framework-provided reactive flags (§4.1: derived from the normalized
    /// `InputEvent` stream + hit-test — M5; in M2 they are per-instance
    /// signals the tests drive directly, which is exactly the seam the
    /// hit-test will write through).
    pub fn hovered(&self) -> Signal<bool> {
        self.flag(|rec| &mut rec.hovered)
    }

    pub fn pressed(&self) -> Signal<bool> {
        self.flag(|rec| &mut rec.pressed)
    }

    pub fn focused(&self) -> Signal<bool> {
        self.flag(|rec| &mut rec.focused)
    }

    /// Reactive hover movement/dwell counter (Round 17.3, decision 319):
    /// increments on every pointer move over this instance (or dwell tick),
    /// allowing hover-dependent state (such as tooltips) to re-evaluate.
    pub fn hover_move(&self) -> Signal<u64> {
        let mut instances = self.host.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&self.instance)
            .expect("hover_move on dead instance");
        if let Some(sig) = rec.hover_move.clone() {
            return sig;
        }
        let sig = self.host.inner.rt.signal(0u64);
        rec.hover_move = Some(sig.clone());
        sig
    }

    fn flag(
        &self,
        pick: impl FnOnce(&mut InstanceRecord) -> &mut Option<Signal<bool>>,
    ) -> Signal<bool> {
        let mut instances = self.host.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&self.instance)
            .expect("flag on dead instance");
        let slot = pick(rec);
        if let Some(sig) = slot.clone() {
            return sig;
        }
        let sig = self.host.inner.rt.signal(false);
        *slot = Some(sig.clone());
        sig
    }

    /// Framework-owned scroll position (§4.2 `ctx.scroll_offset()`).
    pub fn scroll_offset(&self) -> ScrollOffset {
        let mut instances = self.host.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&self.instance)
            .expect("scroll on dead instance");
        if let Some(sig) = rec.scroll.clone() {
            return ScrollOffset { signal: sig };
        }
        let sig = self.host.inner.rt.signal(0.0f32);
        rec.scroll = Some(sig.clone());
        ScrollOffset { signal: sig }
    }

    /// Framework-owned horizontal scroll position (Round 9.3, decision
    /// 302 — the `ctx.scroll_offset()` twin for `dx`: same per-instance
    /// residence, same controlled read-your-signal rendering, fed by
    /// `bind_scroll_x` instead of `bind_scroll`).
    pub fn scroll_x(&self) -> ScrollOffset {
        let mut instances = self.host.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&self.instance)
            .expect("scroll on dead instance");
        if let Some(sig) = rec.scroll_x.clone() {
            return ScrollOffset { signal: sig };
        }
        let sig = self.host.inner.rt.signal(0.0f32);
        rec.scroll_x = Some(sig.clone());
        ScrollOffset { signal: sig }
    }

    /// Framework-owned 2D scroll position (Phase 36 PR2b, decision
    /// 354 — G15): one view over the instance's `scroll` + `scroll_x`
    /// signals (gets-or-creates both — same residence, same keying as
    /// the 1D twins, so mixing `scroll_offset()` and `scroll_2d()` in
    /// one body shares state, never forks it). 2D `ScrollArea`
    /// containers bind both feeds via
    /// [`ComponentHost::bind_scroll_2d`].
    pub fn scroll_2d(&self) -> ScrollOffset2D {
        let y = self.scroll_offset();
        let x = self.scroll_x();
        ScrollOffset2D {
            x: x.signal,
            y: y.signal,
        }
    }

    /// App theme (Round 11.2, decision 306): `let t =
    /// ctx.theme();` then `t.tokens().primary` — tracked reads, so a
    /// toggle re-renders every themed control in place (instances,
    /// sessions, and signals survive — only colors re-derive).
    pub fn theme(&self) -> Theme {
        self.host.theme()
    }

    /// App-wide image cache (Phase 36 PR4 / Phase 38c, decision 359):
    /// components load URLs or deposit pre-decoded static pixels here.
    pub fn images(&self) -> ImageCache {
        self.host.images()
    }

    /// Mobile / app lifecycle state (Round 18.3, decision 322):
    /// Tracked signal returning `AppLifecycleState::{Active, Paused, Suspended}`.
    pub fn lifecycle(&self) -> Signal<crate::shell::AppLifecycleState> {
        self.host.lifecycle()
    }

    /// Requests window close from a component (Round 26.2, decision
    /// 342 -- File/Exit menu items, post-save exit): forwards to the
    /// host flag; desktop runners drain it through the veto consult.
    pub fn request_close(&self) {
        self.host.request_close();
    }

    /// Product editing session over an author-owned content signal (G1,
    /// decision 205): `let field = ctx.edit_session(ctx.signal("".into()))`.
    /// Per-instance, keyed by call-site (same re-seed rule as
    /// `ctx.signal`); content follows the controlled pattern (locked
    /// #24 — the component renders the signal it passed in); caret,
    /// selection, composition, and bounded multi-level undo live
    /// core-side and survive hot swaps. See [`EditSession`](crate::editing::EditSession).
    #[track_caller]
    pub fn edit_session(&self, content: Signal<SharedString>) -> EditSession {
        let base = call_site_hash!();
        let key = self.next_site(base);
        self.host.instance_edit_session(self.instance, key, content)
    }

    /// Blessed async state (G7, decision 220): the keyed signal holding
    /// one fetch's [`FetchState`] (`Idle/Loading/Ready/Failed`) —
    /// render it with a plain `match`. Namespaced by [`fetch_key`]
    /// (`"route:name"` strings — one global u64 namespace per runtime,
    /// collisions are app bugs, so hash readable names).
    pub fn fetch_state<T: Clone + Send + 'static>(&self, key: u64) -> Signal<FetchState<T>> {
        self.rt.keyed_state(key, || FetchState::Idle)
    }

    /// Blessed fetch→render driver, native side (G7, decision 220):
    /// sets `Loading` synchronously (first paint already shows it),
    /// runs `fetch` on the executor thread, and submits
    /// `Ready/Failed` through the `keyed_state` rendezvous — the only
    /// `Send`-safe path to UI state (signals never cross threads).
    /// Generation tags ride along (§9.6 — a result landing after a
    /// swap is discarded, never applied half-swapped). Refuses loudly
    /// on wasm (no threads there — the platform binding drives the
    /// same signal from the promise callback, decision 221).
    /// Returns the task id (Phase 37b — cancel via
    /// [`Ctx::cancel_fetch`]; ignoring the return keeps the exact
    /// pre-37b call shape).
    pub fn spawn_fetch<T: Clone + Send + 'static>(
        &self,
        key: u64,
        fetch: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> TaskId {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (key, fetch);
            panic!(
                "spawn_fetch needs OS threads — unavailable on wasm; drive the \
                 FetchState signal from the platform binding instead (promise \
                 callback writes the keyed signal, then requests a frame — G7)"
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.fetch_state::<T>(key).set(FetchState::Loading);
            let rt = self.rt.clone();
            rt.spawn_task(move |scope| {
                let result = fetch();
                // Cancelled fetches reset to Idle (never a stuck
                // Loading, never a late Ready over the reset).
                let cancelled = scope.is_cancelled();
                scope.submit(move |rt| {
                    rt.keyed_state::<FetchState<T>>(key, || FetchState::Idle)
                        .set(if cancelled {
                            FetchState::Idle
                        } else {
                            match result {
                                Ok(v) => FetchState::Ready(v),
                                Err(e) => FetchState::Failed(e),
                            }
                        });
                });
            })
        }
    }

    /// Blessed fetch→render driver with a retry budget (Round 13.1,
    /// decision 308): like [`spawn_fetch`](Self::spawn_fetch), but the
    /// worker re-runs `fetch` up to `attempts` total tries inside one
    /// task run (synchronous immediate retry — no backoff in v1;
    /// backoff is a stated follow-up, never smuggled in). The first
    /// `Ok` wins (`Ready`); exhausting the budget submits a
    /// *distinct* terminal error (`Failed("<last> (retry budget
    /// exhausted after N attempts)")` — greppable, never confusable
    /// with a first-try failure). `attempts == 0` panics loudly (a
    /// zero-try fetch is an authoring bug, never a silent no-op).
    /// Same wasm refusal as `spawn_fetch`. Returns the task id
    /// (Phase 37b — cancel via [`Ctx::cancel_fetch`]).
    pub fn spawn_fetch_with_retry<T: Clone + Send + 'static>(
        &self,
        key: u64,
        attempts: u32,
        fetch: impl FnMut() -> Result<T, String> + Send + 'static,
    ) -> TaskId {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (key, attempts, fetch);
            panic!(
                "spawn_fetch_with_retry needs OS threads — unavailable on wasm; drive the \
                 FetchState signal from the platform binding instead (promise \
                 callback writes the keyed signal, then requests a frame — G7)"
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            assert!(
                attempts >= 1,
                "spawn_fetch_with_retry budget must be >= 1 attempt, got {attempts} — refused, never silent"
            );
            self.fetch_state::<T>(key).set(FetchState::Loading);
            let rt = self.rt.clone();
            rt.spawn_task(move |scope| {
                let mut fetch = fetch;
                let mut attempt = 0u32;
                let result = loop {
                    attempt += 1;
                    match fetch() {
                        ok @ Ok(_) => break ok,
                        Err(_) if attempt < attempts => continue,
                        Err(e) => {
                            break Err(format!(
                                "{e} (retry budget exhausted after {attempt} attempts)"
                            ));
                        }
                    }
                };
                // Cancelled fetches reset to Idle (the `spawn_fetch` rule).
                let cancelled = scope.is_cancelled();
                scope.submit(move |rt| {
                    rt.keyed_state::<FetchState<T>>(key, || FetchState::Idle)
                        .set(if cancelled {
                            FetchState::Idle
                        } else {
                            match result {
                                Ok(v) => FetchState::Ready(v),
                                Err(e) => FetchState::Failed(e),
                            }
                        });
                });
            })
        }
    }

    /// Paged fetch into a collection (Round 13.3, decision 310):
    /// loads page `page` (`per_page` rows) with the 13.1 retry
    /// budget, tracks it in `FetchState<Vec<T>>` at
    /// [`page_key`](crate::fetch::page_key), and streams won rows
    /// into the [`Collection`](crate::store::Collection) at
    /// `collection_key` (ids mint in stage order — pages accumulate
    /// streaming-style). A superseding load of the same page bumps
    /// the [`page_gen_key`](crate::fetch::page_gen_key) counter, so
    /// stale applies discard (never half-apply an old page over a
    /// fresh query). Fresh searches swap by clearing first
    /// (`collection.clear()` + load page 0 — the documented recipe,
    /// not a flag). `attempts == 0` panics loudly; same wasm
    /// refusal as `spawn_fetch`. Returns the task id (Phase 37b —
    /// cancel via [`Ctx::cancel_fetch`]).
    pub fn spawn_fetch_page<T: Clone + Send + 'static>(
        &self,
        collection_key: u64,
        page: usize,
        per_page: usize,
        attempts: u32,
        fetch_page: impl FnMut(usize, usize) -> Result<Vec<T>, String> + Send + 'static,
    ) {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (collection_key, page, per_page, attempts, fetch_page);
            panic!(
                "spawn_fetch_page needs OS threads — unavailable on wasm; drive the \
                 FetchState signal from the platform binding instead (promise \
                 callback writes the keyed signal, then requests a frame — G7)"
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use crate::fetch::{page_gen_key, page_key};
            use crate::store::Collection;
            assert!(
                attempts >= 1,
                "spawn_fetch_page budget must be >= 1 attempt, got {attempts} — refused, never silent"
            );
            let gen_key = page_gen_key(collection_key, page);
            let gen = {
                let counter = self.rt.keyed_state::<u64>(gen_key, || 0);
                counter.update(|g| g + 1);
                counter.get()
            };
            let state_key = page_key(collection_key, page);
            self.fetch_state::<Vec<T>>(state_key)
                .set(FetchState::Loading);
            let rt = self.rt.clone();
            rt.spawn_task(move |scope| {
                let mut fetch_page = fetch_page;
                let mut attempt = 0u32;
                let result = loop {
                    attempt += 1;
                    match fetch_page(page, per_page) {
                        ok @ Ok(_) => break ok,
                        Err(_) if attempt < attempts => continue,
                        Err(e) => {
                            break Err(format!(
                                "{e} (retry budget exhausted after {attempt} attempts)"
                            ));
                        }
                    }
                };
                // Cancelled page loads reset to Idle (the
                // `spawn_fetch` rule — never a stuck Loading,
                // never rows over the reset).
                let cancelled = scope.is_cancelled();
                scope.submit(move |rt| {
                    let cur = rt.keyed_state::<u64>(gen_key, || 0).get();
                    if cur != gen {
                        return;
                    }
                    if cancelled {
                        rt.keyed_state::<FetchState<Vec<T>>>(state_key, || FetchState::Idle)
                            .set(FetchState::Idle);
                        return;
                    }
                    match result {
                        Ok(rows) => {
                            let ready = rows.clone();
                            Collection::<T>::ingest_batch(rt, collection_key, rows);
                            rt.keyed_state::<FetchState<Vec<T>>>(state_key, || FetchState::Idle)
                                .set(FetchState::Ready(ready));
                        }
                        Err(e) => {
                            rt.keyed_state::<FetchState<Vec<T>>>(state_key, || FetchState::Idle)
                                .set(FetchState::Failed(e));
                        }
                    }
                });
            });
        }
    }

    /// Re-exports [`fetch_key`](crate::fetch::fetch_key) at the call
    /// site (`ctx.fetch_key("settings:avatar")` reads better than the
    /// free function in component bodies).
    pub fn fetch_key(&self, name: &str) -> u64 {
        fetch_key(name)
    }

    /// Blessed fetch→render driver over a pluggable backend (Phase
    /// 37b, decision 362 — G16): like [`Ctx::spawn_fetch`], but the
    /// bytes come from a [`Fetcher`](crate::fetch::Fetcher)
    /// (`ScriptedFetcher` doubles, app closures through
    /// [`ClosureFetcher`](crate::fetch::ClosureFetcher), the wasm
    /// platform binding outside threads). Refuses loudly on wasm
    /// (same rule as `spawn_fetch`). Returns the task id (cancel via
    /// [`Ctx::cancel_fetch`]).
    pub fn spawn_fetch_with(
        &self,
        fetcher: std::sync::Arc<dyn crate::fetch::Fetcher>,
        key: u64,
        url: &str,
    ) -> TaskId {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (fetcher, key, url);
            panic!(
                "spawn_fetch_with needs OS threads — unavailable on wasm; drive the \
                 FetchState signal from the platform binding instead (promise \
                 callback writes the keyed signal, then requests a frame — G7)"
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let url = url.to_string();
            self.fetch_state::<String>(key).set(FetchState::Loading);
            let rt = self.rt.clone();
            rt.spawn_task(move |scope| {
                let result = fetcher.fetch(&url);
                // Cancelled fetches reset to Idle (the `spawn_fetch` rule).
                let cancelled = scope.is_cancelled();
                scope.submit(move |rt| {
                    rt.keyed_state::<FetchState<String>>(key, || FetchState::Idle)
                        .set(if cancelled {
                            FetchState::Idle
                        } else {
                            match result {
                                Ok(v) => FetchState::Ready(v),
                                Err(e) => FetchState::Failed(e),
                            }
                        });
                });
            })
        }
    }

    /// Cancels a fetch task and resets its state (Phase 37b, G16):
    /// [`Ctx::cancel_task`] plus the `FetchState` back to `Idle`
    /// (never a stuck `Loading`, never a late `Ready` over the
    /// reset). Returns what `cancel_task` reported (false = already
    /// completed or unknown — a completed `Ready`/`Failed` is kept,
    /// never wiped). The recipe for abandoning a load.
    pub fn cancel_fetch<T: Clone + Send + 'static>(&self, key: u64, id: TaskId) -> bool {
        if !self.cancel_task(id) {
            return false;
        }
        self.fetch_state::<T>(key).set(FetchState::Idle);
        true
    }

    /// Signal-backed write-through persistence (Phase 37b, decision
    /// 364): seeds `initial` from `store` under `key` on first run
    /// (store wins when present and decodable), then hands a
    /// [`Persisted`](crate::store::Persisted) handle whose reads
    /// track and whose writes hit the signal AND the store
    /// synchronously. Seed failures (backend read errors, undecodable
    /// bytes) fall back to `initial` with a `Warn` diagnostic (boot
    /// never crashes on corrupt settings — the fallback is
    /// observable, never silent); write failures panic loudly (data
    /// loss is never silent). Single-writer per key (this handle
    /// owns the key — external writers between runs are last-read,
    /// stated).
    pub fn persisted<T: Clone + 'static>(
        &self,
        key: &str,
        initial: T,
        encode: impl Fn(&T) -> Vec<u8> + 'static,
        decode: impl Fn(&[u8]) -> Option<T> + 'static,
        store: std::rc::Rc<std::cell::RefCell<dyn crate::store::KvStore>>,
    ) -> crate::store::Persisted<T> {
        let seed = match store.borrow().get(key) {
            Ok(Some(bytes)) => decode(&bytes).unwrap_or_else(|| {
                self.host.diag_log(
                    crate::diag::LogLevel::Warn,
                    format!("persisted {key:?}: undecodable bytes — seeding initial"),
                );
                initial.clone()
            }),
            Ok(None) => initial.clone(),
            Err(e) => {
                self.host.diag_log(
                    crate::diag::LogLevel::Warn,
                    format!("persisted {key:?}: store read failed ({e}) — seeding initial"),
                );
                initial.clone()
            }
        };
        // `signal` seeds once (later runs re-read the store but the
        // seed is only consumed on first run — write-through keeps
        // the store current, so re-reads agree anyway).
        let signal = self.signal(seed);
        crate::store::Persisted::new(signal, key, encode, store)
    }

    /// String specialization of [`Ctx::persisted`]: UTF-8 bytes
    /// (invalid UTF-8 warns and seeds `initial`, like undecodable
    /// bytes above).
    pub fn persisted_string(
        &self,
        key: &str,
        initial: &str,
        store: std::rc::Rc<std::cell::RefCell<dyn crate::store::KvStore>>,
    ) -> crate::store::Persisted<String> {
        self.persisted(
            key,
            initial.to_string(),
            |s: &String| s.as_bytes().to_vec(),
            |b: &[u8]| String::from_utf8(b.to_vec()).ok(),
            store,
        )
    }

    /// Pushes one diagnostic entry (Phase 37b, decision 363 — G17):
    /// the host-level zero-stdout ring (untracked — logging never
    /// schedules). Reads drain through
    /// [`ComponentHost::take_diag_logs`].
    pub fn log(&self, level: crate::diag::LogLevel, message: impl Into<String>) {
        self.host.diag_log(level, message);
    }

    /// Inline child component with its own instance scope (state keying).
    /// Scheduling stays the running effect's in M2 (see module docs).
    /// `name` is the component's symbol (hot-reload identity); `key`
    /// disambiguates siblings (slot keys, §4.2).
    ///
    /// Zero-boilerplate twins are [`Ctx::child_auto`] (static children)
    /// and [`Ctx::child_keyed`] (keyed siblings) — same instances,
    /// same state, no manual strings or ordinals.
    ///
    /// M8 (finding F6): the child's run is tagged with the CHILD
    /// instance for handler-owner attribution (the M5 router resolves
    /// per-instance flags through the handler's owning instance, so a
    /// recycled slot's hover/press/focus must attribute to the slot's
    /// instance, not the parent's). The parent owner is restored after
    /// the child renders (nesting composes; a panicking child restores
    /// via the guard — it cannot misattribute the next run).
    pub fn child<P: Props>(
        &self,
        name: &str,
        key: u64,
        props: &P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> VNode {
        let id = self.host.child_instance(self.instance, name, key);
        self.host.run_instance_cleanups(id);
        let ctx = Ctx {
            host: self.host.clone(),
            rt: self.rt.clone(),
            instance: id,
            sites: RefCell::new(HashMap::new()),
        };
        struct ChildOwnerGuard {
            pub(crate) rt: Runtime,
            prev: Option<u64>,
        }
        impl Drop for ChildOwnerGuard {
            fn drop(&mut self) {
                self.rt.set_input_owner(self.prev);
            }
        }
        let prev = self.rt.input_owner();
        self.rt.set_input_owner(Some(id));
        let _guard = ChildOwnerGuard {
            rt: self.rt.clone(),
            prev,
        };
        let vnode = render(&ctx, props);
        // M8 (finding F6): stamp the child instance (inner children
        // already stamped theirs — only `None`s are filled).
        crate::vnode::stamp_handler_owner(&vnode, id);
        vnode
    }

    /// Zero-boilerplate static child (Phase 37a, decision 360): the
    /// instance is keyed on `(Location::caller(), ordinal)` — stable
    /// for stable bodies (the §5.1 re-seed rule covers body edits),
    /// with the call site as the hot-reload symbol. Static children
    /// drop manual string/ordinal args (`ctx.child_auto(&p, Comp)`
    /// replaces `ctx.child("path::Comp", 1, &p, Comp)`).
    ///
    /// No `TypeId` anywhere in the keying (different `TypeId`s across
    /// the rlib↔dylib boundary would fork reload state — the symbol
    /// stays a source string, stable across the boundary). Keyed
    /// siblings (loops, slots) take [`Ctx::child_keyed`]; the manual
    /// [`Ctx::child`] form stays for hand-rolled symbols.
    #[track_caller]
    pub fn child_auto<P: Props>(&self, props: &P, render: fn(&Ctx, &P) -> VNode) -> VNode {
        let base = call_site_hash!();
        let site = self.next_site(base);
        let loc = std::panic::Location::caller();
        let name = format!("auto:{}:{}#{}", loc.file(), loc.line(), site.ordinal);
        let key = site
            .hash
            .wrapping_add((site.ordinal as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        self.child(&name, key, props, render)
    }

    /// Zero-boilerplate keyed child (Phase 37a, decision 360): the
    /// explicit `key` disambiguates siblings (loop indices, slot ids —
    /// same contract as [`Ctx::child`]'s key), the call site names the
    /// hot-reload symbol. `ctx.child_keyed(i, &p, Row)` replaces
    /// `ctx.child("Row", i, &p, Row)`. Same-site duplicate keys share
    /// one instance (keys must be unique per site — the React rule,
    /// stated).
    #[track_caller]
    pub fn child_keyed<P: Props>(
        &self,
        key: u64,
        props: &P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> VNode {
        let loc = std::panic::Location::caller();
        let name = format!("keyed:{}:{}", loc.file(), loc.line());
        self.child(&name, key, props, render)
    }

    /// Executes a render closure, catching unwinding panics.
    ///
    /// If the closure panics, restores any modified input owner state and
    /// returns `Err(String)` containing the panic payload message.
    pub fn catch_unwind<F: FnOnce() -> VNode>(&self, f: F) -> Result<VNode, String> {
        let prev_owner = self.rt.input_owner();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        if res.is_err() {
            self.rt.set_input_owner(prev_owner);
        }
        res.map_err(|err| {
            if let Some(s) = err.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = err.downcast_ref::<String>() {
                s.clone()
            } else {
                "component render panicked".to_string()
            }
        })
    }

    /// Inline child component execution wrapped in an error boundary.
    ///
    /// Catches panics from the child component render and returns `Err(String)`
    /// without propagating the panic to the host or crashing the thread.
    pub fn try_child<P: Props>(
        &self,
        name: &str,
        key: u64,
        props: &P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> Result<VNode, String> {
        let id = self.host.child_instance(self.instance, name, key);
        self.host.run_instance_cleanups(id);
        let ctx = Ctx {
            host: self.host.clone(),
            rt: self.rt.clone(),
            instance: id,
            sites: RefCell::new(HashMap::new()),
        };
        struct ChildOwnerGuard {
            pub(crate) rt: Runtime,
            prev: Option<u64>,
        }
        impl Drop for ChildOwnerGuard {
            fn drop(&mut self) {
                self.rt.set_input_owner(self.prev);
            }
        }
        let prev = self.rt.input_owner();
        self.rt.set_input_owner(Some(id));
        let _guard = ChildOwnerGuard {
            rt: self.rt.clone(),
            prev,
        };
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(&ctx, props)));
        outcome
            .inspect(|vnode| {
                crate::vnode::stamp_handler_owner(vnode, id);
            })
            .map_err(|err| {
                if let Some(s) = err.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = err.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "component render panicked".to_string()
                }
            })
    }

    /// Dispatches a handler id with no payload (M2 headless `emit`: the
    /// payload-carrying `InputEvent` routing is M5; the registry path —
    /// handler-as-id resolved through the table — is what this proves).
    pub fn emit(&self, id: HandlerId) {
        self.rt.dispatch(Event {
            kind: EventKind::Press,
            handler: id,
        });
    }

    /// Spawns a task on the framework executor (§9.6, M2b): `Send`-safe
    /// off-thread computation; UI effects enter via the task scope's
    /// generation-tagged submits. Same capture discipline as handlers —
    /// signals/ids only — because non-`Send` captures do not compile.
    /// Returns the minted id (Round 13.1 — later tasks can name it
    /// in their `deps`).
    pub fn spawn(&self, f: impl FnOnce(TaskScope) + Send + 'static) -> TaskId {
        self.rt.spawn_task(f)
    }

    /// Submits a task with dependencies (Round 13.1, decision 308):
    /// the body parks in `Queued` until every dep id reads `Done`,
    /// then promotes and schedules — same `Send` discipline as
    /// [`spawn`](Self::spawn). Returns the minted id.
    pub fn prepare_task(
        &self,
        deps: &[TaskId],
        f: impl FnOnce(TaskScope) + Send + 'static,
    ) -> TaskId {
        self.rt.prepare_task(deps, f)
    }

    /// Current preparation stage of a task (`Done` once its body
    /// returned, `Cancelled` for cancelled-before-running, `None`
    /// for unknown or dropped ids).
    pub fn task_stage(&self, id: TaskId) -> Option<TaskStage> {
        self.rt.task_stage(id)
    }

    /// Cancels a task by id (Phase 37b, decision 362 — G16): parked
    /// or queued tasks never run (dependents still unblock);
    /// running tasks observe it cooperatively (see
    /// [`TaskScope::is_cancelled`](crate::worker::TaskScope::is_cancelled));
    /// completed or unknown ids report false. Returns true exactly
    /// when the id named a live task. Fetch drivers pair this with a
    /// `FetchState::Idle` reset — see [`Ctx::cancel_fetch`].
    pub fn cancel_task(&self, id: TaskId) -> bool {
        self.rt.cancel_task(id)
    }

    /// Settled layout box for an effect (one-frame-delayed feedback): tracks
    /// the layout generation, so this run's reader re-runs in the next
    /// frame's EFFECTS after a LAYOUT publish. Observes previous-frame
    /// values within the current frame (None before the first commit).
    pub fn settled_layout(&self, id: NodeId) -> Option<LayoutBox> {
        self.host.settled_box(id)
    }

    /// Test/diagnostic access to the running runtime.
    pub fn runtime(&self) -> Runtime {
        self.rt.clone()
    }

    /// The host this run belongs to (round 5.3, OQ-G2-1 —
    /// `on_drag` closures read committed boxes + pointer positions
    /// through it; shares state like every other host handle).
    /// Reads only from handlers (untracked box/position/focus
    /// reads never schedule); mutations stay in signals.
    pub fn host(&self) -> ComponentHost {
        self.host.clone()
    }

    /// This run's component instance id (identity hooks for tests).
    pub fn instance_id(&self) -> u64 {
        self.instance
    }

    /// The retained node count behind this host (recycling assertions).
    pub fn retained_count(&self) -> usize {
        self.host.retained_count()
    }
}
