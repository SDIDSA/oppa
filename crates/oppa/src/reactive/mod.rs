use std::any::Any;
use std::any::TypeId;
use std::cell::RefCell;

use std::marker::PhantomData;
use std::rc::{Rc, Weak};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::arena::GenerationalId;
use crate::clock::{Clock, SystemClock};
use crate::handlers::{HandlerFn, HandlerId};
use crate::worker::{run_task_pump, TaskId, TaskScope, TaskStage, WorkerResult};

mod state;

pub use state::Stats;
use state::{
    CmpFn, DepRecorder, Dependent, EffectNode, KeyedEntry, MemoNode, PassHeap, ReactiveId,
    RuntimeState, SignalSlot,
};

/// The seven-phase frame loop (DESIGN §9.1, locked #18): TIME -> INPUT ->
/// RELOAD -> EFFECTS -> LAYOUT -> PAINT/COMMIT -> A11Y, on-demand.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Phase {
    Time,
    Input,
    Reload,
    Effects,
    Layout,
    PaintCommit,
    A11y,
}

impl Phase {
    pub const ALL: [Phase; 7] = [
        Phase::Time,
        Phase::Input,
        Phase::Reload,
        Phase::Effects,
        Phase::Layout,
        Phase::PaintCommit,
        Phase::A11y,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            Phase::Time => "TIME",
            Phase::Input => "INPUT",
            Phase::Reload => "RELOAD",
            Phase::Effects => "EFFECTS",
            Phase::Layout => "LAYOUT",
            Phase::PaintCommit => "PAINT/COMMIT",
            Phase::A11y => "A11Y",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Memo,
    Effect,
    Untrack,
}

pub(crate) struct RunFrame {
    kind: FrameKind,
    recorder: Option<Rc<RefCell<DepRecorder>>>,
    node: Option<ReactiveId>,
}

// The legacy single-stack TLS (see below): only `untrack` pushes here now.
thread_local! {
    static RUN_STACK: RefCell<Vec<RunFrame>> = const { RefCell::new(Vec::new()) };
}

/// Run stacks live in shared `RuntimeState` (M2b §5.3 hot-reload fix),
/// NOT in TLS: hot code in a reloaded dylib runs on the UI thread but
/// under a DIFFERENT image's TLS, so a TLS stack would be empty for
/// dylib-executed reads/writes and silently drop dependency tracking.
/// State residence makes tracking data-driven and swap-proof regardless
/// of which image executes the primitive. Plain field (no inner
/// `RefCell`): pushes take `&mut` state sequentially, readers go through
/// the shared borrow — neither is ever held across user code.
///
/// `untrack` keeps its legacy TLS stack: it is only ever pushed by user
/// code in its own image, and readers suppress tracking when EITHER
/// stack's top is `Untrack`. Single-image behavior is bit-identical to
/// before; cross-image, an `untrack` block suppresses reads executed in
/// its own image (host callbacks reading signals inside a hot `untrack`
/// block still track — documented limitation, same class as all
/// per-image TLS).
fn legacy_untracking() -> bool {
    RUN_STACK.with(|s| {
        s.borrow()
            .last()
            .map(|f| f.kind == FrameKind::Untrack)
            .unwrap_or(false)
    })
}

fn current_recorder(core: &Rc<RefCell<RuntimeState>>) -> Option<Rc<RefCell<DepRecorder>>> {
    if legacy_untracking() {
        return None;
    }
    core.borrow()
        .run_stack
        .last()
        .and_then(|f| f.recorder.clone())
}

fn running_node(core: &Rc<RefCell<RuntimeState>>) -> Option<ReactiveId> {
    if legacy_untracking() {
        return None;
    }
    core.borrow().run_stack.iter().rev().find_map(|f| f.node)
}

/// Only memos/effects can be dependents; a signal is never one.
fn dependent_of_node(node: Option<ReactiveId>) -> Option<Dependent> {
    match node {
        Some(ReactiveId::Memo(id)) => Some(Dependent::Memo(id)),
        Some(ReactiveId::Effect(id)) => Some(Dependent::Effect(id)),
        _ => None,
    }
}

/// Structural (not tracking): memo writes stay forbidden inside
/// `untrack` — the legacy stack is deliberately NOT consulted here.
fn in_memo_run(core: &Rc<RefCell<RuntimeState>>) -> bool {
    core.borrow()
        .run_stack
        .iter()
        .any(|f| f.kind == FrameKind::Memo)
}

/// Reads made inside `f` create no invalidation dependencies (DESIGN §2.2:
/// `untrack(|| ...)`; one of the five locked primitives, §7.9).
pub fn untrack<R>(f: impl FnOnce() -> R) -> R {
    struct PopGuard;
    impl Drop for PopGuard {
        fn drop(&mut self) {
            RUN_STACK.with(|s| {
                s.borrow_mut().pop();
            });
        }
    }
    RUN_STACK.with(|s| {
        s.borrow_mut().push(RunFrame {
            kind: FrameKind::Untrack,
            recorder: None,
            node: None,
        })
    });
    let _guard = PopGuard;
    f()
}

/// UI-thread owned root: reactive graph, generational storage, handler
/// registry, worker queue, and the seven-phase scheduler (§9.1). Cloning
/// shares one core. The `Rc` inside makes `Runtime` and every handle
/// `!Send` — the reactive pipeline is confined to the UI thread at compile
/// time (locked #20).
#[derive(Clone)]
pub struct Runtime {
    state: Rc<RefCell<RuntimeState>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

mod batch;
mod effect;
mod input;
mod memo;
mod passes;
mod runtime;
mod scheduler;
mod signal;
mod tasks;

pub use batch::BatchGuard;
pub use effect::Effect;
pub use memo::Memo;
pub use signal::Signal;
