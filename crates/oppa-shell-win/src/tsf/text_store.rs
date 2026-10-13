//! The `ShellStore` COM object: `ITextStoreACP` plus the owner faces (`ITfContextOwner`
//! and `ITfContextOwnerCompositionSink`, discovered by context QI).

use std::cell::RefCell;
use std::rc::Rc;

use windows::core::implement;
use windows::core::{Interface, Ref, BOOL, GUID, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::{E_FAIL, E_NOINTERFACE, E_NOTIMPL, POINT, RECT, S_OK};
use windows::Win32::System::Com::{IDataObject, FORMATETC};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::TextServices::{
    ITextStoreACP, ITextStoreACPSink, ITextStoreACP_Impl, ITfCompositionView, ITfContextOwner,
    ITfContextOwnerCompositionSink, ITfContextOwnerCompositionSink_Impl, ITfContextOwner_Impl,
    ITfRange, TEXT_STORE_LOCK_FLAGS, TS_AE_NONE, TS_ATTRVAL, TS_DEFAULT_SELECTION, TS_E_NOLOCK,
    TS_RT_PLAIN, TS_RUNINFO, TS_SELECTIONSTYLE, TS_SELECTION_ACP, TS_STATUS, TS_S_ASYNC,
    TS_TEXTCHANGE,
};

use crate::win::{log_ime, ImeMessage, ShellShared};

use super::store::StoreInner;
// ---------------------------------------------------------------------------
#[implement(ITextStoreACP, ITfContextOwner, ITfContextOwnerCompositionSink)]
pub(super) struct ShellStore {
    pub(super) inner: Rc<RefCell<StoreInner>>,
    pub(super) shared: Rc<RefCell<ShellShared>>,
    pub(super) hwnd: HWND,
}

#[allow(clippy::too_many_arguments)]
impl ITextStoreACP_Impl for ShellStore_Impl {
    fn AdviseSink(
        &self,
        _riid: *const GUID,
        punk: Ref<windows::core::IUnknown>,
        _dwmask: u32,
    ) -> windows::core::Result<()> {
        match punk.cloned() {
            Some(unk) => match unk.cast::<ITextStoreACPSink>() {
                Ok(sink) => {
                    self.inner.borrow_mut().store_sink = Some(sink);
                    self.inner
                        .borrow_mut()
                        .log
                        .push("store: AdviseSink(ITextStoreACPSink) S_OK".to_string());
                    Ok(())
                }
                Err(e) => {
                    self.inner.borrow_mut().log.push(format!(
                        "store: AdviseSink QI(ITextStoreACPSink) FAILED: {e:?}"
                    ));
                    Err(E_NOINTERFACE.into())
                }
            },
            None => Err(E_NOINTERFACE.into()),
        }
    }

    fn UnadviseSink(&self, _punk: Ref<windows::core::IUnknown>) -> windows::core::Result<()> {
        self.inner.borrow_mut().store_sink = None;
        self.inner
            .borrow_mut()
            .log
            .push("store: UnadviseSink (cleared)".to_string());
        Ok(())
    }

    fn RequestLock(&self, dwlockflags: u32) -> windows::core::Result<HRESULT> {
        // Grant scope = duration of the OnLockGranted call (the standard
        // ACP sample pattern): set the flag, drop the borrow, grant,
        // clear. Holding our RefCell across the TIP callback would
        // panic on the TIP's re-entrant SetText.
        let sink = {
            let mut inner = self.inner.borrow_mut();
            if inner.store_sink.is_none() {
                inner
                    .log
                    .push("store: RequestLock with no advised sink -> E_FAIL".to_string());
                return Err(E_FAIL.into());
            }
            if inner.lock_flags != 0 {
                inner.pending_lock = Some(dwlockflags);
                inner.log.push(format!(
                    "store: RequestLock({dwlockflags:#x}) while locked -> TS_S_ASYNC (stashed)"
                ));
                return Ok(TS_S_ASYNC);
            }
            inner.lock_flags = dwlockflags;
            inner.store_sink.clone()
        };
        let granted = match sink {
            Some(sink) => unsafe { sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(dwlockflags)) },
            None => unreachable!(),
        };
        self.inner.borrow_mut().lock_flags = 0;
        match granted {
            Ok(()) => {
                self.inner.borrow_mut().log.push(format!(
                    "store: RequestLock({dwlockflags:#x}) granted synchronously"
                ));
                Ok(S_OK)
            }
            Err(e) => {
                self.inner.borrow_mut().log.push(format!(
                    "store: OnLockGranted({dwlockflags:#x}) FAILED: {e:?}"
                ));
                Err(e)
            }
        }
    }

    fn GetStatus(&self) -> windows::core::Result<TS_STATUS> {
        Ok(TS_STATUS {
            dwDynamicFlags: 0,
            dwStaticFlags: 0,
        })
    }

    fn QueryInsert(
        &self,
        acpteststart: i32,
        acptestend: i32,
        _cch: u32,
        pacpresultstart: *mut i32,
        pacpresultend: *mut i32,
    ) -> windows::core::Result<()> {
        let inner = self.inner.borrow();
        unsafe {
            if !pacpresultstart.is_null() {
                *pacpresultstart = inner.clamp(acpteststart);
            }
            if !pacpresultend.is_null() {
                *pacpresultend = inner.clamp(acptestend);
            }
        }
        Ok(())
    }

    fn GetSelection(
        &self,
        ulindex: u32,
        ulcount: u32,
        pselection: *mut TS_SELECTION_ACP,
        pcfetched: *mut u32,
    ) -> windows::core::Result<()> {
        // Reads are lenient (no lock required); one selection only.
        let inner = self.inner.borrow();
        let want = ulindex == TS_DEFAULT_SELECTION || ulindex == 0;
        unsafe {
            if want && ulcount >= 1 && !pselection.is_null() {
                *pselection = TS_SELECTION_ACP {
                    acpStart: inner.sel_start,
                    acpEnd: inner.sel_end,
                    style: TS_SELECTIONSTYLE {
                        ase: TS_AE_NONE,
                        fInterimChar: BOOL(0),
                    },
                };
                if !pcfetched.is_null() {
                    *pcfetched = 1;
                }
            } else if !pcfetched.is_null() {
                *pcfetched = 0;
            }
        }
        Ok(())
    }

    fn SetSelection(
        &self,
        ulcount: u32,
        pselection: *const TS_SELECTION_ACP,
    ) -> windows::core::Result<()> {
        if ulcount == 0 || pselection.is_null() {
            return Ok(());
        }
        let mut inner = self.inner.borrow_mut();
        if inner.lock_flags == 0 {
            inner
                .log
                .push("store: SetSelection without lock -> TS_E_NOLOCK".to_string());
            return Err(TS_E_NOLOCK.into());
        }
        let sel = unsafe { &*pselection };
        inner.sel_start = inner.clamp(sel.acpStart);
        inner.sel_end = inner.clamp(sel.acpEnd);
        let (s, e) = (inner.sel_start, inner.sel_end);
        inner.log.push(format!("store: SetSelection ({s},{e})"));
        Ok(())
    }

    fn GetText(
        &self,
        acpstart: i32,
        acpend: i32,
        pchplain: PWSTR,
        cchplainreq: u32,
        pcchplainret: *mut u32,
        prgruninfo: *mut TS_RUNINFO,
        cruninforeq: u32,
        pcruninforet: *mut u32,
        pacpnext: *mut i32,
    ) -> windows::core::Result<()> {
        let inner = self.inner.borrow();
        let len = inner.len();
        let start = inner.clamp(acpstart);
        let end = if acpend == -1 {
            len
        } else {
            inner.clamp(acpend).max(start)
        };
        let count = ((end - start) as u32).min(cchplainreq);
        unsafe {
            if !pchplain.is_null() && cchplainreq > 0 {
                std::ptr::copy_nonoverlapping(
                    inner.text.as_ptr().add(start as usize),
                    pchplain.0,
                    count as usize,
                );
            }
            if !pcchplainret.is_null() {
                *pcchplainret = count;
            }
            if !prgruninfo.is_null() && cruninforeq > 0 {
                *prgruninfo = TS_RUNINFO {
                    uCount: count,
                    r#type: TS_RT_PLAIN,
                };
                if !pcruninforet.is_null() {
                    *pcruninforet = 1;
                }
            } else if !pcruninforet.is_null() {
                *pcruninforet = 0;
            }
            if !pacpnext.is_null() {
                *pacpnext = start + count as i32;
            }
        }
        Ok(())
    }

    fn SetText(
        &self,
        dwflags: u32,
        acpstart: i32,
        acpend: i32,
        pchtext: &PCWSTR,
        cch: u32,
    ) -> windows::core::Result<TS_TEXTCHANGE> {
        let incoming: Vec<u16> =
            unsafe { std::slice::from_raw_parts(pchtext.0, cch as usize) }.to_vec();
        let mut inner = self.inner.borrow_mut();
        if inner.lock_flags == 0 {
            inner
                .log
                .push("store: SetText without lock -> TS_E_NOLOCK".to_string());
            return Err(TS_E_NOLOCK.into());
        }
        let start = inner.clamp(acpstart);
        let end = inner.clamp(acpend).max(start);
        inner.text.splice(start as usize..end as usize, incoming);
        inner.sel_start = start + cch as i32;
        inner.sel_end = start + cch as i32;
        let change = TS_TEXTCHANGE {
            acpStart: start,
            acpOldEnd: end,
            acpNewEnd: start + cch as i32,
        };
        inner.track_span(start, cch as i32);
        let (cs, ce, ss, se) = (
            inner.comp_start,
            inner.comp_end,
            inner.sel_start,
            inner.sel_end,
        );
        inner.log.push(format!(
            "store: SetText flags={dwflags:#x} ({start},{end}) +{cch}u16 span=({cs},{ce}) sel=({ss},{se})"
        ));
        Ok(change)
    }

    fn GetFormattedText(&self, _acpstart: i32, _acpend: i32) -> windows::core::Result<IDataObject> {
        Err(E_NOTIMPL.into())
    }

    fn GetEmbedded(
        &self,
        _acppos: i32,
        _rguidservice: *const GUID,
        _riid: *const GUID,
    ) -> windows::core::Result<windows::core::IUnknown> {
        Err(E_NOTIMPL.into())
    }

    fn QueryInsertEmbedded(
        &self,
        _pguidservice: *const GUID,
        _pformatetc: *const FORMATETC,
    ) -> windows::core::Result<BOOL> {
        Err(E_NOTIMPL.into())
    }

    fn InsertEmbedded(
        &self,
        _dwflags: u32,
        _acpstart: i32,
        _acpend: i32,
        _pdataobject: Ref<IDataObject>,
    ) -> windows::core::Result<TS_TEXTCHANGE> {
        Err(E_NOTIMPL.into())
    }

    fn InsertTextAtSelection(
        &self,
        _dwflags: u32,
        pchtext: &PCWSTR,
        cch: u32,
        pacpstart: *mut i32,
        pacpend: *mut i32,
        pchange: *mut TS_TEXTCHANGE,
    ) -> windows::core::Result<()> {
        let incoming: Vec<u16> =
            unsafe { std::slice::from_raw_parts(pchtext.0, cch as usize) }.to_vec();
        let mut inner = self.inner.borrow_mut();
        if inner.lock_flags == 0 {
            inner
                .log
                .push("store: InsertTextAtSelection without lock -> TS_E_NOLOCK".to_string());
            return Err(TS_E_NOLOCK.into());
        }
        let start = inner.sel_start.min(inner.sel_end).clamp(0, inner.len());
        let end = inner.sel_start.max(inner.sel_end).clamp(start, inner.len());
        inner.text.splice(start as usize..end as usize, incoming);
        inner.sel_start = start + cch as i32;
        inner.sel_end = start + cch as i32;
        unsafe {
            if !pacpstart.is_null() {
                *pacpstart = start;
            }
            if !pacpend.is_null() {
                *pacpend = start + cch as i32;
            }
            if !pchange.is_null() {
                *pchange = TS_TEXTCHANGE {
                    acpStart: start,
                    acpOldEnd: end,
                    acpNewEnd: start + cch as i32,
                };
            }
        }
        inner.track_span(start, cch as i32);
        let (cs, ce) = (inner.comp_start, inner.comp_end);
        inner.log.push(format!(
            "store: InsertTextAtSelection ({start},{end}) +{cch}u16 span=({cs},{ce})"
        ));
        Ok(())
    }

    fn InsertEmbeddedAtSelection(
        &self,
        _dwflags: u32,
        _pdataobject: Ref<IDataObject>,
        _pacpstart: *mut i32,
        _pacpend: *mut i32,
        _pchange: *mut TS_TEXTCHANGE,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn RequestSupportedAttrs(
        &self,
        _dwflags: u32,
        cfilterattrs: u32,
        pafilterattrs: *const GUID,
    ) -> windows::core::Result<()> {
        // No display attributes supported; S_OK with zero fetched downstream.
        // Log what the TIP asks for (provenance evidence for incremental
        // finalization: if it wants an attr we never supply, that shows here).
        if cfilterattrs > 0 && !pafilterattrs.is_null() {
            let first = unsafe { *pafilterattrs };
            self.inner.borrow_mut().log.push(format!(
                "store: RequestSupportedAttrs ({cfilterattrs} filter attrs, first {first:?})"
            ));
        }
        Ok(())
    }

    fn RequestAttrsAtPosition(
        &self,
        _acppos: i32,
        _cfilterattrs: u32,
        _pafilterattrs: *const GUID,
        _dwflags: u32,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn RequestAttrsTransitioningAtPosition(
        &self,
        _acppos: i32,
        _cfilterattrs: u32,
        _pafilterattrs: *const GUID,
        _dwflags: u32,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn FindNextAttrTransition(
        &self,
        _acpstart: i32,
        acphalt: i32,
        _cfilterattrs: u32,
        _pafilterattrs: *const GUID,
        _dwflags: u32,
        pacpnext: *mut i32,
        pffound: *mut BOOL,
        plfoundoffset: *mut i32,
    ) -> windows::core::Result<()> {
        // No attributes anywhere: halt immediately, not found.
        unsafe {
            if !pacpnext.is_null() {
                *pacpnext = acphalt;
            }
            if !pffound.is_null() {
                *pffound = BOOL(0);
            }
            if !plfoundoffset.is_null() {
                *plfoundoffset = 0;
            }
        }
        Ok(())
    }

    fn RetrieveRequestedAttrs(
        &self,
        _ulcount: u32,
        _paattrvals: *mut TS_ATTRVAL,
        pcfetched: *mut u32,
    ) -> windows::core::Result<()> {
        unsafe {
            if !pcfetched.is_null() {
                *pcfetched = 0;
            }
        }
        Ok(())
    }

    fn GetEndACP(&self) -> windows::core::Result<i32> {
        Ok(self.inner.borrow().len())
    }

    fn GetActiveView(&self) -> windows::core::Result<u32> {
        // No real view cookie; samples return a zero cookie.
        Ok(0)
    }

    fn GetACPFromPoint(
        &self,
        _vcview: u32,
        _ptscreen: *const POINT,
        _dwflags: u32,
    ) -> windows::core::Result<i32> {
        Err(E_NOTIMPL.into())
    }

    fn GetTextExt(
        &self,
        _vcview: u32,
        _acpstart: i32,
        _acpend: i32,
        _prc: *mut RECT,
        _pfclipped: *mut BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn GetScreenExt(&self, _vcview: u32) -> windows::core::Result<RECT> {
        Err(E_NOTIMPL.into())
    }

    fn GetWnd(&self, _vcview: u32) -> windows::core::Result<HWND> {
        Ok(self.hwnd)
    }
}

impl ITfContextOwner_Impl for ShellStore_Impl {
    fn GetACPFromPoint(
        &self,
        _ptscreen: *const POINT,
        _dwflags: u32,
    ) -> windows::core::Result<i32> {
        Err(E_NOTIMPL.into())
    }

    fn GetTextExt(
        &self,
        _acpstart: i32,
        _acpend: i32,
        _prc: *mut RECT,
        _pfclipped: *mut BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn GetScreenExt(&self) -> windows::core::Result<RECT> {
        Err(E_NOTIMPL.into())
    }

    fn GetStatus(&self) -> windows::core::Result<TS_STATUS> {
        Ok(TS_STATUS {
            dwDynamicFlags: 0,
            dwStaticFlags: 0,
        })
    }

    fn GetWnd(&self) -> windows::core::Result<HWND> {
        Ok(self.hwnd)
    }

    fn GetAttribute(&self, _rguidattribute: *const GUID) -> windows::core::Result<VARIANT> {
        Err(E_NOTIMPL.into())
    }
}

impl ITfContextOwnerCompositionSink_Impl for ShellStore_Impl {
    fn OnStartComposition(
        &self,
        _pcomposition: Ref<ITfCompositionView>,
    ) -> windows::core::Result<BOOL> {
        self.inner.borrow_mut().start_composition();
        log_ime(&self.shared, ImeMessage::StartComposition);
        Ok(BOOL(1))
    }

    fn OnUpdateComposition(
        &self,
        _pcomposition: Ref<ITfCompositionView>,
        _prangenew: Ref<ITfRange>,
    ) -> windows::core::Result<()> {
        if let Some(msg) = self.inner.borrow().composition_message() {
            log_ime(&self.shared, msg);
        }
        Ok(())
    }

    fn OnEndComposition(
        &self,
        _pcomposition: Ref<ITfCompositionView>,
    ) -> windows::core::Result<()> {
        let msgs = self.inner.borrow_mut().end_composition();
        for msg in msgs {
            log_ime(&self.shared, msg);
        }
        Ok(())
    }
}
