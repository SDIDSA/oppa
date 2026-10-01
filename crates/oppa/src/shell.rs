use crate::clipboard::Clipboard;
use crate::dialog::FileDialog;
use crate::handlers::HandlerId;
use crate::ime::ImeOps;
use crate::style::CursorIcon;

/// Mobile / application lifecycle state (Round 18.3, decision 322).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum AppLifecycleState {
    /// Application is foregrounded, active, and interactive.
    #[default]
    Active,
    /// Application is partially obscured or backgrounded; rendering and tickers suspended.
    Paused,
    /// Application is stopped or suspended, saving state before termination.
    Suspended,
}

/// Normalized event classification (DESIGN §2.2). M0 carries only kind +
/// pre-routed target handler; the full payload-carrying [`InputEvent`]
/// enum (Pointer/Scroll/Key/Ime/Focus) lives in [`crate::input`] since
/// M5 and routes through the host's hit-test — this stays the registry-
/// dispatch seam underneath it (handlers stay ids, ADR-0007).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EventKind {
    Press,
    /// Distinct hold action (OQ-G11-2 closed): fired by the router's
    /// long-press arm when the owner declares `on_long_press`;
    /// owners without one fall back to `Press` (additive — every
    /// pre-OQ control behaves exactly as before).
    LongPress,
    Release,
    PointerMove,
    Key,
    Focus,
    Blur,
    Scroll,
    Ime,
    /// Monitor DPI change (Round 2.4, OQ-G10-2 — shell→runner
    /// plumbing, never an app handler kind: the runner re-bases DPR
    /// and resizes through it).
    DpiChanged,
    /// OS light/dark change (Round 16.2, decision 315 — shell→runner
    /// plumbing, never an app handler kind: the runner re-queries
    /// the system theme and forwards it into `host.set_theme`).
    SystemTheme,
    /// Window close request (Round 16.3, decision 316 — shell→runner
    /// plumbing, never an app handler kind: the runner consults the
    /// loop's close veto through it).
    CloseRequested,
    /// Touch swipe (Round 3.2, OQ-G11-1 — router-recognized fast
    /// far lift, fired on the Down owner's `on_swipe` handler when
    /// declared, quiet otherwise). Never scroll: `Scroll` dispatches
    /// only from `Scroll` input events (wheel).
    Swipe,
    /// Arrow-key press, by direction (Round 5.3, OQ-G2-1 —
    /// payload-less handlers stay payload-less: four kinds instead
    /// of one keyed payload, the Press/LongPress/Swipe precedent).
    /// Fired on the focused owner's directional handler when
    /// declared; otherwise the generic `Key` handler runs (existing
    /// ambient path, unchanged), else quiet.
    KeyLeft,
    KeyUp,
    KeyRight,
    KeyDown,
    /// Pointer drag move (Round 5.3, OQ-G2-1 — fired on every Move
    /// while the node is the pointer's capture owner; the position
    /// rides [`ComponentHost::pointer_position`](crate::component::ComponentHost::pointer_position),
    /// never the closure).
    Drag,
    /// Plain drag release (Round 21.3, decision 330 — a slow far
    /// lift inside its capture owner's subtree that is neither a
    /// tap, a swipe, a hold, nor a scroll-drag: fired on the
    /// capture owner when it declares `on_drag_release`, quiet
    /// otherwise — the Swipe/Drag declared-only precedent, so
    /// every pre-21.3 control behaves exactly as before. The
    /// release point rides
    /// [`ComponentHost::last_drag_release`](crate::component::ComponentHost::last_drag_release),
    /// never the closure; tap chains never see it).
    DragRelease,
    /// Secondary-button tap (Round 9.2, decision 301 — the raw
    /// right-click: fired on the capture owner when it declares
    /// `on_secondary_press`, quiet otherwise, and never a primary
    /// `Press`). Fires alongside [`EventKind::ContextMenu`] when both
    /// are declared (DOM order precedent: raw tap first, menu second).
    SecondaryPress,
    /// Context-menu request (Round 9.2, decision 301 — the semantic
    /// right-click: fired on the capture owner when it declares
    /// `on_context_menu`; the menu primitives of a later round
    /// consume this). Independent of `SecondaryPress` (each fires
    /// when declared — no fallback guessing).
    ContextMenu,
    /// Mobile / app lifecycle event (Round 18.3, decision 322 — shell→runner
    /// plumbing: notifies of pause/resume/save-state transitions).
    Lifecycle,
}

/// One input event, pre-routed to the handler id that receives it. GPU-side
/// hit-testing (M5) is what will produce the routing; the registry contract
/// (handler-as-id) is already M0.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Event {
    pub kind: EventKind,
    pub handler: HandlerId,
}

/// Platform shell seam (DESIGN §2.2). M0 exercises `pump_events`; the IME
/// control surface (`set_ime`) is the §9.2 spike's anchor seam and defaults
/// to a no-op until a backend wires a real IME. The rest of §2.2's trait
/// (`request_frame`, `set_dpi_aware`, `set_cursor`, `semantics`, `text`)
/// grows with the per-platform shells — method set stays additive so the
/// M0 phase loop does not move.
pub trait PlatformShell {
    /// Drain pending platform events. Called once per frame in INPUT.
    fn pump_events(&mut self) -> Vec<Event>;

    /// IME/candidate-window control (device-px caret anchor from
    /// `ShapedRun::caret_rect`).
    fn set_ime(&mut self, _ops: ImeOps) {}

    /// Clipboard backend (G3, decision 210): `None` by default — every
    /// pre-G3 shell keeps compiling and refuses loudly through the
    /// `None` (callers map it to `ClipboardError::Unsupported`). Shells
    /// with a real backend (Win32 this round) return `Some`.
    fn clipboard(&mut self) -> Option<&mut dyn Clipboard> {
        None
    }

    /// File-open dialog backend (G12, decision 231): `None` by default
    /// (same additive shape as the clipboard seam — pre-G12 shells
    /// compile untouched and refuse loudly through `PickError::
    /// Unsupported`). Shells with a real backend (Win32 this round)
    /// return `Some`.
    fn file_dialog(&mut self) -> Option<&mut dyn FileDialog> {
        None
    }

    /// Pointer cursor shape (Round 8.3, decision 299): the runner
    /// resolves the hovered style per pointer move and publishes it
    /// here; the shell applies it at the next OS cursor query
    /// (Win32 `WM_SETCURSOR`, winit `set_cursor`, DOM CSS). Default
    /// no-op (the `set_ime` precedent — cursor is an advisory
    /// presentational hint; headless/test shells have no cursor to
    /// move, and runners pin behavior through `hover_cursor` reads,
    /// never through this call).
    fn set_cursor(&mut self, _cursor: CursorIcon) {}
}
