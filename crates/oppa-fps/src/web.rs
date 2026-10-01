//! Web driver: hand-rolled `requestAnimationFrame` loop (no
//! winit on wasm — winit 0.30.13's web backend panics at window
//! creation here, `RefCell already borrowed` from its
//! intersection-observer setup). The scene, text, atlas, and
//! present helpers are the shared ones in `app.rs`; only the
//! event loop and the canvas handles are Web-specific.
//!
//! Render path: detect-first, never poisoned. A canvas gets ONE
//! context type for its lifetime, so CPU-first-then-upgrade is
//! broken by construction (softbuffer's `getContext("2d")` would
//! make WebGPU's `getContext("webgpu")` return null). Instead:
//! a sync `navigator.gpu` check picks the lane — absent means
//! CPU immediately (no doomed probe), present means async GPU
//! bring-up first with loud CPU fallback. Adapter/device
//! requests are JS promises, so bring-up runs in a
//! `spawn_local` task around
//! [`ensure_gpu_for_surface_async`](oppa_vello::VelloBackend::ensure_gpu_for_surface_async)
//! (never `block_on` — that hangs the tab).
//!
//! Bounds (v1): the canvas fills the page and a ResizeObserver
//! refits backing store + swapchain + scene to the live CSS box
//! (the decision-202 pattern); dpr stays 1.0. rAF paces the loop
//! (uncapped on native, browser-paced here).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WebCanvasWindowHandle,
    WebDisplayHandle, WindowHandle,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};

use oppa::{Color, RendererBackend, SurfaceDesc, SurfaceId};
use oppa_vello::{install_vello_paint_hook_shared, VelloBackend};

use crate::app::{
    atlas_pair, build_cpu_scene, layout_service, pack_rgba_to_xrgb, present_cpu_frame, CpuScene,
    FpsCore, DEFAULT_H, DEFAULT_W,
};
use crate::clock::now_secs;

#[wasm_bindgen]
extern "C" {
    pub type HtmlElement;
    pub type HtmlCanvasElement;

    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn console_log_impl(s: &str);

    #[wasm_bindgen(js_namespace = document, js_name = getElementById)]
    fn get_element_by_id(id: &str) -> Option<HtmlElement>;

    #[wasm_bindgen(method, setter, js_name = textContent)]
    fn set_text(this: &HtmlElement, s: &str);

    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now_impl() -> f64;

    #[wasm_bindgen(js_namespace = document, js_name = createElement)]
    fn create_element(tag: &str) -> JsValue;

    #[wasm_bindgen(method, js_name = appendChild)]
    fn append_child(this: &HtmlElement, child: &HtmlCanvasElement);

    #[wasm_bindgen(method, setter, js_name = width)]
    fn set_width(this: &HtmlCanvasElement, w: u32);

    #[wasm_bindgen(method, setter, js_name = height)]
    fn set_height(this: &HtmlCanvasElement, h: u32);

    #[wasm_bindgen(method, getter, js_name = clientWidth)]
    fn client_width(this: &HtmlCanvasElement) -> i32;

    #[wasm_bindgen(method, getter, js_name = clientHeight)]
    fn client_height(this: &HtmlCanvasElement) -> i32;

    pub type ResizeObserver;

    #[wasm_bindgen(constructor, js_name = ResizeObserver)]
    fn new(callback: &js_sys::Function) -> ResizeObserver;

    #[wasm_bindgen(method, js_name = observe)]
    fn observe(this: &ResizeObserver, target: &HtmlCanvasElement);

    #[wasm_bindgen(js_name = requestAnimationFrame)]
    fn request_animation_frame(cb: &js_sys::Function) -> u32;
}

/// Crate-visible sink for logs: devtools console plus the
/// `#oppa-status` div (Web has no stderr; the div makes the last
/// status visible in screenshots too).
pub fn console_log(s: &str) {
    console_log_impl(s);
    if let Some(el) = get_element_by_id("oppa-status") {
        el.set_text(s);
    }
}

/// `performance.now()` milliseconds (the Web clock source —
/// `std::time::Instant` panics on wasm).
pub fn performance_now() -> f64 {
    performance_now_impl()
}

/// Raw-window-handle shim over our own canvas: softbuffer's web
/// backend reads the canvas element out of the window handle.
#[derive(Clone)]
struct CanvasHandles {
    canvas: JsValue,
}

impl HasWindowHandle for CanvasHandles {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let ptr = std::ptr::NonNull::new(&self.canvas as *const JsValue as *mut core::ffi::c_void)
            .ok_or(HandleError::Unavailable)?;
        Ok(unsafe { WindowHandle::borrow_raw(WebCanvasWindowHandle::new(ptr).into()) })
    }
}

impl HasDisplayHandle for CanvasHandles {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(WebDisplayHandle::new().into()) })
    }
}

struct CpuPart {
    scene: CpuScene,
    _ctx: softbuffer::Context<CanvasHandles>,
    sb: softbuffer::Surface<CanvasHandles, CanvasHandles>,
}

struct GpuPart {
    backend: Rc<RefCell<VelloBackend>>,
    surface: wgpu::Surface<'static>,
    surface_id: Rc<Cell<SurfaceId>>,
    last_stage_log: f64,
    /// Stage accumulator: `performance.now()` ticks at 1ms, so a
    /// single frame quantizes to 0.00 — means over the ~5s window
    /// (≈800 frames at 165Hz) recover two decimals.
    /// Order: acquire, target, render, blit_setup, blit_submit,
    /// present, cpu.
    acc: [f64; 7],
    count: u32,
}

struct WebState {
    core: FpsCore,
    /// `None` on the GPU-first path (the 2D context is never
    /// created there — see the module docs).
    cpu: Option<CpuPart>,
    gpu: Option<GpuPart>,
    size: (u32, u32),
    last: f64,
    last_stage_log: f64,
    cpu_acc: f64,
    cpu_count: u32,
    /// Creation-time canvas handle (resize reads the CSS box and
    /// sizes the backing store through it).
    canvas: JsValue,
    /// ResizeObserver + callback, kept alive for the page (dropping
    /// either stops the observations or traps on fire).
    _ro: Option<ResizeObserver>,
    _ro_cb: Option<RoClosure>,
}

/// Page-lifetime resize-callback slot.
type RoClosure = Closure<dyn FnMut()>;

/// The re-armed rAF closure slot (page-lifetime loop).
type RafSlot = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

fn tick(state: &Rc<RefCell<WebState>>, pending: &Rc<Cell<bool>>, raf: &RafSlot, t_ms: f64) {
    let t = t_ms / 1000.0;
    // `try_borrow_mut`: the observer can fire reentrantly out of a
    // wgpu call's microtask pump while a previous borrow is live
    // (proven by the user's console). A skipped frame at 165Hz is
    // invisible; a trapped page is not. Contention also sets the
    // pending bit, so the resize still applies below.
    match state.try_borrow_mut() {
        Ok(mut s) => {
            let dt = t - s.last;
            s.last = t;
            s.core.step(dt, t);
            s.core.frame();
            present_once(&mut s, t);
        }
        Err(_) => {
            console_log("oppa-fps: tick skipped (state busy, resize pending)");
            pending.set(true);
        }
    }
    // A resize deferred mid-frame (borrow was held across a wgpu
    // call that pumped the observer reentrantly) applies now that
    // the cell is free — same frame, never lost.
    if pending.take() {
        on_resize(state, pending);
    }
    let cb = raf.borrow();
    let cb = cb.as_ref().expect("rAF closure").as_ref().unchecked_ref();
    request_animation_frame(cb);
}

/// Presents one stepped frame through whichever lane is live
/// (shared by the rAF tick and the resize path's immediate
/// repaint).
fn present_once(s: &mut WebState, t: f64) {
    let size = s.size;
    if let Some(gpu) = s.gpu.as_mut() {
        let surface_id = gpu.surface_id.get();
        // `try_borrow_mut`: a reentrant observer can land here while
        // an outer frame still holds the backend (its wgpu call
        // pumped the microtask queue). Skip-and-log beats trap;
        // rAF re-arms regardless.
        match gpu.backend.try_borrow_mut() {
            Ok(mut backend) => match backend.present_surface(surface_id, &gpu.surface) {
                Ok(rep) => {
                    gpu.acc[0] += rep.acquire_ms;
                    gpu.acc[1] += rep.target_ms;
                    gpu.acc[2] += rep.render_ms;
                    gpu.acc[3] += rep.blit_setup_ms;
                    gpu.acc[4] += rep.blit_submit_ms;
                    gpu.acc[5] += rep.present_ms;
                    gpu.acc[6] += rep.cpu_ms;
                    gpu.count += 1;
                    if t - gpu.last_stage_log >= 5.0 {
                        gpu.last_stage_log = t;
                        let n = gpu.count.max(1) as f64;
                        console_log(&format!(
                            "oppa-fps stages ms mean over {} frames: acquire={:.2} target={:.2} render={:.2} blit_setup={:.2} blit_submit={:.2} present={:.2} cpu={:.2}",
                            gpu.count,
                            gpu.acc[0] / n,
                            gpu.acc[1] / n,
                            gpu.acc[2] / n,
                            gpu.acc[3] / n,
                            gpu.acc[4] / n,
                            gpu.acc[5] / n,
                            gpu.acc[6] / n
                        ));
                        gpu.acc = [0.0; 7];
                        gpu.count = 0;
                    }
                }
                Err(e) => {
                    console_log(&format!("oppa-fps: gpu present failed: {e}"));
                }
            },
            Err(_) => {
                console_log("oppa-fps: present skipped (backend busy)");
            }
        }
    } else if let Some(cpu) = s.cpu.as_mut() {
        let backend = cpu.scene.backend.clone();
        let surface_id = cpu.scene.surface_id;
        let t0 = now_secs();
        if let Err(e) = present_cpu_frame(&backend, surface_id, &mut cpu.sb, size) {
            console_log(&format!("oppa-fps: cpu present failed: {e}"));
            return;
        }
        s.cpu_acc += (now_secs() - t0) * 1000.0;
        s.cpu_count += 1;
        if t - s.last_stage_log >= 5.0 {
            s.last_stage_log = t;
            console_log(&format!(
                "oppa-fps stages ms mean over {} frames: cpu_present={:.2}",
                s.cpu_count,
                s.cpu_acc / s.cpu_count.max(1) as f64
            ));
            s.cpu_acc = 0.0;
            s.cpu_count = 0;
        }
    } else {
        console_log("oppa-fps: no render state (GPU upgrade lost the core)");
    }
}

/// Refits everything to the live CSS box (the decision-202
/// pattern, web edition): backing store, viewport + recenter
/// (shared `FpsCore::set_size`), swapchain reconfigure +
/// scene-surface rebuild on the GPU lane, scene rebuild +
/// softbuffer resize on the CPU lane, then an immediate
/// repaint. Same-size calls are a cheap no-op (the observer
/// fires on every layout, not just resizes).
///
/// Reentrancy: wgpu calls pump JS microtasks, so this can run
/// while the tick holds the cell. Every borrow is a
/// `try_borrow` — on contention the resize is flagged in
/// `pending` (a `Cell` outside the `RefCell`, so flagging never
/// traps) and the tick applies it the moment the cell is free.
fn on_resize(state: &Rc<RefCell<WebState>>, pending: &Rc<Cell<bool>>) {
    let (canvas_js, size_now) = match state.try_borrow() {
        Ok(s) => (s.canvas.clone(), s.size),
        Err(_) => {
            pending.set(true);
            return;
        }
    };
    let canvas_el: HtmlCanvasElement = canvas_js.unchecked_into();
    let w = canvas_el.client_width().max(1) as u32;
    let h = canvas_el.client_height().max(1) as u32;
    if (w, h) == size_now {
        return;
    }
    console_log(&format!(
        "oppa-fps: resize {}x{} → {w}x{h}",
        size_now.0, size_now.1
    ));
    // Backing store follows the CSS box (dpr 1.0 in v1 — the
    // hook, atlas, and viewport all work in CSS px).
    canvas_el.set_width(w);
    canvas_el.set_height(h);
    let t = now_secs();
    let mut s = match state.try_borrow_mut() {
        Ok(s) => s,
        Err(_) => {
            pending.set(true);
            return;
        }
    };
    s.size = (w, h);
    s.core.set_size(w, h);
    if let Some(gpu) = s.gpu.as_mut() {
        // `try_` throughout: an outer frame can hold the backend
        // across a wgpu call that pumps us reentrantly. On
        // contention abort loudly and re-flag — the tick retries
        // next frame, so the resize still lands.
        match gpu.backend.try_borrow() {
            Ok(backend) => {
                let _ = backend.configure_surface(&gpu.surface, w, h);
            }
            Err(_) => {
                console_log("oppa-fps: resize configure skipped (backend busy)");
                pending.set(true);
                return;
            }
        }
        let old = gpu.surface_id.get();
        let id = match gpu.backend.try_borrow_mut() {
            Ok(mut backend) => match backend.create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: Color(0xFF_FF_FF),
            }) {
                Ok(id) => id,
                Err(e) => {
                    console_log(&format!("oppa-fps: resize surface failed: {e:?}"));
                    pending.set(true);
                    return;
                }
            },
            Err(_) => {
                console_log("oppa-fps: resize surface skipped (backend busy)");
                pending.set(true);
                return;
            }
        };
        gpu.surface_id.set(id);
        match gpu.backend.try_borrow_mut() {
            Ok(mut backend) => {
                let _ = backend.destroy_surface(old);
            }
            Err(_) => {
                console_log("oppa-fps: resize destroy skipped (backend busy)");
                pending.set(true);
            }
        }
    } else if s.cpu.is_some() {
        // Fresh scene surface at the new size (hook reinstall
        // replaces the paint pass — same as lane setup).
        let scene = match build_cpu_scene(&s.core, (w, h)) {
            Ok(scene) => scene,
            Err(e) => {
                console_log(&format!("oppa-fps: resize scene failed: {e}"));
                return;
            }
        };
        let cpu = s.cpu.as_mut().expect("checked above");
        cpu.scene = scene;
        if let Err(e) = cpu.sb.resize(
            std::num::NonZeroU32::new(w).expect("width"),
            std::num::NonZeroU32::new(h).expect("height"),
        ) {
            console_log(&format!("oppa-fps: resize softbuffer failed: {e:?}"));
            return;
        }
    }
    s.core.frame();
    present_once(&mut s, t);
}

/// Watches the canvas box: fires on first layout (correcting
/// the startup size) and on every window/zoom/devtools change.
/// Both handles live in state for the page lifetime. Setup runs
/// outside any state borrow (the observer can fire
/// synchronously on `observe`), so this uses `try_borrow_mut`
/// like everything else on this path.
fn watch_resize(
    canvas_el: &HtmlCanvasElement,
    state: Rc<RefCell<WebState>>,
    pending: Rc<Cell<bool>>,
) {
    let cb_state = state.clone();
    let cb_pending = pending.clone();
    let cb: RoClosure = Closure::wrap(Box::new(move || {
        on_resize(&cb_state, &cb_pending);
    }) as Box<dyn FnMut()>);
    let ro = ResizeObserver::new(cb.as_ref().unchecked_ref());
    ro.observe(canvas_el);
    match state.try_borrow_mut() {
        Ok(mut s) => {
            s._ro = Some(ro);
            s._ro_cb = Some(cb);
        }
        Err(_) => {
            // Near-unreachable (setup runs borrow-free), but a
            // synchronous fire inside `observe` with a held borrow
            // must never trap: leak both as page-lifetime handles
            // (the design lifetime anyway) and the observations
            // keep flowing.
            core::mem::forget(ro);
            cb.forget();
        }
    }
}

/// Sync WebGPU feature-detect: `navigator.gpu` undefined means
/// no WebGPU (skip the probe entirely — the surface attempt
/// would fail loudly but pointlessly). A `JsValue` round-trip
/// through `Reflect` (never a bare-`fn` property binding — those
/// throw silent fatal traps, see `start`).
fn webgpu_present() -> bool {
    let nav: JsValue = match web_sys::window() {
        Some(w) => w.navigator().into(),
        None => return false,
    };
    if nav.is_null() || nav.is_undefined() {
        return false;
    }
    match js_sys::Reflect::get(&nav, &JsValue::from_str("gpu")) {
        Ok(gpu) => !gpu.is_undefined() && !gpu.is_null(),
        Err(_) => false,
    }
}

/// `?bench=N` query param: run N offscreen renders at startup
/// and report mean ms → uncapped-equivalent fps (the rAF loop
/// itself is vsync-paced, so throughput is measured here, not
/// in the label). Clamped 1..=1000 (a thousand tiny-skia frames
/// still return in ~a second); garbage logs loudly and is
/// ignored.
fn bench_param() -> Option<u32> {
    let search = web_sys::window()?.location().search().unwrap_or_default();
    for pair in search.trim_start_matches('?').split('&') {
        let mut it = pair.splitn(2, '=');
        if it.next() == Some("bench") {
            match it.next().and_then(|s| s.parse::<u32>().ok()) {
                Some(n) => return Some(n.clamp(1, 1000)),
                None => {
                    console_log(&format!(
                        "oppa-fps: ignoring bad ?bench={pair} (want 1..=1000)"
                    ));
                    return None;
                }
            }
        }
    }
    None
}
/// Async WebGPU bring-up: instance → canvas surface → adapter +
/// device (awaited, never blocked) → configure → scene surface +
/// paint hook → warmup present. Returns the running GPU state;
/// any failure returns the core for the CPU fallback (loud at
/// the call site, never a silent blank page).
async fn try_gpu_state(
    core: FpsCore,
    canvas_js: JsValue,
    size: (u32, u32),
    pending: Rc<Cell<bool>>,
) -> Result<(Rc<RefCell<WebState>>, String), (FpsCore, String)> {
    match try_gpu_inner(&core, canvas_js.clone(), size).await {
        Ok((gpu, adapter_info)) => {
            let state = Rc::new(RefCell::new(WebState {
                core,
                cpu: None,
                gpu: Some(gpu),
                size,
                last: now_secs(),
                last_stage_log: now_secs(),
                cpu_acc: 0.0,
                cpu_count: 0,
                canvas: canvas_js.clone(),
                _ro: None,
                _ro_cb: None,
            }));
            let canvas_el: HtmlCanvasElement = canvas_js.unchecked_into();
            watch_resize(&canvas_el, state.clone(), pending);
            Ok((state, adapter_info))
        }
        Err(e) => Err((core, e)),
    }
}

async fn try_gpu_inner(
    core: &FpsCore,
    canvas_js: JsValue,
    size: (u32, u32),
) -> Result<(GpuPart, String), String> {
    let canvas: web_sys::HtmlCanvasElement = canvas_js.unchecked_into();
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::BROWSER_WEBGPU;
    // The surface borrows the instance; the winner lives for the
    // page (same leak pattern as the native driver).
    let instance: &'static wgpu::Instance = Box::leak(Box::new(wgpu::Instance::new(desc)));
    let surface: wgpu::Surface<'static> = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        .map_err(|e| format!("create surface: {e:?}"))?;
    // Owned (not yet shared): no RefCell guard is held across the
    // awaits below (that pattern deadlocks under reentrancy and
    // trips `await_holding_refcell_ref`); the Rc wrap happens
    // after bring-up completes.
    let mut backend = VelloBackend::new();
    let adapter_info = backend
        .ensure_gpu_for_surface_async(instance, &surface)
        .await
        .map_err(|e| format!("{e:?}"))?;
    backend
        .configure_surface(&surface, size.0, size.1)
        .map_err(|e| format!("{e:?}"))?;
    let surface_id = Rc::new(Cell::new(
        backend
            .create_surface(SurfaceDesc {
                width_px: size.0,
                height_px: size.1,
                background: Color(0xFF_FF_FF),
            })
            .map_err(|e| format!("{e:?}"))?,
    ));
    let backend = Rc::new(RefCell::new(backend));
    install_vello_paint_hook_shared(
        core.host(),
        backend.clone(),
        surface_id.clone(),
        1.0,
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
    );
    let (fid, bytes, index) = atlas_pair(core.text_service());
    backend.borrow_mut().set_font_for(fid, bytes, index);
    core.frame();
    backend
        .borrow_mut()
        .present_surface(surface_id.get(), &surface)
        .map_err(|e| format!("{e:?}"))?;
    // Uncapped-throughput probe (`?bench=N`): N render+submit
    // cycles with no acquire/present (the compositor gates
    // display at panel rate regardless). Runs before the loop
    // starts so the rAF cadence never contaminates it.
    if let Some(n) = bench_param() {
        let id = surface_id.get();
        let t0 = now_secs();
        for _ in 0..n {
            backend
                .borrow_mut()
                .render_submit_only(id)
                .map_err(|e| format!("{e:?}"))?;
        }
        let ms = (now_secs() - t0) * 1000.0;
        console_log(&format!(
            "oppa-fps bench: {n} GPU renders in {ms:.1}ms → mean {:.2}ms → ~{:.0} fps uncapped-equivalent",
            ms / n as f64,
            1000.0 / (ms / n as f64)
        ));
    }
    Ok((
        GpuPart {
            backend,
            surface,
            surface_id,
            last_stage_log: now_secs(),
            acc: [0.0; 7],
            count: 0,
        },
        adapter_info,
    ))
}

/// CPU lane: softbuffer state, warmup present, rAF loop.
fn start_cpu(core: FpsCore, canvas_js: JsValue, size: (u32, u32), pending: Rc<Cell<bool>>) {
    let canvas_el: HtmlCanvasElement = canvas_js.clone().unchecked_into();
    let handles = CanvasHandles {
        canvas: canvas_js.clone(),
    };
    let scene = build_cpu_scene(&core, size).expect("CPU scene");
    let ctx = softbuffer::Context::new(handles.clone()).expect("softbuffer context");
    let mut sb = softbuffer::Surface::new(&ctx, handles.clone()).expect("softbuffer surface");
    sb.resize(
        std::num::NonZeroU32::new(size.0).expect("width"),
        std::num::NonZeroU32::new(size.1).expect("height"),
    )
    .expect("softbuffer resize");
    let mut state = WebState {
        core,
        cpu: Some(CpuPart {
            scene,
            _ctx: ctx,
            sb,
        }),
        gpu: None,
        size,
        last: now_secs(),
        last_stage_log: now_secs(),
        cpu_acc: 0.0,
        cpu_count: 0,
        canvas: canvas_js,
        _ro: None,
        _ro_cb: None,
    };
    // Warmup frame before the loop: first paint is ready, never blank.
    state.core.frame();
    if let Some(cpu) = state.cpu.as_mut() {
        let backend = cpu.scene.backend.clone();
        let surface_id = cpu.scene.surface_id;
        if let Err(e) = present_cpu_frame(&backend, surface_id, &mut cpu.sb, state.size) {
            console_log(&format!("oppa-fps: warmup present failed: {e}"));
        }
    }
    // Same uncapped probe on the CPU lane: scheduler + raster +
    // pack, minus the canvas present (compositor-gated).
    if let Some(n) = bench_param() {
        if let Some(cpu) = state.cpu.as_ref() {
            let t0 = now_secs();
            for _ in 0..n {
                state.core.frame();
                let borrowed = cpu.scene.backend.borrow();
                let pix = borrowed.pixmap(cpu.scene.surface_id).expect("bench pixmap");
                let _ =
                    pack_rgba_to_xrgb(pix.data(), pix.width(), pix.height()).expect("bench pack");
            }
            let ms = (now_secs() - t0) * 1000.0;
            console_log(&format!(
                "oppa-fps bench: {n} CPU renders in {ms:.1}ms → mean {:.2}ms → ~{:.0} fps uncapped-equivalent",
                ms / n as f64,
                1000.0 / (ms / n as f64)
            ));
        }
    }
    let state = Rc::new(RefCell::new(state));
    watch_resize(&canvas_el, state.clone(), pending.clone());
    run_loop(state, pending);
}

/// Arms the page-lifetime rAF loop around shared state.
fn run_loop(state: Rc<RefCell<WebState>>, pending: Rc<Cell<bool>>) {
    let raf: RafSlot = Rc::new(RefCell::new(None));
    let raf_tick = raf.clone();
    let state_tick = state.clone();
    let pending_tick = pending.clone();
    *raf.borrow_mut() = Some(Closure::wrap(Box::new(move |t_ms: f64| {
        tick(&state_tick, &pending_tick, &raf_tick, t_ms);
    }) as Box<dyn FnMut(f64)>));
    let cb = raf.borrow();
    request_animation_frame(cb.as_ref().expect("rAF closure").as_ref().unchecked_ref());
}

#[wasm_bindgen(start)]
pub fn start() {
    std::panic::set_hook(Box::new(|info| {
        console_log(&format!("oppa-fps panic: {info}"));
    }));
    console_log("oppa-fps: web start");
    // One JsValue, three uses: typed wrapper for attributes +
    // stage append (via clone), softbuffer handles, WebGPU
    // surface. (`unchecked_into` consumes, so clones feed each.)
    let canvas_js: JsValue = create_element("canvas");
    let canvas: HtmlCanvasElement = canvas_js.clone().unchecked_into();
    canvas.set_width(DEFAULT_W);
    canvas.set_height(DEFAULT_H);
    // The stage div (function call, like getElementById — never a
    // property binding: `document.body`-style getters need explicit
    // `getter` bindings, and a bare `fn` binding against a property
    // throws a silent fatal trap, not a hook-reported panic).
    let stage = get_element_by_id("oppa-stage").expect("oppa-stage div");
    stage.append_child(&canvas);
    let size = (DEFAULT_W, DEFAULT_H);
    let core = FpsCore::new(layout_service(), layout_service(), size, None);
    // Deferred-resize flag, shared by the rAF tick and the
    // observer (see `on_resize`): a `Cell` outside the state
    // `RefCell`, so flagging never traps.
    let pending: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    if !webgpu_present() {
        console_log("oppa-fps: Web CPU path (tiny-skia + canvas; navigator.gpu absent)");
        start_cpu(core, canvas_js, size, pending);
        return;
    }
    console_log("oppa-fps: WebGPU detected, bringing up GPU…");
    wasm_bindgen_futures::spawn_local(async move {
        let canvas_fallback = canvas_js.clone();
        match try_gpu_state(core, canvas_js, size, pending.clone()).await {
            Ok((state, adapter_info)) => {
                console_log(&format!("oppa-fps: Web GPU path ({adapter_info})"));
                run_loop(state, pending);
            }
            Err((core, e)) => {
                console_log(&format!(
                    "oppa-fps: Web GPU unavailable ({e}); CPU fallback"
                ));
                start_cpu(core, canvas_fallback, size, pending);
            }
        }
    });
}
