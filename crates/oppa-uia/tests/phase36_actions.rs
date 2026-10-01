//! Phase 36 PR1 (decision 352 — G18): AT-action parity through real
//! UIA interfaces. `IInvokeProvider` on Button/MenuItem and
//! `IRangeValueProvider` on Slider/ProgressBar serve through COM and
//! drive back into the host loop through the installed callbacks;
//! validation marks (`required`/`invalid`/`error_message`) read
//! through `IsRequiredForForm`/`IsDataValidForForm`/`FullDescription`.
//!
//! In-process COM (MTA), no HWND hosting: the provider layer is
//! what's under test (the `uia_emit` scope + action round-trips).
//! This is the crate's consolidated Phase-36 binary — `uia_emit.rs`
//! stays untouched.

#![cfg(windows)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use std::cell::RefCell;
use std::rc::Rc;

use oppa::{
    compute_semantics_diff, find_retained_by_debug, ComponentHost, Ctx, NodeId, Semantics,
    SemanticsSnapshot, Style, VNode,
};
use oppa_macros::{component, Props};
use oppa_uia::{uia_control_type, OppaProvider, UiaAction, UiaTree};
use windows::core::Interface;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{
    IInvokeProvider, IRangeValueProvider, IRawElementProviderSimple, UIA_ButtonControlTypeId,
    UIA_ControlTypePropertyId, UIA_FullDescriptionPropertyId, UIA_InvokePatternId,
    UIA_IsDataValidForFormPropertyId, UIA_IsRequiredForFormPropertyId, UIA_MenuItemControlTypeId,
    UIA_NamePropertyId, UIA_ProgressBarControlTypeId, UIA_RangeValueIsReadOnlyPropertyId,
    UIA_RangeValueLargeChangePropertyId, UIA_RangeValueMaximumPropertyId,
    UIA_RangeValueMinimumPropertyId, UIA_RangeValuePatternId, UIA_RangeValueSmallChangePropertyId,
    UIA_RangeValueValuePropertyId, UIA_SliderControlTypeId, UIA_TogglePatternId,
    UIA_TreeControlTypeId, UIA_TreeItemControlTypeId,
};

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

fn read_r8(v: &windows::Win32::System::Variant::VARIANT) -> f64 {
    unsafe { v.Anonymous.Anonymous.Anonymous.dblVal }
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
struct ItemProps {
    label: String,
}

#[component]
fn Item(ctx: &Ctx, props: &ItemProps) -> VNode {
    let _ = ctx.signal(0u32);
    oppa::Div("item")
        .style(Style::new().size(96, 24))
        .semantics(Semantics::menu_item().label(&props.label))
        .on_press(|| {})
        .build()
}

#[derive(Clone, Props)]
struct SldProps {
    label: String,
    ranged: bool,
}

#[component]
fn Sld(ctx: &Ctx, props: &SldProps) -> VNode {
    let _ = ctx.signal(0u32);
    let mut sem = Semantics::slider()
        .label(&props.label)
        .value_text("50 percent");
    if props.ranged {
        sem = sem.value_num(50.0).min_value(0.0).max_value(100.0);
    }
    oppa::Div("sld")
        .style(Style::new().size(160, 32))
        .semantics(sem)
        .build()
}

#[derive(Clone, Props)]
struct FieldProps {
    label: String,
}

#[component]
fn Field(ctx: &Ctx, props: &FieldProps) -> VNode {
    let _ = ctx.signal(0u32);
    oppa::Div("field")
        .style(Style::new().size(160, 32))
        .semantics(
            Semantics::text_field()
                .label(&props.label)
                .invalid(true)
                .required(true)
                .error_message("err-age"),
        )
        .build()
}

#[derive(Clone, Props)]
struct TreeProps {}

#[component]
fn TreeApp(ctx: &Ctx, _props: &TreeProps) -> VNode {
    let _ = ctx.signal(0u32);
    oppa::Div("tree")
        .style(Style::new().size(200, 200))
        .semantics(Semantics::tree().label("Files"))
        .children([oppa::Div("leaf")
            .style(Style::new().size(200, 24))
            .semantics(Semantics::tree_item(true).label("src"))
            .build()])
}

#[derive(Clone, Props)]
struct ActionsAppProps {
    btn: BtnProps,
    item: ItemProps,
    sld: SldProps,
    field: FieldProps,
}

#[component]
fn ActionsApp(ctx: &Ctx, props: &ActionsAppProps) -> VNode {
    let _ = ctx.signal(0u32);
    oppa::Div("actions").children([
        ctx.child("Btn", 1, &props.btn, Btn),
        ctx.child("Item", 2, &props.item, Item),
        ctx.child("Sld", 3, &props.sld, Sld),
        ctx.child("Field", 4, &props.field, Field),
    ])
}

struct Fixture {
    host: ComponentHost,
    tree: Rc<RefCell<UiaTree>>,
}

impl Fixture {
    fn mount_app(props: ActionsAppProps) -> Self {
        let host = ComponentHost::new();
        host.set_viewport(300.0, 200.0);
        host.mount("ActionsApp", props, ActionsApp);
        host.run_until_idle();
        let tree = Rc::new(RefCell::new(UiaTree::new()));
        let mut snap = SemanticsSnapshot::new();
        let diff = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
        tree.borrow_mut().apply(&diff, &|id| {
            host.with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.parent))
        });
        Self { host, tree }
    }

    fn provider(&self, id: NodeId, actions: UiaAction) -> IRawElementProviderSimple {
        OppaProvider::new(self.tree.clone(), id, id, actions).into()
    }
}

fn app_props(ranged: bool) -> ActionsAppProps {
    ActionsAppProps {
        btn: BtnProps {
            label: "OK".to_string(),
        },
        item: ItemProps {
            label: "Copy".to_string(),
        },
        sld: SldProps {
            label: "Volume".to_string(),
            ranged,
        },
        field: FieldProps {
            label: "Age".to_string(),
        },
    }
}

#[test]
fn invoke_drives_button_and_menuitem() {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .expect("com init")
    };
    let fx = Fixture::mount_app(app_props(true));
    let btn = find_retained_by_debug(&fx.host, "btn")[0];
    let item = find_retained_by_debug(&fx.host, "item")[0];

    let fired: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
    let record = fired.clone();
    let actions = UiaAction {
        on_toggle: None,
        value_for: None,
        on_invoke: Some(Rc::new(move |id: NodeId| {
            record.borrow_mut().push(id);
        })),
        on_set_value: None,
    };

    unsafe {
        // Button: Button type + Invoke serves and drives.
        let simple = fx.provider(btn, actions.clone());
        let ct = simple
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("button type");
        assert_eq!(read_i4(&ct), UIA_ButtonControlTypeId.0);
        let unk = simple
            .GetPatternProvider(UIA_InvokePatternId)
            .expect("invoke serves on Button");
        let inv: IInvokeProvider = unk.cast().expect("invoke cast");
        inv.Invoke().expect("AT Invoke drives");
        assert_eq!(*fired.borrow(), vec![btn]);
        // Toggle stays refused on buttons (no silent wrong pattern).
        assert!(simple.GetPatternProvider(UIA_TogglePatternId).is_err());

        // MenuItem: MenuItem type + Invoke serves and drives.
        let isimple = fx.provider(item, actions.clone());
        let ict = isimple
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("item type");
        assert_eq!(read_i4(&ict), UIA_MenuItemControlTypeId.0);
        assert_eq!(
            uia_control_type(oppa::Role::MenuItem).0,
            UIA_MenuItemControlTypeId.0
        );
        let iunk = isimple
            .GetPatternProvider(UIA_InvokePatternId)
            .expect("invoke serves on MenuItem");
        let iinv: IInvokeProvider = iunk.cast().expect("invoke cast");
        iinv.Invoke().expect("AT Invoke drives");
        assert_eq!(*fired.borrow(), vec![btn, item]);

        // Uninstalled driver fails loudly (AT action without a
        // driver is loud, never a silent no-op).
        let bare = fx.provider(btn, UiaAction::default());
        let bunk = bare
            .GetPatternProvider(UIA_InvokePatternId)
            .expect("pattern serves without a driver");
        let binv: IInvokeProvider = bunk.cast().expect("invoke cast");
        assert!(binv.Invoke().is_err());
        CoUninitialize();
    }
}

#[test]
fn range_value_serves_and_drives_slider() {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .expect("com init")
    };
    let fx = Fixture::mount_app(app_props(true));
    let sld = find_retained_by_debug(&fx.host, "sld")[0];

    let writes: Rc<RefCell<Vec<(NodeId, f64)>>> = Rc::new(RefCell::new(Vec::new()));
    let record = writes.clone();
    let actions = UiaAction {
        on_toggle: None,
        value_for: None,
        on_invoke: None,
        on_set_value: Some(Rc::new(move |id: NodeId, v: f64| {
            record.borrow_mut().push((id, v));
        })),
    };

    unsafe {
        let simple = fx.provider(sld, actions.clone());
        let ct = simple
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("slider type");
        assert_eq!(read_i4(&ct), UIA_SliderControlTypeId.0);
        let unk = simple
            .GetPatternProvider(UIA_RangeValuePatternId)
            .expect("range serves on ranged Slider");
        let rv: IRangeValueProvider = unk.cast().expect("range cast");
        assert_eq!(rv.Value().expect("value"), 50.0);
        assert_eq!(rv.Minimum().expect("min"), 0.0);
        assert_eq!(rv.Maximum().expect("max"), 100.0);
        assert_eq!(rv.LargeChange().expect("large"), 10.0);
        assert_eq!(rv.SmallChange().expect("small"), 1.0);
        assert_eq!(rv.IsReadOnly().expect("writable").0, 0);
        rv.SetValue(75.0).expect("AT SetValue drives");
        assert_eq!(*writes.borrow(), vec![(sld, 75.0)]);

        // Property reads agree with the pattern reads.
        let v = simple
            .GetPropertyValue(UIA_RangeValueValuePropertyId)
            .expect("Value prop");
        assert_eq!(read_r8(&v), 50.0);
        let mn = simple
            .GetPropertyValue(UIA_RangeValueMinimumPropertyId)
            .expect("Min prop");
        assert_eq!(read_r8(&mn), 0.0);
        let mx = simple
            .GetPropertyValue(UIA_RangeValueMaximumPropertyId)
            .expect("Max prop");
        assert_eq!(read_r8(&mx), 100.0);
        let lc = simple
            .GetPropertyValue(UIA_RangeValueLargeChangePropertyId)
            .expect("LargeChange prop");
        assert_eq!(read_r8(&lc), 10.0);
        let sc = simple
            .GetPropertyValue(UIA_RangeValueSmallChangePropertyId)
            .expect("SmallChange prop");
        assert_eq!(read_r8(&sc), 1.0);
        let ro = simple
            .GetPropertyValue(UIA_RangeValueIsReadOnlyPropertyId)
            .expect("IsReadOnly prop");
        assert!(!read_bool(&ro));

        // Uninstalled write driver: reads serve, writes fail loudly.
        let bare = fx.provider(sld, UiaAction::default());
        let bunk = bare
            .GetPatternProvider(UIA_RangeValuePatternId)
            .expect("range serves without a driver");
        let brv: IRangeValueProvider = bunk.cast().expect("range cast");
        assert_eq!(brv.Value().expect("reads serve"), 50.0);
        assert!(brv.SetValue(10.0).is_err());
        assert_ne!(brv.IsReadOnly().expect("read-only").0, 0);
        CoUninitialize();
    }
}

#[test]
fn range_gates_on_presence_and_progressbar_types() {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .expect("com init")
    };
    // Slider without `value_num`: no value interface (never an
    // invented number).
    let fx = Fixture::mount_app(app_props(false));
    let sld = find_retained_by_debug(&fx.host, "sld")[0];
    unsafe {
        let simple = fx.provider(sld, UiaAction::default());
        assert!(simple.GetPatternProvider(UIA_RangeValuePatternId).is_err());
        // ProgressBar type maps (pure mapping — constant resolves).
        assert_eq!(
            uia_control_type(oppa::Role::ProgressBar).0,
            UIA_ProgressBarControlTypeId.0
        );
        CoUninitialize();
    }
}

#[test]
fn validation_marks_read_through_uia() {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .expect("com init")
    };
    let fx = Fixture::mount_app(app_props(true));
    let field = find_retained_by_debug(&fx.host, "field")[0];
    unsafe {
        let simple = fx.provider(field, UiaAction::default());
        let req = simple
            .GetPropertyValue(UIA_IsRequiredForFormPropertyId)
            .expect("required");
        assert!(read_bool(&req));
        let valid = simple
            .GetPropertyValue(UIA_IsDataValidForFormPropertyId)
            .expect("valid");
        assert!(!read_bool(&valid), "invalid reports data-invalid");
        let desc = simple
            .GetPropertyValue(UIA_FullDescriptionPropertyId)
            .expect("description");
        assert_eq!(read_bstr(&desc), "err-age");
        let name = simple.GetPropertyValue(UIA_NamePropertyId).expect("Name");
        assert_eq!(read_bstr(&name), "Age");
        CoUninitialize();
    }
}

#[test]
fn tree_roles_map_to_tree_types() {
    let host = ComponentHost::new();
    host.set_viewport(300.0, 200.0);
    host.mount("TreeApp", TreeProps {}, TreeApp);
    host.run_until_idle();
    let tree = Rc::new(RefCell::new(UiaTree::new()));
    let mut snap = SemanticsSnapshot::new();
    let diff = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    tree.borrow_mut().apply(&diff, &|id| {
        host.with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.parent))
    });
    let root = find_retained_by_debug(&host, "tree")[0];
    let leaf = find_retained_by_debug(&host, "leaf")[0];
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .expect("com init");
        let rsimple: IRawElementProviderSimple =
            OppaProvider::new(tree.clone(), root, root, UiaAction::default()).into();
        let rct = rsimple
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("tree type");
        assert_eq!(read_i4(&rct), UIA_TreeControlTypeId.0);
        assert_eq!(
            uia_control_type(oppa::Role::Tree).0,
            UIA_TreeControlTypeId.0
        );
        let lsimple: IRawElementProviderSimple =
            OppaProvider::new(tree.clone(), leaf, root, UiaAction::default()).into();
        let lct = lsimple
            .GetPropertyValue(UIA_ControlTypePropertyId)
            .expect("item type");
        assert_eq!(read_i4(&lct), UIA_TreeItemControlTypeId.0);
        assert_eq!(
            uia_control_type(oppa::Role::TreeItem).0,
            UIA_TreeItemControlTypeId.0
        );
        let name = lsimple.GetPropertyValue(UIA_NamePropertyId).expect("Name");
        assert_eq!(read_bstr(&name), "src");
        CoUninitialize();
    }
}
