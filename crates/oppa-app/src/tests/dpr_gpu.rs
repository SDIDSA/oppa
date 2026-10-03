use super::*;

/// Round 2.4 (decision 259): DPR re-basing keeps CSS stable,
/// doubles device geometry, and re-shapes text.
#[test]
fn set_device_pixel_ratio_rescales_device_keep_css() {
    let (mut loop_, _) = field_harness();
    // field_harness mounts 200x150 at dpr 1: field-box 200x32.
    let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
        .into_iter()
        .next()
        .expect("field box");
    let before = loop_.host().committed_box(field).expect("field box");
    assert_eq!((before.w, before.h), (200.0, 32.0));
    assert_eq!(loop_.dpr(), 1.0);
    loop_.set_device_pixel_ratio(2.0).expect("re-bases");
    assert_eq!(loop_.dpr(), 2.0);
    // CSS stable (200x150), device doubled: viewport() reads
    // device px, so it doubles too.
    assert_eq!(loop_.viewport(), (400, 300));
    let after = loop_.host().committed_box(field).expect("field box");
    assert_eq!((after.w, after.h), (400.0, 64.0), "device geometry doubles");
    assert_eq!(
        loop_.rgba8().expect("pixels").len(),
        400 * 300 * 4,
        "surface refits to device px"
    );
    // Text re-shaped at the new scale (measure keys carry the
    // DPR — a re-shape ran, not a stale reuse).
    assert!(
        loop_.host().layout_stats().nodes_shaped > 0,
        "text re-shapes at the new DPR"
    );
    // Same-value re-set is a no-op.
    loop_.set_device_pixel_ratio(2.0).expect("no-op");
    // Back down restores exactly.
    loop_.set_device_pixel_ratio(1.0).expect("re-bases");
    let back = loop_.host().committed_box(field).expect("field box");
    assert_eq!((back.w, back.h), (200.0, 32.0));
    assert_eq!(loop_.rgba8().expect("pixels").len(), 200 * 150 * 4);
}

#[test]
fn resize_at_non_unity_dpr_divides_to_css() {
    let (mut loop_, _) = field_harness();
    loop_.set_device_pixel_ratio(2.0).expect("re-bases");
    loop_.resize(400, 300).expect("resize works");
    // 400x300 device px at dpr 2 = 200x150 CSS: the field keeps
    // its full-width row (block child of the root).
    let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
        .into_iter()
        .next()
        .expect("field box");
    let box_ = loop_.host().committed_box(field).expect("field box");
    assert_eq!((box_.w, box_.h), (400.0, 64.0));
    assert_eq!(loop_.viewport(), (400, 300));
}

#[test]
#[should_panic(expected = "refused")]
fn zero_dpr_refuses_loudly() {
    let (mut loop_, _) = field_harness();
    let _ = loop_.set_device_pixel_ratio(0.0);
}

#[test]
#[should_panic(expected = "refused")]
fn nan_dpr_refuses_loudly() {
    let (mut loop_, _) = field_harness();
    let _ = loop_.set_device_pixel_ratio(f32::NAN);
}

/// Decision 389: `OPPA_RENDERER` parsing is pure and
/// case-insensitive; anything unrecognized (including the retired
/// `vello*` and `custom*` spellings) resolves to auto (GPU-first
/// with loud CPU fallback — the wrapper logs). Explicit backend
/// pins (`gpu-vulkan` / `gpu-dx12` / `gpu-gl`, hyphen or
/// underscore) parse to their variants; bare `vulkan` stays auto
/// (never an implicit pin).
#[test]
fn renderer_override_parses_cpu_gpu_auto() {
    assert_eq!(parse_renderer_override(None), None);
    assert_eq!(
        parse_renderer_override(Some("cpu")),
        Some(RendererKind::Cpu)
    );
    assert_eq!(
        parse_renderer_override(Some("CPU")),
        Some(RendererKind::Cpu)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu")),
        Some(RendererKind::Gpu)
    );
    assert_eq!(
        parse_renderer_override(Some("  Gpu  ")),
        Some(RendererKind::Gpu)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu-auto")),
        Some(RendererKind::GpuAuto)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu_auto")),
        Some(RendererKind::GpuAuto)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu-vulkan")),
        Some(RendererKind::GpuVulkan)
    );
    assert_eq!(
        parse_renderer_override(Some("GPU-VULKAN")),
        Some(RendererKind::GpuVulkan)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu-dx12")),
        Some(RendererKind::GpuDx12)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu_dx12")),
        Some(RendererKind::GpuDx12)
    );
    assert_eq!(
        parse_renderer_override(Some("  Gpu-GL  ")),
        Some(RendererKind::GpuGl)
    );
    assert_eq!(parse_renderer_override(Some("")), None);
    assert_eq!(parse_renderer_override(Some("vulkan")), None);
    // Retired spellings never parse (decisions 388–389 — no silent
    // substitution).
    assert_eq!(parse_renderer_override(Some("vello")), None);
    assert_eq!(parse_renderer_override(Some("vello-vulkan")), None);
    assert_eq!(parse_renderer_override(Some("vello-dx12")), None);
    assert_eq!(parse_renderer_override(Some("custom_gpu")), None);
    assert_eq!(parse_renderer_override(Some("custom-auto")), None);
    assert_eq!(parse_renderer_override(Some("custom-dx12")), None);
}

#[test]
fn presenter_badge_name_follows_the_active_renderer() {
    // The overlay badge must name the pixels. The name derives from
    // the active renderer instead of a hardcoded per-outcome string.
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    assert_eq!(loop_.renderer_kind(), RendererKind::Cpu);
    assert_eq!(loop_.presenter_badge_name(), "CPU");
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    assert_eq!(loop_.renderer_kind(), RendererKind::GpuAuto);
    assert_eq!(loop_.presenter_badge_name(), "Gpu");
    loop_.disable_gpu();
    assert_eq!(loop_.presenter_badge_name(), "CPU");
    // The derived name is what the badge renders.
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    loop_.host().set_debug_overlay(DebugOverlayConfig::all());
    loop_
        .host()
        .set_presenter_name(loop_.presenter_badge_name());
    let badge = loop_
        .host()
        .debug_overlay_badge()
        .expect("badge renders when enabled");
    assert!(
        badge.contains("[Gpu]"),
        "badge names the active renderer, got {badge:?}"
    );
}

#[test]
fn gpu_backend_pins_share_the_gpu_twin_and_badge() {
    // Decision 389: every GPU variant paints the same twin with
    // the bare `Gpu` badge until bring-up records a winner.
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    for kind in [
        RendererKind::Gpu,
        RendererKind::GpuAuto,
        RendererKind::GpuVulkan,
        RendererKind::GpuDx12,
        RendererKind::GpuGl,
    ] {
        loop_.scene_mut().rgb.active = kind;
        assert!(loop_.is_gpu(), "is_gpu for {kind:?}");
        assert_eq!(loop_.presenter_badge_name(), "Gpu");
    }
    loop_.scene_mut().rgb.active = RendererKind::GpuDx12;
    assert_eq!(loop_.renderer_name(), "gpu-dx12");
    loop_.scene_mut().rgb.active = RendererKind::GpuVulkan;
    assert_eq!(loop_.renderer_name(), "gpu-vulkan");
    loop_.scene_mut().rgb.active = RendererKind::GpuGl;
    assert_eq!(loop_.renderer_name(), "gpu-gl");
    loop_.scene_mut().rgb.active = RendererKind::Cpu;
    assert!(!loop_.is_gpu());
}

#[test]
fn gpu_badge_names_the_recorded_wgpu_backend() {
    // Decision 389: the badge names the actual API
    // (`[Gpu/DX12]`), never a bare `Gpu` once bring-up recorded
    // a winner; headless loops with no recording stay `Gpu`.
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    assert_eq!(loop_.gpu_api(), None);
    loop_.scene_mut().rgb.active = RendererKind::GpuAuto;
    assert_eq!(loop_.presenter_badge_name(), "Gpu");
    loop_.set_gpu_api(GpuApi::Dx12);
    assert_eq!(loop_.gpu_api(), Some(GpuApi::Dx12));
    assert_eq!(loop_.presenter_badge_name(), "Gpu/DX12");
    loop_.set_gpu_api(GpuApi::Vulkan);
    assert_eq!(loop_.presenter_badge_name(), "Gpu/Vulkan");
    loop_.set_gpu_api(GpuApi::Gl);
    assert_eq!(loop_.presenter_badge_name(), "Gpu/GL");
    assert_eq!(GpuApi::from_row_name("dx12"), Some(GpuApi::Dx12));
    assert_eq!(GpuApi::from_row_name("vulkan"), Some(GpuApi::Vulkan));
    assert_eq!(GpuApi::from_row_name("gl"), Some(GpuApi::Gl));
    assert_eq!(GpuApi::from_row_name("metal"), None);
    loop_.disable_gpu();
    assert_eq!(loop_.gpu_api(), None);
    assert_eq!(loop_.presenter_badge_name(), "CPU");
}

#[test]
fn gpu_family_parses_pins_and_badges_the_recorded_api() {
    // Decision 389: `gpu` / `gpu-auto` request the automatic
    // rasterizer; the backend pins select one wgpu row; every
    // variant paints the GPU twin and badges the recorded API.
    assert_eq!(
        parse_renderer_override(Some("gpu")),
        Some(RendererKind::Gpu)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu-auto")),
        Some(RendererKind::GpuAuto)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu_vulkan")),
        Some(RendererKind::GpuVulkan)
    );
    assert_eq!(
        parse_renderer_override(Some("GPU-DX12")),
        Some(RendererKind::GpuDx12)
    );
    assert_eq!(
        parse_renderer_override(Some("  gpu-gl  ")),
        Some(RendererKind::GpuGl)
    );
    for kind in [
        RendererKind::Gpu,
        RendererKind::GpuAuto,
        RendererKind::GpuVulkan,
        RendererKind::GpuDx12,
        RendererKind::GpuGl,
    ] {
        assert!(kind.is_gpu(), "is_gpu for {kind:?}");
    }
    assert!(!RendererKind::Cpu.is_gpu());
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.scene_mut().rgb.active = RendererKind::GpuDx12;
    assert_eq!(loop_.renderer_name(), "gpu-dx12");
    assert!(loop_.is_gpu());
    assert_eq!(loop_.presenter_badge_name(), "Gpu");
    loop_.set_gpu_api(GpuApi::Dx12);
    assert_eq!(loop_.presenter_badge_name(), "Gpu/DX12");
    loop_.set_gpu_api(GpuApi::Gl);
    assert_eq!(loop_.presenter_badge_name(), "Gpu/GL");
}
