//! The advised edit sink: flush path for TIPs that transact without composition sinks.

use std::cell::RefCell;
use std::rc::Rc;

use windows::core::{implement, Ref};
use windows::Win32::UI::TextServices::{
    ITfContext, ITfEditRecord, ITfTextEditSink, ITfTextEditSink_Impl,
};

use crate::win::{log_ime, ShellShared};

use super::store::StoreInner;
// ---------------------------------------------------------------------------
#[implement(ITfTextEditSink)]
pub(super) struct EditSink {
    pub(super) inner: Rc<RefCell<StoreInner>>,
    pub(super) shared: Rc<RefCell<ShellShared>>,
}

impl ITfTextEditSink_Impl for EditSink_Impl {
    fn OnEndEdit(
        &self,
        _pic: Ref<ITfContext>,
        _ecreadonly: u32,
        _peditrecord: Ref<ITfEditRecord>,
    ) -> windows::core::Result<()> {
        // Flush path for TIPs that transact without composition sinks:
        // re-emit the current span as an update (idempotent at the
        // session: CompositionUpdated just replaces the buffer).
        let msg = self.inner.borrow().composition_message();
        self.inner
            .borrow_mut()
            .log
            .push("store: OnEndEdit (flushed span as update if active)".to_string());
        if let Some(msg) = msg {
            log_ime(&self.shared, msg);
        }
        Ok(())
    }
}
