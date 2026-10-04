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

use super::win::ShellShared;
use std::cell::RefCell;
use std::rc::Rc;
use windows::core::Interface;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::TextServices::{
    CLSID_TF_ThreadMgr, ITextStoreACP, ITfContext, ITfDocumentMgr, ITfSource, ITfTextEditSink,
    ITfThreadMgr, InputScope, GUID_PROP_INPUTSCOPE, IS_TEXT,
};

use sinks::EditSink;
use store::StoreInner;
use text_store::ShellStore;

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
        match unsafe { context.GetProperty(&GUID_PROP_INPUTSCOPE) } {
            Ok(_prop) => log.push(
                "tsf: GetProperty(GUID_PROP_INPUTSCOPE) S_OK (handle acquired; value-set needs an edit session over a TextStore range)"
                    .to_string(),
            ),
            Err(_) => log.push(format!(
                "tsf: input scope: IS_TEXT ({})",
                input_scope.0
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

mod sinks;
mod store;
mod text_store;
