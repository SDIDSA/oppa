use super::*;

// ---------------------------------------------------------------------------
// Scenario 1 — reload mid-scroll (slot-keyed virtualized list mid-gesture)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ListProps {
    store: Store<ContactId, Contact>,
    viewport_h: f32,
    row_h: f32,
    over: usize,
}

impl Props for ListProps {}

#[derive(Clone)]
struct RowProps {
    store: Store<ContactId, Contact>,
    item: oppa::Memo<Option<ContactId>>,
    row_h: f32,
}

impl Props for RowProps {}

fn list_body(ctx: &Ctx, props: &ListProps, epoch: bool) -> VNode {
    if epoch {
        let _e = ctx.signal(0u32);
    }
    let offset = ctx.scroll_offset();
    let n = props.store.len();
    let k = slot_count(props.viewport_h, props.row_h, props.over);
    let row_h = props.row_h;
    let over = props.over;
    let first = window_first(offset.get(), row_h, over, n, k);
    let items: Vec<oppa::Memo<Option<ContactId>>> = (0..k)
        .map(|slot| {
            let store = props.store.clone();
            let offset = offset.clone();
            ctx.binding(move || {
                let f = window_first(offset.get(), row_h, over, n, k);
                store.get(f + slot)
            })
        })
        .collect();
    oppa::ScrollArea("list")
        .content_size(n as f32 * row_h)
        .style(Style::new().h(props.viewport_h).fill_width())
        .on_scroll(|| {})
        .children(items.into_iter().enumerate().map(|(slot, item)| {
            let row = first + slot;
            let row_props = RowProps {
                store: props.store.clone(),
                item,
                row_h: props.row_h,
            };
            oppa::Row("slot")
                .style(
                    Style::new()
                        .absolute_y(row as f32 * props.row_h)
                        .h(props.row_h)
                        .fill_width(),
                )
                .key(slot as u64)
                .child(ctx.child("Row", slot as u64, &row_props, row_body_for(epoch)))
        }))
}

// Function-pointer indirection so v1/v2 lists render through their own row
// body (child code follows the parent revision — the product-loop shape).
fn row_body_for(epoch: bool) -> fn(&Ctx, &RowProps) -> VNode {
    if epoch {
        list_v2::Row
    } else {
        list_v1::Row
    }
}

fn row_body(ctx: &Ctx, props: &RowProps, epoch: bool) -> VNode {
    if epoch {
        let _e = ctx.signal(1u32);
    }
    let contact = ctx.memo({
        let store = props.store.clone();
        let item = props.item.clone();
        move || {
            item.read()
                .and_then(|id| store.lookup(&id))
                .unwrap_or_else(|| Contact {
                    display_name: Arc::from(""),
                    status: Arc::from(""),
                })
        }
    });
    let item_id = props.item.read();
    let sel: Signal<bool> = ctx.keyed_state(item_id.map(|id| id.0).unwrap_or(u64::MAX), || false);
    // Showcase 120 ms transition: every recycle is a real bg delta, and
    // the binding-edge stamp must suppress all of it (created delta 0).
    let style = Style::new()
        .h(props.row_h)
        .fill_width()
        .pad_x(12)
        .bg(cell_bg(item_id, sel.get()))
        .transition(Transition::new(120.ms(), Ease::Out));
    let item = props.item.clone();
    let rt = ctx.runtime();
    oppa::Row("cell")
        .style(style)
        .semantics(
            oppa::Semantics::list_item()
                .selected(sel.get())
                .label(&contact.read().display_name),
        )
        .on_press(move || {
            if let Some(id) = item.read() {
                let flag: Signal<bool> = rt.keyed_state(id.0, || false);
                flag.set(!flag.get());
            }
        })
        .children([
            Text {
                text: contact.read().display_name.clone(),
                style: Text::title_small,
            }
            .into(),
            Text {
                text: contact.read().status.clone(),
                style: Text::body_secondary,
            }
            .into(),
        ])
}

mod list_v1 {
    use super::*;

    #[component]
    pub fn List(ctx: &Ctx, props: &ListProps) -> VNode {
        list_body(ctx, props, false)
    }

    #[component]
    pub fn Row(ctx: &Ctx, props: &RowProps) -> VNode {
        row_body(ctx, props, false)
    }

    component_manifest![List(ListProps), Row(RowProps)];
}

mod list_v2 {
    use super::*;

    #[component]
    pub fn List(ctx: &Ctx, props: &ListProps) -> VNode {
        // Edited body: inserted signal shifts later sites → re-seed (§5.1).
        list_body(ctx, props, true)
    }

    #[component]
    pub fn Row(ctx: &Ctx, props: &RowProps) -> VNode {
        row_body(ctx, props, true)
    }

    component_manifest![List(ListProps), Row(RowProps)];
}

#[test]
fn m9_reload_mid_scroll() {
    let mut rng = seed_of("mid-scroll", 0x5011_0001);
    const N_ITEMS: usize = 200;
    const ITERS: usize = 250;

    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let (ids, values) = seed_contacts(N_ITEMS);
    let store = Store::new(&rt, ids, values);
    host.set_keyed_capacity(N_ITEMS + 64);
    let k = slot_count(VIEWPORT_H, ROW_H, OVER);
    let handle = host.mount(
        "List",
        ListProps {
            store: store.clone(),
            viewport_h: VIEWPORT_H,
            row_h: ROW_H,
            over: OVER,
        },
        list_v1::List,
    );
    host.run_until_idle();
    let root = handle.root_instance();
    let offset: ScrollOffset = host.instance_scroll(root).expect("scroll handle");
    let list_node = find_retained_by_debug(&host, "list")[0];
    host.bind_scroll(list_node, offset.clone());

    let mut reg = HotRegistry::new(host.clone());
    reg.install(Box::new(StaticSource::new(
        "list-v1",
        list_v1::__oppa_manifest_descs(),
    )));
    // Hook registry for same-frame INPUT→RELOAD swaps (phase-ordering arm).
    let hook_reg = Rc::new(RefCell::new(HotRegistry::new(host.clone())));
    hook_reg.borrow_mut().install(Box::new(StaticSource::new(
        "list-v1-hook",
        list_v1::__oppa_manifest_descs(),
    )));
    HotRegistry::arm(&hook_reg);

    let mut retired: Vec<oppa::HotGeneration> = Vec::new();
    let mut use_v2 = false;
    let mut swaps = 0u32;
    let mut hook_swaps = 0u32;
    let mut direct_swaps = 0u32;
    let mut swaps_mid_sweep = 0u32;
    let mut input_fed = 0u32;
    let mut direct_fed = 0u32;
    let mut nonvacuous_stamped = 0u32;
    let mut offset_px = 0.0f32;
    let max_offset = N_ITEMS as f32 * ROW_H - VIEWPORT_H;
    let mut last_scroll_iter = 0usize;

    for i in 0..ITERS {
        let op = xorshift(&mut rng) % 10;
        match op {
            // 0-4: scroll step — mixed TIME-style direct writes and
            // INPUT-fed Scroll events (window-lag arm: sub-row straddles
            // and full-window jumps both occur).
            0..=4 => {
                let r = xorshift(&mut rng);
                let step = match r % 4 {
                    0 => (r % 7) as f32 + 0.5,           // sub-row straddle
                    1 => ROW_H * ((r % 5) as f32 + 1.0), // whole rows
                    2 => (r % 300) as f32,               // arbitrary jump
                    _ => -(offset_px / 2.0),             // fling back toward top
                };
                offset_px = (offset_px + step).clamp(0.0, max_offset);
                // Stamp instruments, measured around the TICK (the swap
                // re-run below changes nothing, so it stamps nothing —
                // measuring there would be vacuous by construction).
                let created_before = host.with_evaluator(|e| e.created());
                let suppressed_before = host.with_evaluator(|e| e.suppressed());
                if r.is_multiple_of(2) {
                    // INPUT-fed (browser-scroll mapping): routed dy
                    // accumulates at the INPUT boundary.
                    let cur = offset.get();
                    let dy = offset_px - cur;
                    host.inject_input(InputEvent::Scroll {
                        target: list_node,
                        dx: 0.0,
                        dy,
                    });
                    input_fed += 1;
                } else {
                    // TIME-physics style direct write.
                    offset.set(offset_px);
                    direct_fed += 1;
                }
                host.run_until_idle();
                last_scroll_iter = i;
                // Payoff trace, every tick: zero structure ops.
                let tail = host.last_diff().expect("scroll commits");
                assert_eq!(
                    tail.structure_ops(),
                    0,
                    "iter {i}: scroll tick tore structure ({})",
                    tail.structure_ops()
                );
                // Binding-edge ticks create zero interpolators; ticks
                // that rebind count suppressions (non-vacuous stamp).
                let created_after = host.with_evaluator(|e| e.created());
                let suppressed_after = host.with_evaluator(|e| e.suppressed());
                assert_eq!(
                    created_after, created_before,
                    "iter {i}: scroll tick interpolated a rebind"
                );
                if suppressed_after > suppressed_before {
                    nonvacuous_stamped += 1;
                }
            }
            // 5: rare selection flip (unstamped real change — creates a
            // live 120 ms interpolation; wall-settles inside run_until).
            5 => {
                if xorshift(&mut rng).is_multiple_of(4) {
                    let id = ContactId(xorshift(&mut rng) % N_ITEMS as u64);
                    let flag: Signal<bool> = rt.keyed_state(id.0, || false);
                    flag.set(!flag.get());
                    host.run_until_idle();
                } else {
                    host.run_until_idle();
                }
            }
            // 6: rare store mutation (coarse invalidation — all rows
            // re-derive; text deltas jump by lock, never interpolate).
            6 => {
                if xorshift(&mut rng).is_multiple_of(8) {
                    let (mut ids2, mut vals) = (Vec::new(), HashMap::new());
                    for j in 0..N_ITEMS {
                        let id = ContactId(j as u64);
                        ids2.push(id);
                        vals.insert(
                            id,
                            Contact {
                                display_name: Arc::from(format!("Contact {j:03}")),
                                status: Arc::from(if j % 2 == 0 { "busy" } else { "away" }),
                            },
                        );
                    }
                    store.set(ids2, vals);
                    host.run_until_idle();
                }
            }
            // 7-8: swap — direct or hook, at a random point mid-sweep.
            7 | 8 => {
                // Swap re-runs change no values, so they must create no
                // interpolators either (measured here); the stamp itself
                // is measured around scroll ticks above.
                let created_before = host.with_evaluator(|e| e.created());
                let suppressed_before = host.with_evaluator(|e| e.suppressed());
                let _ = suppressed_before;
                use_v2 = !use_v2 || xorshift(&mut rng).is_multiple_of(2);
                let slots_before: Vec<oppa::NodeId> = {
                    let mut v = find_retained_by_debug(&host, "slot");
                    v.sort();
                    v
                };
                assert_eq!(slots_before.len(), k, "slot count drifted pre-swap");
                if xorshift(&mut rng).is_multiple_of(3) {
                    // Hook path: same-frame INPUT→RELOAD ordering. Drain
                    // the phase log first so the ordering check below
                    // reads THIS swap's frame, not stale history.
                    let _ = rt.take_phase_log();
                    let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                        Box::new(StaticSource::new(
                            "list-v2",
                            list_v2::__oppa_manifest_descs(),
                        ))
                    } else {
                        Box::new(StaticSource::new(
                            "list-v1",
                            list_v1::__oppa_manifest_descs(),
                        ))
                    };
                    HotRegistry::request_swap(&hook_reg, src);
                    host.run_until_idle();
                    let rep = hook_reg.borrow();
                    let rep = rep.last_report().expect("hook swap ran");
                    assert!(rep.ok(), "hook swap evicted: {:?}", rep.evicted);
                    retired.push(rep.outgoing);
                    hook_swaps += 1;
                    // Phase ordering: INPUT before RELOAD in the swap frame.
                    let log = rt.take_phase_log();
                    let inp = log.iter().position(|p| *p == oppa::Phase::Input);
                    let rel = log.iter().position(|p| *p == oppa::Phase::Reload);
                    if let (Some(a), Some(b)) = (inp, rel) {
                        assert!(a < b, "RELOAD ran before INPUT ({log:?})");
                    }
                } else {
                    let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                        Box::new(StaticSource::new(
                            "list-v2",
                            list_v2::__oppa_manifest_descs(),
                        ))
                    } else {
                        Box::new(StaticSource::new(
                            "list-v1",
                            list_v1::__oppa_manifest_descs(),
                        ))
                    };
                    let rep = reg.reload_to(src);
                    assert!(rep.ok(), "direct swap evicted: {:?}", rep.evicted);
                    retired.push(rep.outgoing);
                    direct_swaps += 1;
                }
                swaps += 1;
                if i.saturating_sub(last_scroll_iter) <= 2 {
                    swaps_mid_sweep += 1;
                }
                host.run_until_idle();
                // Slot identity survives the swap (keys are slots).
                let mut slots_after = find_retained_by_debug(&host, "slot");
                slots_after.sort();
                assert_eq!(slots_before, slots_after, "iter {i}: slot ids tore");
                // Stamped sweep: swap re-runs create zero interpolators.
                let created_after = host.with_evaluator(|e| e.created());
                assert_eq!(
                    created_after, created_before,
                    "iter {i}: swap created interpolators on stamped commits"
                );
                assert_no_retired_props(&host, &retired, "mid-scroll");
                assert_evaluator_live_nodes(&host, "mid-scroll");
            }
            // 9: settle + invariant sweep.
            _ => {
                host.run_until_idle();
                assert_no_retired_props(&host, &retired, "mid-scroll");
                assert_evaluator_live_nodes(&host, "mid-scroll");
            }
        }
    }

    // Adversarial-proof counters: the fuzzer must actually have swapped
    // mid-sweep through both paths and seen real stamps — else clean.
    eprintln!(
        "m9[mid-scroll] iters={ITERS} swaps={swaps} (hook={hook_swaps} direct={direct_swaps}) \
         mid_sweep={swaps_mid_sweep} input_fed={input_fed} direct_fed={direct_fed} \
         nonvacuous_stamped={nonvacuous_stamped} slots={k}"
    );
    assert!(swaps >= 15, "too few swaps to stress mid-scroll");
    assert!(hook_swaps >= 3, "hook path never exercised mid-scroll");
    assert!(direct_swaps >= 3, "direct path never exercised mid-scroll");
    assert!(
        swaps_mid_sweep >= 5,
        "no swap landed mid-sweep — timing vacuous"
    );
    assert!(input_fed >= 10, "INPUT-fed scroll never exercised");
    assert!(direct_fed >= 10, "TIME-style scroll never exercised");
    assert!(
        nonvacuous_stamped >= 3,
        "stamp never fired non-vacuously — suppression claim vacuous"
    );
}
