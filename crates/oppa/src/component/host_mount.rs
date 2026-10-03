use super::*;

impl ComponentHost {
    pub fn new() -> Self {
        Self::with_clock(Rc::new(SystemClock::new()))
    }

    /// Host over an injected clock (M6: deterministic vsync-cadence tests
    /// drive frames off a [`MockClock`](crate::clock::MockClock); the
    /// TIME phase services animations from this clock exactly as it does
    /// from the system clock).
    pub fn with_clock(clock: Rc<dyn Clock>) -> Self {
        let rt = Runtime::with_clock(clock);
        let layout = RefCell::new(LayoutLedger::new(&rt));
        let inner = Rc::new(HostInner {
            rt,
            rec: RefCell::new(Reconciler::new()),
            styles: RefCell::new(Interner::new()),
            instances: RefCell::new(HashMap::new()),
            next_instance: Cell::new(1),
            effects: RefCell::new(HashMap::new()),
            render_table: RefCell::new(HashMap::new()),
            layout,
            text_service: RefCell::new(None),
            break_source: RefCell::new(None),
            viewport: Cell::new((800.0, 600.0)),
            input: RefCell::new(InputState::default()),
            scroll_feeds: RefCell::new(HashMap::new()),
            scroll_x_feeds: RefCell::new(HashMap::new()),
            theme: RefCell::new(None),
            lifecycle: RefCell::new(None),
            focus_visible: RefCell::new(None),
            field_feeds: RefCell::new(HashMap::new()),
            fetch_gens: RefCell::new(HashMap::new()),
            evaluator: RefCell::new(TransitionEvaluator::new()),
            diag: RefCell::new(crate::diag::RingLog::default_log()),
            trans_anim: Cell::new(false),
            close_requested_flag: Cell::new(false),
            longpress: RefCell::new(HashMap::new()),
            scroll_drags: RefCell::new(HashMap::new()),
            flings: RefCell::new(Vec::new()),
            timers: RefCell::new(Vec::new()),
            next_timer: Cell::new(1),
            images: RefCell::new(ImageCache::new()),
            debug_overlay: RefCell::new(crate::debug_overlay::OverlayMeter::default()),
            root_instances: RefCell::new(Vec::new()),
        });
        // Framework-owned INPUT router (M5): hit-test + capture/focus +
        // dispatch over each queued InputEvent, inside INPUT's BatchGuard.
        // Weak so the hook never keeps a dead host alive.
        let weak_in: Weak<HostInner> = Rc::downgrade(&inner);
        inner.rt.set_input_hook(move |rt, ev| {
            if let Some(inner) = weak_in.upgrade() {
                ComponentHost { inner }.route_input(rt, ev);
            }
        });
        // Framework-owned LAYOUT pass (M3): engine code, never user code.
        // Weak so the pass never keeps a dead host alive; a TextService
        // impl must not re-enter the host (RefCell guards loudly).
        let weak: Weak<HostInner> = Rc::downgrade(&inner);
        inner.rt.set_layout_pass(move |_rt| {
            if let Some(inner) = weak.upgrade() {
                Self::run_layout_pass(&inner);
            }
        });
        Self { inner }
    }

    /// The LAYOUT-phase engine run: dirty subtrees in, settled boxes out.
    fn run_layout_pass(inner: &HostInner) {
        let service = inner.text_service.borrow();
        let service_ref = service.as_deref();
        let (viewport_w, viewport_h) = inner.viewport.get();
        let mut rec = inner.rec.borrow_mut();
        let styles = inner.styles.borrow();
        let mut layout = inner.layout.borrow_mut();
        layout.run(&mut rec, &styles, service_ref, viewport_w, viewport_h);
    }

    pub(super) fn alloc_instance(
        &self,
        component: SymbolHash,
        key: Option<u64>,
        parent: Option<u64>,
    ) -> u64 {
        let id = self.inner.next_instance.get();
        self.inner.next_instance.set(id + 1);
        self.inner
            .instances
            .borrow_mut()
            .insert(id, InstanceRecord::new(component, key, parent));
        id
    }

    /// Mounts a root component: one scheduler effect (§9.1 — component
    /// re-runs *are* effects) whose runs reconcile the retained tree and
    /// stamp the binding-edge commit flag. Props live core-side opaquely
    /// from the first line (lock #25), cloned out per run.
    pub fn mount<P>(&self, name: &str, props: P, render: fn(&Ctx, &P) -> VNode) -> MountHandle<P>
    where
        P: Props,
    {
        let component = SymbolHash::of(name);
        let inst = self.alloc_instance(component, None, None);
        let gen = self.inner.rt.generation();
        let fallback: RenderFn = Rc::new(move |ctx, opaque| render(ctx, opaque.get::<P>()));
        {
            let mut instances = self.inner.instances.borrow_mut();
            let rec = instances.get_mut(&inst).expect("fresh instance");
            rec.props = Some(OpaqueProps::new(props, gen));
            rec.fallback = Some(fallback);
        }
        let effect = self.spawn_root_effect(inst);
        self.inner.effects.borrow_mut().insert(inst, effect.clone());
        self.inner.root_instances.borrow_mut().push(inst);
        MountHandle {
            host: self.clone(),
            instance: inst,
            effect,
            _p: PhantomData,
        }
    }

    /// Mounts a root component from already-erased parts (M2b): used for
    /// components discovered by manifest rescan (no typed render fn in
    /// scope — the manifest's `render` entry is the code). Returns the
    /// instance id; the caller drives frames.
    pub fn mount_erased(&self, name: &str, props: OpaqueProps, render: RenderFn) -> u64 {
        let component = SymbolHash::of(name);
        let inst = self.alloc_instance(component, None, None);
        {
            let mut instances = self.inner.instances.borrow_mut();
            let rec = instances.get_mut(&inst).expect("fresh instance");
            rec.props = Some(props);
            rec.fallback = Some(render);
        }
        let effect = self.spawn_root_effect(inst);
        let id = effect.id();
        self.inner.effects.borrow_mut().insert(inst, effect);
        self.inner.root_instances.borrow_mut().push(inst);
        self.inner.rt.mark_effect_dirty(id);
        inst
    }

    fn spawn_root_effect(&self, inst: u64) -> crate::reactive::Effect {
        let host = self.clone();
        self.inner.rt.effect(move || {
            Self::run_instance(&host, inst);
        })
    }

    /// One component run: clones props out opaquely (releasing the borrow
    /// before user code runs — render re-enters the instance map),
    /// resolves the CURRENT render code for the symbol (harness table
    /// first, mount-time fallback second), and reconciles.
    ///
    /// The resolution is the use-after-unload fix: post-swap re-runs must
    /// execute incoming code looked up by symbol, never the mount-time
    /// pointer baked into the effect (which may point into an unloaded
    /// dylib).
    fn run_instance(host: &ComponentHost, inst: u64) {
        // Run cleanups from the previous run before executing the new render
        host.run_instance_cleanups(inst);
        let (owned, render_fn) = {
            let instances = host.inner.instances.borrow();
            let rec = instances.get(&inst).expect("live component instance");
            let owned = rec
                .props
                .as_ref()
                .expect("mounted component lost its props")
                .clone();
            // Current code first (manifest table), mount-time fallback
            // second. Either way the value below is built fresh per run:
            // no stale pointers, no unloaded code.
            let render_fn: RenderFn = host
                .inner
                .render_table
                .borrow()
                .get(&rec.component)
                .cloned()
                .or_else(|| rec.fallback.clone())
                .expect("root instance without render code");
            (owned, render_fn)
        };
        let ctx = Ctx {
            host: host.clone(),
            rt: host.inner.rt.clone(),
            instance: inst,
            sites: RefCell::new(HashMap::new()),
        };
        // Tag handler registrations with the running instance (M5
        // routing table — restored on drop so a panicking body cannot
        // misattribute the next run).
        struct OwnerGuard {
            rt: Runtime,
        }
        impl Drop for OwnerGuard {
            fn drop(&mut self) {
                self.rt.set_input_owner(None);
            }
        }
        host.inner.rt.set_input_owner(Some(inst));
        let _guard = OwnerGuard {
            rt: host.inner.rt.clone(),
        };
        let mut vnode = render_fn(&ctx, &owned);
        // M8 (finding F6): stamp the creating instance on the run's
        // handler attachments while the child context still exists —
        // the reconciler drains closures in the root effect, where the
        // running owner is always the root.
        crate::vnode::stamp_handler_owner(&vnode, inst);

        // When debug performance overlay is enabled, wrap the root so the
        // app subtree lays out exactly as the bare root: a block `Div`
        // passes the viewport width constraint through (a `Stack` would
        // measure the app intrinsically and left-align centered content),
        // while the badge floats in an intrinsic `Stack` layer so it hugs
        // its text instead of stretching into a full-width strip.
        if let Some(badge) = host.debug_overlay_badge() {
            let overlay = crate::vnode::Div("oppa_debug_overlay")
                .style(
                    crate::style::Style::new()
                        .bg(crate::style::Color(0x1E_1E_1E))
                        .ink(crate::style::Color(0x00_FF_66))
                        .pad_x(8)
                        .pad_y(4)
                        .radius(4.0)
                        .opacity(Some(0.88)),
                )
                .child(
                    crate::vnode::Text {
                        text: crate::vnode::SharedString::from(badge),
                        style: crate::vnode::TextClass::Custom {
                            size_px: 12,
                            weight: crate::text::FontWeight::BOLD,
                        },
                    }
                    .into(),
                );
            let layer = crate::vnode::Stack("oppa_debug_layer")
                .style(crate::style::Style::new().x(8.0).absolute_y(8.0))
                .child(overlay);
            vnode = crate::vnode::Div("oppa_debug_root").children(vec![vnode, layer]);
        }

        host.reconcile_root(vnode);
    }

    /// Installs the harness's current render table (called on every
    /// install/swap from the rescanned manifest).
    pub fn set_render_table(&self, table: HashMap<SymbolHash, RenderFn>) {
        *self.inner.render_table.borrow_mut() = table;
    }

    fn reconcile_root(&self, vnode: VNode) {
        let fired = self.inner.rt.take_binding_fired();
        let diff = {
            let mut rec = self.inner.rec.borrow_mut();
            let mut styles = self.inner.styles.borrow_mut();
            rec.reconcile(&self.inner.rt, &mut styles, fired, vnode)
        };
        // §9.4 end-to-end (M8): every commit feeds the TIME evaluator
        // with the frame's clock time, so the binding-edge stamp the
        // reconciler just carried suppresses interpolation for exactly
        // this commit with no manual wiring.
        {
            let rec = self.inner.rec.borrow();
            let styles = self.inner.styles.borrow();
            let mut ev = self.inner.evaluator.borrow_mut();
            ev.track_commit(&diff, &rec, &styles, self.inner.rt.now_secs());
        }
        // Reconciler-side hygiene for the evaluator's per-node tables
        // (Remove forgets roots; descendants drop here).
        self.inner
            .evaluator
            .borrow_mut()
            .prune_dead(&self.inner.rec.borrow());
        // Same hygiene for the U8 text feeds (decision 188).
        {
            let rec = self.inner.rec.borrow();
            self.inner
                .field_feeds
                .borrow_mut()
                .retain(|id, _| rec.get(*id).is_some());
        }
        // Round 1.4 input hygiene (decision 255): retired nodes release
        // router state — focus inside a removed subtree resets to None
        // (no focus trap survives an unmounted overlay) and captures
        // owned by removed nodes are released with their pressed flags
        // cleared through the instance recorded at Down time (never
        // through the retired node, whose paths refuse loudly).
        self.clear_retired_input(&diff);
        // TIME drive (M8, §9.1): the commit that creates interpolations
        // registers the settle animation (once — the flag holds while it
        // lives); frames continue at cadence until it retires, then the
        // loop idles. Stamped-only commits never register (zero actives).
        if self.inner.evaluator.borrow().active_count() > 0 && !self.inner.trans_anim.get() {
            self.inner.trans_anim.set(true);
            let weak: Weak<HostInner> = Rc::downgrade(&self.inner);
            self.inner.rt.add_animation(move |now| {
                let Some(inner) = weak.upgrade() else {
                    return false;
                };
                let live = inner.evaluator.borrow_mut().settle(now);
                if live == 0 {
                    inner.trans_anim.set(false);
                } else {
                    // Repaint drive (M8): interpolation progress writes no
                    // signals, so the damage discipline would never see it
                    // — re-dirty exactly the live nodes every frame. The
                    // builder rebuilds them, backends re-splice them, the
                    // surface advances; settled frames go quiet again.
                    let nodes = inner.evaluator.borrow().live_nodes();
                    inner.rec.borrow_mut().mark_paint_dirty(&nodes);
                }
                live > 0
            });
            self.inner.rt.request_frame();
        }
        // Commit dirt implies frame demand (decision 75): component effects
        // run synchronously at mount (outside any frame), so without this
        // the LAYOUT/PAINT phases would never see the first commit. The
        // on-demand loop still idles when commits are empty.
        if !diff.is_empty() {
            self.inner.rt.request_frame();
        }

        // Debug performance overlay dirt readout: the commit size
        // rides the badge. The FPS clock is deliberately untouched
        // here — it ticks on presents (`note_overlay_frame`), never
        // on reconciles, so an interaction after a long idle gap can
        // never divide by the gap and print 0 FPS.
        {
            let mut meter = self.inner.debug_overlay.borrow_mut();
            if meter.config.enabled {
                meter.note_commit_dirty(diff.ops.len());
            }
        }
    }
}
impl Default for ComponentHost {
    fn default() -> Self {
        Self::new()
    }
}

/// Root-component handle: props updates go through opaque core-side storage
/// (same path as mount — no typed backdoor) and explicitly re-schedule the
/// component effect (props are not signals, so no automatic invalidation).
pub struct MountHandle<P> {
    host: ComponentHost,
    instance: u64,
    effect: crate::reactive::Effect,
    _p: PhantomData<P>,
}

impl<P: Props> MountHandle<P> {
    pub fn root_instance(&self) -> u64 {
        self.instance
    }

    /// Unmounts this root component, running all cleanups and retiring its effect.
    pub fn unmount(self) {
        self.host.unmount(self.instance);
    }

    pub fn set_props(&self, props: P) {
        let gen = self.host.inner.rt.generation();
        self.host
            .inner
            .instances
            .borrow_mut()
            .get_mut(&self.instance)
            .expect("live root")
            .props = Some(OpaqueProps::new(props, gen));
        self.host.inner.rt.mark_effect_dirty(self.effect.id());
    }

    pub fn run_until_idle(&self) -> usize {
        self.host.run_until_idle()
    }
}
