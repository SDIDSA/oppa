// ---------------------------------------------------------------------------
// Ordered presenter ladder (decision 432): one implementation of the
// "first suitable presenter wins" rule for every runner.
// ---------------------------------------------------------------------------
//
// Every platform walks the same ladder: ordered GPU rows (name +
// `wgpu` backends) with a terminal CPU row, first success takes the
// frame, every refusal logs loudly. Platforms supply only three
// things — their default GPU order, a [`SurfaceFactory`] (the ~10
// lines of OS window-handle code that cannot be shared), and the
// row tail (what a won row does: desktop picks the vsync mode,
// configures, enables and records the badge row; Android configures
// its pinned mode and records nothing — zygote env must not decide
// the arm). The walk, the pin handling, the loud refusal lines, and
// the CPU-terminal shape live here once.
//
// The CPU presenter is a row like any other ([`PresenterRow::Cpu`]):
// it needs no bring-up (the CPU pixmap paints every frame anyway),
// so it always resolves — runners that own a software blit (GDI on
// Windows, softbuffer on Linux) register it as the terminal row and
// fall to their blit leg; runners without one (Android, until its
// software present lands) register GPU rows only and fail loudly
// when the ladder exhausts (never a silent black frame).

use super::*;

/// One ordered GPU presenter candidate: the log/badge name plus the
/// `wgpu` backends the row probes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuRow {
    pub name: &'static str,
    pub backends: wgpu::Backends,
}

/// Closed row vocabulary (decision 387 — [`GpuApi::from_row_name`]
/// refuses unknown names loudly instead of guessing).
pub const GPU_VULKAN_ROW: GpuRow = GpuRow {
    name: "vulkan",
    backends: wgpu::Backends::VULKAN,
};
/// Closed row vocabulary (see [`GPU_VULKAN_ROW`]).
pub const GPU_DX12_ROW: GpuRow = GpuRow {
    name: "dx12",
    backends: wgpu::Backends::DX12,
};
/// Closed row vocabulary (see [`GPU_VULKAN_ROW`]).
pub const GPU_GL_ROW: GpuRow = GpuRow {
    name: "gl",
    backends: wgpu::Backends::GL,
};

/// Windows default order: Vulkan first, then DX12, then GL
/// (decision 390 — a DX12-dead but GL-alive box, e.g. pre-Win10,
/// crippled VMs, corrupt D3D12 stacks, still gets hardware; healthy
/// machines never reach past Vulkan).
pub const WINDOWS_GPU_ORDER: [GpuRow; 3] = [GPU_VULKAN_ROW, GPU_DX12_ROW, GPU_GL_ROW];
/// Linux default order: Vulkan first, then GL (decision 391 — same
/// argument as Windows decision 390, and stronger here: GL is native
/// Mesa, with llvmpipe as the software backstop; pre-Vulkan hardware
/// and bare VMs still get acceleration).
pub const LINUX_GPU_ORDER: [GpuRow; 2] = [GPU_VULKAN_ROW, GPU_GL_ROW];
/// Android default order: GL first, then Vulkan (the installed base
/// is GLES-first; see the Vulkan-first twin below).
pub const ANDROID_GPU_ORDER_GL_FIRST: [GpuRow; 2] = [GPU_GL_ROW, GPU_VULKAN_ROW];
/// Android order once Vulkan has won (winner-stickiness: a picker
/// trip re-runs GL, not Vulkan-after-GL — the recorded winner
/// re-probes first after a heal).
pub const ANDROID_GPU_ORDER_VULKAN_FIRST: [GpuRow; 2] = [GPU_VULKAN_ROW, GPU_GL_ROW];

/// One ladder row: a GPU probe, or the terminal CPU presenter (no
/// bring-up — always resolves where registered).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresenterRow {
    Gpu(GpuRow),
    Cpu,
}

/// Ordered presenter rows honoring the `OPPA_RENDERER` pins (pure —
/// headless-testable): an explicit `gpu-*` pin attempts exactly one
/// GPU row (never a silent substitution when the user named one —
/// including the meaningless-on-Linux `gpu-dx12`, which fails loudly
/// rather than substituting); everything else walks the platform
/// default. Runners with a software blit pass `with_cpu_terminal`
/// (the CPU row always closes the ladder); runners without one pass
/// `false` and fail loudly on exhaustion.
pub fn presenter_rows_for(
    default_gpu_order: &[GpuRow],
    with_cpu_terminal: bool,
    kind: Option<RendererKind>,
) -> Vec<PresenterRow> {
    let gpu: Vec<GpuRow> = match kind {
        Some(RendererKind::GpuDx12) => vec![GPU_DX12_ROW],
        Some(RendererKind::GpuVulkan) => vec![GPU_VULKAN_ROW],
        Some(RendererKind::GpuGl) => vec![GPU_GL_ROW],
        // Forced CPU probes no GPU rows (runners skip bring-up
        // before this runs; the shape stays total anyway).
        Some(RendererKind::Cpu) => vec![],
        // Auto/unset → platform default.
        _ => default_gpu_order.to_vec(),
    };
    let mut rows: Vec<PresenterRow> = gpu.into_iter().map(PresenterRow::Gpu).collect();
    if with_cpu_terminal {
        rows.push(PresenterRow::Cpu);
    }
    rows
}

/// [`presenter_rows_for`] through the live `OPPA_RENDERER` override.
pub fn presenter_rows(default_gpu_order: &[GpuRow], with_cpu_terminal: bool) -> Vec<PresenterRow> {
    presenter_rows_for(default_gpu_order, with_cpu_terminal, renderer_override())
}

/// A platform window ready for one probe: the `wgpu` instance the
/// row needs plus the surface built from it. Factories own whatever
/// the OS call needs (leaked instances on desktop, the reused
/// instance table on Android) — the ladder only borrows the pair
/// for the attempt.
pub struct BuiltSurface<'a> {
    pub instance: &'a wgpu::Instance,
    pub surface: wgpu::Surface<'a>,
}

/// Builds one probe surface per GPU row (platform-owned — the one
/// piece of bring-up that cannot be shared). `Err` is the raw
/// refusal reason; the ladder formats it into the single refusal
/// vocabulary (`{name} surface failed: …`, logged + recorded).
pub trait SurfaceFactory<'a> {
    fn make_surface(&mut self, backends: wgpu::Backends) -> Result<BuiltSurface<'a>, String>;
}

/// What a won GPU row applied through the shared engine (the row
/// tail is caller-owned — see the module docs — but its product has
/// one shape): adapter record string, negotiated mode, configured
/// format. The surface itself travels back inside
/// [`LadderOutcome::Gpu`], never here.
pub struct GpuApplied {
    pub adapter: String,
    pub mode: wgpu::PresentMode,
    pub format: wgpu::TextureFormat,
}

/// One GPU row tail (caller-owned — see the module docs): probes
/// the row through the shared engine and applies the win. A named
/// type (the closure shape repeats on all four call sites).
pub type GpuRowAttempt = dyn FnMut(
    &mut SceneCore,
    GpuRow,
    &wgpu::Instance,
    &wgpu::Surface<'_>,
) -> Result<GpuApplied, RowFail>;

/// One won GPU row, with its live surface.
pub struct GpuUp<'a> {
    pub name: &'static str,
    pub backends: wgpu::Backends,
    pub adapter: String,
    pub mode: wgpu::PresentMode,
    pub format: wgpu::TextureFormat,
    pub surface: wgpu::Surface<'a>,
    /// Refusal notes from the rows probed before the winner (walk
    /// order) — runners record them in the run summary; a first-try
    /// win carries none.
    pub attempts: Vec<String>,
}

/// A resolved ladder: a live GPU presenter, or the terminal CPU row
/// (the runner falls to its software blit and logs the refusal line
/// itself — it owns the CPU wording).
pub enum LadderOutcome<'a> {
    Gpu(GpuUp<'a>),
    Cpu,
}

/// A refused row tail: `Next` tries the following row (the note
/// joins the loud refusal vocabulary), `Abort` fails the whole
/// bring-up with the reason (desktop configure failures abort —
/// Round 19.8; Android configure failures continue to the next row).
pub enum RowFail {
    Next(String),
    Abort(String),
}

/// An exhausted ladder: the terminal reason plus every refusal note
/// in walk order. Displays as the reason alone, so desktop runners
/// keep their exact `{reason}; CPU fallback` line while Android
/// records the full attempt list (`no surface-compatible GPU: …`).
pub struct LadderFailure {
    pub reason: String,
    pub attempts: Vec<String>,
}

impl std::fmt::Display for LadderFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}

/// Walks the ladder (decision 432 — the one first-wins rule): GPU
/// rows probe through the factory + row tail, first success returns
/// with its live surface; the CPU row resolves immediately; total
/// exhaustion fails loudly with every refusal recorded. Surface
/// refusals always continue (a dead surface never aborts the rows
/// behind it); row-tail refusals continue or abort per [`RowFail`].
pub fn walk_presenter_rows<'a>(
    rows: &[PresenterRow],
    factory: &mut dyn SurfaceFactory<'a>,
    attempt_gpu: &mut GpuRowAttempt,
    scene: &mut SceneCore,
) -> Result<LadderOutcome<'a>, LadderFailure> {
    let mut attempts = Vec::new();
    for row in rows {
        let gpu = match row {
            PresenterRow::Gpu(g) => g,
            PresenterRow::Cpu => return Ok(LadderOutcome::Cpu),
        };
        let built = match factory.make_surface(gpu.backends) {
            Ok(b) => b,
            Err(e) => {
                let note = format!("{} surface failed: {e}", gpu.name);
                eprintln!("oppa-app: {note}");
                attempts.push(note);
                continue;
            }
        };
        match attempt_gpu(scene, *gpu, built.instance, &built.surface) {
            Ok(applied) => {
                return Ok(LadderOutcome::Gpu(GpuUp {
                    name: gpu.name,
                    backends: gpu.backends,
                    adapter: applied.adapter,
                    mode: applied.mode,
                    format: applied.format,
                    surface: built.surface,
                    attempts,
                }));
            }
            Err(RowFail::Next(note)) => {
                eprintln!("oppa-app: {note}");
                attempts.push(note);
            }
            Err(RowFail::Abort(reason)) => {
                return Err(LadderFailure { reason, attempts });
            }
        }
    }
    Err(LadderFailure {
        reason: "no working GPU backend".to_string(),
        attempts,
    })
}
