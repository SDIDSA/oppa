use super::*;

// ---------------------------------------------------------------------------
// Scenario 4 — reload mid-input-burst (INPUT drain vs RELOAD ordering)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct BurstProps {
    label: SharedString,
    hits: Signal<u32>,
}

mod burst_v1 {
    use super::*;

    #[component]
    pub fn Target(ctx: &Ctx, props: &BurstProps) -> VNode {
        let local = ctx.signal(0u32);
        let _ = local.get();
        let _ = props.label.clone();
        Div("tgt")
            .style(Style::new().size(44, 24))
            .semantics(Semantics::switch().checked(false).label(&props.label))
            .on_press({
                let hits = props.hits.clone();
                move || hits.set(hits.get() + 1)
            })
            .build()
    }

    component_manifest![Target(BurstProps)];
}

mod burst_v2 {
    use super::*;

    #[component]
    pub fn Target(ctx: &Ctx, props: &BurstProps) -> VNode {
        let _epoch = ctx.signal(0u32);
        let local = ctx.signal(0u32);
        let _ = local.get();
        let _ = props.label.clone();
        Div("tgt")
            .style(Style::new().size(44, 24))
            .semantics(Semantics::switch().checked(false).label(&props.label))
            .on_press({
                let hits = props.hits.clone();
                move || hits.set(hits.get() + 1)
            })
            .build()
    }

    component_manifest![Target(BurstProps)];
}

#[test]
fn m9_reload_mid_input_burst() {
    let mut rng = seed_of("mid-input-burst", 0x5011_0004);
    const ITERS: usize = 200;

    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let hits = rt.signal(0u32);
    host.mount(
        "Target",
        BurstProps {
            label: Arc::from("tgt"),
            hits: hits.clone(),
        },
        burst_v1::Target,
    );
    host.run_until_idle();
    // Sanity: the hit point lands on the target headless (explicit size
    // → committed box with no text service).
    assert!(host.hit_test(10.0, 12.0).is_some(), "burst rig misses");

    let hook_reg = Rc::new(RefCell::new(HotRegistry::new(host.clone())));
    hook_reg.borrow_mut().install(Box::new(StaticSource::new(
        "burst-v1",
        burst_v1::__oppa_manifest_descs(),
    )));
    HotRegistry::arm(&hook_reg);
    let mut direct_reg = HotRegistry::new(host.clone());
    direct_reg.install(Box::new(StaticSource::new(
        "burst-v1-direct",
        burst_v1::__oppa_manifest_descs(),
    )));

    let mut retired: Vec<oppa::HotGeneration> = Vec::new();
    let mut use_v2 = false;
    let mut hook_swaps = 0u32;
    let mut direct_swaps = 0u32;
    let mut bursts = 0u32;
    let mut events_injected = 0u64;
    let mut ordered_frames = 0u32;

    for i in 0..ITERS {
        let op = xorshift(&mut rng) % 6;
        match op {
            // 0-3: burst of 5-40 pointer Down/Up pairs (+ quiet keys and
            // Tabs that must NOT dispatch), then a swap — hook
            // (INPUT→RELOAD same frame) or direct (swap-then-drain).
            0..=3 => {
                let pairs = 5 + (xorshift(&mut rng) % 36) as usize;
                for _ in 0..pairs {
                    host.inject_input(InputEvent::pointer_down(10.0, 12.0));
                    host.inject_input(InputEvent::pointer_up(10.0, 12.0));
                    events_injected += 2;
                }
                // Ambient keys: quiet no-ops (never dispatch).
                let quiets = (xorshift(&mut rng) % 6) as usize;
                for _ in 0..quiets {
                    host.inject_input(InputEvent::key(0x41, KeyState::Pressed));
                    events_injected += 1;
                }
                if xorshift(&mut rng).is_multiple_of(2) {
                    host.inject_input(InputEvent::key(oppa::input::keys::TAB, KeyState::Pressed));
                    events_injected += 1;
                }
                let before = hits.get();
                bursts += 1;
                use_v2 = !use_v2 || xorshift(&mut rng).is_multiple_of(2);
                if xorshift(&mut rng).is_multiple_of(2) {
                    // Hook path: burst drains at INPUT, swap at RELOAD,
                    // same frame — assert the ordering mechanically.
                    // Pre-drain the log so the check reads this frame.
                    let _ = rt.take_phase_log();
                    let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                        Box::new(StaticSource::new(
                            "burst-v2",
                            burst_v2::__oppa_manifest_descs(),
                        ))
                    } else {
                        Box::new(StaticSource::new(
                            "burst-v1",
                            burst_v1::__oppa_manifest_descs(),
                        ))
                    };
                    HotRegistry::request_swap(&hook_reg, src);
                    assert!(host.run_once(), "burst+swap frame had no demand");
                    let log = rt.take_phase_log();
                    let inp = log.iter().position(|p| *p == oppa::Phase::Input);
                    let rel = log.iter().position(|p| *p == oppa::Phase::Reload);
                    match (inp, rel) {
                        (Some(a), Some(b)) => {
                            assert!(a < b, "iter {i}: RELOAD before INPUT ({log:?})");
                            ordered_frames += 1;
                        }
                        _ => panic!("iter {i}: swap frame missing INPUT/RELOAD ({log:?})"),
                    }
                    host.run_until_idle();
                    let rep = hook_reg.borrow();
                    let rep = rep.last_report().expect("hook swap ran");
                    assert!(rep.ok(), "hook swap evicted: {:?}", rep.evicted);
                    retired.push(rep.outgoing);
                    hook_swaps += 1;
                } else {
                    // Direct path: swap with the burst still queued —
                    // inputs drain post-swap through the new registry.
                    let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                        Box::new(StaticSource::new(
                            "burst-v2",
                            burst_v2::__oppa_manifest_descs(),
                        ))
                    } else {
                        Box::new(StaticSource::new(
                            "burst-v1",
                            burst_v1::__oppa_manifest_descs(),
                        ))
                    };
                    let rep = direct_reg.reload_to(src);
                    assert!(rep.ok(), "direct swap evicted: {:?}", rep.evicted);
                    retired.push(rep.outgoing);
                    direct_swaps += 1;
                    host.run_until_idle();
                }
                // Exactly-once under burst load, both orders: every Down/Up
                // pair dispatched exactly once; quiet keys never did.
                assert_eq!(
                    hits.get(),
                    before + pairs as u32,
                    "iter {i}: burst lost or duplicated under reload"
                );
                assert_no_retired_props(&host, &retired, "input-burst");
            }
            // 4: swap at rest.
            4 => {
                use_v2 = !use_v2;
                let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                    Box::new(StaticSource::new(
                        "burst-v2",
                        burst_v2::__oppa_manifest_descs(),
                    ))
                } else {
                    Box::new(StaticSource::new(
                        "burst-v1",
                        burst_v1::__oppa_manifest_descs(),
                    ))
                };
                let rep = direct_reg.reload_to(src);
                assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                retired.push(rep.outgoing);
                direct_swaps += 1;
                host.run_until_idle();
                assert_no_retired_props(&host, &retired, "input-burst");
            }
            // 5: settle.
            _ => {
                host.run_until_idle();
                assert_no_retired_props(&host, &retired, "input-burst");
            }
        }
    }

    eprintln!(
        "m9[input-burst] iters={ITERS} bursts={bursts} events={events_injected} \
         hook={hook_swaps} direct={direct_swaps} ordered_frames={ordered_frames}"
    );
    assert!(bursts >= 60, "too few bursts to stress the drain");
    assert!(hook_swaps >= 10, "hook path never exercised under burst");
    assert!(
        direct_swaps >= 10,
        "direct path never exercised under burst"
    );
    assert_eq!(
        ordered_frames, hook_swaps,
        "every hook frame must show INPUT→RELOAD ordering"
    );
}
