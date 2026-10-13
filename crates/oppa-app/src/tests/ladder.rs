//! Presenter-ladder proofs (decision 432): the row tables keep
//! every platform's probe order (pins select exactly one row, the
//! CPU row closes desktop ladders), and the walk fails loudly in
//! walk order when every surface refuses. Win paths need a live
//! window + GPU (device-covered); the `Abort` tail (desktop
//! configure failure) is preserved verbatim beside them and
//! unreachable headlessly for the same reason.

use super::*;
use std::cell::RefCell;

fn row_names(rows: &[crate::PresenterRow]) -> Vec<&'static str> {
    rows.iter()
        .map(|row| match row {
            crate::PresenterRow::Gpu(g) => g.name,
            crate::PresenterRow::Cpu => "cpu",
        })
        .collect()
}

/// Refusing factory (records probed backends in walk order — the
/// headless stand-in for a windowless box).
struct RefusingFactory {
    calls: RefCell<Vec<wgpu::Backends>>,
}

impl<'a> crate::SurfaceFactory<'a> for RefusingFactory {
    fn make_surface(
        &mut self,
        backends: wgpu::Backends,
    ) -> Result<crate::BuiltSurface<'a>, String> {
        self.calls.borrow_mut().push(backends);
        Err("no window headless".to_string())
    }
}

#[test]
fn ladder_row_tables_keep_platform_orders() {
    // One fact per platform, in probe order (decisions 390/391 +
    // the Android GLES-first rule) — the walk below trusts these.
    assert_eq!(
        row_names(&crate::presenter_rows_for(
            &crate::WINDOWS_GPU_ORDER,
            false,
            None
        )),
        vec!["vulkan", "dx12", "gl"],
        "windows probes vulkan, dx12, gl"
    );
    assert_eq!(
        row_names(&crate::presenter_rows_for(
            &crate::LINUX_GPU_ORDER,
            false,
            None
        )),
        vec!["vulkan", "gl"],
        "linux probes vulkan, gl"
    );
    assert_eq!(
        row_names(&crate::presenter_rows_for(
            &crate::ANDROID_GPU_ORDER_GL_FIRST,
            false,
            None
        )),
        vec!["gl", "vulkan"],
        "android probes gl first"
    );
    assert_eq!(
        row_names(&crate::presenter_rows_for(
            &crate::ANDROID_GPU_ORDER_VULKAN_FIRST,
            false,
            None
        )),
        vec!["vulkan", "gl"],
        "android re-probes a vulkan winner first"
    );
}

#[test]
fn ladder_pins_select_single_rows_and_cpu_closes() {
    // Explicit pins attempt exactly one GPU row (never a silent
    // substitution); the CPU row still closes desktop ladders so
    // pins fall back loudly to the blit leg.
    for (kind, name) in [
        (RendererKind::GpuDx12, "dx12"),
        (RendererKind::GpuVulkan, "vulkan"),
        (RendererKind::GpuGl, "gl"),
    ] {
        assert_eq!(
            row_names(&crate::presenter_rows_for(
                &crate::LINUX_GPU_ORDER,
                true,
                Some(kind)
            )),
            vec![name, "cpu"],
            "{name} pin probes once, then cpu"
        );
        assert_eq!(
            row_names(&crate::presenter_rows_for(
                &crate::LINUX_GPU_ORDER,
                false,
                Some(kind)
            )),
            vec![name],
            "{name} pin without a cpu leg probes once"
        );
    }
    // Auto walks the platform default; the CPU terminal is the only
    // difference between desktop and Android ladders.
    assert_eq!(
        row_names(&crate::presenter_rows_for(
            &crate::WINDOWS_GPU_ORDER,
            true,
            None
        )),
        vec!["vulkan", "dx12", "gl", "cpu"],
        "desktop ladders close with cpu"
    );
    assert_eq!(
        row_names(&crate::presenter_rows_for(
            &crate::WINDOWS_GPU_ORDER,
            false,
            None
        )),
        vec!["vulkan", "dx12", "gl"],
        "android ladders end at the last gpu row"
    );
}

#[test]
fn ladder_total_refusal_fails_loudly_in_order() {
    // Every row probed in order, every refusal recorded in order,
    // one terminal reason — the Android record shape rides this (no
    // CPU terminal there; the desktop Cpu close is proven below).
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let rows = crate::presenter_rows(&crate::WINDOWS_GPU_ORDER, false);
    let mut factory = RefusingFactory {
        calls: RefCell::new(Vec::new()),
    };
    let mut attempt = |_: &mut crate::SceneCore,
                       _: crate::GpuRow,
                       _: &wgpu::Instance,
                       _: &wgpu::Surface<'_>|
     -> Result<crate::GpuApplied, crate::RowFail> {
        panic!("no surface built, no row tail runs")
    };
    match crate::walk_presenter_rows(&rows, &mut factory, &mut attempt, loop_.scene_mut()) {
        Err(e) => {
            assert_eq!(e.reason, "no working GPU backend", "terminal reason");
            assert_eq!(
                e.attempts,
                vec![
                    "vulkan surface failed: no window headless".to_string(),
                    "dx12 surface failed: no window headless".to_string(),
                    "gl surface failed: no window headless".to_string(),
                ],
                "refusals recorded in walk order"
            );
        }
        Ok(_) => panic!("all rows refused — the ladder must fail"),
    }
    assert_eq!(
        *factory.calls.borrow(),
        vec![
            wgpu::Backends::VULKAN,
            wgpu::Backends::DX12,
            wgpu::Backends::GL
        ],
        "factory probed in walk order"
    );
}

#[test]
fn ladder_cpu_terminal_resolves_without_probing() {
    // The terminal CPU row needs no bring-up: neither the factory
    // nor the row tail runs (a forced-CPU desktop never probes).
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let rows = crate::presenter_rows_for(&crate::LINUX_GPU_ORDER, true, Some(RendererKind::Cpu));
    assert_eq!(row_names(&rows), vec!["cpu"], "cpu override skips gpu rows");
    let mut factory = RefusingFactory {
        calls: RefCell::new(Vec::new()),
    };
    let mut attempt = |_: &mut crate::SceneCore,
                       _: crate::GpuRow,
                       _: &wgpu::Instance,
                       _: &wgpu::Surface<'_>|
     -> Result<crate::GpuApplied, crate::RowFail> {
        panic!("cpu terminal runs no row tail")
    };
    match crate::walk_presenter_rows(&rows, &mut factory, &mut attempt, loop_.scene_mut()) {
        Ok(crate::LadderOutcome::Cpu) => {}
        _ => panic!("cpu terminal must resolve"),
    }
    assert!(
        factory.calls.borrow().is_empty(),
        "no surface probed for the cpu row"
    );
}
