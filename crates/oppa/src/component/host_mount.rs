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
            settle_observers: RefCell::new(Vec::new()),
            deferred: RefCell::new(Vec::new()),
            cancelled_exits: RefCell::new(Vec::new()),
            close_requested_flag: Cell::new(false),
            system_bars_request: Cell::new(None),
            selection_handles_visible: Cell::new(true),
            system_bars_visible_sig: RefCell::new(None),
            theme_token_overrides: RefCell::new(None),
            toolbar_model_sig: RefCell::new(None),
            toolbar_actions: RefCell::new(std::collections::VecDeque::new()),
            caption_hover_sig: RefCell::new(None),
            maximized_sig: RefCell::new(None),
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
        // Decision 519: stamp the same instance on the run's elements
        // (node ownership for the subtree-unmount sweep — innermost
        // `Ctx::child` stamps already landed, this fills the rest).
        crate::vnode::stamp_node_owner(&vnode, inst);

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

        // Decision 455: renderer-drawn text toolbar — the overlay
        // portal appends to the app root's children when the runner
        // published a model (`None` returns the app vnode untouched).
        // Portals never join parent flow, so the app lays out
        // byte-identically with and without the bar; appearing and
        // vanishing as one appended child, siblings keep their
        // instances, focus, and sessions across toggles (the bar must
        // never destroy what it annotates).
        let model = host.toolbar_model().get();
        vnode = Self::toolbar_portal(host, vnode, model.as_ref());

        host.reconcile_root(vnode);
    }

    /// Appends the scene-toolbar overlay portal (decision 455 — the
    /// root-append half; interaction resolves geometrically in the
    /// router, never through handlers here). `None` returns the app
    /// vnode untouched (no wrap, no remount — the portal appears and
    /// vanishes as one appended child, so siblings keep their
    /// instances, focus, and sessions across toggles). Container-less
    /// roots (bare text leaves) carry no toolbar (documented bound —
    /// every real app roots a container; IME mirroring still covers
    /// typing there).
    fn toolbar_portal(
        host: &ComponentHost,
        vnode: VNode,
        model: Option<&crate::input::ToolbarModel>,
    ) -> VNode {
        use crate::input::{TOOLBAR_EST_W_PX, TOOLBAR_ROW_H_PX};
        let Some(model) = model else {
            return vnode;
        };
        let VNode::Element(mut e) = vnode else {
            return vnode;
        };
        let tokens = host.theme_tokens();
        // Coordinate spaces: the model anchor arrives in committed
        // (device px, viewport-space) boxes while style offsets are
        // CSS px — convert once here. The portal itself stays
        // offset-less (viewport-anchored by construction, immune to
        // parent-origin shifts) with the anchor offsets on the bar.
        let dpr = host.layout_config().device_pixel_ratio.max(f32::EPSILON);
        let (vw, _) = host.viewport_size();
        let (l, t, _r, b) = model.anchor;
        let (al, at, ab) = (l as f32 / dpr, t as f32 / dpr, b as f32 / dpr);
        // Measured content width (pre-layout, through the installed
        // shaper — the portal's given width stretches the bar
        // full-viewport, so an explicit width must name the intrinsic
        // content: each label's shaped advance plus item padding plus
        // the bar border. Exact on the first frame, no converge loop,
        // no extra root re-runs. `None` without a text service or on
        // a shaping miss (headless/setup — falls back to the estimate
        // clamp, never a loud toolbar).
        let content_w: Option<f32> = (|| {
            let service = host.inner.text_service.borrow();
            let service = service.as_deref()?;
            let family = host.layout_config().family.clone();
            let mut w = 2.0;
            for item in model.items.iter() {
                let style = crate::text::TextStyle::new(&family, 14.0);
                w += service.shape(item.label(), &style).ok()?.total_advance + 24.0;
            }
            Some(w)
        })();
        let x = al.clamp(0.0, (vw - content_w.unwrap_or(TOOLBAR_EST_W_PX)).max(0.0));
        let y = if at >= TOOLBAR_ROW_H_PX + 8.0 {
            at - TOOLBAR_ROW_H_PX - 8.0
        } else {
            ab + 8.0
        };
        let items: Vec<VNode> = model
            .items
            .iter()
            .map(|item| {
                crate::vnode::Div(item.debug_name())
                    .style(crate::style::Style::new().pad_x(12).pad_y(8))
                    .child(crate::vnode::VNode::from(
                        crate::vnode::Text::new(item.label()).size(14),
                    ))
            })
            .collect();
        let mut bar_style = crate::style::Style::new()
            .bg(tokens.surface)
            .ink(tokens.text_primary)
            .border(1, tokens.border)
            .radius(8.0)
            .x(x)
            .absolute_y(y);
        if let Some(w) = content_w {
            bar_style = bar_style.w(w);
        }
        let bar = crate::vnode::Div("oppa_toolbar_bar")
            .style(bar_style)
            .child(crate::vnode::Row("oppa_toolbar_row").children(items));
        let portal = crate::vnode::Portal("oppa_toolbar_layer").child(bar);
        e.children.push(portal);
        VNode::Element(e)
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
        // Decision 520: deferred exits — pinned snapshots outlive the
        // removal commit (paint past removal + evaluator drive).
        // Stamped commits snap (pins ignored everywhere, owners sweep
        // normally); re-showed subtrees cancel (fresh paint owns the
        // slot before the sweep below runs).
        if !diff.suppress_transitions && !diff.exits.is_empty() {
            self.inner
                .deferred
                .borrow_mut()
                .extend(diff.exits.iter().cloned());
        }
        self.cancel_superseded_exits(&diff);
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
        // Decision 519: subtree-unmount instance retirement — inline
        // instances that own no live node after this commit are evicted
        // in the same cut (cleanups run first, cancelling per-render
        // timers; leftover timers and handler-ownership rows are swept).
        // Effect-holding roots are never swept (explicit unmount only —
        // multi-root hosts park non-winning roots). Tracks already
        // retired above; completion observers keep the fire-on-unmount
        // contract (no behavior change there).
        self.retire_dead_instances(&diff);
        // TIME drive (M8, §9.1): the commit that creates interpolations
        // arms the shared settle drain (one animation while anything
        // lives — tracks or observers); frames continue at cadence
        // until it retires, then the loop idles. Stamped-only commits
        // never arm (zero actives and no observers).
        if self.inner.evaluator.borrow().active_count() > 0 {
            self.ensure_settle_drain();
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

    /// Evicts component instances that own no live retained node after
    /// this commit's removals (decision 519 — the subtree-unmount
    /// retirement closing Blocked row 2).
    ///
    /// Candidates come from two channels: `Remove` owners (snapshotted
    /// pre-retire — the arena cannot answer after) and displaced owners
    /// (slot-keyed recycling re-homing a survivor with no removal op).
    /// A candidate survives when any live node still names it (a shared
    /// ancestor's other branch, a recycled slot elsewhere) or when it
    /// holds a scheduler effect — effect-holders are roots, and roots
    /// unmount only through their handle (`MountHandle::unmount`): one
    /// host may mount several roots over a single last-run-wins tree,
    /// so a non-winning root is parked, never dead, and the sweep must
    /// not retire its effect. Otherwise the instance is dead and is
    /// evicted in this ordering:
    ///
    /// 1. `evict_instance` per dead instance (sorted for determinism):
    ///    child instances first (recursion), then LIFO cleanups — the
    ///    per-render timer cancels land here — then record drop and
    ///    parent-link scrub (re-show allocates fresh and re-seeds
    ///    state). Dead subtree instances never hold effects (roots are
    ///    skipped above), so no effect retires in this cut.
    /// 2. Leftover timer sweep by owner (backstop for timers that
    ///    outlived their cleanup — the tick backstop alone cannot see
    ///    them while the record lingers, which is exactly the bug).
    /// 3. Handler-ownership row prune (routing never resolves through
    ///    evicted instances; closures stay registered — identity is
    ///    id-keyed, never owner-keyed).
    ///
    /// Quiet when nothing was removed or displaced (no census walk).
    /// Writes no signals itself (user cleanups may schedule — the normal
    /// pump carries that); paint plans never see the owner metadata.
    fn retire_dead_instances(&self, diff: &TreeDiff) {
        let mut candidates: HashSet<u64> = HashSet::new();
        for op in &diff.ops {
            if let DiffOp::Remove {
                owner: Some(owner), ..
            } = op
            {
                candidates.insert(*owner);
            }
        }
        candidates.extend(diff.displaced_owners.iter().copied());
        if candidates.is_empty() {
            return;
        }
        // Live-owner census over surviving nodes only (dead instances
        // own zero — that is the eviction predicate), plus deferred
        // owners (decision 520 — pinned ghosts keep their instances
        // past removal; the post-exit orphan sweep collects them),
        // plus in-flight gesture owners (decision 520 — a Down whose
        // effects unmount its own subtree must still clear its
        // pressed flag on the Up; captures pin owners for exactly one
        // gesture, then the next sweep collects).
        let live: HashSet<u64> = {
            let rec = self.inner.rec.borrow();
            let mut live = HashSet::new();
            for id in rec.retained_ids() {
                if let Some(owner) = rec.get(id).and_then(|n| n.owner) {
                    live.insert(owner);
                }
            }
            for pin in self.inner.deferred.borrow().iter() {
                for owner in pin.root.owners().into_iter().flatten() {
                    live.insert(owner);
                }
            }
            for inst in self.inner.input.borrow().capture_instances.values() {
                live.insert(*inst);
            }
            live
        };
        let effects = self.inner.effects.borrow();
        let mut dead: Vec<u64> = candidates
            .difference(&live)
            .copied()
            .filter(|inst| !effects.contains_key(inst))
            .collect();
        drop(effects);
        if dead.is_empty() {
            return;
        }
        dead.sort_unstable();
        self.evict_all(dead);
    }

    /// Evicts `dead` (sorted by caller) one record at a time
    /// ([`evict_single`](Self::evict_single) — live children keep
    /// their records, rows, and timers; only dead links go).
    fn evict_all(&self, mut dead: Vec<u64>) {
        dead.sort_unstable();
        for instance in dead {
            self.evict_single(instance);
        }
    }

    /// Pinned exit snapshots for paint layers (decision 520): cloned
    /// per repaint for the builder/DOM exit layers (exit pins are
    /// overlay-small by adoption).
    pub fn exit_snapshots(&self) -> Vec<crate::reconciler::ExitPin> {
        self.inner.deferred.borrow().clone()
    }

    /// Cancels deferred exits superseded by this commit's additions
    /// (decision 520): an `(owner, debug)` add matching a live pin's
    /// root means the same instance re-rendered the same-labeled
    /// subtree — fresh paint owns the slot, the ghost releases and
    /// its tracks forget. Same-commit replace-exits cancel instantly
    /// (re-show reads as snap, never a crossfade — stated).
    fn cancel_superseded_exits(&self, diff: &TreeDiff) {
        if self.inner.deferred.borrow().is_empty() {
            return;
        }
        let rec = self.inner.rec.borrow();
        let mut added: Vec<(Option<u64>, String)> = Vec::new();
        for op in &diff.ops {
            if let DiffOp::Add { id, .. } = op {
                if let Some(node) = rec.get(*id) {
                    added.push((node.owner, node.debug.clone()));
                }
            }
        }
        if added.is_empty() {
            return;
        }
        let mut cancelled: Vec<NodeId> = Vec::new();
        {
            let mut deferred = self.inner.deferred.borrow_mut();
            let mut kept = Vec::with_capacity(deferred.len());
            for pin in deferred.drain(..) {
                let superseded = added
                    .iter()
                    .any(|(owner, debug)| pin.root.owner == *owner && pin.root.debug == *debug);
                if superseded {
                    cancelled.extend(pin.root.ids());
                } else {
                    kept.push(pin);
                }
            }
            *deferred = kept;
        }
        if !cancelled.is_empty() {
            self.inner.evaluator.borrow_mut().forget_exits(&cancelled);
            self.inner.cancelled_exits.borrow_mut().extend(cancelled);
        }
    }

    /// Takes node ids whose deferred exits were cancelled by re-show
    /// (decision 520 — registry-persistent presenters, i.e. DOM, drop
    /// the kept ghosts through this drain; plan-built backends rebuild
    /// from the host table and never call it).
    pub fn drain_cancelled_exits(&self) -> Vec<NodeId> {
        std::mem::take(&mut *self.inner.cancelled_exits.borrow_mut())
    }

    /// Releases deferred exits whose every node settled (decision
    /// 520): drops the snapshots, forgets the tracks, then sweeps
    /// newly orphaned instances (their owners outlived removal only
    /// through the exemption — without this they would linger, with
    /// live timers, to the next unrelated removal). Returns the
    /// released pin count. Runs inside the settle drive before
    /// observer-fire, so completion chains see the post-exit world.
    fn finalize_settled_exits(&self) -> usize {
        let released: Vec<crate::reconciler::ExitPin> = {
            let mut deferred = self.inner.deferred.borrow_mut();
            if deferred.is_empty() {
                return 0;
            }
            let ev = self.inner.evaluator.borrow();
            let (done, live): (Vec<_>, Vec<_>) = deferred
                .drain(..)
                .partition(|pin| pin.root.ids().iter().all(|id| ev.is_settled(*id)));
            *deferred = live;
            done
        };
        if released.is_empty() {
            return 0;
        }
        let mut ids = Vec::new();
        let mut owners = HashSet::new();
        for pin in &released {
            ids.extend(pin.root.ids());
            for owner in pin.root.owners().into_iter().flatten() {
                owners.insert(owner);
            }
        }
        self.inner.evaluator.borrow_mut().forget_exits(&ids);
        self.sweep_orphans(&owners);
        released.len()
    }

    /// Evicts just-released exit owners left nodeless (decision 520 —
    /// post-exit collection for exactly the instances this exit
    /// orphaned; the commit sweep already retires removal-driven
    /// dead globally, so this pass never garbage-collects unrelated
    /// nodeless instances — slot reserves, conditional branches, and
    /// timer-only controllers keep their records until their own
    /// removals name them). Roots, live owners, still-deferred owners,
    /// and in-flight gesture owners never collect here.
    fn sweep_orphans(&self, released: &HashSet<u64>) {
        if released.is_empty() {
            return;
        }
        let (live, deferred_owners): (HashSet<u64>, HashSet<u64>) = {
            let rec = self.inner.rec.borrow();
            let mut live = HashSet::new();
            for id in rec.retained_ids() {
                if let Some(owner) = rec.get(id).and_then(|n| n.owner) {
                    live.insert(owner);
                }
            }
            for inst in self.inner.input.borrow().capture_instances.values() {
                live.insert(*inst);
            }
            let mut deferred_owners = HashSet::new();
            for pin in self.inner.deferred.borrow().iter() {
                for owner in pin.root.owners().into_iter().flatten() {
                    deferred_owners.insert(owner);
                }
            }
            (live, deferred_owners)
        };
        let effects = self.inner.effects.borrow();
        let dead: Vec<u64> = released
            .iter()
            .copied()
            .filter(|inst| {
                !live.contains(inst)
                    && !deferred_owners.contains(inst)
                    && !effects.contains_key(inst)
                    && self.inner.instances.borrow().contains_key(inst)
            })
            .collect();
        drop(effects);
        if dead.is_empty() {
            return;
        }
        self.evict_all(dead);
    }

    /// Arms the shared TIME settle drain unless already armed
    /// (decision 512): one animation retires tracks, re-dirties live
    /// nodes for repaint, and fires completion observers — tracks and
    /// observers share it, never two animations.
    pub(super) fn ensure_settle_drain(&self) {
        if self.inner.trans_anim.get() {
            return;
        }
        self.inner.trans_anim.set(true);
        let weak: Weak<HostInner> = Rc::downgrade(&self.inner);
        self.inner.rt.add_animation(move |now| {
            let Some(inner) = weak.upgrade() else {
                return false;
            };
            let live = inner.evaluator.borrow_mut().settle(now);
            // Decision 520: completed exits release before observer
            // fire (completion chains see the post-exit world) and
            // sweep their orphaned instances (no leak through the
            // 519 retirement — proven headless).
            ComponentHost {
                inner: inner.clone(),
            }
            .finalize_settled_exits();
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
            // Decision 512: completion observers — fire one-shot
            // callbacks for nodes with no live track (retired this
            // tick or already settled). Take-then-extend: chained
            // re-observations during firing re-queue into the freed
            // table instead of clobbering the kept set. Callbacks run
            // without the evaluator borrowed (they may observe or
            // signal freely).
            if !inner.settle_observers.borrow().is_empty() {
                let pending = std::mem::take(&mut *inner.settle_observers.borrow_mut());
                let (fire, keep): (Vec<_>, Vec<_>) = {
                    let ev = inner.evaluator.borrow();
                    pending
                        .into_iter()
                        .partition(|(node, _)| ev.is_settled(*node))
                };
                for (_, cb) in fire {
                    cb();
                }
                inner.settle_observers.borrow_mut().extend(keep);
            }
            live > 0
        });
        self.inner.rt.request_frame();
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
