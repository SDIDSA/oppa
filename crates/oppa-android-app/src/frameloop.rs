//! Frame-loop decomposition (phone round, bet follow-up): splits
//! the single 86 ms "frame cost" number into first vs steady,
//! render vs readback, so the hatch decision rests on parts, not
//! a bundle.
//!
//! Method (stated): N=20 consecutive full `render_pixels`
//! (render + copy + map + 10 MB read, CPU wall each), first
//! reported separately (pipeline compilation lives there), steady
//! mean/min/max over the rest; then N=20 `render_noread`
//! (same GPU work + queue stall, no copy/map) the same way.
//! Implied readback overhead = full_mean − bare_mean. Present CPU
//! cost is recorded separately in the swapchain loop
//! (`PresentReport::cpu_ms` averaged in `taps.txt`'s record).
//!
//! Every iteration asserts the byte length (a GPU that degrades
//! mid-loop fails loudly instead of averaging garbage).

use std::path::Path;
use std::time::Instant;

use oppa::ComponentHost;
use oppa::RendererBackend;
use oppa_cpu::{CpuBackend, FramePlanBuilder};
use oppa_vello::VelloBackend;

const FRAMES: usize = 20;

fn stats(name: &str, samples: &[f64]) -> String {
    let first = samples[0];
    let rest = &samples[1..];
    let mean = rest.iter().sum::<f64>() / rest.len() as f64;
    let min = rest.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = rest.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    format!("{name}_first={first:.1} {name}_mean={mean:.1} {name}_min={min:.1} {name}_max={max:.1}")
}

/// Runs the decomposition on an ensured backend (the first full
/// render here is the true first render — pipeline compilation
/// included — so call this before any other GPU render).
/// Writes `frameloop.txt`; returns the meta record line.
pub fn run_frame_loop(
    backend: &mut VelloBackend,
    vsurf: oppa::SurfaceId,
    dir: &Path,
    expected_bytes: usize,
    tag: &str,
) -> Result<String, String> {
    let mut full: Vec<f64> = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let t = Instant::now();
        let img = backend
            .render_pixels(vsurf)
            .map_err(|e| format!("{tag} loop full render: {e:?}"))?;
        if img.pixels.len() != expected_bytes {
            return Err(format!(
                "{tag} loop byte drift: {} != {expected_bytes}",
                img.pixels.len()
            ));
        }
        full.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let mut bare: Vec<f64> = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        bare.push(
            backend
                .render_noread(vsurf)
                .map_err(|e| format!("{tag} loop bare render: {e:?}"))?,
        );
    }
    let full_mean = full[1..].iter().sum::<f64>() / (FRAMES - 1) as f64;
    let bare_mean = bare[1..].iter().sum::<f64>() / (FRAMES - 1) as f64;
    let body = format!(
        "path={tag} frames={FRAMES}\n{} \n{} \nreadback_implied_ms={:.1}\n",
        stats("full", &full),
        stats("bare", &bare),
        full_mean - bare_mean,
    );
    std::fs::write(dir.join("frameloop.txt"), &body)
        .map_err(|e| format!("write frameloop.txt: {e}"))?;
    Ok(format!(
        "frameloop_{tag}=[{} {} readback_implied={:.1}]",
        stats("full", &full),
        stats("bare", &bare),
        full_mean - bare_mean,
    ))
}

/// Sustained incremental frames per damage-loop run (Round 20.4,
/// decision 327): long enough for a steady p50/p95, short enough
/// the on-device proof phase stays bounded (60 CPU-arm iterations
/// over the live scene size).
pub const DAMAGE_FRAMES: usize = 60;

/// Steady-state incremental damage timings over one damage-loop
/// run (all ms, CPU wall around flip → settle → build → paint →
/// readback).
#[derive(Clone, Debug, PartialEq)]
pub struct DamageStats {
    pub n: usize,
    pub min_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

/// Nearest-rank percentiles over frame timings (`None` when empty
/// — a run with no samples is a loud caller error, never a
/// zero-stat line in the oracle).
pub fn damage_stats(samples: &[f64]) -> Option<DamageStats> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    // Nearest-rank: rank r = ceil(p * n), 1-based (clamped — p95
    // of 60 lands on the 57th fastest frame).
    let rank = |p: f64| sorted[((p * n as f64).ceil() as usize).clamp(1, n) - 1];
    Some(DamageStats {
        n,
        min_ms: sorted[0],
        p50_ms: rank(0.50),
        p95_ms: rank(0.95),
        max_ms: sorted[n - 1],
    })
}

/// One-line oracle/meta record: the cold full-scene baseline next
/// to the steady-state incremental damage stats (same line, so
/// `oracle.txt` / `meta.txt` carry both timings together).
pub fn format_damage_record(tag: &str, cold_full_ms: f64, stats: &DamageStats) -> String {
    format!(
        "damage_{tag}_cold_full_ms={cold_full_ms:.1} damage_{tag}_n={} damage_{tag}_min_ms={:.1} damage_{tag}_p50_ms={:.1} damage_{tag}_p95_ms={:.1} damage_{tag}_max_ms={:.1}\n",
        stats.n, stats.min_ms, stats.p50_ms, stats.p95_ms, stats.max_ms,
    )
}

/// Sustained damage loop (Round 20.4, decision 327): `DAMAGE_FRAMES`
/// consecutive single-control state flips through
/// `host.run_until_idle()` → `builder.build_full` → `cpu.paint` →
/// pixmap readback, timing each incremental frame. The cold
/// full-scene build+paint+readback runs first (pipeline/bookkeeping
/// warmup lives there, like `run_frame_loop`'s first render —
/// steady stats exclude it). Every iteration asserts the readback
/// byte length (a backend that degrades mid-loop fails loudly
/// instead of averaging garbage). Writes `damage.txt`; returns the
/// oracle/meta record line (the caller appends it next to the
/// full-scene cold timings).
///
/// `flip` is one state flip against the host (a tap, a signal
/// toggle — whatever moves exactly one control); it runs inside
/// the timed section so the settle it schedules is measured too.
#[allow(clippy::too_many_arguments)]
pub fn run_sustained_damage_loop(
    host: &mut ComponentHost,
    builder: &mut FramePlanBuilder,
    cpu: &mut CpuBackend,
    csurf: oppa::SurfaceId,
    expected_bytes: usize,
    dir: &Path,
    tag: &str,
    mut flip: impl FnMut(&mut ComponentHost),
) -> Result<String, String> {
    // Cold full-scene baseline (excluded from the steady stats).
    let t = Instant::now();
    host.run_until_idle();
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    cpu.paint(csurf, &plan)
        .map_err(|e| format!("{tag} damage cold paint: {e:?}"))?;
    let cold_len = readback_len(cpu, csurf)?;
    if cold_len != expected_bytes {
        return Err(format!(
            "{tag} damage cold byte drift: {cold_len} != {expected_bytes}"
        ));
    }
    let cold_full_ms = t.elapsed().as_secs_f64() * 1000.0;
    let mut samples: Vec<f64> = Vec::with_capacity(DAMAGE_FRAMES);
    for i in 0..DAMAGE_FRAMES {
        let t = Instant::now();
        flip(host);
        host.run_until_idle();
        let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
        cpu.paint(csurf, &plan)
            .map_err(|e| format!("{tag} damage frame {i} paint: {e:?}"))?;
        let len = readback_len(cpu, csurf)?;
        if len != expected_bytes {
            return Err(format!(
                "{tag} damage frame {i} byte drift: {len} != {expected_bytes}"
            ));
        }
        samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let stats = damage_stats(&samples).ok_or_else(|| format!("{tag} damage loop: no samples"))?;
    let record = format_damage_record(tag, cold_full_ms, &stats);
    std::fs::write(dir.join("damage.txt"), &record)
        .map_err(|e| format!("write damage.txt: {e}"))?;
    Ok(record)
}

/// Readback byte length off the CPU pixmap (the byte-drift guard —
/// shared by the cold baseline and every damage frame).
fn readback_len(cpu: &CpuBackend, csurf: oppa::SurfaceId) -> Result<usize, String> {
    let px = cpu.pixmap(csurf).ok_or("damage loop: cpu pixmap missing")?;
    Ok(px.pixels().len() * 4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oppa::{Color, Ctx, Div, Props, Style, SurfaceDesc, VNode};

    #[test]
    fn damage_stats_aggregates_known_samples() {
        // 1.0..=10.0: nearest-rank p50 = 5th value, p95 = 10th.
        let samples: Vec<f64> = (1..=10).map(|v| v as f64).collect();
        let stats = damage_stats(&samples).expect("nonempty aggregates");
        assert_eq!(
            stats,
            DamageStats {
                n: 10,
                min_ms: 1.0,
                p50_ms: 5.0,
                p95_ms: 10.0,
                max_ms: 10.0,
            }
        );
        // Unsorted input still ranks (min/max are order-free).
        let shuffled = vec![10.0, 1.0, 7.0, 3.0, 5.0, 9.0, 2.0, 8.0, 4.0, 6.0];
        assert_eq!(damage_stats(&shuffled), Some(stats));
    }

    #[test]
    fn damage_stats_empty_is_none() {
        assert_eq!(damage_stats(&[]), None, "no samples is loud, never zeros");
    }

    #[test]
    fn damage_record_carries_cold_and_steady_keys() {
        let stats = DamageStats {
            n: 60,
            min_ms: 1.1,
            p50_ms: 2.2,
            p95_ms: 5.5,
            max_ms: 9.9,
        };
        let line = format_damage_record("cpu", 86.4, &stats);
        for key in [
            "damage_cpu_cold_full_ms=86.4",
            "damage_cpu_n=60",
            "damage_cpu_min_ms=1.1",
            "damage_cpu_p50_ms=2.2",
            "damage_cpu_p95_ms=5.5",
            "damage_cpu_max_ms=9.9",
        ] {
            assert!(line.contains(key), "record carries {key}: {line}");
        }
    }

    #[derive(Clone)]
    struct FlipProps {
        on: oppa::Signal<bool>,
    }
    impl Props for FlipProps {}

    /// One control whose box width flips (10 vs 30): every toggle
    /// re-renders exactly one component and damages the plan, so
    /// the loop measures real incremental frames, not no-ops.
    fn flip_app(_ctx: &Ctx, props: &FlipProps) -> VNode {
        let w = if props.on.get() { 30.0 } else { 10.0 };
        Div("root").child(
            Div("flip")
                .style(Style::new().size(w, 20.0).bg(Color(0x22_66_CC)))
                .build(),
        )
    }

    /// The full damage pipeline headlessly: mount, commit, then 60
    /// signal flips through settle/build/paint/readback. The record
    /// parses back to n=60 with ordered steady stats.
    #[test]
    fn sustained_loop_times_sixty_real_flips() {
        let mut host = ComponentHost::new();
        host.set_viewport(200.0, 150.0);
        let on = host.runtime().signal(false);
        let _mount = host.mount("Flip", FlipProps { on: on.clone() }, flip_app);
        host.run_until_idle();
        let mut builder = FramePlanBuilder::new(1.0);
        let mut cpu = CpuBackend::new();
        let csurf = cpu
            .create_surface(SurfaceDesc {
                width_px: 200,
                height_px: 150,
                background: Color(0xFF_FF_FF),
            })
            .expect("cpu surface builds");
        for d in host.diffs_from(0) {
            cpu.commit(&d).expect("cpu commit replays");
        }
        let dir = std::env::temp_dir();
        let mut flips = 0u32;
        let record = run_sustained_damage_loop(
            &mut host,
            &mut builder,
            &mut cpu,
            csurf,
            200 * 150 * 4,
            &dir,
            "test",
            |_| {
                flips += 1;
                on.set(!on.get());
            },
        )
        .expect("damage loop runs");
        assert_eq!(flips, DAMAGE_FRAMES as u32, "every frame flips once");
        assert!(
            record.contains("damage_test_n=60"),
            "record counts: {record}"
        );
        assert!(
            record.contains("damage_test_cold_full_ms="),
            "cold rides along: {record}"
        );
        assert!(
            record.contains("damage_test_p50_ms=") && record.contains("damage_test_p95_ms="),
            "steady stats ride along: {record}"
        );
    }
}
