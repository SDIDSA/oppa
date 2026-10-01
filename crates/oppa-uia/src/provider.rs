//! The COM provider (M10 close-v1): one `OppaProvider` per node,
//! reading a shared [`UiaTree`](super::tree::UiaTree) snapshot.
//!
//! Unsupported patterns fail with `E_NOINTERFACE` (not NULL+S_OK —
//! both signal absence; the failure is explicit and the test
//! asserts it). `SetFocus` and pattern-less `Toggle`/`Select`
//! without an installed action callback fail `E_NOTIMPL`: focus
//! follows click in v1 (M5) and actions need a driver — both
//! loud, never silent no-ops.

// Windows API constants keep their canonical mixed-case names.
#![allow(non_upper_case_globals)]

use std::cell::RefCell;
use std::rc::Rc;

use windows::core::BOOL;
use windows::core::{implement, Error, IUnknown, BSTR};
use windows::Win32::Foundation::{E_NOINTERFACE, E_NOTIMPL, E_OUTOFMEMORY, HWND, VARIANT_BOOL};
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
use windows::Win32::System::Variant::{VARIANT, VT_ARRAY, VT_BOOL, VT_BSTR, VT_I4, VT_R8};
use windows::Win32::UI::Accessibility::{
    IRawElementProviderAdviseEvents, IRawElementProviderAdviseEvents_Impl,
};
use windows::Win32::UI::Accessibility::{
    IRawElementProviderFragment, IRawElementProviderFragmentRoot,
    IRawElementProviderFragmentRoot_Impl, IRawElementProviderFragment_Impl,
    IRawElementProviderSimple, IRawElementProviderSimple_Impl, ISelectionItemProvider,
    ISelectionItemProvider_Impl, IToggleProvider, IToggleProvider_Impl, IValueProvider,
    IValueProvider_Impl, NavigateDirection, NavigateDirection_FirstChild,
    NavigateDirection_LastChild, NavigateDirection_NextSibling, NavigateDirection_Parent,
    NavigateDirection_PreviousSibling, ProviderOptions, ProviderOptions_ServerSideProvider,
    ToggleState, ToggleState_Indeterminate, ToggleState_Off, ToggleState_On,
    UIA_BoundingRectanglePropertyId, UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId,
    UIA_ComboBoxControlTypeId, UIA_ControlTypePropertyId, UIA_EditControlTypeId,
    UIA_GroupControlTypeId, UIA_IsEnabledPropertyId, UIA_ListItemControlTypeId, UIA_NamePropertyId,
    UIA_ProgressBarControlTypeId, UIA_RadioButtonControlTypeId, UIA_SelectionItemPatternId,
    UIA_SliderControlTypeId, UIA_StatusBarControlTypeId, UIA_TabControlTypeId,
    UIA_TabItemControlTypeId, UIA_TogglePatternId, UIA_ToggleToggleStatePropertyId,
    UIA_ValuePatternId, UIA_WindowControlTypeId, UiaAppendRuntimeId, UiaRect, UIA_CONTROLTYPE_ID,
    UIA_PATTERN_ID, UIA_PROPERTY_ID,
};

use super::tree::UiaTree;
use oppa::{NodeId, Role};

/// Host-loop drivers the provider calls into for AT actions (and
/// the text seam for Value reads). Uninstalled entries behave
/// loudly (`E_NOTIMPL`), never as silent no-ops.
#[derive(Clone, Default)]
pub struct UiaAction {
    /// Press-like activation for Toggle/Select (test wires this to
    /// `inject_input` at the node's center).
    pub on_toggle: Option<Rc<dyn Fn(NodeId)>>,
    /// Current text for Value reads (the M1 editor seam).
    pub value_for: Option<Rc<dyn Fn(NodeId) -> String>>,
}

/// UIA control type for one framework role (the total table; G2 adds
/// Button/Checkbox/Slider — Button/Slider expose no AT-action pattern
/// in v1, OQ-G2-2; decision 241 adds Dialog as a Window — UIA has no
/// separate dialog type, dialog windows surface as windows; decision
/// 244 adds RadioButton as its own type; decision 245 adds Tab +
/// TabList as their own types; decision 247 adds ComboBox as its own
/// type — ExpandCollapse for the open state is OQ-G2-2 class, the box
/// exposes no AT-action pattern in v1; decision 251 adds ProgressBar
/// as its own type — RangeValue for the percentage is OQ-G2-2 class,
/// the bar exposes no AT-action pattern in v1; round 5.1 adds
/// TextArea as Edit (same contract as TextField — multi-line is a
/// Value-pattern detail UIA does not type separately)).
pub fn uia_control_type(role: Role) -> UIA_CONTROLTYPE_ID {
    match role {
        Role::Switch => UIA_CheckBoxControlTypeId,
        Role::Checkbox => UIA_CheckBoxControlTypeId,
        Role::ListItem => UIA_ListItemControlTypeId,
        Role::TextField | Role::TextArea => UIA_EditControlTypeId,
        Role::Button => UIA_ButtonControlTypeId,
        Role::Slider => UIA_SliderControlTypeId,
        Role::Dialog => UIA_WindowControlTypeId,
        Role::RadioButton => UIA_RadioButtonControlTypeId,
        Role::Tab => UIA_TabItemControlTypeId,
        Role::TabList => UIA_TabControlTypeId,
        Role::ComboBox => UIA_ComboBoxControlTypeId,
        Role::ProgressBar => UIA_ProgressBarControlTypeId,
        // StatusBar is the closest stock type for a transient message
        // (decision 337 — the Toast card; announced, never focused).
        Role::Status => UIA_StatusBarControlTypeId,
        Role::Generic => UIA_GroupControlTypeId,
    }
}

#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot,
    IRawElementProviderAdviseEvents,
    IToggleProvider,
    ISelectionItemProvider,
    IValueProvider
)]
pub struct OppaProvider {
    tree: Rc<RefCell<UiaTree>>,
    id: NodeId,
    root: NodeId,
    actions: UiaAction,
    /// Hosting-layer HWND association (unset = headless). The
    /// framework provider never creates windows (decision 149);
    /// the shell window (or test host) installs its HWND here so
    /// `HostRawElementProvider` can answer in HWND context.
    /// Unset answers stay loud `E_NOTIMPL` (uia_emit's scope).
    host: RefCell<Option<HWND>>,
}

impl OppaProvider {
    pub fn new(tree: Rc<RefCell<UiaTree>>, id: NodeId, root: NodeId, actions: UiaAction) -> Self {
        Self {
            tree,
            id,
            root,
            actions,
            host: RefCell::new(None),
        }
    }

    /// Installs the hosting HWND (the shell window owns this call).
    pub fn set_host_hwnd(&self, hwnd: HWND) {
        *self.host.borrow_mut() = Some(hwnd);
    }

    fn node<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&super::tree::UiaNode) -> R,
    {
        self.tree
            .borrow()
            .get(self.id)
            .map(f)
            .ok_or_else(|| Error::from(E_NOINTERFACE))
    }

    /// A derived provider for navigation results: same tree, root,
    /// and actions, with the host token propagated (a tokenless
    /// derived fragment breaks HWND-context event subscription —
    /// UIA asks the fragment root for its host during subscribe).
    fn derived(&self, id: NodeId) -> OppaProvider {
        let next = OppaProvider::new(self.tree.clone(), id, self.root, self.actions.clone());
        if let Some(hwnd) = self.host.borrow().as_ref().copied() {
            next.set_host_hwnd(hwnd);
        }
        next
    }

    fn sibling(&self, dir: NavigateDirection) -> Result<IRawElementProviderFragment, Error> {
        let tree = self.tree.borrow();
        let me = tree
            .get(self.id)
            .ok_or_else(|| Error::from(E_NOINTERFACE))?;
        // Missing targets fail E_NOINTERFACE (null fragments are not
        // representable — absence is explicit, tested).
        let frag = |id: NodeId| {
            let p: IRawElementProviderFragment = self.derived(id).into();
            Ok(p)
        };
        let target: Option<Result<IRawElementProviderFragment, Error>> = match dir {
            NavigateDirection_Parent => me.parent.map(frag),
            NavigateDirection_FirstChild => {
                tree.children_of(Some(self.id)).first().map(|c| frag(*c))
            }
            NavigateDirection_LastChild => tree.children_of(Some(self.id)).last().map(|c| frag(*c)),
            NavigateDirection_NextSibling | NavigateDirection_PreviousSibling => {
                let sibs = tree.children_of(me.parent);
                sibs.iter()
                    .position(|s| *s == self.id)
                    .and_then(|i| {
                        if dir == NavigateDirection_NextSibling {
                            sibs.get(i + 1)
                        } else {
                            i.checked_sub(1).and_then(|j| sibs.get(j))
                        }
                    })
                    .map(|n| frag(*n))
            }
            _ => Some(Err(Error::from(E_NOTIMPL))),
        };
        match target {
            Some(r) => r,
            None => Err(Error::from(E_NOINTERFACE)),
        }
    }
}

fn var_bstr(s: &str) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_BSTR;
        inner.Anonymous.bstrVal = std::mem::ManuallyDrop::new(BSTR::from(s));
    }
    v
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

fn var_bool(b: bool) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_BOOL;
        inner.Anonymous.boolVal = VARIANT_BOOL(if b { -1 } else { 0 });
    }
    v
}

fn safearray_i4(values: &[i32]) -> Result<*mut SAFEARRAY, Error> {
    unsafe {
        let psa = SafeArrayCreateVector(VT_I4, 0, values.len() as u32);
        if psa.is_null() {
            return Err(Error::from(E_OUTOFMEMORY));
        }
        for (i, v) in values.iter().enumerate() {
            let idx = i as i32;
            SafeArrayPutElement(psa, &idx, v as *const i32 as *const _)?;
        }
        Ok(psa)
    }
}

fn safearray_f64(values: &[f64]) -> Result<*mut SAFEARRAY, Error> {
    unsafe {
        let psa = SafeArrayCreateVector(VT_R8, 0, values.len() as u32);
        if psa.is_null() {
            return Err(Error::from(E_OUTOFMEMORY));
        }
        for (i, v) in values.iter().enumerate() {
            let idx = i as i32;
            SafeArrayPutElement(psa, &idx, v as *const f64 as *const _)?;
        }
        Ok(psa)
    }
}

fn var_array_f64(values: &[f64]) -> Result<VARIANT, Error> {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_ARRAY;
        // VARENUM has no BitOr; the flag word is plain u16 math.
        inner.vt.0 |= VT_R8.0;
        inner.Anonymous.parray = safearray_f64(values)?;
    }
    Ok(v)
}

impl IRawElementProviderSimple_Impl for OppaProvider_Impl {
    fn ProviderOptions(&self) -> windows::core::Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, patternid: UIA_PATTERN_ID) -> windows::core::Result<IUnknown> {
        let role = self.node(|n| n.role)?;
        let supported = matches!(
            (role, patternid),
            (Role::Switch, UIA_TogglePatternId)
                | (Role::Checkbox, UIA_TogglePatternId)
                | (Role::ListItem, UIA_SelectionItemPatternId)
                | (Role::RadioButton, UIA_SelectionItemPatternId)
                | (Role::Tab, UIA_SelectionItemPatternId)
                | (Role::TextField, UIA_ValuePatternId)
        ) && (role != Role::TextField || self.actions.value_for.is_some());
        if !supported {
            return Err(Error::from(E_NOINTERFACE));
        }
        let p: IUnknown = self.derived(self.id).into();
        Ok(p)
    }

    fn GetPropertyValue(&self, propertyid: UIA_PROPERTY_ID) -> windows::core::Result<VARIANT> {
        let tree = self.tree.borrow();
        let n = tree
            .get(self.id)
            .ok_or_else(|| Error::from(E_NOINTERFACE))?;
        if propertyid == UIA_NamePropertyId {
            Ok(var_bstr(&n.name))
        } else if propertyid == UIA_ControlTypePropertyId {
            Ok(var_i4(uia_control_type(n.role).0))
        } else if propertyid == UIA_IsEnabledPropertyId {
            Ok(var_bool(!n.disabled))
        } else if propertyid == UIA_ToggleToggleStatePropertyId {
            // CheckBox state as a readable property (the event test
            // subscribes to it; pattern reads stay on IToggleProvider).
            // Non-switch roles report Indeterminate, never a stale state
            // (G2: Checkbox rides the same arms as Switch).
            let state = match (n.role, n.checked) {
                (Role::Switch, Some(true)) | (Role::Checkbox, Some(true)) => ToggleState_On.0,
                (Role::Switch, Some(false)) | (Role::Checkbox, Some(false)) => ToggleState_Off.0,
                _ => ToggleState_Indeterminate.0,
            };
            Ok(var_i4(state))
        } else if propertyid == UIA_BoundingRectanglePropertyId {
            let (x, y, w, h) = n.bounds;
            var_array_f64(&[x as f64, y as f64, w as f64, h as f64])
        } else {
            Ok(VARIANT::default())
        }
    }

    fn HostRawElementProvider(&self) -> windows::core::Result<IRawElementProviderSimple> {
        // HWND context only: the hosting layer installs its window
        // via `set_host_hwnd`; headless use stays loud E_NOTIMPL.
        match self.host.borrow().as_ref().copied() {
            Some(hwnd) => unsafe {
                windows::Win32::UI::Accessibility::UiaHostProviderFromHwnd(hwnd)
                    .map_err(|_| Error::from(E_NOTIMPL))
            },
            None => Err(Error::from(E_NOTIMPL)),
        }
    }
}

impl IRawElementProviderFragment_Impl for OppaProvider_Impl {
    fn Navigate(
        &self,
        direction: NavigateDirection,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        self.sibling(direction)
    }

    fn GetRuntimeId(&self) -> windows::core::Result<*mut SAFEARRAY> {
        safearray_i4(&[
            UiaAppendRuntimeId as i32,
            self.id.index() as i32,
            self.id.generation() as i32,
        ])
    }

    fn BoundingRectangle(&self) -> windows::core::Result<UiaRect> {
        self.node(|n| UiaRect {
            left: n.bounds.0 as f64,
            top: n.bounds.1 as f64,
            width: n.bounds.2 as f64,
            height: n.bounds.3 as f64,
        })
    }

    fn GetEmbeddedFragmentRoots(&self) -> windows::core::Result<*mut SAFEARRAY> {
        safearray_i4(&[])
    }

    fn SetFocus(&self) -> windows::core::Result<()> {
        // Focus follows click in v1 (M5) — no AT-driven focus path.
        Err(Error::from(E_NOTIMPL))
    }

    fn FragmentRoot(&self) -> windows::core::Result<IRawElementProviderFragmentRoot> {
        let p: IRawElementProviderFragmentRoot = self.derived(self.root).into();
        Ok(p)
    }
}

impl IRawElementProviderFragmentRoot_Impl for OppaProvider_Impl {
    fn ElementProviderFromPoint(
        &self,
        _x: f64,
        _y: f64,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        // Hit-testing lives in the framework router (M5) — the root
        // does not duplicate it. Loud, not a wrong element.
        Err(Error::from(E_NOTIMPL))
    }

    fn GetFocus(&self) -> windows::core::Result<IRawElementProviderFragment> {
        let p: IRawElementProviderFragment = self.derived(self.root).into();
        Ok(p)
    }
}

impl IRawElementProviderAdviseEvents_Impl for OppaProvider_Impl {
    fn AdviseEventAdded(
        &self,
        _eventid: windows::Win32::UI::Accessibility::UIA_EVENT_ID,
        _propertyids: *const SAFEARRAY,
    ) -> windows::core::Result<()> {
        // Capability advertisement: the host loop raises explicitly
        // (`UiaRaiseAutomationPropertyChangedEvent`) whenever the
        // tree flips, so there is no pump to start — but UIA refuses
        // event subscriptions on providers without this interface
        // (E_NOTIMPL to the client), so serving it is required.
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

impl IToggleProvider_Impl for OppaProvider_Impl {
    fn Toggle(&self) -> windows::core::Result<()> {
        match &self.actions.on_toggle {
            Some(f) => {
                f(self.id);
                Ok(())
            }
            None => Err(Error::from(E_NOTIMPL)),
        }
    }

    fn ToggleState(&self) -> windows::core::Result<ToggleState> {
        self.node(|n| match n.checked {
            Some(true) => ToggleState_On,
            Some(false) => ToggleState_Off,
            None => ToggleState_Indeterminate,
        })
    }
}

impl ISelectionItemProvider_Impl for OppaProvider_Impl {
    fn Select(&self) -> windows::core::Result<()> {
        self.AddToSelection()
    }

    fn AddToSelection(&self) -> windows::core::Result<()> {
        match &self.actions.on_toggle {
            Some(f) => {
                f(self.id);
                Ok(())
            }
            None => Err(Error::from(E_NOTIMPL)),
        }
    }

    fn RemoveFromSelection(&self) -> windows::core::Result<()> {
        Err(Error::from(E_NOTIMPL))
    }

    fn IsSelected(&self) -> windows::core::Result<BOOL> {
        self.node(|n| BOOL::from(n.selected == Some(true)))
    }

    fn SelectionContainer(&self) -> windows::core::Result<IRawElementProviderSimple> {
        let parent = self
            .node(|n| n.parent)?
            .ok_or_else(|| Error::from(E_NOINTERFACE))?;
        let p: IRawElementProviderSimple = self.derived(parent).into();
        Ok(p)
    }
}

impl IValueProvider_Impl for OppaProvider_Impl {
    fn SetValue(&self, _val: &windows::core::PCWSTR) -> windows::core::Result<()> {
        // AT-driven editing is out of v1 scope (M1 editor seam) — loud.
        Err(Error::from(E_NOTIMPL))
    }

    fn Value(&self) -> windows::core::Result<windows::core::BSTR> {
        match &self.actions.value_for {
            Some(f) => Ok(windows::core::BSTR::from(f(self.id).as_str())),
            None => Err(Error::from(E_NOTIMPL)),
        }
    }

    fn IsReadOnly(&self) -> windows::core::Result<BOOL> {
        Ok(BOOL::from(true))
    }
}
