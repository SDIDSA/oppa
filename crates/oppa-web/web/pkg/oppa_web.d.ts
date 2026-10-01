/* tslint:disable */
/* eslint-disable */

export class WebApp {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Pointer tap at CSS px: down + up through the shared pipeline.
     * Returns the DOM patch JSON, or `None` when untouched. A
     * touched frame persists the toggle (Round 4.3 — settings
     * follow paint, so a stored value never disagrees with the
     * pixels; persist failures panic loudly, harness-gated).
     */
    click(x: number, y: number): string | undefined;
    /**
     * Live device pixel ratio (G10, decision 226): the browser's
     * `window.devicePixelRatio` through the shared
     * `dpr_from_scale_factor` rule. Host builds (tests) read 1.0 by
     * cfg-gate — `web_sys` traps on non-wasm targets instead of
     * returning `None`, so the fallback is compile-time, never a
     * runtime guess.
     */
    device_pixel_ratio(): number;
    /**
     * Resolves a platform fetch from the promise callback (Round
     * 4.1): `ok` selects `Ready(text)` vs `Failed(text)` (HTTP
     * status / exception message — the bootstrap decides the
     * text). Applies only when `generation` is still current
     * (stale results discard → `None`, never half-swapped state;
     * garbage generations saturate through `as u64` into a
     * mismatch — discarded, never panicking). Returns the DOM patch
     * JSON, or `None` when discarded or untouched.
     */
    fetch_resolve(name: string, generation: number, ok: boolean, text: string): string | undefined;
    /**
     * Starts a platform fetch (Round 4.1, web fetch — the wasm half
     * of decision 221): sets the named keyed signal to `Loading`
     * and returns its generation for the promise callback. The key
     * derives from `name` through `fetch_key` (one source of truth
     * — JS never sees raw keys). Generations ride `f64` (exact to
     * 2^53 — counters never approach it).
     */
    fetch_start(name: string): number;
    /**
     * Pointer move at CSS px (hover). Returns the DOM patch JSON,
     * or `None` when untouched (moves over handler-less gaps
     * change nothing and must never fail — decision 95).
     */
    hover(x: number, y: number): string | undefined;
    /**
     * Current full-page HTML (the bootstrap's initial mount —
     * primes the patch snapshot, so later patches diff against
     * exactly this tree).
     */
    html(): string;
    /**
     * Key event (`code` is the framework keycode). Returns the DOM
     * patch JSON, or `None` when untouched.
     */
    key(code: number, pressed: boolean): string | undefined;
    /**
     * Handles a `popstate` (Round 4.2, OQ-G6-1 — history→app
     * half): the URL is the popstate state (no `state` object
     * needed). A URL matching the entry below the top is a true
     * back → pop; anything else (forward, or a divergent entry)
     * replaces the top — so the stack tracks browser depth
     * instead of accumulating duplicates. Unparsable URLs fall
     * back to `home` (standard SPA behavior — the URL stays,
     * content is home). Returns the DOM patch JSON, or `None`
     * when untouched.
     */
    nav_pop(): string | undefined;
    /**
     * Pushes a route (Round 4.2, OQ-G6-1 — app→history half):
     * stack push + `history.pushState` (wasm only; host builds
     * skip the history call, same cfg-gate as DPR) + render.
     * Invalid names panic loudly (authoring bug — the bootstrap
     * passes developer-chosen names, never user input). Returns
     * the DOM patch JSON, or `None` when untouched.
     */
    nav_push(name: string): string | undefined;
    /**
     * Replaces the top route (Round 4.2): stack replace +
     * `history.replaceState` + render. Same loudness + patch rules
     * as `nav_push`.
     */
    nav_replace(name: string): string | undefined;
    /**
     * Mounts the scene and renders the first page.
     */
    constructor();
    /**
     * Mounts the shared kitchen-sink showcase instead of the demo
     * scene (Round 7.17): the same shell (clock, DejaVu measuring,
     * DOM backend, settings store) with `KitchenSinkApp` as the
     * root — the web leg of the sink's every-target claim
     * (desktop `run_desktop`, Android `mount_app`). Served as
     * `sink.html` (the demo `new()` stays the default entry
     * point); the sink Edge harness (`spike/web/sink.mjs`) drives
     * this constructor. Demo-only state (nav, settings toggle,
     * demo image) still rides along inertly — custom scenes own
     * their state through `props`, like every `new_with_root`
     * root.
     */
    static new_sink(): WebApp;
    /**
     * Follows the OS light/dark flip (Round 16.2, decision 315 —
     * the bootstrap's `matchMedia` listener calls this on
     * `change`): sets the reactive theme from the live query when
     * it differs (same-value writes still invalidate — the signal
     * carries no equality gate — so a matching reading returns
     * `None` without touching anything, never a repaint spin).
     * Returns the DOM patch JSON, or `None` when untouched.
     */
    sync_system_theme(): string | undefined;
    /**
     * Text value from a DOM field (U8): resolves the rendered
     * `data-pid` to the retained node and feeds the full value.
     * Returns the DOM patch JSON, or `None` when untouched or the
     * pid is unknown (stale post-swap markup — never a silent
     * node, never a panic).
     */
    text(pid: string, value: string): string | undefined;
    /**
     * rAF tick: advances TIME to `now_ms` and runs the frame.
     * Returns the DOM patch JSON, or `None` when untouched
     * (transition tails settle through these calls).
     */
    tick(now_ms: number): string | undefined;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_webapp_free: (a: number, b: number) => void;
    readonly webapp_click: (a: number, b: number, c: number) => [number, number];
    readonly webapp_device_pixel_ratio: (a: number) => number;
    readonly webapp_fetch_resolve: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
    readonly webapp_fetch_start: (a: number, b: number, c: number) => number;
    readonly webapp_hover: (a: number, b: number, c: number) => [number, number];
    readonly webapp_html: (a: number) => [number, number];
    readonly webapp_key: (a: number, b: number, c: number) => [number, number];
    readonly webapp_nav_pop: (a: number) => [number, number];
    readonly webapp_nav_push: (a: number, b: number, c: number) => [number, number];
    readonly webapp_nav_replace: (a: number, b: number, c: number) => [number, number];
    readonly webapp_new: () => number;
    readonly webapp_new_sink: () => number;
    readonly webapp_sync_system_theme: (a: number) => [number, number];
    readonly webapp_text: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly webapp_tick: (a: number, b: number) => [number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
