//! M10 close-v1 UIA proof: toggle + list-item semantics flow from
//! the retained tree through `compute_semantics_diff` into the
//! `UiaTree`, where real UIA interfaces serve them — and AT
//! actions drive back into the framework (Toggle()/Select() flip
//! framework state, re-read through the same interfaces).
//!
//! In-process COM (MTA), no HWND hosting: the provider layer is
//! what's under test — the same scope the AT-SPI emitter layer
//! holds, plus action round-trips (which need no window).

#![cfg(windows)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use oppa::{
    compute_semantics_diff, find_retained_by_debug, ComponentHost, Ctx, NodeId, Semantics,
    SemanticsSnapshot, Style, VNode,
};
use oppa_macros::{component, Props};
use oppa_uia::{uia_control_type, OppaProvider, UiaAction, UiaTree};
use windows::core::Interface;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::System::Ole::SafeArrayGetElement;
use windows::Win32::UI::Accessibility::{
    IRawElementProviderFragment, IRawElementProviderSimple, ISelectionItemProvider,
    IToggleProvider, NavigateDirection_FirstChild, NavigateDirection_NextSibling,
    NavigateDirection_Parent, ToggleState_Off, ToggleState_On, UIA_CheckBoxControlTypeId,
    UIA_ControlTypePropertyId, UIA_IsEnabledPropertyId, UIA_ListItemControlTypeId,
    UIA_NamePropertyId, UIA_SelectionItemPatternId, UIA_TogglePatternId,
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
    key: u64,
}

#[component]
fn Row(ctx: &Ctx, props: &RowProps) -> VNode {
    // Keyed flag (M8 shape): AT Select() presses, the flag flips.
    let flag: oppa::Signal<bool> = ctx.keyed_state(props.key, || false);
    let f = flag.clone();
    oppa::Div("row")
        .style(Style::new().size(200, 24))
        .semantics(
            Semantics::list_item()
                .selected(flag.get())
                .label(&props.name),
        )
        .on_press(move || f.set(!f.get()))
        .build()
}

#[derive(Clone, Props)]
struct AppProps {
    toggle: ToggleProps,
    row: RowProps,
}

#[component]
fn App(ctx: &Ctx, props: &AppProps) -> VNode {
    let _ = ctx.signal(0u32);
    oppa::Div("app")
        .semantics(Semantics::default().label("app"))
        .children([
            ctx.child("Toggle", 1, &props.toggle, Toggle),
            ctx.child("Row", 2, &props.row, Row),
        ])
}

fn read_bstr(v: &windows::Win32::System::Variant::VARIANT) -> String {
    unsafe {
        let b: &windows::core::BSTR = &v.Anonymous.Anonymous.Anonymous.bstrVal;
        String::from_utf16_lossy(std::slice::from_raw_parts(b.as_ptr(), b.len()))
    }
}

fn read_i4(v: &windows::Win32::System::Variant::VARIANT) -> i32 {
    unsafe { v.Anonymous.Anonymous.Anonymous.lVal }
}

fn read_bool(v: &windows::Win32::System::Variant::VARIANT) -> bool {
    unsafe { v.Anonymous.Anonymous.Anonymous.boolVal.0 != 0 }
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
                toggle: ToggleProps {
                    label: "Wi-Fi".to_string(),
                    initial: false,
                },
                row: RowProps {
                    name: "Bob".to_string(),
                    key: 7,
                },
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

    /// Recomputes the semantics delta and applies it (the tree reads
    /// live — providers need no rebuild).
    fn refresh(&mut self) {
        let diff = self
            .host
            .with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut self.snap));
        self.tree.borrow_mut().apply(&diff, &|id| {
            self.host
                .with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.parent))
        });
    }

    fn provider(&self, id: NodeId, actions: UiaAction) -> IRawElementProviderSimple {
        OppaProvider::new(self.tree.clone(), id, self.root, actions).into()
    }

    fn center_of(&self, id: NodeId) -> (f32, f32) {
        let b = self
            .host
            .with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.layout.clone()));
        let b = b.expect("committed box");
        (b.x + b.w / 2.0, b.y + b.h / 2.0)
    }
}

fn press_at(host: &ComponentHost, x: f32, y: f32) {
    host.inject_input(oppa::InputEvent::pointer_down(x, y));
    host.inject_input(oppa::InputEvent::pointer_up(x, y));
    host.run_until_idle();
}

#[test]
fn toggle_serves_and_drives_through_uia() {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .expect("com init")
    };
    let mut fx = Fixture::new();
    let track = find_retained_by_debug(&fx.host, "track")[0];
    let row = find_retained_by_debug(&fx.host, "row")[0];

    // Action driver: AT Toggle()/Select() presses the node's center
    // through the shared pipeline and settles synchronously.
    let mut points = HashMap::new();
    points.insert(track, fx.center_of(track));
    points.insert(row, fx.center_of(row));
    let host2 = fx.host.clone();
    let actions = UiaAction {
        on_toggle: Some(Rc::new(move |id: NodeId| {
            let (x, y) = points[&id];
            press_at(&host2, x, y);
        })),
        value_for: None,
    };

    // Properties through the real interface (all COM calls are
    // unsafe — one block for the whole client half).
    unsafe {
        let simple = fx.provider(track, actions.clone());
        let name = simple.GetPropertyValue(UIA_NamePropertyId).expect("Name");
        assert_eq!(read_bstr(&name), "Wi-Fi");
        let ct = simple
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("ControlType");
        assert_eq!(read_i4(&ct), UIA_CheckBoxControlTypeId.0);
        assert_eq!(
            uia_control_type(oppa::Role::Switch).0,
            UIA_CheckBoxControlTypeId.0
        );
        let en = simple
            .GetPropertyValue(UIA_IsEnabledPropertyId)
            .expect("IsEnabled");
        assert!(read_bool(&en));

        // Toggle pattern: Off → AT Toggle() → framework press → On.
        let unk = simple
            .GetPatternProvider(UIA_TogglePatternId)
            .expect("toggle pattern serves");
        let tog: IToggleProvider = unk.cast().expect("toggle cast");
        assert_eq!(tog.ToggleState().expect("state"), ToggleState_Off);
        tog.Toggle().expect("AT Toggle drives");
        fx.refresh();
        assert_eq!(tog.ToggleState().expect("state"), ToggleState_On);

        // ListItem: SelectionItem pattern reads; Select() drives.
        let rsimple = fx.provider(row, actions.clone());
        let rct = rsimple
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("row type");
        assert_eq!(read_i4(&rct), UIA_ListItemControlTypeId.0);
        let runk = rsimple
            .GetPatternProvider(UIA_SelectionItemPatternId)
            .expect("selection pattern serves");
        let sel: ISelectionItemProvider = runk.cast().expect("selection cast");
        assert!(sel.IsSelected().expect("unselected").0 == 0);
        sel.Select().expect("AT Select drives");
        fx.refresh();
        assert!(sel.IsSelected().expect("selected").0 != 0);

        // Unsupported pattern fails loudly (toggle pattern on a row).
        assert!(rsimple.GetPatternProvider(UIA_TogglePatternId).is_err());

        // Fragment navigation: row → parent (root) → first child.
        let frag: IRawElementProviderFragment =
            OppaProvider::new(fx.tree.clone(), row, fx.root, UiaAction::default()).into();
        let parent = frag
            .Navigate(NavigateDirection_Parent)
            .expect("parent navigates");
        let first = parent
            .Navigate(NavigateDirection_FirstChild)
            .expect("first child navigates");
        let sib = first
            .Navigate(NavigateDirection_NextSibling)
            .expect("sibling navigates");
        // Runtime ids are [UiaAppendRuntimeId, index, generation].
        let rid = sib.GetRuntimeId().expect("runtime id");
        let mut parts = [0i32; 3];
        for (i, p) in parts.iter_mut().enumerate() {
            let idx = i as i32;
            SafeArrayGetElement(rid, &idx, p as *mut i32 as *mut _).expect("rid element");
        }
        assert_eq!(parts[0], 3);
        assert_eq!(parts[1] as u32, row.index());
        CoUninitialize();
    }
}

// ---------------------------------------------------------------------------
// G13 catalog leg: checkbox Toggle round-trips through COM;
// button/slider serve control types (+ button documents the
// Invoke-pattern OQ by refusing loudly).
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct CheckProps {
    label: String,
    initial: bool,
}

#[component]
fn Check(ctx: &Ctx, props: &CheckProps) -> VNode {
    let checked = ctx.signal(props.initial);
    let c = checked.clone();
    oppa::Div("check")
        .style(Style::new().size(44, 24))
        .semantics(
            Semantics::checkbox()
                .checked(checked.get())
                .label(&props.label),
        )
        .on_press(move || c.set(!c.get()))
        .build()
}

#[derive(Clone, Props)]
struct BtnProps {
    label: String,
}

#[component]
fn Btn(ctx: &Ctx, props: &BtnProps) -> VNode {
    let _ = ctx.signal(0u32);
    oppa::Div("btn")
        .style(Style::new().size(96, 32))
        .semantics(Semantics::button().label(&props.label))
        .on_press(|| {})
        .build()
}

#[derive(Clone, Props)]
struct SldProps {
    label: String,
}

#[component]
fn Sld(ctx: &Ctx, props: &SldProps) -> VNode {
    let _ = ctx.signal(0u32);
    oppa::Div("sld")
        .style(Style::new().size(160, 32))
        .semantics(
            Semantics::slider()
                .label(&props.label)
                .value_text("50 percent"),
        )
        .build()
}

#[derive(Clone, Props)]
struct CatalogAppProps {
    check: CheckProps,
    btn: BtnProps,
    sld: SldProps,
}

#[component]
fn CatalogApp(ctx: &Ctx, props: &CatalogAppProps) -> VNode {
    let _ = ctx.signal(0u32);
    oppa::Div("catalog").children([
        ctx.child("Check", 1, &props.check, Check),
        ctx.child("Btn", 2, &props.btn, Btn),
        ctx.child("Sld", 3, &props.sld, Sld),
    ])
}

#[test]
fn catalog_serves_and_drives_through_uia() {
    use windows::Win32::UI::Accessibility::{
        UIA_ButtonControlTypeId, UIA_ComboBoxControlTypeId, UIA_ProgressBarControlTypeId,
        UIA_RadioButtonControlTypeId, UIA_SliderControlTypeId, UIA_TabControlTypeId,
        UIA_TabItemControlTypeId,
    };
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .expect("com init")
    };
    let host = ComponentHost::new();
    host.set_viewport(300.0, 200.0);
    host.mount(
        "CatalogApp",
        CatalogAppProps {
            check: CheckProps {
                label: "T&C".to_string(),
                initial: false,
            },
            btn: BtnProps {
                label: "OK".to_string(),
            },
            sld: SldProps {
                label: "Volume".to_string(),
            },
        },
        CatalogApp,
    );
    host.run_until_idle();
    let tree = Rc::new(RefCell::new(UiaTree::new()));
    let mut snap = SemanticsSnapshot::new();
    let diff = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    tree.borrow_mut().apply(&diff, &|id| {
        host.with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.parent))
    });
    // Roles ride the component Divs (semantics attached inline).
    let check = find_retained_by_debug(&host, "check")[0];
    let btn = find_retained_by_debug(&host, "btn")[0];
    let sld = find_retained_by_debug(&host, "sld")[0];

    let host_c = host.clone();
    let center = move |id: NodeId| {
        let b = host_c.committed_box(id).expect("box");
        (b.x + b.w / 2.0, b.y + b.h / 2.0)
    };
    let host2 = host.clone();
    let actions = UiaAction {
        on_toggle: Some(Rc::new(move |id: NodeId| {
            let (x, y) = center(id);
            press_at(&host2, x, y);
        })),
        value_for: None,
    };
    // Tree refresh after AT-driven flips (same as the Fixture).
    let mut refresh = || {
        let diff = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
        tree.borrow_mut().apply(&diff, &|id| {
            host.with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.parent))
        });
    };

    unsafe {
        // Checkbox: CheckBox type + Toggle pattern round-trip.
        let simple: IRawElementProviderSimple =
            OppaProvider::new(tree.clone(), check, check, actions.clone()).into();
        let ct = simple
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("ControlType");
        assert_eq!(read_i4(&ct), UIA_CheckBoxControlTypeId.0);
        let unk = simple
            .GetPatternProvider(UIA_TogglePatternId)
            .expect("checkbox serves Toggle");
        let tog: IToggleProvider = unk.cast().expect("toggle cast");
        assert_eq!(tog.ToggleState().expect("state"), ToggleState_Off);
        tog.Toggle().expect("AT Toggle drives");
        refresh();
        assert_eq!(tog.ToggleState().expect("state"), ToggleState_On);

        // Button: Button type; Toggle pattern refuses (Invoke is OQ-G2-2).
        let bsimple: IRawElementProviderSimple =
            OppaProvider::new(tree.clone(), btn, btn, actions.clone()).into();
        let bct = bsimple
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("button type");
        assert_eq!(read_i4(&bct), UIA_ButtonControlTypeId.0);
        assert!(bsimple.GetPatternProvider(UIA_TogglePatternId).is_err());

        // Slider: Slider type + name (RangeValue is OQ-G2-2).
        let ssimple: IRawElementProviderSimple =
            OppaProvider::new(tree.clone(), sld, sld, actions.clone()).into();
        let sct = ssimple
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("slider type");
        assert_eq!(read_i4(&sct), UIA_SliderControlTypeId.0);
        let name = ssimple.GetPropertyValue(UIA_NamePropertyId).expect("Name");
        assert_eq!(read_bstr(&name), "Volume");
        // RadioButton: own control type (decision 244 — pure mapping,
        // no fixture change).
        assert_eq!(
            uia_control_type(oppa::Role::RadioButton).0,
            UIA_RadioButtonControlTypeId.0
        );
        // Tab + TabList: own control types (decision 245 — same).
        assert_eq!(
            uia_control_type(oppa::Role::Tab).0,
            UIA_TabItemControlTypeId.0
        );
        assert_eq!(
            uia_control_type(oppa::Role::TabList).0,
            UIA_TabControlTypeId.0
        );
        // ComboBox: own control type (decision 247 — same; the
        // constant resolves at build, which is the verification).
        assert_eq!(
            uia_control_type(oppa::Role::ComboBox).0,
            UIA_ComboBoxControlTypeId.0
        );
        // ProgressBar: own control type (decision 251 — same).
        assert_eq!(
            uia_control_type(oppa::Role::ProgressBar).0,
            UIA_ProgressBarControlTypeId.0
        );
        CoUninitialize();
    }
}
