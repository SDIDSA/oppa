use super::*;

// ---------------------------------------------------------------------------
// Scenario 5 — reload with in-flight async tasks (§9.6 residence rule)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct TaskProps {
    label: SharedString,
}

mod task_v1 {
    use super::*;

    #[component]
    pub fn WorkerRoot(ctx: &Ctx, props: &TaskProps) -> VNode {
        let local = ctx.signal(0u32);
        let _ = local.get();
        let _ = props.label.clone();
        Div("worker").build()
    }

    component_manifest![WorkerRoot(TaskProps)];
}

mod task_v2 {
    use super::*;

    #[component]
    pub fn WorkerRoot(ctx: &Ctx, props: &TaskProps) -> VNode {
        let _epoch = ctx.signal(0u32);
        let local = ctx.signal(0u32);
        let _ = local.get();
        let _ = props.label.clone();
        Div("worker").build()
    }

    component_manifest![WorkerRoot(TaskProps)];
}

/// Mailbox key for the task test lives inside the test body (`KEY`).

#[test]
fn m9_reload_inflight_tasks() {
    let mut rng = seed_of("inflight-tasks", 0x5011_0005);
    const ITERS: usize = 150;
    // Separate mailbox key per run would collide across suite runs in one
    // process (keyed_state is per-runtime, and runtimes are per-test —
    // no collision). Fixed key is fine.
    const KEY: u64 = 0x5A51;

    let host = ComponentHost::new();
    let rt = host.runtime();
    host.mount(
        "WorkerRoot",
        TaskProps {
            label: Arc::from("worker"),
        },
        task_v1::WorkerRoot,
    );
    host.run_until_idle();

    let mut reg = HotRegistry::new(host.clone());
    reg.install(Box::new(StaticSource::new(
        "task-v1",
        task_v1::__oppa_manifest_descs(),
    )));

    let mut retired: Vec<oppa::HotGeneration> = Vec::new();
    let mut use_v2 = false;
    let mut swaps = 0u32;
    let mut raced = 0u32;
    let mut spawned: u64 = 0;
    let mut settled_applied = 0u32;

    for i in 0..ITERS {
        let op = xorshift(&mut rng) % 6;
        match op {
            // 0-2: spawn 1-3 tasks with 0-3 ms sleeps, then swap
            // IMMEDIATELY without quiescing — pending tasks hit
            // cancel-at-RELOAD, running ones finish into a retired tag.
            0..=2 => {
                let n = 1 + xorshift(&mut rng) % 3;
                for _ in 0..n {
                    let sleep_ms = xorshift(&mut rng) % 4;
                    spawned += 1;
                    rt.spawn_task(move |scope| {
                        if sleep_ms > 0 {
                            std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
                        }
                        scope.submit(move |rt| {
                            rt.keyed_state::<u32>(KEY, || 0).update(|v| v + 1);
                        });
                    });
                }
                let mail_before: u32 = rt.keyed_state::<u32>(KEY, || 0).get();
                let dropped_before = rt.stats().tasks_dropped;
                let discarded_before = rt.stats().worker_discarded;
                use_v2 = !use_v2 || xorshift(&mut rng).is_multiple_of(2);
                let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                    Box::new(StaticSource::new(
                        "task-v2",
                        task_v2::__oppa_manifest_descs(),
                    ))
                } else {
                    Box::new(StaticSource::new(
                        "task-v1",
                        task_v1::__oppa_manifest_descs(),
                    ))
                };
                let rep = reg.reload_to(src);
                assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                assert!(
                    rep.tasks_dropped as u64 <= n,
                    "iter {i}: dropped more than spawned"
                );
                retired.push(rep.outgoing);
                swaps += 1;
                // Quiesce the executor, then drain: every spawned task
                // reaches exactly one terminal state.
                let mut waited = 0;
                loop {
                    let s = rt.stats();
                    if s.tasks_done + s.tasks_dropped >= spawned {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    waited += 1;
                    assert!(waited < 10_000, "iter {i}: tasks never resolved");
                }
                host.run_until_idle();
                // Retired-generation results never apply: the mailbox is
                // untouched by the racing tasks.
                assert_eq!(
                    rt.keyed_state::<u32>(KEY, || 0).get(),
                    mail_before,
                    "iter {i}: retired task result applied"
                );
                let s = rt.stats();
                if s.tasks_dropped > dropped_before || s.worker_discarded > discarded_before {
                    raced += 1;
                }
                assert_no_retired_props(&host, &retired, "inflight-tasks");
            }
            // 3: spawn + settle WITHOUT swapping (applied path — the
            // mailbox works; retired-discards above are not vacuous).
            3 => {
                spawned += 1;
                rt.spawn_task(move |scope| {
                    scope.submit(move |rt| {
                        rt.keyed_state::<u32>(KEY, || 0).update(|v| v + 1);
                    });
                });
                let mut waited = 0;
                loop {
                    let s = rt.stats();
                    if s.tasks_done + s.tasks_dropped >= spawned {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    waited += 1;
                    assert!(waited < 10_000, "iter {i}: task never resolved");
                }
                let before: u32 = rt.keyed_state::<u32>(KEY, || 0).get();
                host.run_until_idle();
                assert_eq!(
                    rt.keyed_state::<u32>(KEY, || 0).get(),
                    before + 1,
                    "iter {i}: same-generation task did not apply"
                );
                settled_applied += 1;
            }
            // 4-5: settle + global accounting.
            _ => {
                host.run_until_idle();
                assert_no_retired_props(&host, &retired, "inflight-tasks");
            }
        }
    }

    // Quiesce + exactly-once end to end.
    let mut waited = 0;
    loop {
        let s = rt.stats();
        if s.tasks_done + s.tasks_dropped >= spawned {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "tasks never resolved");
    }
    host.run_until_idle();
    let s = rt.stats();
    eprintln!(
        "m9[inflight-tasks] iters={ITERS} spawned={spawned} swaps={swaps} raced={raced} \
         applied_path={settled_applied} done={} dropped={} applied={} discarded={}",
        s.tasks_done, s.tasks_dropped, s.worker_applied, s.worker_discarded,
    );
    // Every retired task resolves exactly once (discard XOR drop); the
    // applied-path tasks prove the mailbox counts real applies.
    assert_eq!(s.tasks_done + s.tasks_dropped, spawned);
    assert_eq!(
        s.worker_applied, settled_applied as u64,
        "only the no-swap tasks may apply"
    );
    assert!(swaps >= 15, "too few swaps");
    assert!(raced >= 5, "cancel/discard race never hit — timing vacuous");
    assert!(settled_applied >= 5, "applied path never exercised");
}
