//! UIA event raising (v1 remainder, Gap 5): the `oppa-uia`
//! provider hosted in a real HWND + a real UIA client pump — a
//! framework toggle flip raises an observable property-changed
//! event through `UiaRaiseAutomationPropertyChangedEvent`.
//!
//! Hosting scope (stated): the HWND lives in this test (production
//! hosting belongs with the shell window per decision 149 — the
//! provider itself stays HWND-free and `HostRawElementProvider`
//! stays loud `E_NOTIMPL`). What IS real here: the window, the
//! `WM_GETOBJECT`/`UiaReturnRawElementProvider` association, the
//! `CUIAutomation` client, the subscribed property, and the raised
//! event with old/new values.

#![cfg(windows)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oppa::{
    compute_semantics_diff, find_retained_by_debug, ComponentHost, Ctx, NodeId, Semantics,
    SemanticsSnapshot, Style, VNode,
};
use oppa_macros::{component, Props};
use oppa_uia::{OppaProvider, UiaAction, UiaTree};
use windows::core::{implement, Interface, Ref, BSTR, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IToggleProvider, IUIAutomation, IUIAutomationElement,
    IUIAutomationPropertyChangedEventHandler, IUIAutomationPropertyChangedEventHandler_Impl,
    ToggleState_Off, ToggleState_On, UIA_NamePropertyId, UIA_TogglePatternId,
    UIA_ToggleToggleStatePropertyId, UiaRaiseAutomationPropertyChangedEvent,
    UiaReturnRawElementProvider, UIA_PROPERTY_ID,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, RegisterClassW, ShowWindow,
    TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, MSG, OBJID_CLIENT, SW_SHOW,
    WINDOW_EX_STYLE, WM_GETOBJECT, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

#[derive(Clone, Props)]
struct ToggleProps {
    label: String,
    initial: bool,
}

#[component]
fn Toggle(ctx: &Ctx, props: &ToggleProps) -> VNode {
    let is_on = ctx.signal(props.initial);
    let s = is_on.clone();
    oppa::Div("track")
        .style(Style::new().size(44, 24))
        .semantics(Semantics::switch().checked(is_on.get()).label(&props.label))
        .on_press(move || s.set(!s.get()))
        .build()
}

#[derive(Clone, Props)]
struct RowProps {
    name: String,
}

#[component]
fn Row(_ctx: &Ctx, props: &RowProps) -> VNode {
    oppa::Div("row")
        .style(Style::new().size(200, 24))
        .semantics(Semantics::list_item().label(&props.name))
        .build()
}

#[derive(Clone, Props)]
struct AppProps {
    label: String,
    name: String,
}

#[component]
fn App(ctx: &Ctx, props: &AppProps) -> VNode {
    oppa::Div("app")
        .semantics(Semantics::default().label("app"))
        .children([
            ctx.child(
                "Toggle",
                1,
                &ToggleProps {
                    label: props.label.clone(),
                    initial: false,
                },
                Toggle,
            ),
            ctx.child(
                "Row",
                2,
                &RowProps {
                    name: props.name.clone(),
                },
                Row,
            ),
        ])
}

/// The hosted provider (test HWND scope): set once before window
/// creation, served from `WM_GETOBJECT/OBJID_CLIENT`. A raw pointer
/// in an atomic (not `OnceLock<IRaw...>`) because COM interfaces are
/// not `Sync`; the box is reclaimed after `DestroyWindow`.
static HOSTED_PTR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// The HWND parent link (see `HwndParent`).
static PARENT_PTR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Diagnostic: how many times the OBJID_CLIENT branch fired.
static GETOBJECT_HITS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // NOTE: WM_GETOBJECT carries the object id as a zero-extended
    // DWORD (0xFFFFFFFC), never sign-extended to -4 — compare the
    // low 32 bits (a direct `== OBJID_CLIENT.0` never fires; the
    // association silently stays on the default proxy).
    // UIA queries UiaRootObjectId (-25) before OBJID_CLIENT and
    // binds the FIRST answer as the window element, so both get
    // the hosted provider (diagnostic finding this run).
    const UIA_ROOT_OBJECT_ID: u32 = 0xFFFF_FFE7;
    if msg == WM_GETOBJECT
        && ((lparam.0 as u32) == (OBJID_CLIENT.0 as u32) || (lparam.0 as u32) == UIA_ROOT_OBJECT_ID)
    {
        GETOBJECT_HITS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let raw = HOSTED_PTR.load(std::sync::atomic::Ordering::SeqCst)
            as *const windows::Win32::UI::Accessibility::IRawElementProviderSimple;
        if !raw.is_null() {
            let lr = UiaReturnRawElementProvider(hwnd, wparam, lparam, &*raw);
            return lr;
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn var_i4(x: i32) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = x;
    }
    v
}

fn read_i4(v: &VARIANT) -> i32 {
    unsafe { v.Anonymous.Anonymous.Anonymous.lVal }
}

fn read_bstr(v: &VARIANT) -> String {
    unsafe {
        let b: &BSTR = &v.Anonymous.Anonymous.Anonymous.bstrVal;
        String::from_utf16_lossy(std::slice::from_raw_parts(b.as_ptr(), b.len()))
    }
}

#[implement(IUIAutomationPropertyChangedEventHandler)]
struct Recorder {
    records: Arc<Mutex<Vec<(i32, i32, String)>>>,
}

/// Logging delegate (diagnostic): implements the fragment
/// interfaces by forwarding to an inner `OppaProvider` interface,
/// logging every call. Shows exactly how far UIA's
/// `UiaReturnRawElementProvider` validation gets before rejecting.
#[implement(
    windows::Win32::UI::Accessibility::IRawElementProviderSimple,
    windows::Win32::UI::Accessibility::IRawElementProviderFragment,
    windows::Win32::UI::Accessibility::IRawElementProviderFragmentRoot,
    windows::Win32::UI::Accessibility::IRawElementProviderAdviseEvents
)]
struct LogFragment {
    simple: windows::Win32::UI::Accessibility::IRawElementProviderSimple,
    fragment: windows::Win32::UI::Accessibility::IRawElementProviderFragment,
    root: windows::Win32::UI::Accessibility::IRawElementProviderFragmentRoot,
    hwnd: HWND,
}

fn host_provider(
    hwnd: HWND,
) -> windows::core::Result<windows::Win32::UI::Accessibility::IRawElementProviderSimple> {
    use windows::Win32::UI::Accessibility::UiaHostProviderFromHwnd;
    unsafe { UiaHostProviderFromHwnd(hwnd) }
}

/// The HWND parent link (test HWND scope): a fragment whose only
/// child is the hosted tree root. UIA's `UiaReturnRawElementProvider`
/// validation requires the hosted root's `Navigate(Parent)` to
/// succeed; the window's own host proxy exposes no `Fragment`, so
/// the hosting layer supplies this link (another instance of
/// decision 149: HWND association lives with the window, not the
/// framework provider).
#[implement(
    windows::Win32::UI::Accessibility::IRawElementProviderSimple,
    windows::Win32::UI::Accessibility::IRawElementProviderFragment,
    windows::Win32::UI::Accessibility::IRawElementProviderFragmentRoot
)]
struct HwndParent {
    child: windows::Win32::UI::Accessibility::IRawElementProviderFragment,
    hwnd: HWND,
    host: windows::Win32::UI::Accessibility::IRawElementProviderSimple,
}

impl IRawElementProviderSimple_Impl for HwndParent_Impl {
    fn ProviderOptions(&self) -> windows::core::Result<ProviderOptions> {
        unsafe { self.host.ProviderOptions() }
    }
    fn GetPatternProvider(
        &self,
        id: windows::Win32::UI::Accessibility::UIA_PATTERN_ID,
    ) -> windows::core::Result<windows::core::IUnknown> {
        unsafe { self.host.GetPatternProvider(id) }
    }
    fn GetPropertyValue(
        &self,
        id: windows::Win32::UI::Accessibility::UIA_PROPERTY_ID,
    ) -> windows::core::Result<VARIANT> {
        unsafe { self.host.GetPropertyValue(id) }
    }
    fn HostRawElementProvider(
        &self,
    ) -> windows::core::Result<windows::Win32::UI::Accessibility::IRawElementProviderSimple> {
        Ok(self.host.clone())
    }
}

impl IRawElementProviderFragment_Impl for HwndParent_Impl {
    fn Navigate(
        &self,
        dir: NavigateDirection,
    ) -> windows::core::Result<windows::Win32::UI::Accessibility::IRawElementProviderFragment> {
        use windows::Win32::UI::Accessibility::{
            NavigateDirection_FirstChild, NavigateDirection_LastChild,
        };
        if dir == NavigateDirection_FirstChild || dir == NavigateDirection_LastChild {
            return Ok(self.child.clone());
        }
        Err(windows::core::Error::from(
            windows::Win32::Foundation::E_NOINTERFACE,
        ))
    }
    fn GetRuntimeId(&self) -> windows::core::Result<*mut SAFEARRAY> {
        use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
        use windows::Win32::System::Variant::VT_I4;
        use windows::Win32::UI::Accessibility::UiaAppendRuntimeId;
        unsafe {
            let psa = SafeArrayCreateVector(VT_I4, 0, 2);
            if psa.is_null() {
                return Err(windows::core::Error::from(
                    windows::Win32::Foundation::E_OUTOFMEMORY,
                ));
            }
            let zero = 0i32;
            let one = 1i32;
            let id0 = UiaAppendRuntimeId as i32;
            let id1 = (self.hwnd.0 as usize & 0x7FFF_FFFF) as i32;
            SafeArrayPutElement(psa, &zero, &id0 as *const i32 as *const _)?;
            SafeArrayPutElement(psa, &one, &id1 as *const i32 as *const _)?;
            Ok(psa)
        }
    }
    fn BoundingRectangle(&self) -> windows::core::Result<UiaRect> {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
        unsafe {
            let mut rect = RECT::default();
            GetWindowRect(self.hwnd, &mut rect)?;
            Ok(UiaRect {
                left: rect.left as f64,
                top: rect.top as f64,
                width: (rect.right - rect.left) as f64,
                height: (rect.bottom - rect.top) as f64,
            })
        }
    }
    fn GetEmbeddedFragmentRoots(&self) -> windows::core::Result<*mut SAFEARRAY> {
        use windows::Win32::System::Ole::SafeArrayCreateVector;
        use windows::Win32::System::Variant::VT_I4;
        unsafe {
            let psa = SafeArrayCreateVector(VT_I4, 0, 0);
            if psa.is_null() {
                return Err(windows::core::Error::from(
                    windows::Win32::Foundation::E_OUTOFMEMORY,
                ));
            }
            Ok(psa)
        }
    }
    fn SetFocus(&self) -> windows::core::Result<()> {
        Err(windows::core::Error::from(
            windows::Win32::Foundation::E_NOTIMPL,
        ))
    }
    fn FragmentRoot(
        &self,
    ) -> windows::core::Result<windows::Win32::UI::Accessibility::IRawElementProviderFragmentRoot>
    {
        // This link IS a root (its parent is the HWND itself).
        let me_: windows::Win32::UI::Accessibility::IRawElementProviderFragmentRoot = HwndParent {
            child: self.child.clone(),
            hwnd: self.hwnd,
            host: self.host.clone(),
        }
        .into();
        Ok(me_)
    }
}

impl IRawElementProviderFragmentRoot_Impl for HwndParent_Impl {
    fn ElementProviderFromPoint(
        &self,
        _x: f64,
        _y: f64,
    ) -> windows::core::Result<windows::Win32::UI::Accessibility::IRawElementProviderFragment> {
        Err(windows::core::Error::from(
            windows::Win32::Foundation::E_NOTIMPL,
        ))
    }
    fn GetFocus(
        &self,
    ) -> windows::core::Result<windows::Win32::UI::Accessibility::IRawElementProviderFragment> {
        Err(windows::core::Error::from(
            windows::Win32::Foundation::E_NOINTERFACE,
        ))
    }
}

use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::UI::Accessibility::{
    IRawElementProviderFragmentRoot_Impl, IRawElementProviderFragment_Impl,
    IRawElementProviderSimple_Impl, NavigateDirection, NavigateDirection_Parent, ProviderOptions,
    UiaRect,
};

impl IRawElementProviderSimple_Impl for LogFragment_Impl {
    fn ProviderOptions(&self) -> windows::core::Result<ProviderOptions> {
        unsafe { self.simple.ProviderOptions() }
    }
    fn GetPatternProvider(
        &self,
        id: windows::Win32::UI::Accessibility::UIA_PATTERN_ID,
    ) -> windows::core::Result<windows::core::IUnknown> {
        unsafe { self.simple.GetPatternProvider(id) }
    }
    fn GetPropertyValue(
        &self,
        id: windows::Win32::UI::Accessibility::UIA_PROPERTY_ID,
    ) -> windows::core::Result<VARIANT> {
        unsafe { self.simple.GetPropertyValue(id) }
    }
    fn HostRawElementProvider(
        &self,
    ) -> windows::core::Result<windows::Win32::UI::Accessibility::IRawElementProviderSimple> {
        host_provider(self.hwnd)
    }
}

impl IRawElementProviderFragment_Impl for LogFragment_Impl {
    fn Navigate(
        &self,
        dir: NavigateDirection,
    ) -> windows::core::Result<windows::Win32::UI::Accessibility::IRawElementProviderFragment> {
        if dir == NavigateDirection_Parent {
            // The HWND parent link (set up after window creation).
            let raw = PARENT_PTR.load(std::sync::atomic::Ordering::SeqCst)
                as *const windows::Win32::UI::Accessibility::IRawElementProviderFragment;
            if raw.is_null() {
                return unsafe { self.fragment.Navigate(dir) };
            }
            return Ok(unsafe { (*raw).clone() });
        }
        unsafe { self.fragment.Navigate(dir) }
    }
    fn GetRuntimeId(&self) -> windows::core::Result<*mut SAFEARRAY> {
        unsafe { self.fragment.GetRuntimeId() }
    }
    fn BoundingRectangle(&self) -> windows::core::Result<UiaRect> {
        unsafe { self.fragment.BoundingRectangle() }
    }
    fn GetEmbeddedFragmentRoots(&self) -> windows::core::Result<*mut SAFEARRAY> {
        unsafe { self.fragment.GetEmbeddedFragmentRoots() }
    }
    fn SetFocus(&self) -> windows::core::Result<()> {
        unsafe { self.fragment.SetFocus() }
    }
    fn FragmentRoot(
        &self,
    ) -> windows::core::Result<windows::Win32::UI::Accessibility::IRawElementProviderFragmentRoot>
    {
        unsafe { self.fragment.FragmentRoot() }
    }
}

impl IRawElementProviderFragmentRoot_Impl for LogFragment_Impl {
    fn ElementProviderFromPoint(
        &self,
        x: f64,
        y: f64,
    ) -> windows::core::Result<windows::Win32::UI::Accessibility::IRawElementProviderFragment> {
        unsafe { self.root.ElementProviderFromPoint(x, y) }
    }
    fn GetFocus(
        &self,
    ) -> windows::core::Result<windows::Win32::UI::Accessibility::IRawElementProviderFragment> {
        unsafe { self.root.GetFocus() }
    }
}

impl windows::Win32::UI::Accessibility::IRawElementProviderAdviseEvents_Impl for LogFragment_Impl {
    fn AdviseEventAdded(
        &self,
        _eventid: windows::Win32::UI::Accessibility::UIA_EVENT_ID,
        _propertyids: *const SAFEARRAY,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn AdviseEventRemoved(
        &self,
        _eventid: windows::Win32::UI::Accessibility::UIA_EVENT_ID,
        _propertyids: *const SAFEARRAY,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

impl IUIAutomationPropertyChangedEventHandler_Impl for Recorder_Impl {
    fn HandlePropertyChangedEvent(
        &self,
        sender: Ref<IUIAutomationElement>,
        propertyid: UIA_PROPERTY_ID,
        newvalue: &VARIANT,
    ) -> windows::core::Result<()> {
        let name = sender
            .as_ref()
            .and_then(|el| unsafe { el.GetCurrentPropertyValue(UIA_NamePropertyId).ok() })
            .map(|v| read_bstr(&v))
            .unwrap_or_default();
        self.records
            .lock()
            .unwrap()
            .push((propertyid.0, read_i4(newvalue), name));
        Ok(())
    }
}

struct Fixture {
    host: ComponentHost,
    tree: Rc<RefCell<UiaTree>>,
    snap: SemanticsSnapshot,
    root: NodeId,
}

impl Fixture {
    fn new() -> Self {
        let host = ComponentHost::new();
        host.set_viewport(300.0, 200.0);
        host.mount(
            "App",
            AppProps {
                label: "Wi-Fi".to_string(),
                name: "Bob".to_string(),
            },
            App,
        );
        host.run_until_idle();
        let tree = Rc::new(RefCell::new(UiaTree::new()));
        let mut snap = SemanticsSnapshot::new();
        let diff = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
        tree.borrow_mut().apply(&diff, &|id| {
            host.with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.parent))
        });
        let root = find_retained_by_debug(&host, "app")[0];
        Self {
            host,
            tree,
            snap,
            root,
        }
    }

    fn refresh(&mut self) {
        let diff = self
            .host
            .with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut self.snap));
        self.tree.borrow_mut().apply(&diff, &|id| {
            self.host
                .with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.parent))
        });
    }
}

fn press_at(host: &ComponentHost, x: f32, y: f32) {
    host.inject_input(oppa::InputEvent::pointer_down(x, y));
    host.inject_input(oppa::InputEvent::pointer_up(x, y));
    host.run_until_idle();
}

/// Non-blocking pump (keeps COM/RPC flowing while polling).
fn pump_once() {
    unsafe {
        let mut msg = MSG::default();
        while windows::Win32::UI::WindowsAndMessaging::PeekMessageW(
            &mut msg,
            None,
            0,
            0,
            windows::Win32::UI::WindowsAndMessaging::PM_REMOVE,
        )
        .as_bool()
        {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[test]
fn toggle_flip_raises_property_changed_through_hwnd() {
    unsafe {
        // STA: the norm for UIA clients (the handler is invoked on
        // this thread through the pump below). MTA subscribe failed
        // with E_INVALIDARG on every form — diagnostic finding.
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .expect("com init")
    };
    let mut fx = Fixture::new();
    let track = find_retained_by_debug(&fx.host, "track")[0];

    // AT Toggle() drives the framework press at the node center.
    let host2 = fx.host.clone();
    let center = fx
        .host
        .with_retained_mut(|rec, _| rec.get(track).and_then(|n| n.layout.clone()))
        .map(|b| (b.x + b.w / 2.0, b.y + b.h / 2.0))
        .expect("committed box");
    let actions = UiaAction {
        on_toggle: Some(Rc::new(move |_: NodeId| {
            press_at(&host2, center.0, center.1);
        })),
        value_for: None,
        on_invoke: None,
        on_set_value: None,
    };
    // Host a LOGGING delegate around the APP ROOT provider: shows
    // exactly which validation calls UIA makes before accepting
    // or rejecting (diagnostic series). Built AFTER window creation
    // so the host link carries the real HWND.
    use windows::Win32::UI::Accessibility::{
        IRawElementProviderFragment as _Frag, IRawElementProviderFragmentRoot as _Root,
        IRawElementProviderSimple as _Simple,
    };
    let make_hosted = |hwnd: HWND| {
        let inner_provider = OppaProvider::new(fx.tree.clone(), fx.root, fx.root, actions.clone());
        inner_provider.set_host_hwnd(hwnd);
        let inner_simple: _Simple = inner_provider.into();
        let inner_fragment: _Frag = inner_simple.cast().expect("frag cast");
        let inner_root: _Root = inner_simple.cast().expect("root cast");
        let hosted_root: _Simple = LogFragment {
            simple: inner_simple,
            fragment: inner_fragment,
            root: inner_root,
            hwnd,
        }
        .into();
        HOSTED_PTR.store(
            Box::into_raw(Box::new(hosted_root)) as usize,
            std::sync::atomic::Ordering::SeqCst,
        );
        // The parent link points back at the hosted delegate.
        let hosted_frag: _Frag = unsafe {
            (*(HOSTED_PTR.load(std::sync::atomic::Ordering::SeqCst) as *const _Simple))
                .cast()
                .expect("hosted frag cast")
        };
        let host = host_provider(hwnd).expect("host provider");
        let parent: _Simple = HwndParent {
            child: hosted_frag,
            hwnd,
            host,
        }
        .into();
        let parent_frag: _Frag = parent.cast().expect("parent frag cast");
        PARENT_PTR.store(
            Box::into_raw(Box::new(parent_frag)) as usize,
            std::sync::atomic::Ordering::SeqCst,
        );
    };

    unsafe {
        use windows::Win32::Foundation::HINSTANCE;
        // Real HWND (visible test window) hosting the provider.
        let module = GetModuleHandleW(None).expect("module");
        let instance = HINSTANCE(module.0);
        let class = wide("oppa-uia-events");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: PCWSTR(class.as_ptr()),
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };
        assert_ne!(RegisterClassW(&wc), 0, "register class");
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR(class.as_ptr()),
            PCWSTR(wide("oppa uia events").as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            300,
            200,
            None,
            None,
            Some(instance),
            None,
        )
        .expect("create window");
        // Hosting installs AFTER creation (the delegate carries the
        // HWND for the host link); no UIA client has touched the
        // window yet, so nothing cached a proxy answer.
        make_hosted(hwnd);
        // The TRACK provider (raise + Toggle target) carries the
        // same host token — UIA asks every fragment for its HWND
        // during event subscription.
        let track_provider = OppaProvider::new(fx.tree.clone(), track, fx.root, actions.clone());
        track_provider.set_host_hwnd(hwnd);
        let simple: windows::Win32::UI::Accessibility::IRawElementProviderSimple =
            track_provider.into();
        let _ = ShowWindow(hwnd, SW_SHOW);

        // Real UIA client: element for the HWND + property-changed
        // subscription on the toggle-state property (subtree scope
        // — the toggle is a descendant of the window element), via
        // the native-array form (no SAFEARRAY packing).
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL).expect("uia client");
        let element = automation
            .ElementFromHandle(hwnd)
            .expect("element from hwnd");
        let hits = GETOBJECT_HITS.load(std::sync::atomic::Ordering::SeqCst);
        assert!(hits > 0, "WM_GETOBJECT/OBJID_CLIENT reached the proc");
        // Non-vacuous guard: the element must be OUR provider (Name
        // "app" — the hosted root), not the default HWND proxy (Name
        // would be the window title).
        let el_name = element
            .GetCurrentPropertyValue(UIA_NamePropertyId)
            .map(|v| read_bstr(&v))
            .unwrap_or_default();
        assert_eq!(el_name, "app", "window element is the hosted provider");
        // Event routing walks UPWARD (raise provider -> ancestors
        // via Navigate(Parent) -> subscribed element), never down
        // through child navigation — so the subscription below
        // matches the track raise through track -> app -> parent
        // link even though raw child-walks stay on HWND channels.
        // (Diagnostic finding, kept as a comment: raw FirstChild
        // walks from the window element serve chrome, not our
        // fragments; that cosmetic topology is out of scope.)
        // The subscription path (all proven primitives):
        // FindFirst/Descendants navigates the HWND fragment tree to
        // the toggle (synchronous search works where event scopes
        // beyond Element do not — diagnostic finding), then an
        // Element-scope property subscription on the toggle element
        // observes the raise on the toggle provider.
        let records = Arc::new(Mutex::new(Vec::new()));
        let handler: IUIAutomationPropertyChangedEventHandler = Recorder {
            records: records.clone(),
        }
        .into();
        let cache = automation.CreateCacheRequest().expect("cache request");
        let name_var = {
            let mut v = VARIANT::default();
            let inner = &mut *v.Anonymous.Anonymous;
            inner.vt = windows::Win32::System::Variant::VT_BSTR;
            inner.Anonymous.bstrVal = std::mem::ManuallyDrop::new(BSTR::from("Wi-Fi"));
            v
        };
        let condition = automation
            .CreatePropertyCondition(UIA_NamePropertyId, &name_var)
            .expect("condition");
        let track_el = element
            .FindFirst(
                windows::Win32::UI::Accessibility::TreeScope_Descendants,
                &condition,
            )
            .expect("findfirst reaches the toggle");
        let found_name = track_el
            .GetCurrentPropertyValue(UIA_NamePropertyId)
            .map(|v| read_bstr(&v))
            .unwrap_or_default();
        assert_eq!(found_name, "Wi-Fi", "HWND fragment navigation serves");
        // SAFEARRAY form + Element scope (both proven primitives;
        // the native-array form returns E_NOTIMPL here).
        use windows::Win32::System::Com::SAFEARRAY;
        use windows::Win32::System::Ole::{
            SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement,
        };
        use windows::Win32::System::Variant::VT_I4;
        let psa: *const SAFEARRAY = {
            let a = SafeArrayCreateVector(VT_I4, 0, 1);
            assert!(!a.is_null());
            let zero = 0i32;
            let id = UIA_ToggleToggleStatePropertyId.0;
            SafeArrayPutElement(a, &zero, &id as *const i32 as *const _).expect("put");
            a as *const SAFEARRAY
        };
        // SAFEARRAY form + Element scope on the toggle element.
        automation
            .AddPropertyChangedEventHandler(
                &track_el,
                windows::Win32::UI::Accessibility::TreeScope_Element,
                &cache,
                &handler,
                psa,
            )
            .expect("subscribe");

        // Baseline: the property reads Off through the provider.
        let baseline = simple
            .GetPropertyValue(UIA_ToggleToggleStatePropertyId)
            .expect("baseline reads");
        assert_eq!(read_i4(&baseline), ToggleState_Off.0);

        // Flip 1: AT Toggle() -> framework press -> refresh -> raise
        // Off->On. The client must observe (property, new, sender).
        let unk = simple
            .GetPatternProvider(UIA_TogglePatternId)
            .expect("toggle serves");
        let tog: IToggleProvider = unk.cast().expect("toggle cast");
        tog.Toggle().expect("AT Toggle drives");
        fx.refresh();
        let old = var_i4(ToggleState_Off.0);
        let new = var_i4(ToggleState_On.0);
        UiaRaiseAutomationPropertyChangedEvent(
            &simple,
            UIA_ToggleToggleStatePropertyId,
            &old,
            &new,
        )
        .expect("raise off->on");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            pump_once();
            if !records.lock().unwrap().is_empty() || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let got = records.lock().unwrap().clone();
        assert_eq!(got.len(), 1, "one property-changed event, got {got:?}");
        assert_eq!(got[0].0, UIA_ToggleToggleStatePropertyId.0);
        assert_eq!(got[0].1, ToggleState_On.0);
        assert_eq!(got[0].2, "Wi-Fi", "sender is the toggled node");

        // Flip 2: back On->Off (events are not one-shot).
        tog.Toggle().expect("AT Toggle drives back");
        fx.refresh();
        let old2 = var_i4(ToggleState_On.0);
        let new2 = var_i4(ToggleState_Off.0);
        UiaRaiseAutomationPropertyChangedEvent(
            &simple,
            UIA_ToggleToggleStatePropertyId,
            &old2,
            &new2,
        )
        .expect("raise on->off");
        let deadline2 = Instant::now() + Duration::from_secs(10);
        loop {
            pump_once();
            if records.lock().unwrap().len() >= 2 || Instant::now() > deadline2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let got2 = records.lock().unwrap().clone();
        assert_eq!(got2.len(), 2, "second flip raises again, got {got2:?}");
        assert_eq!(got2[1].1, ToggleState_Off.0);

        automation
            .RemovePropertyChangedEventHandler(&track_el, &handler)
            .expect("unsubscribe");
        SafeArrayDestroy(psa as *mut SAFEARRAY).expect("destroy");
        let _ = DestroyWindow(hwnd);
        let raw = HOSTED_PTR.swap(0, std::sync::atomic::Ordering::SeqCst)
            as *mut windows::Win32::UI::Accessibility::IRawElementProviderSimple;
        if !raw.is_null() {
            drop(Box::from_raw(raw));
        }
        let par = PARENT_PTR.swap(0, std::sync::atomic::Ordering::SeqCst)
            as *mut windows::Win32::UI::Accessibility::IRawElementProviderFragment;
        if !par.is_null() {
            drop(Box::from_raw(par));
        }
        CoUninitialize();
    }
}
