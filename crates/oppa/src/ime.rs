//! IME composition surface (§9.2): the callback shape the spike needs to
//! exist now so per-backend IME wiring fills it in rather than inventing it
//! during the spike. No backend wires a real IME at M0b — this is the
//! contract, the scripted feed, and the recorder the spike's pass/fail
//! criteria (esp. criterion 3, composition event fidelity) assert against.

/// One IME composition event in the normalized begin/update/commit shape
/// (§9.2). Byte offsets are UTF-8 byte positions in the field's content
/// signal.
#[derive(Clone, Debug, PartialEq)]
pub enum ImeCompositionEvent {
    /// Composition begins; the platform may have pre-deleted a range.
    CompositionStarted { start_byte: usize },
    /// The composing string changed; caret is within the composition.
    CompositionUpdated {
        composition: String,
        caret_byte: usize,
    },
    /// The composition committed as final text (replaces the composition
    /// range).
    CompositionCommitted { committed: String },
    /// Composition cancelled mid-flight (no text applied).
    CompositionCancelled,
    /// IME requests a range deletion (e.g. commit replacing a selection).
    DeleteRange { range: (usize, usize) },
}

/// Core-side sink for platform IME events; the editing session service (§9.2)
/// implements this. The shell/platform side pushes events through
/// [`dispatch_ime_event`].
pub trait ImeCompositionHandler {
    fn composition_started(&mut self, _start_byte: usize) {}
    fn composition_updated(&mut self, _composition: &str, _caret_byte: usize) {}
    fn composition_committed(&mut self, _committed: &str) {}
    fn composition_cancelled(&mut self) {}
    fn delete_range(&mut self, _range: (usize, usize)) {}
}

/// The one dispatch point: normalized event → handler callbacks. Keeping a
/// single function is the no-lost/no-duplicated guarantee's seam — both the
/// scripted feed in tests and the per-backend platform wiring route through
/// this exact match.
pub fn dispatch_ime_event(handler: &mut dyn ImeCompositionHandler, event: &ImeCompositionEvent) {
    match event {
        ImeCompositionEvent::CompositionStarted { start_byte } => {
            handler.composition_started(*start_byte);
        }
        ImeCompositionEvent::CompositionUpdated {
            composition,
            caret_byte,
        } => {
            handler.composition_updated(composition, *caret_byte);
        }
        ImeCompositionEvent::CompositionCommitted { committed } => {
            handler.composition_committed(committed);
        }
        ImeCompositionEvent::CompositionCancelled => {
            handler.composition_cancelled();
        }
        ImeCompositionEvent::DeleteRange { range } => {
            handler.delete_range(*range);
        }
    }
}

/// Scripted IME event sequences: what the spike's pass/fail rig drives and
/// what platform backends feed from their native event streams.
pub struct ImeCompositionFeed {
    queued: Vec<ImeCompositionEvent>,
}

impl Default for ImeCompositionFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl ImeCompositionFeed {
    pub fn new() -> Self {
        Self { queued: Vec::new() }
    }

    pub fn push(&mut self, event: ImeCompositionEvent) {
        self.queued.push(event);
    }

    /// Dispatches queued events in order, exactly once each (§9.2
    /// criterion 3: no lost or duplicated characters).
    pub fn drain(&mut self, handler: &mut dyn ImeCompositionHandler) {
        for event in std::mem::take(&mut self.queued) {
            dispatch_ime_event(handler, &event);
        }
    }

    pub fn len(&self) -> usize {
        self.queued.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queued.is_empty()
    }
}

/// Candidate-window control, as passed to the platform via
/// [`crate::shell::PlatformShell::set_ime`] (DESIGN §2.1/§9.2): the anchor
/// rect is the framework-computed caret box; candidate window show/hide is
/// backend policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImeOps {
    /// Cursor/candidate anchoring rect in device px.
    SetCaretRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    ShowCandidateWindow,
    HideCandidateWindow,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    struct Recorder {
        log: Rc<RefCell<Vec<String>>>,
        committed_total: Rc<Cell<u32>>,
    }

    impl Recorder {
        fn new(log: Rc<RefCell<Vec<String>>>, committed_total: Rc<Cell<u32>>) -> Self {
            Self {
                log,
                committed_total,
            }
        }
    }

    impl ImeCompositionHandler for Recorder {
        fn composition_started(&mut self, start_byte: usize) {
            self.log.borrow_mut().push(format!("started@{start_byte}"));
        }
        fn composition_updated(&mut self, composition: &str, caret_byte: usize) {
            self.log
                .borrow_mut()
                .push(format!("updated[{} bytes]@{caret_byte}", composition.len()));
        }
        fn composition_committed(&mut self, committed: &str) {
            self.committed_total
                .set(self.committed_total.get() + committed.len() as u32);
            self.log
                .borrow_mut()
                .push(format!("committed[{} bytes]", committed.len()));
        }
        fn composition_cancelled(&mut self) {
            self.log.borrow_mut().push("cancelled".to_string());
        }
        fn delete_range(&mut self, range: (usize, usize)) {
            self.log
                .borrow_mut()
                .push(format!("deleted({}-{})", range.0, range.1));
        }
    }

    /// §9.2 criterion 3's shape: begin → update(s) → commit, exactly once
    /// each, nothing lost or duplicated, routed through the one dispatch fn.
    #[test]
    fn scripted_composition_sequence_dispatches_in_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let committed_total = Rc::new(Cell::new(0u32));
        let mut feed = ImeCompositionFeed::new();
        feed.push(ImeCompositionEvent::CompositionStarted { start_byte: 4 });
        feed.push(ImeCompositionEvent::CompositionUpdated {
            composition: "ni".to_string(),
            caret_byte: 6,
        });
        feed.push(ImeCompositionEvent::CompositionUpdated {
            composition: "ni hao".to_string(),
            caret_byte: 10,
        });
        feed.push(ImeCompositionEvent::CompositionCommitted {
            committed: "ni hao".to_string(),
        });
        feed.drain(&mut Recorder::new(log.clone(), committed_total.clone()));
        assert_eq!(
            *log.borrow(),
            vec![
                "started@4".to_string(),
                "updated[2 bytes]@6".to_string(),
                "updated[6 bytes]@10".to_string(),
                "committed[6 bytes]".to_string(),
            ],
            "every event delivered exactly once, in order"
        );
        assert_eq!(committed_total.get(), 6);
        assert!(feed.is_empty());
    }

    #[test]
    fn cancel_mid_composition_reaches_the_handler() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let committed_total = Rc::new(Cell::new(0u32));
        let mut feed = ImeCompositionFeed::new();
        feed.push(ImeCompositionEvent::CompositionStarted { start_byte: 0 });
        feed.push(ImeCompositionEvent::CompositionUpdated {
            composition: "ka".to_string(),
            caret_byte: 2,
        });
        feed.push(ImeCompositionEvent::CompositionCancelled);
        feed.drain(&mut Recorder::new(log.clone(), committed_total.clone()));
        assert_eq!(*log.borrow().last().unwrap(), "cancelled");
        assert_eq!(
            committed_total.get(),
            0,
            "cancelled composition commits nothing"
        );
    }

    #[test]
    fn delete_range_event_maps_directly() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let committed_total = Rc::new(Cell::new(0u32));
        let mut feed = ImeCompositionFeed::new();
        feed.push(ImeCompositionEvent::DeleteRange { range: (3, 7) });
        feed.drain(&mut Recorder::new(log.clone(), committed_total));
        assert_eq!(*log.borrow(), vec!["deleted(3-7)".to_string()]);
    }
}
