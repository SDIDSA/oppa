use super::*;

// ---------------------------------------------------------------------------
// Scenario 2 — reload mid-transition (in-flight interpolation at swap)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct FlipProps {
    label: SharedString,
    /// Author-owned toggle (survives swaps — not a ctx local).
    flag: Signal<bool>,
    probe: Signal<u32>,
}

mod flip_v1 {
    use super::*;

    #[component]
    pub fn Flip(ctx: &Ctx, props: &FlipProps) -> VNode {
        let local = ctx.signal(0u32);
        let bg = if props.flag.get() {
            oppa::Color(0x11_11_11)
        } else {
            oppa::Color(0x22_22_22)
        };
        props
            .probe
            .set(local.get() + if props.flag.get() { 100 } else { 0 });
        let _ = props.label.clone();
        Div("flip")
            .style(
                Style::new()
                    .size(44, 24)
                    .bg(bg)
                    // Fuzz duration (stated): 20 ms wall-settle, not the
                    // 120 ms showcase — stamp semantics are duration-free.
                    .transition(Transition::new(20.ms(), Ease::Out)),
            )
            .on_press({
                let flag = props.flag.clone();
                move || flag.set(!flag.get())
            })
            .build()
    }

    component_manifest![Flip(FlipProps)];
}

mod flip_v2 {
    use super::*;

    #[component]
    pub fn Flip(ctx: &Ctx, props: &FlipProps) -> VNode {
        let _epoch = ctx.signal(0u32);
        let local = ctx.signal(0u32);
        let bg = if props.flag.get() {
            oppa::Color(0x11_11_11)
        } else {
            oppa::Color(0x22_22_22)
        };
        props
            .probe
            .set(local.get() + if props.flag.get() { 100 } else { 0 });
        let _ = props.label.clone();
        Div("flip")
            .style(
                Style::new()
                    .size(44, 24)
                    .bg(bg)
                    .transition(Transition::new(20.ms(), Ease::Out)),
            )
            .on_press({
                let flag = props.flag.clone();
                move || flag.set(!flag.get())
            })
            .build()
    }

    component_manifest![Flip(FlipProps)];
}

#[test]
fn m9_reload_mid_transition() {
    let mut rng = seed_of("mid-transition", 0x5011_0002);
    const ITERS: usize = 250;

    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let flag = rt.signal(false);
    let probe = rt.signal(0u32);
    let mk = |label: &str| FlipProps {
        label: Arc::from(label),
        flag: flag.clone(),
        probe: probe.clone(),
    };
    host.mount("Flip", mk("flip"), flip_v1::Flip);
    host.run_until_idle();

    let mut reg = HotRegistry::new(host.clone());
    reg.install(Box::new(StaticSource::new(
        "flip-v1",
        flip_v1::__oppa_manifest_descs(),
    )));

    let mut retired: Vec<oppa::HotGeneration> = Vec::new();
    let mut use_v2 = false;
    let mut swaps = 0u32;
    let mut swaps_with_live = 0u32;
    let mut flips = 0u32;
    let mut settles_exact = 0u32;

    for i in 0..ITERS {
        let op = xorshift(&mut rng) % 6;
        match op {
            // 0-1: flip the toggle (unstamped real change → one live
            // interpolation), then sometimes swap in the SAME iteration
            // while it is still live — deterministically mid-transition.
            // NOTE: `run_until_idle` wall-settles the 20 ms interpolation
            // (system clock), so liveness is observed after a single
            // `run_once`, never after a full settle; `created` (monotonic)
            // is the creation proof.
            0 | 1 => {
                let created_before = host.with_evaluator(|e| e.created());
                flag.set(!flag.get());
                flips += 1;
                assert!(host.run_once(), "flip frame had no demand");
                let live_now = host.with_evaluator(|e| e.active_count());
                assert!(live_now > 0, "iter {i}: flip created no interpolator");
                assert_eq!(
                    host.with_evaluator(|e| e.created()),
                    created_before + 1,
                    "iter {i}: flip must create exactly one bg interpolator"
                );
                assert_evaluator_live_nodes(&host, "mid-transition flip");
                if xorshift(&mut rng).is_multiple_of(2) {
                    use_v2 = !use_v2;
                    let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                        Box::new(StaticSource::new(
                            "flip-v2",
                            flip_v2::__oppa_manifest_descs(),
                        ))
                    } else {
                        Box::new(StaticSource::new(
                            "flip-v1",
                            flip_v1::__oppa_manifest_descs(),
                        ))
                    };
                    let live_at_swap = host.with_evaluator(|e| e.active_count());
                    let rep = reg.reload_to(src);
                    assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                    retired.push(rep.outgoing);
                    swaps += 1;
                    if live_at_swap > 0 {
                        swaps_with_live += 1;
                    }
                    // Live nodes still resolve post-swap (no retired
                    // generation's style values under interpolation).
                    assert_evaluator_live_nodes(&host, "mid-transition swap");
                    assert_no_retired_props(&host, &retired, "mid-transition");
                }
                // Wall-settle then exact-settle assert.
                host.run_until_idle();
                let settled = host.with_evaluator(|e| e.active_count());
                assert_eq!(settled, 0, "iter {i}: interpolation never settled");
                let expect = if flag.get() { 100 } else { 0 };
                assert_eq!(probe.get(), expect, "iter {i}: probe diverged");
                settles_exact += 1;
            }
            // 2: swap at rest (evaluator-target survival, no live interp).
            2 => {
                use_v2 = !use_v2;
                let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                    Box::new(StaticSource::new(
                        "flip-v2",
                        flip_v2::__oppa_manifest_descs(),
                    ))
                } else {
                    Box::new(StaticSource::new(
                        "flip-v1",
                        flip_v1::__oppa_manifest_descs(),
                    ))
                };
                let rep = reg.reload_to(src);
                assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                retired.push(rep.outgoing);
                swaps += 1;
                host.run_until_idle();
                let expect = if flag.get() { 100 } else { 0 };
                assert_eq!(probe.get(), expect);
                assert_no_retired_props(&host, &retired, "mid-transition");
                assert_evaluator_live_nodes(&host, "mid-transition");
            }
            // 3-5: settle + invariants.
            _ => {
                host.run_until_idle();
                let expect = if flag.get() { 100 } else { 0 };
                assert_eq!(probe.get(), expect);
                assert_no_retired_props(&host, &retired, "mid-transition");
                assert_evaluator_live_nodes(&host, "mid-transition");
            }
        }
    }

    eprintln!(
        "m9[mid-transition] iters={ITERS} flips={flips} swaps={swaps} \
         swaps_with_live={swaps_with_live} settles_exact={settles_exact}"
    );
    assert!(flips >= 30, "too few flips to stress interpolation");
    assert!(swaps >= 15, "too few swaps");
    assert!(
        swaps_with_live >= 10,
        "no swap landed with a live interpolator — mid-transition timing vacuous"
    );
}
