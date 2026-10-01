# Cross-Platform UI Framework — Design Reference (v1, closed)

This is the single, self-contained design document for the project. It
replaces the prior discussion documents; nothing in here is still being
decided — the one item R5 left spike-gated (text-editing authority,
§9.2) was resolved by the M1 spike and merged into this document
(locked #27; §2.3's second text path; origin: spike/REPORT.md).
Round references: **R1** = core
architecture, **R2** = language decision, **R3** = hot-reload grounding,
**R4** = rasterizer, **R5** = stress-test resolutions (scheduler &
threading, text editing, web scroll, transition × rebind, rasterizer
attribution) — §9.

---

# 1. Goals & constraints

**Platforms:** Windows, Linux, Android, Web.

**Priorities (settled, in order):** 1) rendering performance, 2) developer
ergonomics, 3) binary size, 4) hot reload / iteration speed.

Three constraints from the original goal list, settled as decisions rather
than open pushback:

1. **Pipeline ownership is split by design, not uniform.** We own the
   pipeline on Windows/Linux/Android; on Web we own the scene schema and
   the browser owns the pixels. DOM is a first-class renderer backend, not
   a platform we lose on. Owning the pipeline on Web would mean shipping a
   wasm GPU rasterizer (~1.5–2.5 MB, slow first paint, preloaded fonts,
   Flutter-web's accessibility problems) — rejected. *(R1)*
2. **Accessibility is an architecture constraint, not a feature.** The
   accessibility/semantic tree is computed from the UI model and emitted as
   part of the renderer contract from day one. Retrofitting it later is the
   single most expensive mistake in UI frameworks (ask Flutter). *(R1)*
3. **Perf #1 vs. binary size #3 resolves as: buy the rasterizer, own
   everything above it.** Writing our own vector GPU rasterizer
   (tessellation, AA, glyph atlas, blend modes) is a multi-year effort and
   is a v2 decision; v1 embeds an existing rasterizer behind our own
   display list. *(R1; concretized in §6, R4)*

Language/runtime is decision #0 and is settled: **Rust** (§3).

---

# 2. Architecture

## 2.1 Core abstraction layer

**A retained, declarative scene graph produced by a fine-grained reactive
model, consumed by diff-driven per-platform presenters via tree diffs +
frame plans.** Retained at the model level, immediate at the submit level.

The pipeline:

```
signals/state → reactive components → VNode tree (ephemeral, discarded)
             → reconcile vs. RetainedNode tree (stable identity)
             → layout pass (shared engine)
             → paint pass → per-frame display lists + damage
             → a11y semantic-tree diff
             → renderer commits & presents
```

**Retained vs. immediate trade-off, as settled:**

| | Immediate mode (egui-style) | Fully retained (Flutter 3-tree) | **Chosen: retained model, 2 trees** |
|---|---|---|---|
| Rendering perf | Redraws every frame; damage/caching/layer caches get *bolted on* and re-create retained concepts scattered per-widget | Best: only dirty subtrees repaint, layout cached | Same win, minus one tree of overhead |
| Ergonomics | Great for simple tools; collapses for complex apps (implicit widget state, no tree ownership) | Fine but verbose | Fine-grained reactivity gives React-level ergonomics without reconciliation sweeps |
| Binary size | Smallest | Largest | Between; shared reconciler is one-time cost |
| Hot reload | Trivial | Hard (identity across reloads must be preserved) | Medium — explicit key/identity discipline (§5) |
| a11y / text / IME | Must be reinvented ad hoc (egui's real debt) | Natural: retained tree exists | Natural |

**Key divergence from Flutter: two trees, not three.** Flutter's
widget→element→renderobject triple exists because Dart reconciliation is
coarse. With fine-grained dependency tracking (signals/memos), invalidation
knows *which component instances* to re-run, so the "widget tree" collapses
into functions and the ephemeral VNodes they return diff only locally
against retained nodes.

**The minimal interface a new renderer backend must implement:**

```rust
enum PresenterKind { GpuDrawList, Dom }

trait RendererBackend {
    fn kind(&self) -> PresenterKind;
    fn create_surface(&mut self, desc: SurfaceDesc) -> SurfaceId;  // window/canvas/element
    fn destroy_surface(&mut self, id: SurfaceId);
    fn commit(&mut self, surface: SurfaceId, diff: &TreeDiff);      // retained scene update
    fn paint(&mut self, surface: SurfaceId, plan: &FramePlan);      // ordered display list + damage
    fn caps(&self) -> Caps;  // max layers, blur/backdrop support, msaa, text-as-paths, etc.
}

trait PlatformShell {           // shared across backends — NOT per-renderer
    fn pump_events(&mut self) -> Vec<PlatformEvent>;
    fn request_frame(&mut self);
    fn set_dpi_aware(&mut self, f: f32);
    fn set_ime(&mut self, ops: ImeOps);          // cursor rect, candidate window control
    fn set_cursor(&mut self, icon: CursorIcon);
    fn semantics(&mut self, diff: Option<&SemanticsDiff>);  // None = "not supported/asked for"
    fn text(&self) -> &dyn TextService;          // font enumeration, shaping, measurement
}
```

Rule: **everything inside those two traits is throwaway per-platform code;
everything above them is written once.** A target "exists" when it
implements `RendererBackend` + `PlatformShell`. That boundary is what makes
the UI model renderer-agnostic rather than the model just being "a thing a
rasterizer eats."

## 2.2 UI model shape

```rust
// --- Reactive core (fine-grained; these five primitives are the whole contract)
Signal<T>, Memo<T>, Effect, BatchGuard, untrack(|| ...)

// --- Ephemeral layer: components are plain functions, not classes/types
type Component<P> = fn(&Ctx, &P) -> VNode;
struct Ctx { /* signal reads here become invalidation deps for THIS instance only */ }

enum VNode {
    Element(Element),
    Text(Arc<str>),
    Fragment(Vec<VNode>),   // also: keyed lists via Fragment + keys
    Hole,                   // "no render" — reconciles to absent
}

struct Element {
    tag: Tag,                // enum: Div, Stack, Text, Image, ScrollArea, ... (~small closed set + escape hatch)
    key: Option<u64>,        // explicit stable identity for lists/hot-reload
    style: StyleId,          // interned; see below
    children: Children,
    handlers: SmallVec<(EventKind, HandlerId)>,
}

// --- Style: typed structs, NOT a CSS cascade
struct Style {
    layout:  LayoutProps,   // flex/component stack, optional grid, box: padding/margin/size/constraints
    paint:   PaintProps,    // bg, border, radius, shadow, opacity, clip, text style
    behavior: BehaviorProps,// scrollable, focusable, pointer_mode, aria-ish semantics
}
// inheritance: a small explicit set (text style, direction) inherits down the tree;
// no specificity, no cascade merging. Themes = token tables resolved to Styles.
// Interned into StyleId + intern table — payload shared across thousands of nodes.

// --- Retained tree: what renderers and layout actually see
struct RetainedNode {
    id: NodeId,
    parent: NodeId, children: SmallVec<[NodeId; 4]>,
    style: StyleId,
    layout: LayoutBox { x, y, w, h, content_size, text_runs: Vec<ShapedLine> },
    flags: Flags,           // own_layer, scrollable, focusable, hit_target, ...
    pass_dirty: PassMask,   // STRUCTURE | STYLE | LAYOUT | PAINT | TEXT | SEMANTICS
}

// --- Display list (the "immediate" half; what a Gpu backend consumes)
enum DrawOp {
    Rect(FormatRect), RImg(ImageId, Box2), RRect(RRect),
    Text(ShapedRun), Path(PathId, Paint),
    PushClip(Clip), PushLayer { opacity: f32, blend: BlendMode }, Pop,
    // damage metadata lives on the FramePlan, not per-op
}

// --- Events: one normalized enum everywhere; hit-testing is a tree walk
enum InputEvent {
    Pointer(PointerId, PointerAction /* Down/Move/Up/Cancel */, Position, Modifiers),
    Scroll { target: NodeId, delta: Vec2, phase: ScrollPhase },  // physics normalized away on
                                                                 // GPU backends; Web = browser
                                                                 // physics, fed back at INPUT (§9.3)
    Key(PhysicalKey, LogicalKey, Modifiers, KeyState),
    Ime(ImeEvent /* begin, update, commit, delete-range */),
    Focus(FocusChange),
}
// dispatch = hit-test against RetainedNode tree (GPU backends)
//          = browser event → enum mapping (Web backend), framework still owns logical routing
```

Important shapes: **closed-set `Tag`** (components exist only in the
ephemeral layer as groupings), **interned styles**, **dirty-mask per node
instead of a global rebuild flag**, and **handlers as ids, not closures, so
the retained tree is copyable, serializable, and survives hot reload**.

## 2.3 Renderer contract

**What renderers receive, concretely:**

- **Input, structural:** `TreeDiff` = added/removed/moved node ids +
  per-node updated payload (interned style id changed, text content
  changed, handler id changed). Renderers materialize their own
  representation keyed by `NodeId`. They never see components, state, or
  diffs of the ephemeral tree.
- **Input, per frame:**
  `FramePlan { viewport, build/destroy-layer, ordered draw_ops: Vec<DrawOp>, damage: [Rect; N], target_layers: Vec<LayerPlan> }`.
  Built only from dirty subtrees; static UIs cost ~0 CPU on repaint.
- **Output:** backend-defined. GPU backends: draw calls on a surface we
  nominate. Web backend: DOM mutations + CSS-rule updates driven by the
  same `TreeDiff`.
- **In both directions:** `ExternalTexture` for embedded native content
  (video, webview-of-last-resort) on GPU backends; on Web, embedded native
  content is "render this node as `<video>`/`<iframe>`" — the DOM backend
  gets an "external element hole" in its contract.

**Layout — owned by the framework, not the renderer.** One engine, written
once, producing `LayoutBox` per node. Reasons: (a) renderer-side layout
means N implementations of the hardest code in the project, (b) the Web
backend must NOT run browser layout for boxes (split-brain), (c) layout
results must be stable for hot reload and a11y bounds. Consequences:

- The engine speaks to `TextService` for measurement; **layout of text runs
  belongs to the model layer**, so the display list carries *pre-shaped,
  pre-positioned glyph runs*. Renderers just rasterize them (glyph atlas
  upload or DOM text). This keeps the per-renderer text burden near zero.
- Web caveat (documented parity limitation, see §8): our engine lays out
  boxes; the browser lays out *text flow inside* text nodes it hosts. v1
  constrains Web to flat flexbox/text-in-flex.

**Text — the contract's second text path (first-class clause; M1 spike
verdict (b), locked #27; origin: spike/REPORT.md §5).** Editable text
fields are a **presenter-recognized special case on Web**: for fields
recognized as editable, the DOM backend owns editing authority (caret,
selection, IME, undo); the framework's renderer contract guarantees
**behavior** — via the shared editing-operation suite (§9.2) — not
**mechanism**. Non-editable text stays on the existing
framework-measured/shaped path above: this clause changes nothing about
how static text is handled; it applies only to fields recognized as
editable. Two contract rules are permanent parts of this clause:

- **Framework-measured tracked text (letter-spacing) is never delegated
  to CSS `letter-spacing`.** The two diverge by exactly one tracking
  unit at the trailing edge (REPORT.md §5: "Hello world", 16 px with
  1 px tracking → DOM **92.03125** px vs framework **91.03125** px —
  CSS letter-spacing applies after the final character; the framework's
  tracking convention does not). This applies wherever the framework
  measures tracked text for rendering on Web, independent of the
  editing-authority question.
- **The DOM text path's freeze is gated, not automatic.** Blocking
  condition, not a note: the DOM text/editing contract may not be marked
  frozen until (a) one manual pass with a real OS IME verifies the
  delete-range-mid-composition path (REPORT.md finding #5 —
  unverifiable via CDP scripting in the M1 rig) **[CLOSED — locked #28]**,
  and (b) bidi/combining/ZWJ corpus coverage **[PARTIAL — locked #29]**:
  combining-mark cluster parity + ZWJ single-cluster geometry/hit-test
  closed on both-arm evidence; bidi visual ordering re-deferred to the
  M3 layout engine; word-segmentation + scalar combining-caret
  remainders tracked as shared-suite spec items (M2 editing session).
  The M3 visual-ordering deferral is now the only open freeze item on
  this contract.

**Who does what:**

| Concern | Owner |
|---|---|
| Component model, state, reconciliation | Core (shared) |
| Layout engine | Core (shared) |
| Event classification, hit-testing (GPU), routing | Core (shared) |
| Surface/window/IME/DPI/lifecycle | PlatformShell (per-platform) |
| Text shaping/measurement/fallback | TextService (per-platform) |
| Pixels or DOM ops | RendererBackend (per-platform; diff-driven, holds no app state — amended framing, §9.2) |
| A11y tree materialization | PlatformShell, driven by SemanticsDiff from core |

## 2.4 Prior art — lessons actually adopted

Kept only where a lesson is reflected in a locked decision above.

- **Flutter** (pipeline ownership, damage/layer discipline, hot reload as a
  product feature) → adopted; **diverged**: two trees instead of three,
  DOM-on-web, a11y-in-contract from day one.
- **Solid / Svelte / Dioxus** (fine-grained signals with dependency
  tracking beat VDOM sweeps without sacrificing ergonomics; components as
  plain functions returning tree literals) → adopted wholesale as the
  invalidation and authoring model.
- **egui** (refused: immediate mode) → its lesson stands: immediate mode
  optimizes iteration speed at the direct expense of rendering perf and of
  everything not on the priority list (a11y, IME, virtualized lists become
  retrofits fighting the model).
- **Slint** (refused: custom DSL) → a DSL means a second toolchain, split
  tooling/FFI story, smaller ecosystem. Components live in the host
  language instead.
- **Skia-based stacks (Avalonia, Compose Multiplatform)** → proof that one
  mature rasterizer behind a custom scene graph buys years of correctness
  for cheap (informs the §6 decision); native-widget-wrapper approaches are
  rejected (they trade away pipeline ownership for free a11y/fill-in).
- **Dioxus desktop's webview route** → rejected for desktop (it abdicates
  pipeline ownership on a target platform); its RSX ergonomics kept.

---

# 3. Language & runtime (settled)

**Decision: Rust for the core (scheduler, reconciler, layout, contracts);
components also authored in Rust, hot-swapped as dylibs.**

| | Perf (#1) | Ergonomics (#2) | Binary size (#3) | Hot reload (#4) | 4-platform story |
|---|---|---|---|---|---|
| **Rust** | No GC, no refcount traffic, predictable frame pacing | Steepest of the four | No runtime; wasm target is first-class | Body edits ≈ 0.1–1s (dylib patching); shape changes = relink/restart | Strongest of the four (incl. wasm → free DOM backend) |
| **C++** | Parity | Worst: you rebuild a borrow checker out of `shared_ptr` to make the retained tree safe | Fine | Windows Edit&Continue is narrow; Linux needs an embedded interpreter (Cling-class) or commercial tooling | No package story; each build system per platform is its own project |
| **Swift** | ARC traffic on hot paths; fine but not free | Excellent (SwiftUI proves it) | Must bundle Swift runtime on Win/Linux (multi-MB, no stable ABI) | Good only inside Apple tooling; on Android/Linux you're an early adopter of the *toolchain itself* | Android/Linux are second-class; language roadmap is set by Apple's platform priorities |
| **Kotlin** | JVM: JIT warmup + GC pauses directly violate #1; Kotlin/Native: viable but slower GC, multi-MB | Best of the four (Compose proves the reactive model) | jlink desktop bundles 30–60MB | Good on JVM; nearly nonexistent on Native | Fine, but writing the core in Kotlin locks every non-Kotlin consumer behind FFI |

The deciding frame: C++ never wins under any weighting of these priorities;
Swift and Kotlin win ergonomics but both drag a runtime (violating #3) or a
platform-anchored toolchain (violating the Windows/Linux/Web coverage).
Rust is the only candidate that is simultaneously adequate on #1, free on
#3, and has the best four-target story.

**What ownership costs us, concretely — settled as design constraints:**

1. **The retained tree is never referenced by user code — only by id.**
   Handlers-as-ids is load-bearing: the runtime owns a handler registry
   keyed by stable symbol hashes, re-resolved after each hot swap. Signals
   are generational slots (`Arc`-erased); node storage is an arena +
   `NodeId` with generation checks.
2. **Closures can't capture tree references** → components capture
   *signals*, not state. Props are by value; the `.clone()` tax is paid
   deliberately and interned away (`StyleId`, `Arc<str>`).
3. **Hot reload is achievable but never Dart-grade** (details in §5).
4. **Compile times** are budgeted via crate splitting + `mold`/`lld`; the
   wasm/dylib component boundary keeps the big core out of the iteration
   loop.

Lock justification: *Rust buys the #1/#3 priorities outright, has the only
credible four-platform story, and its ownership model, while hostile to
naive retained-mode designs, actively forces the id-based indirection that
makes hot reload and recycling safe — the costs are design constraints we
were going to adopt anyway; C++/Swift/Kotlin would let us build the unsafe
version first.*

---

# 4. Component authoring model

The traditional retained-toolkit inventory for one control
(WPF/WinUI/Avalonia): a **control class** with a registered
`DependencyProperty` per visual field, each with change-callback + "affects
render/measure" metadata; a **template** (`VisualStateManager` state groups
with transitions); a **behavior/interaction class** wiring pointer events
to `GoToState` and getting stuck-pressed-on-cancel right; an
**`AutomationPeer` subclass**; and a **`[TemplatePart]`** naming contract.

Our model has **one** change-propagation mechanism — signal invalidation →
re-run → diff → pass dirty flags — where traditional toolkits have six
(property system, templating, interaction, a11y, virtualization, events),
each needing its own hook. All six collapse into one function plus
framework primitives.

## 4.1 Toggle switch

```rust
#[derive(Props)]
struct ToggleProps {
    label: SharedString,
    initial: bool,
    enabled: bool,                     // plain value; change = component re-run
    on_change: HandlerId,              // parent-registered callback, resolved via registry
    theme: ToggleTheme,                // interned token table, not a style sheet
}

#[component]
fn Toggle(ctx: &Ctx, props: ToggleProps) -> VNode {
    let is_on    = ctx.signal(props.initial);
    let hovered  = ctx.hovered();      // framework-provided reactive flags, derived from
    let pressed  = ctx.pressed();      // the normalized InputEvent stream + hit-test
    let focused  = ctx.focused();

    let track = match (props.enabled, pressed(), hovered(), is_on()) {
        (false, _, _, _) => props.theme.track_disabled,
        (_,  true, _, _) => props.theme.track_pressed,
        (_,  _, true, _) => props.theme.track_hover,
        (_,  _, _, true) => props.theme.track_on,
        _                => props.theme.track_off,
    };
    let knob_x = if is_on() { 23.0 } else { 3.0 };

    Div("track")
        .style(Style::new()
            .size(44, 24).radius(12).bg(track).opacity(props.enabled.then_some(1.0))
            .transition(Transition::new(120.ms(), Ease::Out)))     // declarative; TIME-driven (§9.1)
        .semantics(Semantics::switch()
            .checked(is_on()).label(&props.label).disabled(!props.enabled))
        .on_press(move || { is_on.set(!is_on()); emit(props.on_change, is_on()); })
        .child(
            Div("knob")
                .style(Style::new().size(18, 18).circle()
                    .bg(props.theme.knob).x(knob_x)
                    .shadow(1, 2, props.theme.knob_shadow))
        )
}
```

Usage: `Toggle { label: "Wi-Fi", initial: wifi, on_change: |v| wifi_signal.set(v) }`.

**Which pieces collapse, and why it's safe to collapse them:**

| Traditional piece | Collapses into | Why safe |
|---|---|---|
| Control class + one `DependencyProperty` per field + change callbacks + "affects" metadata | Props struct + local signals | The update mechanism is no longer "property changed → dispatch to subsystems"; it's "signal read invalidated → re-run a 20-line pure function → diff its VNode output → `PASS::STYLE/PACK/PAINT` dirty flags." Per-field plumbing is subsumed by re-running the whole render. Safety invariant: render fns are pure w.r.t. reads and write only via signals/effects — the scheduler (§9.1) guarantees they never run mid-layout, so the diff is always against consistent state. |
| Skin/template (XAML) + `[TemplatePart]` contract | The function body *is* the template; theming via token tables (`props.theme`) + style inheritance | Template replacement exists to let non-code restyle arbitrary composition. Token-table swap covers restyling (styles are interned, so a theme flip is a `StyleId` change per node — cheap). Structural customization is handled by **slots**: `props.leading: Option<Component>`, i.e. direct function composition instead of a stringly-typed parts contract. Nothing to get out of sync because there's no second representation of the control's structure. |
| `VisualStateManager` + behavior/interaction class | `match` over signals + `.transition(...)` | Visual states are plain data (signal values), so they're inspectable/testable without a state-machine runtime. The bugs behavior classes exist to fix — stuck-pressed on pointer cancel, hover drag-out — are solved *once* in the framework's `pressed()`/`hovered()` primitives, not per control. Transitions are interpolated style deltas — evaluated by the scheduler's TIME phase on GPU backends, compiled to CSS transitions on DOM (§9.1); no imperative animation wiring to forget. |
| `AutomationPeer` subclass | Inline `.semantics(...)` on the node | Semantics live in the retained node and diff like style. Role/state can't drift from visuals because they're written in the same expression that produces the visuals. |
| Event wiring boilerplate | `.on_press(closure → HandlerId)` | One normalized `InputEvent` enum, framework-side hit-testing. |

## 4.2 Recycled list cell in a virtualized list

Virtualization is a **model-level feature of `ScrollArea`**, and recycling
falls out of the two-tree reconciler rather than being user-visible
machinery. The design choice that matters: **keys are the *slots*, not the
items.** (Keying by item id = destroy/create ~30 components per scroll
tick; keying by slot = zero structural churn. Recycle pool = the reconciler
doing nothing.)

```rust
#[component]
fn ContactList(ctx: &Ctx, store: Store<ContactId>) -> VNode {
    let viewport_h = 600.0;
    let row_h      = 56.0;
    let offset     = ctx.scroll_offset();              // framework-owned signal: INPUT-fed on
                                                       // Web, TIME-fed physics on GPU (§9.1, §9.3)
    let n_slots    = (viewport_h / row_h).ceil() as usize + 2;   // +2 overscan
    let selected   = ctx.signal(None::<ContactId>);

    ScrollArea("list")
        .content_size(store.len() as f32 * row_h)      // spacer defines scrollbar & physics
        .style(Style::new().h(viewport_h).fill_width())
        .children((0..n_slots).map(|slot| {
            // slot identity is stable; the *item binding* is what changes
            let item = ctx.binding(move || store.get(offset().row(row_h) + slot));
            Row("slot")
                .style(Style::new().absolute_y(slot as f32 * row_h).h(row_h).fill_width())
                .key(slot as u64)
                .child(ContactRow { item, selected, row_h })
        }))
}

#[component]
fn ContactRow(ctx: &Ctx, store: Store<ContactId>,
              item: impl Signal<Item = ContactId>,
              selected: Signal<Option<ContactId>>, row_h: f32) -> VNode {
    // re-derives whenever the slot re-binds to a different id OR the store mutates that id
    let contact = ctx.memo(move || store.lookup(item()));
    let avatar  = ctx.memo(move || image_cache.load(contact().avatar_small));
    let is_sel  = ctx.memo(move || selected() == item());

    Row("cell")
        .style(Style::new().h(row_h).pad_x(12)
            .bg(is_sel().then_some(TOKENS.selection_bg)))
        .semantics(Semantics::list_item().selected(is_sel()).label(contact().display_name))
        .on_press(move || selected.set(Some(item())))
        .children((
            Img { src: avatar, size: 36, radius: 18 },
            Column::new().gap(2).children((
                Text { text: contact().display_name, style: Text::title_small },
                Text { text: contact().status,       style: Text::body_secondary },
            )),
        ))
}
```

**What happens on a scroll tick** (the payoff trace): scroll event →
scheduler sets `offset` → each slot's `item` memo recomputes → each row's
`contact`/`avatar`/`is_sel` memos recompute → rows re-run, VNode diff
yields only text/style-id changes → `TreeDiff` contains **zero** structure
ops → `FramePlan` repaints ~30 cells → a11y diff updates ~30 labels.
Selection toggling re-renders exactly the two affected rows, because
`is_sel` dependencies are tracked per instance — no container restyle
sweep. Compare the traditional equivalent: `ItemContainerGenerator` +
recycle pool + `DataTemplateSelector` + per-container binding teardown on
recycle, with virtualization as a mode flag on a panel
(`VirtualizationMode="Recycling"`) whose bugs (wrong content in a recycled
container, lost focus/selection on scroll) are a genre of their own.

**Why recycling is safe here, mechanically:**

- **Retained identity is the slot**, so layout boxes, hit-test entries, and
  layer assignments persist across item swaps — no re-measure, no
  re-hit-test.
- **Handlers stay valid across item swaps** because they capture *signals*
  (`item`, `selected`), not item data — the handler reads the slot's
  *current* item through the memo. In a GC language you'd never notice; in
  Rust it's why the design is forced and correct.
- **The one real trap — transient per-item state.** A component's *local*
  signals re-seed when its slot re-binds ("is this row mid-slide-in
  animation" must not be local state or it'll teleport to the recycled
  row). Escape hatch: keyed side-state,
  `ctx.keyed_state::<SlideAnim>(item(), || init)`, which lives in a
  `HashMap<ContactId, _>` and is dropped by LRU when the id scrolls far out
   of the window. The only new concept recycling asks of component authors.
- **The other trap — phantom transitions on rebind.** A slot-keyed cell
  keeps its retained identity across item swaps, so a style delta produced
  *because the slot re-bound* (e.g. bg from the previous item's selection
  state) is indistinguishable from a real state change and would
  phantom-animate the recycled cell. Resolved by the binding-edge stamp:
  `item` is created via `ctx.binding`, and any commit whose re-run was
  triggered through it carries `suppress_transitions` for that one commit —
  values jump, no interpolation (§9.4).
- v1 scope: fixed-height rows (prefix-sum index for variable heights is a
  v2 concern) — same scoping decision as the layout engine.

---

# 5. Hot reload (shipped v1 design)

## 5.1 Mechanism and edit-class matrix

Every mechanism in this space obeys the same rule: **swap code, never
types.** Candidates considered: `hot-lib-reloader` (proc-macro proxy
re-`dlopen`s a dylib; battle-tested but every hot function must be declared
in the proxy — adding one means main-binary recompile), `subsecond`
(Dioxus 0.7 in-place code-patch trampolines; desktop + wasm; supports
adding new functions; type/layout changes not patchable).

**Shipped v1 design: a thin custom harness** — the hot crate exports a
stable `#[no_mangle] fn component_manifest() -> &[(SymbolHash, ComponentFn)]`
table; the core re-scans it after every swap (~500 lines over
`libloading`). This buys "adding new functions is a body edit" without
marrying us to either crate. Props crossing into the core are stored
**type-erased with vtable'd clone/drop exported by the hot crate, tagged
with a generation id**.

| Edit class | Supported? | Why / caveat |
|---|---|---|
| Body edit — new match arm producing an **existing** node kind (e.g. `Text` → `Img`) | **Yes** | Pure code swap; nothing crosses the boundary. |
| Adding a **new signal** inside a component body | **Yes, with a state caveat** | Signals are keyed by call-site (source hash + ordinal). Inserting a signal mid-body shifts the site/ordinal of every signal after it → with source-hash keying, later signals' hashes shift → their state **re-seeds** (not shuffles). Authoring rule, enforced (§8). Same rule React hooks live under. |
| Changing a closure's **captured variables** | **Yes** | Closures are internal to the hot crate; the runtime never sees their layout. Post-reload the component re-runs and creates fresh closures; `HandlerId`s re-resolve through the registry. Compile-time rule: captures may only be signals/ids, never tree references (borrowck enforces). |
| Adding a **new component function** | **Yes** (manifest scan) | The reason for the custom harness over `hot-lib-reloader`'s proxy approach. |
| **Adding a field to a `Props` struct** | **Yes under opaque props; otherwise no.** | Props live in the hot crate; the core stores them opaquely with hot-side vtable clone/drop + generation tag. Safe *provided* stored old-layout props are drained before unload (§5.3). If Props types are shared with non-hot crates or stored non-opaquely: **restart**. |
| Adding a **new variant to the core `Tag`/`VNode` enum** or any core type change | **No. Full rebuild + restart.** | Enum layout is linked into both binaries. Mitigation: `Tag` is a deliberately large closed set + `Custom(fn_id)` escape hatch so most "new node type" needs are new component functions (hot), not new core variants (cold). |

Latency expectations: **desktop body edits ≈ 0.1–1s**; anything ending in
"restart" = incremental component-crate rebuild (1–5s) + process relaunch.

## 5.2 Platform parity — Android is restart-only in v1

| Platform | v1 mechanism | Body-edit latency |
|---|---|---|
| Windows / Linux | dylib swap (custom harness) | 0.1–1s |
| Web | wasm module swap (`subsecond`-style wasm reload or rebuild-in-place) | ~0.1s |
| **Android** | **restart-only**: incremental rebuild of the component crate → `adb install -r` → relaunch | **~2–10s** |

Android blocks both swap mechanisms for app processes: apps targeting
modern API levels may not execute code loaded from app-data directories,
and SELinux forbids making data pages executable (required for in-place
patching). A real mobile-parity path exists — run the **component layer**
(not the core) as wasm interpreted in-process (`wasmi`); swapping a wasm
module is a data-file operation, and the keyed-signal-slot state
preservation works identically. It costs an embedded runtime (~1–2MB) and
interpreter overhead on component render. **Deliberate v2 decision** (§7),
not silently assumed.

## 5.3 RELOAD scheduler phase

The scheduler already accounts for identity churn because of handlers-as-
ids, runtime-owned generational signal slots, and "no user code runs
mid-layout." The one addition: an explicit **RELOAD apply point** in the
frame loop.

1. **Reloads apply at a frame boundary, as a queue phase:**
   `TIME → INPUT → RELOAD → EFFECTS → LAYOUT → PAINT/COMMIT → A11Y`
   (the full scheduler lock is §9.1; RELOAD keeps this position and its
   semantics). A reload is
   semantically just a batched set of state mutations plus a code swap; it
   must never apply between layout and paint of the same frame (torn
   tree), and it applies globally (all surfaces), not per-window, in v1.
2. **Drain-before-unload ordering:** at the RELOAD phase, first
   re-run/drain all live component instances whose stored props carry the
   previous generation tag (drop via the old dylib's drop glue), *then*
   unload, *then* re-scan the manifest, then re-run from root. Without this
   drain point the opaque-props safety argument doesn't hold.
3. **Registry re-resolution is atomic with the swap:** `HandlerId` → new
   function pointer mapping flips in the same phase; no in-flight event may
   dispatch against a half-swapped registry (events drained before,
   applied after).

Android's restart path exercises the existing cold-start path; no scheduler
work needed there.

---

# 6. Rasterizer (closed, R4)

**Decision: Vello is the v1 GPU rasterizer for desktop (Windows/Linux);
Skia is rejected for v1 and remains the documented escape hatch; hostile
targets fall back to a CPU backend behind the same `FramePlan`.**

## 6.1 The comparison, against the locked priorities

**Vello** (linebender; Rust-native, wgpu-based):

- *Buys, given our stack:* no C++ anywhere in the toolchain — a pure-Rust
  core stays pure-Rust end to end (no FFI in the debugger, no ninja/clang/
  Python Skia build in CI, no skia-safe version lag against upstream Skia).
  wgpu's wasm story keeps a canvas-fallback web backend *possible* without
  a second technology — and matters more strategically: the project is
  already committed to wasm twice (DOM web backend core runs in wasm;
  Android v2 hosts components in wasmi), and a pure-Rust GPU stack is the
  only option that composes with that trajectory. Skia on wasm is a
  separate, much heavier build (CanvasKit-class) and web is DOM anyway.
- *Actual maturity, honestly:* Vello is a compute-shader rasterizer —
  it requires compute shader support (GL 4.3+/GLES 3.1-class or D3D12/
  Vulkan/Metal); there is no WebGL fallback path. Desktop via wgpu is the
  good case and works today. It is **production-adjacent, not
  Skia-grade-proven**: shipped as an (experimental) Slint backend, driven
  by the Xilem ecosystem; not yet behind anything at Flutter/Compose scale.
  Coverage of our `DrawOp` set: rects/rounded rects/paths/images/
  positioned-glyph text — **yes** (its glyph API consumes exactly the
  pre-shaped positioned runs our `TextService` produces); clips and
  opacity/blend layers — **yes** (scene layer stack). **Blur/backdrop
  filters are partial/immature** → negotiated through `Caps` (locked
  decision), with graceful degradation rather than a blocker. Damage
  tracking stays ours: Vello renders a submitted scene fully per frame;
  `FramePlan`'s damage discipline limits *ops submitted*, not pixels
  touched — acceptable for v1 perf targets, noted as a v2 refinement area.
- *Costs to name:* driver coverage on weak/mobile GPUs is the known weak
  spot (GLES 3.1-class devices vary); frame-latency characteristics of a
  compute pipeline differ from Skia's; the AA/tessellation path is still
  receiving correctness work.

**Skia via `skia-safe`:**

- *What it buys:* it handles our entire `DrawOp` enum today, including
  blur/backdrop/filters, with unmatched robustness across drivers and a
  decade of glyph/edge-case fixes. Zero maturity risk.
- *What it costs against our priorities:* a C++ FFI boundary in a
  pure-Rust core (debugging, ABI management, version churn), a heavy C++
  build injected into CI for **every** target, ~2–6 MB binary on desktop,
  and on wasm a completely different, much heavier artifact class
  (CanvasKit-style trimmed builds) — for a platform where our v1 backend is
  DOM and doesn't use a rasterizer at all.

**Resolution — with corrected attribution (R5; decision unchanged):** the
deciding work here was done by **ergonomics (#2) and binary size (#3)**,
not by perf #1. Perf's contribution was **routing, not merit**: Vello was
accepted as merely *adequate* on desktop GPU, and Skia's real advantages —
driver robustness on weak GPUs, mature filter paths — were routed out of
scope (web = DOM, no rasterizer; hostile Android GPUs → Caps-negotiated
CPU fallback) rather than outperformed. What perf #1 would have bought
outright — Skia's robustness on exactly the hostile mobile GPUs where
rendering performance bites hardest — is what was re-scoped into the
full-scene tiny-skia fallback, which is unvalidated at mobile resolutions
and therefore a re-testable bet (§8.7). Stated plainly: **we chose
pure-Rust toolchain ergonomics and smaller binaries over rendering
robustness on hostile GPUs.** On the merits within R4's own scope, #2 and
#3 decided; perf #1's force had already been spent by R1's platform
carve-outs. Vello v1 stands on that basis; the SkiaBackend escape hatch
(~2–4 weeks, below) and §8.7's watch items are where this trade gets
re-tested if the bet fails.

**Phased path, stated explicitly:** the phase split is *not* "Skia v1 →
Vello v2" (migrating backends twice is waste, and Skia's C++ toolchain
would contaminate CI from day one). It is: **Vello on desktop GPU in v1;
Android ships Vello via wgpu on GLES 3.1-class drivers with a tiny-skia CPU
backend as the Cap-negotiated fallback; the custom GPU pipeline remains v2.**
Skia stays the documented escape hatch — if Vello hits a hard wall, a
`SkiaBackend` implementing the same contract is a weeks-scale backend
project, never a core change.

## 6.2 What this decision does NOT block

The renderer contract already isolates it behind an interface:
`RendererBackend`, `FramePlan`, `Caps`, plus `PlatformShell`/`TextService`
as separate traits. Getting the rasterizer choice wrong costs a **backend
rewrite (~2–4 weeks per backend)**, not a core rewrite. Specifically, this
decision does not block or invalidate:

- the component model, reconciler, and scheduler (§2, §4, §5);
- layout ownership (the engine feeds layout boxes to *any* presenter);
- the text story (`TextService` pre-dates this choice; the display list
  carries shaped runs, so the rasterizer only fills a glyph atlas);
- the DOM web backend (never had a rasterizer dependency);
- the a11y pipeline (SemanticsDiff flows through `PlatformShell`);
- hot reload (the rasterizer lives below `FramePlan`, outside the hot
  boundary).

---

# 7. Locked decisions (consolidated)

**Locked:**

1. Platforms: Windows, Linux, Android, Web. Priorities: perf, ergonomics,
   size, hot reload — in that order. *(Charter, R1)*
2. Web = DOM renderer; we own the scene schema, the browser owns pixels.
   Parity caveats accepted for v1. *(R1)*
3. Accessibility/semantic tree lives in the retained node, diffed and
   shipped in commits from day one. *(R1)*
4. Retained declarative scene graph + fine-grained reactive model;
   immediate at the submit level only. Two trees, not three. *(R1)*
5. Renderer contract = `TreeDiff` + `FramePlan` + `Caps` +
   `PlatformShell` + `TextService`; presenters are diff-driven and hold no
   application/UI-model state, but are **not "stateless"** — they own
   backend-mechanism state (DOM nodes, glyph atlases, layer caches; on
   Web, browser-hosted scroll state (§9.3) and editing sessions). Reason:
   the M1 spike confirmed it — under verdict (b) the Web presenter owns
   editing sessions (browser caret/selection/IME/undo for editable
   fields) on top of browser-hosted scroll state and browser-laid-out
   text. Amendment to the R5 hedged wording ("pending the §9.2 spike,
   possibly editing sessions" — confirmed, hedge dropped); origin:
   spike/REPORT.md §5, verdict (b). *(R1; "mostly stateless" amended
   R5; "possibly" dropped M1)*
6. Layout engine is framework-owned, written once; renderers never compute
   layout. v1 scope: flexbox subset + block-lite. *(R1)*
7. One normalized `InputEvent` enum; framework-side hit-testing on GPU
   targets; browser events mapped into the same enum on Web. *(R1)*
8. No CSS cascade: typed style structs + interned `StyleId` + token-table
   themes; small explicit inheritance set. *(R1)*
9. Reactive core = five primitives (`Signal`, `Memo`, `Effect`,
   `BatchGuard`, `untrack`); exact state APIs beyond these deferred. *(R1)*
10. Language = Rust for core and components. *(R2)*
11. Handlers-as-ids; handlers may only capture signals/ids, never tree
    references. Signals are generational slots; nodes live in a
    generation-checked arena. *(R2)*
12. Components are thin functions over signals, keyed explicitly; per-item
    transient state uses `keyed_state`. *(R2)*
13. Virtualization is model-level (`ScrollArea` + spacer + slot keys);
    recycling is reconciler reuse, fixed-height rows in v1. *(R2)*
14. Hot-reload harness = manifest-scan dylib swap; opaque
    generation-tagged props with drain-before-unload. *(R3)*
15. RELOAD is a scheduler phase: applies between INPUT and EFFECTS in the
    frame loop (full scheduler: §9.1); global apply, atomic registry flip.
    *(R3; the R3 seven-word phase list is superseded by the R5 scheduler
    lock)*
16. Android hot reload is restart-only in v1; stated, not implied. *(R3)*
17. Rasterizer v1 = Vello on desktop GPU; CPU (tiny-skia) fallback for
    hostile targets, Caps-negotiated; Skia is the escape hatch, not the
    default. *(R4; attribution corrected R5 — §9.5, decision unchanged)*
18. Frame loop = `TIME → INPUT → RELOAD → EFFECTS → LAYOUT →
    PAINT/COMMIT → A11Y`, on-demand (event/animation/reload-driven; vsync
    while animating). TIME is the only clock: it services transition
    interpolation (GPU) and scroll physics. On DOM, transitions compile
    to CSS transitions; the browser is the compositor. *(R5, §9.1)*
19. Propagation: topological by dependency depth, one run per node per
    pass, ties by call-site-stable creation order; writes during a run
    fold in (downstream not yet run) or schedule a re-entry pass
    (downstream already ran); **budget = 3 passes/frame — past it, the
    unsettled set is a reported bug (debug-assert with cycle path;
    rate-limited release log + deferred dirt), never a silent livelock**.
    Memos: lazily marked, recomputed in topo order in EFFECTS, reads
    outside EFFECTS pull-recompute (tracked); structural `PartialEq`
    equality gate before downstream propagation (`memo_with_eq` escape);
    memos never write. No user code mid-LAYOUT/PAINT; layout feedback is
    one frame delayed by design. *(R5, §9.1)*
20. Threading: single UI thread owns the entire reactive pipeline
    (`!Send` signals); workers are framework-owned (image decode, glyph
    atlas, wgpu submission, async executor) and hand results back via a
    queue drained at INPUT. Named costs: no parallel layout/reconcile;
    `ExternalTexture` via latest-wins mailbox, not shared memory.
    Subtree-parallel layout is a measured, v2-only escape hatch behind
    unchanged phases. *(R5, §9.1)*
21. Pacing: one global state boundary (RELOAD stays global apply);
    per-surface atomic commit/present on each surface's own cadence;
    unchanged surfaces skip commit; cross-surface skew ≤ 1 frame; no
    cross-window vsync alignment in v1. *(R5, §9.1)*
22. Transitions remain pure style-delta interpolation; deltas produced by
    a re-run triggered through a **binding edge** (`ctx.binding` — e.g.
    a virtualized slot's item memo changing identity) carry
    `suppress_transitions` for exactly that one commit: values jump, no
    interpolator. Coincident real changes under the same stamp are
    suppressed with it — accepted for v1. *(R5, §9.4)*
23. Web scroll = native browser scrolling (overflow container + spacer);
    the `offset` signal is fed from scroll events at the INPUT boundary;
    synthesized scrolling is rejected. "Physics normalized away" is
    scoped to GPU backends; on Web the browser owns feel (CSS-tunable)
    and the offset signal may trail the browser by ≤ 1 frame
    (overscan + `overflow-anchor: none` compensate). *(R5, §9.3)*
24. Text editing: authority model (a) vs (b) was **decided by the §9.2
    spike — verdict (b) on Web, adopted as locked #27 (origin:
    spike/REPORT.md §5)**. Settled now: no new `Tag` (editable
    fields = behavior flag + framework editing-session service built on
    the five primitives); content value = author-owned signal
    (controlled pattern); caret/selection/composition/undo = core-side
    editing session surviving hot swap; undo = minimal per-field stack in
    v1, global undo v2; text-edit a11y (role, value, selection,
    composition, announced edits) required from day one in
    `SemanticsDiff`. GPU backends own editing regardless of the spike.
    *(R5, §9.2)*
25. State residence across the hot/cold boundary: everything that must
    survive a swap lives core-side (`Store<T>`, `image_cache`, signals,
    `keyed_state`); hot crates hold no surviving state (crate-level
    lint); user futures run on the framework executor under the handler
    capture rule, are generation-tagged, and are cancelled at the RELOAD
    drain with retired-generation queue results discarded. *(R5 addendum,
    §9.6)*
26. Memo writes are a hard failure in **all** profiles, not
    debug-assert-only: a memo writing a signal (or creating an effect)
    panics in release builds too. Reason: a silently-dropped write would
    corrupt application state invisibly in the field; the crash message
    ("memos never write signals (§9.1 hard invariant)") is already the
    exact diagnostic. Amendment to #19/§9.1's memo rule; origin: STATE.md
    §5, decision 3 (M0 implementation review, closed). *(M0 review)*
27. Text-editing authority on Web is **(b) — presenter-owned editing**
    (the M1 spike's verdict, argued from its measurements; origin:
    spike/REPORT.md §5). The DOM backend owns editing authority
    (caret/selection/IME/undo) for editable fields — the renderer
    contract's second text path, written first-class in §2.3; the
    framework guarantees **behavior** through the shared
    editing-operation suite, which is the **permanent cross-backend
    contract test**, kept runnable as backends land (M2's editing
    session and M3's DOM text path both consume it). Adopted with the
    verdict as spec, not spike findings: editing sessions **commit
    in-progress composition on focus loss** (REPORT.md finding #6);
    **mid-cluster hit-test ties resolve to the leading edge** and
    **double-click word selection adopts browser-compatible conventions
    including CJK dictionary segmentation** (REPORT.md findings #1/#2)
    — the Windows-GPU session must replicate these conventions to stay
    behaviorally consistent under the shared suite; that scope is not
    only the DOM arm's. Framework-measured tracked text is never
    delegated to CSS `letter-spacing` (one-unit trailing-edge
    divergence — REPORT.md §5). The DOM text/editing contract's freeze
    is **gated**, not automatic (blocking conditions in §2.3).
    *(M1 spike merge; resolves #24's deferred (a)/(b) and #5's hedge)*
28. DOM text/editing contract freeze, blocking condition (a), is
    **closed**: the delete-range-mid-composition path is verified
    against the real OS IME — two consecutive hands-off PASS runs
    (6/6 checks, zero divergences) of the real-Pinyin scenario through
    PlatformShell's own TSF window + text store (origin:
    IME-SESSION.md §§3/7 + the repetition run in ROUNDS.md's
    verification entry). Blocking condition (b) (RTL/bidi +
    combining-marks/ZWJ corpus) remains the only open freeze condition.
    *(IME verification round)*
29. DOM text/editing contract freeze, blocking condition (b),
    **partially closed**: combining-mark cluster parity (decomposed
    e-acute, U+0065 U+0301, shapes one cluster — identical granularity
    to precomposed U+00E9) and ZWJ single-cluster geometry/hit-test
    (the 11-byte U+1F469 U+200D U+1F4BB span is one cluster; zero
    cross-backend sweep mismatches) verified on the Windows-GPU and
    Web-DOM arms (origin: spike corpus rig v2 + this round's ROUNDS
    entry). Bidi visual ordering is re-deferred to the M3 layout
    engine (measured 65 device-px c1 / 60-probe c2 divergence on the
    Latin+Arabic+digits string — the deferred work reasserting itself,
    not a new problem; the rtl flag itself is now unit-pinned on the
    Arabic span). Word-segmentation rules incl. ZWJ-emoji words and
    scalar caret-stepping through combining clusters tracked as
    shared-suite spec items for the M2 editing session.
    *(bidi/combining/ZWJ round)*

**Explicitly deferred to v2:**

- Custom GPU rasterizer (replaces Vello behind the same `FramePlan`; the
  contract already allows it). *(R1, reaffirmed R4)*
- Native-hybrid presenters on desktop (embedding HWNDs/X11 windows) —
  escape hatch, not a v1 path. *(R1)*
- Animation model beyond the `.transition(...)` primitive (tween DSL,
  implicit-vs-explicit animations). *(R1; surfaced by R2 examples)*
- Text-stack consolidation (one bundled cross-OS shaping stack vs.
  per-OS services). *(R1)*
- Wasm-hosted component runtime for Android hot-reload parity (wasmi
  path). *(R3)*
- Grid layout, variable-height list rows (prefix-sum index),
  multi-window/per-surface reload granularity. *(R1/R2/R3 scope cuts)*

---

# 8. Known gaps / follow-ups

Flagged across the rounds but not yet resolved; each needs an owner before
the corresponding subsystem freezes:

1. **Signal-reseeding must become an enforced invariant, not a documented
   paragraph.** The "inserting a signal re-seeds subsequent signal state"
   rule (§5.1) currently lives in prose. Requirement: debug-build
   verification that re-seeding is *detected* — call-site hash/ordinal
   mismatch at reload → debug-assert or warn-with-source-location, and a
   clippy-style lint flagging positional signal creation without a stable
   label where re-seeding is likely to be observed. *(Raised R3)*
2. **Text stack implementations.** The `TextService` interface is locked;
   the per-OS implementations are not built: DirectWrite (Windows),
   HarfBuzz-class stack (Linux), platform APIs (Android). Shaping, font
   fallback, BiDi, emoji, IME composition, line breaking, subpixel/DPR
   rounding all live here. Must exist before the first rectangle draws.
   *(R1 risk #1)*
3. **Linux accessibility (AT-SPI over DBus)** is the highest-risk a11y
   target; incremental sync semantics, focus tracking, and role mapping
   from the closed-set tags are designed but not validated on Linux.
   *(R1 risk #2)*
4. **Reload-during-{scroll, layout, input-burst} stress fuzzer** asserting
   no generational slot is touched after its generation retires — the
   actual proof that identity churn is closed. Before renderers freeze.
   Extended R5: also asserts no retired-generation task result is applied
   and no cancelled future is resumed after unload (§9.6). *(R3, R5)*
5. **Web text/layout round-trip.** v1 constraint: flat flexbox /
   text-in-flex on Web; engine layout for everything + browser-as-display
   is a research item. Parity caveat must be documented for users. *(R1
   risk #5)*
6. **Scroll physics normalization and platform affordances** (clamping vs.
   overscroll glow, scrollbars, scroll anchoring) — spec'd in the model,
   not implemented; the reputational-perf battleground. *(R1 risk #4)*
7. **Vello maturity watch.** Caps-gate blur/backdrop until filters mature;
   validate driver coverage on GLES 3.1-class hardware early; keep the
   `SkiaBackend` escape hatch costed (~2–4 weeks) and re-evaluate only on
   a hard wall. *(R4)*
8. **DPR/rounding determinism across backends** — layout must round
   identically on all presenters or hit-testing/a11y bounds drift;
   untested. *(R1, implicit)*
9. **Image/video stack and `ExternalTexture` plumbing** — named in the
   contract, unscoped. *(R1)*
10. **Text-editing spike (§9.2) — the gate for the editing contract:
    PASSED (M1); verdict (b) on Web merged as locked #27.** One editable
    single-line field, IME (zh Pinyin + ja Romaji) and mouse selection,
    on Windows-GPU and Web-DOM against explicit pass/fail criteria
    (§9.2); the spike ran, every mismatch was classified non-fundamental
    (REPORT.md §4), and its verdict is merged (§2.3 clause, §9.2).
    §8.2's implementation work proceeds independently beneath it.
    Residuals carried as **tracked items, not resolved by the merge**:
    variant A on Web's fidelity remains unmeasured (moot under (b));
    the manual real-IME delete-range pass is outstanding and blocks the
    DOM text/editing contract freeze (§2.3), as does RTL/bidi +
    combining-marks/ZWJ corpus coverage or an explicit re-deferral with
    a named owner/milestone. *(R5; gate passed, verdict merged M1;
    the pass was attempted in the M1 remainder round and did not
    complete — the automation rig could not engage the real OS IME's
    composition engine on a plain Win32 window, evidence in
    `spike/results/ime_manual.json`; the gate stays open, with the
    TSF-aware window (document manager + context + input scope)
    documented as the engagement route)*

---

# 9. Round 5 — stress-test resolutions (R5)

Resolves the five findings accepted from the adversarial stress test of
the closed v1 document. Later items below build on earlier ones; nothing
outside them is re-opened. §9.6 is a follow-up addendum closing one
further stress-test finding (state residence across the hot/cold
boundary) against the infrastructure §9.1 created, plus the threading
lock's revisit tripwire.

## 9.1 Scheduler & threading model

Locked #15 named a phase list, not a scheduler. This subsection is the
scheduler lock; §5.3 keeps only the RELOAD-specific mechanics. The five
locked reactive primitives (§7.9) get their *semantics* here, because
every other subsystem's correctness argument is phrased in them.

**Frame loop (single UI thread, on-demand):**

```
TIME → INPUT → RELOAD → EFFECTS → LAYOUT → PAINT/COMMIT (per surface) → A11Y
```

A frame runs only when needed: input arrived, an animation or physics
simulation is active, a reload landed, or `request_frame` was called.
While the animation set is non-empty the shell schedules at vsync cadence;
when it drains the loop idles (static UI ≈ 0 CPU, unchanged).

**Phase semantics:**

1. **TIME — the clock driver.** Advances the monotonic animation clock to
   this frame's timestamp and services every active animation: transition
   interpolators, scroll physics (decay, overscroll), and the v2 tween
   DSL when it arrives. Animations write interpolated values through the
   same signal/style-delta machinery as everything else; no user code
   runs in this phase. This phase is what makes "animated transitions"
   honest on GPU backends, where the compositor is us; on DOM,
   `.transition(...)` compiles to CSS transitions and the *browser*
   animates — the same backend-mapping rule as everything else, and the
   reason v1's animatable-property and easing subsets are the
   CSS-expressible ones (properties outside the subset jump).
2. **INPUT.** Pump events, hit-test (GPU) or map browser events (Web),
   dispatch to handlers; handlers write signals (batched). Out-of-band
   platform producers enter here too: Web native scroll feeds the
   `offset` signal at this boundary (§9.3), and async worker results are
   drained here (threading, below).
3. **RELOAD.** Unchanged (§5.3): drain → unload → rescan → re-run.
4. **EFFECTS.** The propagation fixpoint: re-run invalidated memos,
   effects, and components (component re-runs *are* effects); diff VNode
   output; reconcile the retained tree; emit `TreeDiff`s; set
   pass-dirty flags.
5. **LAYOUT.** Measure/position dirty subtrees; never runs user code
   (`TextService` measurement only); may dirty PAINT/TEXT/SEMANTICS
   downstream of the nodes it moved.
6. **PAINT/COMMIT.** Build `FramePlan`s from dirty subtrees; commit per
   surface (pacing below).
7. **A11Y.** Semantic-tree diff → `PlatformShell`; bounds from the layout
   just committed.

**Propagation contract (what locked #15 left unspecified):**

- **Order.** The dependency graph is processed **topologically by
  dependency depth**: a memo/effect/component runs only after everything
  it depends on has produced its value for this pass. Ties break by
  creation order (call-site-stable, so ordering is deterministic across
  hot reloads). Each node runs **at most once per pass** — no glitch
  storms; downstream always sees settled values.
- **Writes during a run are legal.** A write whose downstream nodes have
  not yet run this pass folds into the current pass. A write whose
  downstream nodes already ran (or which dirties the running node's own
  dependencies) schedules a **re-entry pass** instead of reordering live.
- **Re-entry budget: 3 passes per frame** (initial + 2 re-entry). Past
  the budget, unsettled propagation is **a bug, not a feature**: debug
  builds hard-assert and print the dependency cycle; release builds
  rate-limit-log the cycle signature and defer the remaining dirt to the
  next frame's EFFECTS (same budget). The visible failure mode of a true
  cycle is "this subtree stops updating and the log says why" — never a
  torn tree, never silent livelock.
- **Memos: lazily marked, settled in EFFECTS.** A dependency write marks
  dependents dirty (no recomputation); the scheduler recomputes dirty
  memos in topological order at the start of EFFECTS. A read of a still-
  dirty memo from an event handler (INPUT) pull-recomputes on demand,
  tracked normally — its downstream propagation still happens in EFFECTS.
- **Equality gate before propagation: structural `PartialEq` by
  default.** A memo whose recomputed value equals its previous value does
  not invalidate dependents. Pointer identity alone is meaningless here
  (values are often fresh allocations), so structural equality is the
  default; `memo_with_eq(f, cmp)` is the per-memo escape hatch for
  expensive or intentionally reference-semantic comparisons.
- **Memos never write signals** (debug-assert; that is what Effects are
  for) — this is what keeps re-entry bounded.
- **No user code runs mid-LAYOUT or mid-PAINT** — the §4.1 safety
  invariant, now guaranteed by phase construction rather than asserted.
  Layout feedback is **one frame delayed, by design**: an effect reading
  an element's settled metrics creates a LAYOUT-phase dependency and
  re-runs in the *next* frame's EFFECTS; effects may not write what
  layout reads (no measure→write→measure loop inside a frame).
- **`BatchGuard`** scopes writes into one invalidation application at the
  end of the batch (nested batches merge). It changes when propagation is
  *scheduled*, never the ordering rules; a batch opened in INPUT closes
  before EFFECTS of the same frame — input → visual response completes
  within one frame whenever no animation or async work intervenes.

**Threading — locked: single UI thread; workers below named boundaries.**

- The UI thread owns the entire reactive pipeline: signals, memos,
  effects, components, reconcile, layout, hit-testing, semantic tree,
  `FramePlan`s. Reactive types are `!Send` — load-bearing, not a
  limitation: it is why handlers may capture signals without locks and
  why generational slots need no synchronization.
- **Workers are framework-owned:** image decode, glyph rasterization and
  atlas upload, wgpu/Vello submission & present (driver threads), and the
  async executor running user futures (`image_cache.load` et al.).
  **Handoff rule:** worker results enter the UI thread only via a queue
  drained at the INPUT boundary; the drain step performs the signal
  writes *on* the UI thread. A worker never touches the reactive graph;
  a future is dropped when its owning `keyed_state` entry is evicted —
  that drop is its cancellation story. Task capture discipline and
  unload safety across hot reload: §9.6.
- **Costs, named honestly:** independent subtrees do not parallelize
  layout or reconcile; `ExternalTexture` producers (video) hand frames
  through a latest-wins mailbox into the queue — not shared memory into
  the scene graph. Consistency with perf #1: the dominant frame costs
  (GPU rasterization, DOM mutation) are off-thread by construction
  (wgpu/Vello's own queues; the browser), and reactive work is O(dirty),
  not O(tree) — single-threaded does not contradict #1 as scoped. The
  escape hatch — subtree-parallel layout and parallel paint-list
  building, both pure functions addable inside their existing phases
  without model changes — stays a measured, v2-only option.
  **Revisit tripwire (the same treatment §8.7 gives the Vello bet):**
  the lock reopens only on measured evidence — frame profiles on
  shipping-class content showing serial core work (layout + reconcile +
  paint-plan) exceeding ~⅓ of the frame budget at 60 Hz on mid-tier
  target hardware, or an `ExternalTexture` producer that cannot sustain
  its frame rate through the mailbox. Ergonomic friction from `!Send`
  signals is not a trigger; it is the price the lock pays for lock-free
  handler capture.

**Multi-surface pacing — locked: global state boundary, per-surface
present.** All surfaces in a frame are built from the same
post-EFFECTS/post-LAYOUT tree; there is one global boundary for *state*
(RELOAD stays global apply, §5.3). **Commit/present is per-surface:**
each surface's backend commits its `FramePlan` and presents on its own
display's cadence; unchanged surfaces skip commit entirely. Guarantees:
(a) no surface ever contains a mix of two logical frames (per-surface
commit is atomic); (b) cross-surface skew is bounded at one frame and
only observable when two windows animate simultaneously on
different-cadence displays. No cross-window vsync alignment in v1.

## 9.2 Text editing & authority model

Editing was a **model gap**, resolved empirically by the M1 spike
(spike/REPORT.md). What settles here is everything decidable without
running code, plus the spike's precise spec — and now the resolution.

**Resolution (M1 spike ran; verdict adopted).** The spike ran on two
arms (Windows-GPU framework-authority session; Web-DOM real `<input>`;
variant A on Web was **not built** — round scope — and its fidelity
remains unmeasured, tracked in §8.10). Verdict: **(b) on Web**, argued
from the criteria in REPORT.md §5 and not re-argued or re-opened here;
adopted as locked **#27**. The merge that made it normative did exactly
the handoff REPORT.md §11 named: locked #5 amended (hedge dropped),
§2.3's second-text-path clause written with its two permanent contract
rules, and the two contract-alignment items adopted below
(commit-on-focus-loss; shared-suite word/undo rules). The DOM text
path's freeze is **gated**, not automatic (§2.3). The (a)/(b) question
is closed.

**Settled now:**

- **No new `Tag`.** An editable field is a component composed of existing
  tags (`Text`, clip, bg), gated by a **behavior flag**
  (`BehaviorProps.editable`). The presenter-facing signal is the flag
  plus the semantics payload, not a new node kind — the closed-set Tag
  rule and the hot-reload "new variant = restart" rule stay untouched.
- **State ownership (controlled-component pattern).** The field's
  *content* lives in an author-owned signal; the editing machinery
  writes it back through that signal on every edit. Caret position,
  selection, IME composition buffer, and undo history are an **editing
  session**: framework-owned, keyed to the focused node, **core-side**
  (generational, survives hot swap exactly like signals — losing a
  half-composed IME buffer on every reload would make fields untestable
  during iteration). At most one active session (the focused field). The
  session API is a **framework service over the five locked primitives**
  (`ctx.edit_session(...)`, the way `ctx.hovered()` is) — not a sixth
  primitive; locked #9 stays as written. **On focus loss
  mid-composition the session commits** the in-progress composition
  (adopted as spec, M1: every native platform commits on blur —
  REPORT.md finding #6; the spike session's cancel-on-blur was the
  outlier, not Chromium).
- **Undo:** v1 ships the **minimal per-field stack** inside the editing
  session (insert/delete runs coalesced, IME composition commits as
  atomic units, bounded depth). Global/cross-field undo management is
  explicitly **v2**.
- **Editing-session behavior contract (adopted M1, from the spike).**
  The shared editing-operation suite — the spike's criterion-4 rig
  (`spike/corpus.json` op suites + cluster tables, driven by
  `spike_win_arm` + `harness.mjs` + `compare.mjs`; REPORT.md §11) — is
  the **permanent cross-backend contract test**, kept runnable as
  backends land (M2's editing session and M3's DOM text path both
  consume it). Its rules are spec, not spike findings: **mid-cluster
  hit-test ties resolve to the leading edge** (REPORT.md finding #1;
  not Chromium's from-mid+1), and **double-click word selection adopts
  browser-compatible conventions, including CJK dictionary
  segmentation** (REPORT.md finding #2). Scope named explicitly: the
  **Windows-GPU session must replicate these conventions** to stay
  behaviorally consistent under the shared suite — behavioral
  consistency is not only the DOM arm's job; it creates real scope on
  the GPU session.
- **A11y (required from day one, per locked #3).** `SemanticsDiff` gains
  a text-edit payload: role `TextField` (UIA `Edit`/`TextPattern`-class,
  AT-SPI `EditableText`, DOM `textbox` role), live **value**, **selection
  range**, and **composition state** (composing string + caret), plus
  selection/IME **change events** so platforms can announce edits.
  Announced composition and selection edits are a v1 contract
  requirement, not a retrofit. Caret-rect exposure doubles as the IME
  anchoring input (`PlatformShell::set_ime`).
- **GPU backends own editing regardless of the spike's outcome** — there
  is no OS text widget to delegate to on a GPU surface; native-hybrid
  embedding stays the documented escape hatch (§7 deferred). The editing
  model therefore exists on GPU in v1 in any case; the spike decided the
  Web side — verdict (b): Web keeps the browser-native mechanism
  (§2.3's second text path).

**The two candidate authority models (what the spike distinguishes):**

- **(a) Framework-owned authority, uniform mechanism.** The framework
  computes glyph runs, caret rects, selection rects, and the
  text-index ↔ geometry mapping on *every* backend; presenters only
  rasterize what they are given (caret = 1-px rect op, selection = rect
  ops, composition text = a styled run). On Web this requires DOM to
  **give up**: native `<input>` caret, native selection, native IME
  anchoring, native undo/autofill. A hidden, visually-neutral `<input>`
  serves only as focus/IME *event source*; we render composition text
  and anchor candidate windows from our own rects. This is Flutter-web's
  path; its known costs (candidate anchoring fidelity, hit-test parity,
  a11y through ARIA instead of native input a11y) are exactly what the
  spike measures.
- **(b) Presenter-owned editing, framework-guaranteed behavior.**
  Editable fields are a **special-cased component type per backend**:
  Web materializes them as real `<input>`/`<textarea>` (browser owns
  caret, selection, IME, undo); GPU backends run the framework editing
  model. The cross-backend contract guarantees **behavior** — validated
  by one shared editing-operation test suite run against both
  mechanisms — not a uniform mechanism. Costs: two editing
  implementations kept behaviorally in sync, and — stated now, not
  after the spike — this completes the case that **presenters are not
  stateless** (locked #5 amended accordingly): the Web presenter would
  own editing sessions, browser-hosted scroll state (§9.3), and
  browser-laid-out text.

**The spike (one week; gates all further text-editing architecture):**

*Scope:* one editable single-line `TextField`: IME composition (zh
Pinyin + ja Romaji) and mouse-driven selection; implemented on
**Windows-GPU** (our pipeline; IME via `PlatformShell::set_ime`) and
**Web-DOM** in *both* variants (A: framework authority over a hidden
input; B: real `<input>`). Prerequisite: the minimal DirectWrite
shaping/measurement slice (first cut of §8.2) — the spike is also the
first real consumer of the measure ↔ layout protocol.

*Pass/fail criteria:*

1. **IME geometry.** For a fixed test set (≥ 4 sentences per language,
   including mixed Latin/CJK): framework-computed caret rects must
   position the platform candidate window within **one caret height** of
   the composition caret, tracked through composition edits and
   in-composition arrow navigation, on Windows and (variant A) Web. On
   Windows, additionally cross-checked against a native edit control
   with identical font/size/DPR: caret x at each index within ±2 device
   px and never across a cluster boundary.
2. **Hit-test parity.** Click-to-index and drag-selection endpoints over
   a 20-string corpus (CJK, Latin, combining marks, ZWJ emoji) must
   resolve to the **same grapheme-cluster index on both backends** given
   the same rendered geometry — zero mismatches.
3. **Composition event fidelity.** The begin/update/commit/delete-range
   event streams for scripted sequences (candidate selection,
   in-composition arrows, cancel mid-composition, focus change
   mid-composition, rapid zh↔ja switching) must match the editing
   model's expectations with no lost or duplicated characters, on both
   arms.
4. **One model.** Both arms pass the same shared editing-operation suite
   (insert/delete/move/select/undo) — proving the *model*, not the
   mechanism, is shared.

*Decision rule:* if variant A meets the criteria on Web, **(a) wins
globally** — one mechanism, presenters stay rasterizers. If A fails the
geometry or parity criteria that B passes, **(b) wins on Web**: editing
authority is presenter-owned, the editable field is documented as a
presenter-recognized special case, and the renderer contract gains an
explicit second text path — written as a first-class contract clause,
not as the parenthetical it was before this round. GPU backends are
unaffected either way.

## 9.3 Web scroll ownership

**Decision: native browser scrolling on Web.** `ScrollArea` materializes
as an `overflow` container whose spacer defines extent; the browser
compositor scrolls it; framework code never scrolls the main thread.

**Justification, with the deciding priority named.** This is **perf #1**
deciding — by the same routing shape the stress test exposed in §6: the
browser's compositor-driven scrolling is faster than anything reachable
from main-thread wasm, and synthesizing scroll would spend wasm budget
re-buying the one platform behavior the DOM lock (R1 constraint 1) was
partly chosen to inherit. The honest corollary: this decision's costs
are **guarantee regressions, not perf regressions**, and they are paid
explicitly here:

- **"Platform physics normalized away" (§2.2) is scoped to GPU
  backends.** On Web, scroll physics *is* the browser's: our physics
  layer (decay curves, overscroll behavior, §8.6 affordances) is
  bypassed; what remains controllable is the CSS-level surface
  (scrollbar styling via tokens, `overscroll-behavior`,
  `scrollbar-gutter`). §8.6's "reputational battleground" is a
  GPU-backend battleground; on Web it becomes CSS tuning plus documented
  parity caveats.
- **"Offset never torn" is full-strength on GPU backends only.** The
  browser scrolls asynchronously and *then* we learn about it: the
  browser scroll event is mapped into the framework-owned `offset`
  signal **at the INPUT phase boundary** (§9.1) — an out-of-band
  producer that enters through the same event gate as every other
  platform event, so the scheduler's phase ordering holds by
  construction. The offset signal can therefore trail the browser's
  visual scroll position by up to one frame; within a frame's commit it
  is never torn. The virtualization consequence is **window lag, not
  tearing**: cells are placed from last-known offset, so Web uses a
  larger overscan constant (v1: +4 slots), and browser scroll anchoring
  is **disabled inside recycled lists** (`overflow-anchor: none` on slot
  nodes — otherwise browser anchoring fights our own spacer
  repositioning). Hit-testing during scroll needs no offset at all on
  Web (the browser hit-tests; we map the event) — which is where the lag
  would otherwise bite hardest.
- **`ctx.scroll_offset()` keeps identical semantics on all backends**
  (read it, derive windows from it); only the feeding mechanism differs
  (GPU: TIME-phase physics writes; Web: INPUT-phase event mapping). That
  is the property virtualization actually depends on.

## 9.4 Transition × recycle-rebind semantics

**The bug, stated:** slot-keyed recycling means a retained cell keeps its
`NodeId` across item swaps; the reconciler sees "same node, `StyleId`
changed" and cannot distinguish *the slot re-bound to a different item*
from *state changed on this item*. With a 120 ms bg transition, every
recycled cell phantom-animates from the previous item's state — the
design as locked reproduced the classic recycled-list flash bug by
construction.

**Mechanism — the binding-edge stamp (locked):**

- `ctx.binding(f)` is a memo variant whose *value change* is an
  **identity event**. Mechanically: the scheduler knows each
  component/effect re-run's trigger set (its dirty dependencies); if a
  trigger edge is a binding edge, the style deltas that run produces for
  this commit are stamped.
- A stamped delta carries `suppress_transitions: true` for **exactly one
  commit** in the `TreeDiff` payload. The transition evaluator — the
  TIME-phase interpolator on GPU; the CSS-transitions mapping on DOM,
  which writes the target values with transitions disabled for that
  commit — applies the delta **instantly**: the value jumps, no
  interpolator is created.
- `ScrollArea` slots bind via `ctx.binding` (§4.2's example updated);
  the stamp covers the re-run component's emitted subtree for that
  commit.

**What this does not change:** transitions are still exactly "interpolate
style delta A→B per property over a duration." The stamp is metadata on
the delta — a scheduling decision about whether a delta is *eligible* for
interpolation this commit — not a second transition system, not
per-component animation code, not a change to the declarative
`.transition(...)` primitive. Locked #4/#12's simplicity stands.

**Accepted limit (v1):** suppression is per-commit, not per-cause. If a
real state change coincides with a rebind in the same commit on the same
node, it is suppressed too (the change jumps). Distinguishing the two
would require per-value provenance — deferred unless measurement shows
it matters.

## 9.5 Rasterizer resolution — corrected attribution

Locked #17 stands unchanged; this subsection corrects only the
attribution, per the stress test's ordering critique.

The deciding work in R4 was done by **ergonomics (#2) and binary size
(#3)**, not by perf #1. Perf's contribution was **routing, not merit**:
Vello was accepted as merely *adequate* on desktop GPU, and Skia's real
advantages — driver robustness on weak GPUs, mature blur/filter paths —
were routed out of scope (web = DOM, no rasterizer; hostile Android GPUs
→ Caps-negotiated CPU fallback) rather than outperformed. What perf #1
should have bought outright — Skia's robustness on exactly the hostile
mobile GPUs where rendering performance bites hardest — was re-scoped
into the full-scene tiny-skia fallback, which is unvalidated at mobile
resolutions and therefore a re-testable bet (§8.7).

Stated plainly: **we chose pure-Rust toolchain ergonomics and smaller
binaries over rendering robustness on hostile GPUs.** On the merits
within R4's own scope, #2 and #3 decided; perf #1's force had already
been spent by R1's platform carve-outs — R4 is #2 + #3 wearing a #1
costume. The Vello decision remains the right call at v1 scope; the
SkiaBackend escape hatch (~2–4 weeks, §6.1) and §8.7's watch items are
where this trade gets re-tested if the bet fails.

## 9.6 State residence across the hot/cold boundary (R5 addendum)

The stress test's Task 3 item 5 named "the single most likely place the
hot-reload design breaks in practice": locked #14's opaque-props
mechanism covers *props* only, and nothing said where `Store<T>`,
`image_cache`, or in-flight async tasks live. §9.1 supplied the
plumbing — framework-owned executor, INPUT-drained queue — but not the
residence rules. Verdict: §9.1 solved the *transport*, not the
*residence*; three decisions were still missing and are locked here.

- **The residence rule: anything that must survive a swap lives
  core-side, in reactive storage.** `Store<T>` is a core-owned reactive
  collection (generational arena, per-key granular subscriptions — the
  shape the §4.2 trace already assumed), reached through an id-handle
  passed as a prop or captured by handlers; handles are ids, so locked
  #11's capture rule already governs them, and no ambient/provider
  mechanism is added in v1 (the §4.2 example now passes `store`
  explicitly). `image_cache` is a framework service alongside
  `TextService`: core-side, keyed by content address; its decode workers
  are §9.1's workers, and in-flight decodes are generation-independent
  (an image is content, not hot code) — only a component's *await* is
  generation-scoped, and the re-run re-requests. Core-side ⇒ all of it
  survives reload, exactly like signals.
- **Async tasks: framework executor, handler capture rule, generation
  tag.** `ctx.spawn(...)` runs futures on the §9.1 executor under the
  **same capture discipline as handlers** — locked #11 extended:
  captures may be signals/ids only, enforced by the same compile-time
  check. Every task and queue message carries the hot generation that
  spawned it.
- **Unload safety (what opaque props didn't cover).** At the RELOAD
  phase, before unload: all in-flight futures of the outgoing generation
  are **cancelled** — dropped, with drop glue executing via the old
  dylib, exactly like the props drain; any results already queued under
  the retired generation are **discarded** at the next INPUT drain
  (generation check — the same mechanism class as generational slots).
  A worker completing between cancel and unload lands in the queue under
  the retired tag and is dropped, closing the race. In-flight tasks can
  therefore never run against, or touch state from, a half-swapped
  registry — the guarantee §5.3 gives events, extended to tasks.
- **Cost, named:** cancel-at-reload restarts in-flight work on every
  reload during iteration. Accepted for v1; the authoring pattern for
  work that must not be lost is incremental writes through signals at
  await points, which the capture rule supports. Tasks surviving across
  reloads (re-binding a hot-crate async body to the next generation) are
  a v2 concern of the same shape as the props-drain problem — deferred
  unless iteration profiling shows the restart cost hurts.
- **Enforcement:** a crate-level lint shipped with the hot-reload
  authoring macro rejects hot-crate `static` / `thread_local!` /
  `OnceCell` app-state (same mechanism class as the §8.1 reseeding
  verification); §8.4's reload fuzzer extends to the task/message path.

---

## One-sentence summary

**A two-tree reactive model with a single-UI-thread phase scheduler
(TIME-driven animations and scroll physics, RELOAD included, bounded
re-entry), feeding diff-driven per-platform presenters that know about
node-id diffs and display lists — rasterized on desktop by Vello, on web
by the browser (native scroll included), with text and accessibility as
contract-level concerns from day one and text-editing authority settled
(b) on Web by the M1 spike — in Rust, with hot reload where the platform
allows it
and honest restarts where it doesn't.**
