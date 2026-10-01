//! TSF-aware path for the shell window (this round).
//!
//! Scope: the engagement route IME-PASS.md §6 names as the fix —
//! `ITfThreadMgr` activation + a document manager (`ITfDocumentMgr`)
//! associated with the shell window + an input scope. This is
//! PlatformShell's own IME infrastructure (M3+ work), so it lives in the
//! shell crate, not the spike binary.
//!
//! What this implements, exactly:
//! 1. `CoCreateInstance(CLSID_TF_ThreadMgr)` → `ITfThreadMgr`.
//! 2. `ITfThreadMgr::Activate()` → client id.
//! 3. `ITfThreadMgr::CreateDocumentMgr()` → `ITfDocumentMgr`.
//! 4. `ITfDocumentMgr::CreateContext(client, 0, punk=ShellStore, …)` →
//!    `ITfContext` + edit cookie. The punk is the real text store:
//!    `ShellStore` implements the full `ITextStoreACP` (28 methods) +
//!    `ITfContextOwner`, backed by the field's UTF-16 text and selection.
//! 5. `ITfDocumentMgr::Push(context)`.
//! 6. `ITfThreadMgr::AssociateFocus(hwnd, docmgr)` + `SetFocus(docmgr)`.
//!    `ITfSource::AdviseSink` for `ITfTextEditSink` (edit-flush path)
//!    and `ITfContextOwnerCompositionSink` (start/update/end → the
//!    IMM-shaped `ImeMessage`s through the same `ShellEvent::Ime`
//!    pipeline, so the mapper and session semantics are untouched).
//! 7. Input scope: the window declares `IS_TEXT`; the code acquires the
//!    `GUID_PROP_INPUTSCOPE` property handle on the context and records the
//!    outcome. Setting a property *value* requires an edit session over a
//!    TextStore-backed range.
//!
//! Every COM call's outcome (including HRESULTs on failure) is pushed into
//! the bridge log so the pass record shows exactly what engaged. All
//! interface signatures below were confirmed against the generated
//! windows-rs 0.62 bindings (`TextServices/mod.rs`) before use — see the
//! round entry for line confirmations.

use super::win::{log_ime, ShellShared};
use super::ImeMessage;
use std::cell::RefCell;
use std::rc::Rc;
use windows::core::implement;
use windows::core::{Interface, Ref, BOOL, GUID, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::{E_FAIL, E_NOINTERFACE, E_NOTIMPL, POINT, RECT, S_OK};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Com::{IDataObject, FORMATETC};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::TextServices::{
    CLSID_TF_ThreadMgr, ITfContext, ITfDocumentMgr, ITfThreadMgr, InputScope, GUID_PROP_INPUTSCOPE,
    IS_TEXT,
};
use windows::Win32::UI::TextServices::{
    ITextStoreACP, ITextStoreACPSink, ITextStoreACP_Impl, ITfCompositionView, ITfContextOwner,
    ITfContextOwnerCompositionSink, ITfContextOwnerCompositionSink_Impl, ITfContextOwner_Impl,
    ITfEditRecord, ITfRange, ITfSource, ITfTextEditSink, ITfTextEditSink_Impl,
    TEXT_STORE_LOCK_FLAGS, TS_AE_NONE, TS_ATTRVAL, TS_DEFAULT_SELECTION, TS_E_NOLOCK, TS_RT_PLAIN,
    TS_RUNINFO, TS_SELECTIONSTYLE, TS_SELECTION_ACP, TS_STATUS, TS_S_ASYNC, TS_TEXTCHANGE,
};

/// Snapshot of the TSF association for the pass record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TsfStatus {
    pub client_id: u32,
    pub edit_cookie: u32,
    pub associated: bool,
    pub focused: bool,
    /// The declared input scope (`IS_TEXT.0`, i.e. 57 when set).
    pub input_scope: i32,
    pub has_context: bool,
}

pub struct TsfBridge {
    thread_mgr: ITfThreadMgr,
    doc_mgr: ITfDocumentMgr,
    context: ITfContext,
    client_id: u32,
    edit_cookie: u32,
    hwnd: HWND,
    input_scope: InputScope,
    focused: bool,
    log: Vec<String>,
    store: Rc<RefCell<StoreInner>>,
    // COM keep-alives: the context holds only weak references back to
    // these; dropping them would unplug the store mid-pass.
    _store_unk: windows::core::IUnknown,
    _edit_sink_unk: windows::core::IUnknown,
    // Cookie for a future UnadviseSink on teardown (process-lifetime
    // binary today; retained, not read).
    _edit_sink_cookie: u32,
}

impl TsfBridge {
    /// Full activation + association for `hwnd`. Returns the bridge on
    /// success; every step is logged either way (failures carry the HRESULT).
    /// `shared` is the shell's event queue (the sinks translate TIP
    /// transactions into `ImeMessage`s through it); `init_text`/`init_sel`
    /// seed the store with the settled field state.
    pub fn activate(
        hwnd: HWND,
        shared: &Rc<RefCell<ShellShared>>,
        init_text: &str,
        init_sel: (usize, usize),
    ) -> Result<Self, (String, Vec<String>)> {
        let mut log: Vec<String> = Vec::new();
        log.push(format!(
            "tsf: activate begin (hwnd {:p})",
            hwnd.0 as *const core::ffi::c_void
        ));

        // 1. ThreadMgr object.
        // Confirmed: `CoCreateInstance<P1, T>(rclsid: *const GUID, punkouter: P1,
        // dwclscontext: CLSCTX) -> Result<T>` (System/Com/mod.rs:117);
        // `CLSID_TF_ThreadMgr` (TextServices/mod.rs:65).
        let thread_mgr: ITfThreadMgr =
            unsafe { CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER) }.map_err(
                |e| {
                    log.push(format!(
                        "tsf: CoCreateInstance(CLSID_TF_ThreadMgr) FAILED: {e:?}"
                    ));
                    (
                        format!("CoCreateInstance(ThreadMgr) failed: {e:?}"),
                        log.clone(),
                    )
                },
            )?;
        log.push("tsf: CoCreateInstance(CLSID_TF_ThreadMgr) S_OK".to_string());

        // 2. Activate → client id.
        // Confirmed: `ITfThreadMgr::Activate(&self) -> Result<u32>`
        // (TextServices/mod.rs:13177).
        let client_id = unsafe { thread_mgr.Activate() }.map_err(|e| {
            log.push(format!("tsf: ITfThreadMgr::Activate FAILED: {e:?}"));
            (format!("ITfThreadMgr::Activate failed: {e:?}"), log.clone())
        })?;
        log.push(format!(
            "tsf: ITfThreadMgr::Activate S_OK (client_id={client_id})"
        ));

        // 3. Document manager.
        // Confirmed: `ITfThreadMgr::CreateDocumentMgr(&self)
        // -> Result<ITfDocumentMgr>` (TextServices/mod.rs:13186).
        let doc_mgr = unsafe { thread_mgr.CreateDocumentMgr() }.map_err(|e| {
            log.push(format!("tsf: CreateDocumentMgr FAILED: {e:?}"));
            (format!("CreateDocumentMgr failed: {e:?}"), log.clone())
        })?;
        log.push("tsf: CreateDocumentMgr S_OK".to_string());

        // 4. Context. The punk is now the real text store (text-store
        // round): `ShellStore` implements ITextStoreACP + ITfContextOwner.
        // Confirmed: `ITfDocumentMgr::CreateContext(&self, tidowner: u32,
        // dwflags: u32, punk: Param<IUnknown>, ppic: *mut Option<ITfContext>,
        // pectextstore: *mut u32)` (TextServices/mod.rs:7028).
        let store = Rc::new(RefCell::new(StoreInner {
            text: init_text.encode_utf16().collect(),
            sel_start: 0,
            sel_end: 0,
            lock_flags: 0,
            pending_lock: None,
            store_sink: None,
            comp_active: false,
            comp_start: 0,
            comp_end: 0,
            log: Vec::new(),
        }));
        {
            let mut inner = store.borrow_mut();
            let end = inner.len();
            inner.sel_start =
                StoreInner::byte_to_acp(init_text, init_sel.0).min(end as usize) as i32;
            inner.sel_end = StoreInner::byte_to_acp(init_text, init_sel.1).min(end as usize) as i32;
        }
        let store_iface: ITextStoreACP = ShellStore {
            inner: store.clone(),
            shared: shared.clone(),
            hwnd,
        }
        .into();
        let store_unk: windows::core::IUnknown = store_iface.cast().map_err(|e| {
            log.push(format!("tsf: store QI(IUnknown) FAILED: {e:?}"));
            (format!("store QI failed: {e:?}"), log.clone())
        })?;
        log.push("tsf: ShellStore created (ITextStoreACP + ITfContextOwner)".to_string());
        let mut ctx_opt: Option<ITfContext> = None;
        let mut edit_cookie: u32 = 0;
        unsafe {
            doc_mgr.CreateContext(
                client_id,
                0,
                Some(&store_unk),
                &mut ctx_opt,
                &mut edit_cookie,
            )
        }
        .map_err(|e| {
            log.push(format!(
                "tsf: CreateContext(client={client_id}, punk=ShellStore) FAILED: {e:?}"
            ));
            (format!("CreateContext failed: {e:?}"), log.clone())
        })?;
        let context = ctx_opt.ok_or_else(|| {
            log.push("tsf: CreateContext S_OK but returned no context".to_string());
            ("CreateContext returned None".to_string(), log.clone())
        })?;
        log.push(format!(
            "tsf: CreateContext S_OK (punk=ShellStore; edit_cookie={edit_cookie})"
        ));

        // 5. Push.
        // Confirmed: `ITfDocumentMgr::Push(&self, pic: Param<ITfContext>)`
        // (TextServices/mod.rs:7034).
        unsafe { doc_mgr.Push(&context) }.map_err(|e| {
            log.push(format!("tsf: Push FAILED: {e:?}"));
            (format!("Push failed: {e:?}"), log.clone())
        })?;
        log.push("tsf: Push(context) S_OK".to_string());

        // 6. Associate + focus.
        // Confirmed: `ITfThreadMgr::AssociateFocus(&self, hwnd: HWND,
        // pdimnew: Param<ITfDocumentMgr>) -> Result<ITfDocumentMgr>`
        // (TextServices/mod.rs:13210);
        // `ITfThreadMgr::SetFocus(&self, pdimfocus: Param<ITfDocumentMgr>)`
        // (TextServices/mod.rs:13204).
        match unsafe { thread_mgr.AssociateFocus(hwnd, &doc_mgr) } {
            Ok(_prev) => {
                log.push("tsf: AssociateFocus(hwnd, docmgr) S_OK".to_string());
            }
            Err(e) => {
                log.push(format!("tsf: AssociateFocus FAILED: {e:?}"));
                return Err((format!("AssociateFocus failed: {e:?}"), log));
            }
        }
        unsafe { thread_mgr.SetFocus(&doc_mgr) }.map_err(|e| {
            log.push(format!("tsf: SetFocus(docmgr) FAILED: {e:?}"));
            (format!("SetFocus failed: {e:?}"), log.clone())
        })?;
        log.push("tsf: SetFocus(docmgr) S_OK".to_string());

        // Advise the text-edit sink (flush path) on the context. The
        // composition sink is NOT advised: `ITfContextOwnerCompositionSink`
        // is discovered by the context QI'ing the owner punk (it is now
        // implemented on `ShellStore` itself) — advising it on the source
        // fails with TS_E_NOOBJECT (0x80040202), found empirically over
        // two runs. Confirmed: `ITfSource::AdviseSink`
        // (TextServices/mod.rs:12561); `ITfTextEditSink::OnEndEdit`
        // (12987); `ITfContextOwnerCompositionSink::{OnStart,OnUpdate,
        // OnEnd}Composition` (6403-6405).
        let source: ITfSource = context.cast().map_err(|e| {
            log.push(format!("tsf: context QI(ITfSource) FAILED: {e:?}"));
            (format!("QI(ITfSource) failed: {e:?}"), log.clone())
        })?;
        let edit_iface: ITfTextEditSink = EditSink {
            inner: store.clone(),
            shared: shared.clone(),
        }
        .into();
        let edit_sink_unk: windows::core::IUnknown = edit_iface.cast().map_err(|e| {
            log.push(format!("tsf: edit sink QI(IUnknown) FAILED: {e:?}"));
            (format!("edit sink QI failed: {e:?}"), log.clone())
        })?;
        let edit_sink_cookie = unsafe { source.AdviseSink(&ITfTextEditSink::IID, &edit_sink_unk) }
            .map_err(|e| {
                log.push(format!("tsf: AdviseSink(TextEditSink) FAILED: {e:?}"));
                (format!("AdviseSink(TextEdit) failed: {e:?}"), log.clone())
            })?;
        log.push(format!(
            "tsf: AdviseSink(ITfTextEditSink) S_OK (cookie={edit_sink_cookie})"
        ));
        log.push(
            "tsf: composition callbacks via owner QI (ShellStore implements ITfContextOwnerCompositionSink; not advised)"
                .to_string(),
        );

        // 7. Input scope: declare IS_TEXT; acquire the property handle.
        // Confirmed: `IS_TEXT: InputScope = InputScope(57)`
        // (TextServices/mod.rs:2418);
        // `GUID_PROP_INPUTSCOPE` (TextServices/mod.rs:154);
        // `ITfContext::GetProperty(&self, guidprop: *const GUID)
        // -> Result<ITfProperty>` (TextServices/mod.rs:5688).
        let input_scope = IS_TEXT;
        log.push(format!(
            "tsf: input scope declared IS_TEXT ({})",
            input_scope.0
        ));
        match unsafe { context.GetProperty(&GUID_PROP_INPUTSCOPE) } {
            Ok(_prop) => log.push(
                "tsf: GetProperty(GUID_PROP_INPUTSCOPE) S_OK (handle acquired; value-set needs an edit session over a TextStore range — not implemented this round)"
                    .to_string(),
            ),
            Err(e) => log.push(format!(
                "tsf: GetProperty(GUID_PROP_INPUTSCOPE) FAILED: {e:?}"
            )),
        }

        log.push("tsf: activate complete".to_string());
        Ok(Self {
            thread_mgr,
            doc_mgr,
            context,
            client_id,
            edit_cookie,
            hwnd,
            input_scope,
            focused: true,
            log,
            store,
            _store_unk: store_unk,
            _edit_sink_unk: edit_sink_unk,
            _edit_sink_cookie: edit_sink_cookie,
        })
    }

    /// Re-assert focus on the document manager. Skipped when already
    /// focused: re-asserting mid-composition churns the TIP's keystroke
    /// state (suspect for per-letter finalization — tested by gating).
    /// Returns the log line for the step's notes.
    pub fn reassert_focus(&mut self) -> String {
        if self.focused {
            return "tsf: reassert skipped (already focused)".to_string();
        }
        match unsafe { self.thread_mgr.SetFocus(&self.doc_mgr) } {
            Ok(()) => {
                self.focused = true;
                "tsf: reassert SetFocus(docmgr) S_OK".to_string()
            }
            Err(e) => format!("tsf: reassert SetFocus FAILED: {e:?}"),
        }
    }

    /// Note a focus change (WM_SETFOCUS/WM_KILLFOCUS arrived as a shell
    /// event). Clearing uses `SetFocus(None)`.
    pub fn note_focus(&mut self, focused: bool) -> String {
        let line = if focused {
            match unsafe { self.thread_mgr.SetFocus(&self.doc_mgr) } {
                Ok(()) => "tsf: SetFocus(docmgr) on focus-gain S_OK".to_string(),
                Err(e) => format!("tsf: SetFocus on focus-gain FAILED: {e:?}"),
            }
        } else {
            let none: Option<&ITfDocumentMgr> = None;
            match unsafe { self.thread_mgr.SetFocus(none) } {
                Ok(()) => "tsf: SetFocus(None) on focus-loss S_OK".to_string(),
                Err(e) => format!("tsf: SetFocus(None) on focus-loss FAILED: {e:?}"),
            }
        };
        self.focused = focused;
        self.log.push(line.clone());
        line
    }

    pub fn status(&self) -> TsfStatus {
        TsfStatus {
            client_id: self.client_id,
            edit_cookie: self.edit_cookie,
            associated: true,
            focused: self.focused,
            input_scope: self.input_scope.0,
            has_context: true,
        }
    }

    /// Drain the activation log (called once by the host into env_log).
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn context(&self) -> &ITfContext {
        &self.context
    }

    /// Mirror the settled session state into the store. Skipped (and
    /// reported) while a TIP composition owns the store — the host must
    /// not clobber TIP state. The advised store sink (when present) gets
    /// an `OnSelectionChange` so the TIP follows the caret.
    pub fn sync_external(&mut self, text: &str, sel: (usize, usize)) -> String {
        let (sink, skipped) = {
            let mut inner = self.store.borrow_mut();
            if inner.comp_active {
                (
                    None,
                    format!(
                        "tsf: sync skipped (composition active, span ({},{}))",
                        inner.comp_start, inner.comp_end
                    ),
                )
            } else {
                inner.text = text.encode_utf16().collect();
                let end = inner.len();
                inner.sel_start = StoreInner::byte_to_acp(text, sel.0).min(end as usize) as i32;
                inner.sel_end = StoreInner::byte_to_acp(text, sel.1).min(end as usize) as i32;
                let line = format!(
                    "tsf: sync external ({} chars, sel ({},{}))",
                    inner.len(),
                    inner.sel_start,
                    inner.sel_end
                );
                (inner.store_sink.clone(), line)
            }
        };
        if skipped.starts_with("tsf: sync skipped") {
            return skipped;
        }
        let mut line = skipped;
        if let Some(sink) = sink {
            match unsafe { sink.OnSelectionChange() } {
                Ok(()) => line.push_str(" + OnSelectionChange S_OK"),
                Err(e) => line.push_str(&format!(" + OnSelectionChange FAILED: {e:?}")),
            }
        } else {
            line.push_str(" (no store sink advised yet)");
        }
        self.store.borrow_mut().log.push(line.clone());
        line
    }

    /// Drain the store's per-call trace for the step notes.
    pub fn take_store_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.store.borrow_mut().log)
    }
}

// ---------------------------------------------------------------------------
// ITextStoreACP text store + composition sinks (text-store round)
// ---------------------------------------------------------------------------
//
// Design (recorded, not assumed): the store is the TIP-facing copy of the
// one field's text (UTF-16/ACP units). The host mirrors the settled
// session into it while no composition is active (`sync_external`);
// while a TIP composition is active the TIP owns it. TIP transactions
// are translated into the SAME `ImeMessage`s the IMM path produces
// (`StartComposition` / `Composition{comp/result}` / `EndComposition`)
// and queued through the same `ShellEvent::Ime` pipeline, so the mapper
// and session semantics are untouched. The composition span is tracked
// from the TIP's own SetText/Insert calls (no lock juggling: all data is
// already in our buffer when the sink callbacks fire).

/// Shared state between the COM store object, the sinks, and the bridge.
struct StoreInner {
    text: Vec<u16>,
    sel_start: i32,
    sel_end: i32,
    lock_flags: u32,
    pending_lock: Option<u32>,
    store_sink: Option<ITextStoreACPSink>,
    comp_active: bool,
    comp_start: i32,
    comp_end: i32,
    log: Vec<String>,
}

impl StoreInner {
    fn len(&self) -> i32 {
        self.text.len() as i32
    }

    fn clamp(&self, acp: i32) -> i32 {
        acp.clamp(0, self.len())
    }

    fn as_string(&self) -> String {
        String::from_utf16_lossy(&self.text)
    }

    fn span_text(&self) -> String {
        let s = self.comp_start.clamp(0, self.len()) as usize;
        let e = self.comp_end.clamp(s as i32, self.len()) as usize;
        String::from_utf16_lossy(&self.text[s..e])
    }

    /// UTF-16 ACP index → UTF-8 byte index (same walk as the host mapper).
    fn acp_to_byte(text: &str, mut acp: usize) -> usize {
        for (byte, ch) in text.char_indices() {
            let units = ch.len_utf16();
            if acp < units {
                return byte;
            }
            acp -= units;
        }
        text.len()
    }

    fn byte_to_acp(text: &str, byte: usize) -> usize {
        let mut acp = 0;
        for (b, ch) in text.char_indices() {
            if b >= byte {
                break;
            }
            acp += ch.len_utf16();
        }
        acp
    }

    /// Maintain the tracked composition span from a TIP SetText/Insert
    /// covering `acp_start` and inserting `inserted` units.
    fn track_span(&mut self, acp_start: i32, inserted: i32) {
        if !self.comp_active {
            return;
        }
        self.comp_start = self.comp_start.min(acp_start);
        self.comp_end = (self.comp_end.max(acp_start + inserted)).max(self.comp_start);
    }

    fn start_composition(&mut self) {
        self.comp_active = true;
        self.comp_start = self.sel_start.min(self.sel_end);
        self.comp_end = self.sel_start.max(self.sel_end);
        self.log.push(format!(
            "store: composition started (span ({},{}))",
            self.comp_start, self.comp_end
        ));
    }

    /// The current composition as an IMM-shaped update message.
    fn composition_message(&self) -> Option<ImeMessage> {
        if !self.comp_active {
            return None;
        }
        let comp = self.span_text();
        let text = self.as_string();
        let caret_acp = self
            .sel_end
            .clamp(self.comp_start, self.comp_end.max(self.comp_start));
        let cursor = (caret_acp - self.comp_start).max(0) as usize;
        Some(ImeMessage::Composition {
            gcs_flags: 0,
            comp: Some(comp),
            attrs: None,
            cursor_pos: Self::acp_to_byte(&text, cursor) as i32,
            delta_start: 0,
            result: None,
        })
    }

    /// Mirror the IMM flow: a non-empty final span commits (result, then
    /// END so the mapper's `comp_active` clears), an empty one cancels
    /// (END only → the mapper drops the buffer).
    fn end_composition(&mut self) -> Vec<ImeMessage> {
        self.comp_active = false;
        let committed = self.span_text();
        self.log.push(format!(
            "store: composition ended (final span text {committed:?})"
        ));
        if committed.is_empty() {
            vec![ImeMessage::EndComposition]
        } else {
            vec![
                ImeMessage::Composition {
                    gcs_flags: 0,
                    comp: None,
                    attrs: None,
                    cursor_pos: 0,
                    delta_start: 0,
                    result: Some(committed),
                },
                ImeMessage::EndComposition,
            ]
        }
    }
}

#[implement(ITextStoreACP, ITfContextOwner, ITfContextOwnerCompositionSink)]
struct ShellStore {
    inner: Rc<RefCell<StoreInner>>,
    shared: Rc<RefCell<ShellShared>>,
    hwnd: HWND,
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

#[implement(ITfTextEditSink)]
struct EditSink {
    inner: Rc<RefCell<StoreInner>>,
    shared: Rc<RefCell<ShellShared>>,
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
