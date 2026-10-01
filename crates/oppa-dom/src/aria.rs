//! [`Semantics`](oppa::Semantics) → ARIA attributes (M7, decision 114).
//!
//! The mapping (total, documented — every payload field has exactly one
//! fate, so the M5 switch payload and the text-edit payload flow through
//! the DOM arm with [`SemanticsDiff`](oppa::SemanticsDiff) parity):
//!
//! | Payload | ARIA |
//! |---|---|
//! | `Switch` | `role="switch"` |
//! | `ListItem` | `role="listitem"` |
//! | `TextField` | no role (a real `<input type="text">` is implicitly `textbox`) |
//! | `Generic` | no role |
//! | `Button` (G2) | `role="button"` |
//! | `Checkbox` (G2) | `role="checkbox"` (+ `aria-checked` via the shared rule) |
//! | `Slider` (G2) | `role="slider"` (+ `aria-valuetext` via `value_text`) |
//! | `RadioButton` (decision 244) | `role="radio"` + `aria-checked` from `selected` (never `aria-selected` — invalid on radios) |
//! | `Tab` (decision 245) | `role="tab"` + `aria-selected` from `selected` (shared rule — valid on tabs) |
//! | `TabList` (decision 245) | `role="tablist"` |
//! | `ComboBox` (decision 247) | `role="combobox"` (+ `aria-label` = current selection; per-option `selected` rides `listitem`) |
//! | `ProgressBar` (decision 251) | `role="progressbar"` + `aria-valuetext` from `value_text` (shared rule — the payload never sets `selected`/`checked`) |
//! | `Dialog` (decision 241) | `role="dialog"` |
//! | `Status` (decision 337) | `role="status"` |
//! | `Tree` (decision 352) | `role="tree"` |
//! | `TreeItem` (decision 352) | `role="treeitem"` + `aria-selected` from `selected` (shared rule) + `aria-expanded` is control-driven (see below, never a payload bool) |
//! | `MenuItem` (decision 352) | `role="menuitem"` |
//! | `checked` (any role but `TextField`) | `aria-checked="true/false"` |
//! | `selected` (any role but `TextField`) | `aria-selected="true/false"` |
//! | `value_text` (any role but `TextField`, G2) | `aria-valuetext="…"` |
//! | `value_num`/`min_value`/`max_value` (decision 352) | `aria-valuenow`/`aria-valuemin`/`aria-valuemax` (numbers only — the human half stays `aria-valuetext`; both ride together, never one silently) |
//! | `invalid = true` (decision 352) | `aria-invalid="true"` |
//! | `required = true` (decision 352) | `aria-required="true"` |
//! | `error_message` (decision 352) | `aria-errormessage="<id>"` |
//! | `label` (any role) | `aria-label="…"` |
//! | `disabled = true` (any role) | `aria-disabled="true"` (+ native `disabled` on `<input>`) |
//!
//! Absent optionals and `disabled = false` emit nothing (no
//! `aria-checked="false"` noise for plain labels — the dump stays
//! minimal like the M4 `SemanticsDiff` dump).
//!
//! `aria-expanded` note: tree-row expansion is owned by the Tree
//! control's expanded signal — the DOM backend reads it off the
//! element the control renders (control scope, Phase 38), never off
//! the semantics payload (which carries no expansion bool by
//! decision 352, so visual state cannot drift from announced
//! state).

use oppa::{Role, Semantics};

/// ARIA attributes for one semantics payload, in deterministic order
/// (role, checked, selected, valuetext, valuenow/min/max, invalid,
/// required, errormessage, label, disabled).
pub fn aria_attrs(s: &Semantics) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    match s.role {
        Role::Switch => out.push(("role".to_string(), "switch".to_string())),
        Role::ListItem => out.push(("role".to_string(), "listitem".to_string())),
        Role::Button => out.push(("role".to_string(), "button".to_string())),
        Role::Checkbox => out.push(("role".to_string(), "checkbox".to_string())),
        Role::Slider => out.push(("role".to_string(), "slider".to_string())),
        Role::Dialog => out.push(("role".to_string(), "dialog".to_string())),
        Role::RadioButton => out.push(("role".to_string(), "radio".to_string())),
        Role::Tab => out.push(("role".to_string(), "tab".to_string())),
        Role::TabList => out.push(("role".to_string(), "tablist".to_string())),
        Role::ComboBox => out.push(("role".to_string(), "combobox".to_string())),
        Role::ProgressBar => out.push(("role".to_string(), "progressbar".to_string())),
        Role::Status => out.push(("role".to_string(), "status".to_string())),
        Role::Tree => out.push(("role".to_string(), "tree".to_string())),
        Role::TreeItem => out.push(("role".to_string(), "treeitem".to_string())),
        Role::MenuItem => out.push(("role".to_string(), "menuitem".to_string())),
        Role::TextField | Role::TextArea | Role::Generic => {}
    }
    if s.role == Role::RadioButton {
        // ARIA radios take `aria-checked`, never `aria-selected`:
        // `selected` is the source; a stray `checked` rides the same
        // attribute (selected wins) so no field is dropped silently.
        if let Some(v) = s.selected.or(s.checked) {
            out.push(("aria-checked".to_string(), v.to_string()));
        }
        if let Some(value) = &s.value_text {
            out.push(("aria-valuetext".to_string(), value.to_string()));
        }
    } else if s.role != Role::TextField {
        if let Some(checked) = s.checked {
            out.push(("aria-checked".to_string(), checked.to_string()));
        }
        if let Some(selected) = s.selected {
            out.push(("aria-selected".to_string(), selected.to_string()));
        }
        if let Some(value) = &s.value_text {
            out.push(("aria-valuetext".to_string(), value.to_string()));
        }
        // Numeric range half (decision 352): numbers only, shortest
        // round-trip (integral values carry no `.0` — the oracle
        // string-compares `aria-valuenow="50"`, never `"50.0"`).
        if let Some(v) = s.value_num {
            out.push(("aria-valuenow".to_string(), fmt_num(v.get())));
        }
        if let Some(v) = s.min_value {
            out.push(("aria-valuemin".to_string(), fmt_num(v.get())));
        }
        if let Some(v) = s.max_value {
            out.push(("aria-valuemax".to_string(), fmt_num(v.get())));
        }
    }
    // Form-validation marks (decision 352 — G7): true-only, like
    // `disabled` (absent/false emits nothing).
    if s.invalid {
        out.push(("aria-invalid".to_string(), "true".to_string()));
    }
    if s.required {
        out.push(("aria-required".to_string(), "true".to_string()));
    }
    if let Some(err) = &s.error_message {
        out.push(("aria-errormessage".to_string(), err.to_string()));
    }
    if let Some(label) = &s.label {
        out.push(("aria-label".to_string(), label.to_string()));
    }
    if s.disabled {
        out.push(("aria-disabled".to_string(), "true".to_string()));
    }
    out
}

/// Shortest round-trip float formatting for `aria-valuenow/min/max`:
/// integral values render without `.0` (`"50"`, never `"50.0"`).
fn fmt_num(v: f32) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_payload_maps_like_the_m5_dump() {
        let s = Semantics::switch().checked(true).label("Wi-Fi");
        assert_eq!(
            aria_attrs(&s),
            vec![
                ("role".to_string(), "switch".to_string()),
                ("aria-checked".to_string(), "true".to_string()),
                ("aria-label".to_string(), "Wi-Fi".to_string()),
            ]
        );
    }

    #[test]
    fn list_item_and_disabled_shapes() {
        let s = Semantics::list_item().selected(false).disabled(true);
        assert_eq!(
            aria_attrs(&s),
            vec![
                ("role".to_string(), "listitem".to_string()),
                ("aria-selected".to_string(), "false".to_string()),
                ("aria-disabled".to_string(), "true".to_string()),
            ]
        );
        // Absent optionals + enabled emit nothing.
        let plain = Semantics::list_item();
        assert_eq!(
            aria_attrs(&plain),
            vec![("role".to_string(), "listitem".to_string())]
        );
    }

    #[test]
    fn text_field_carries_no_role_or_state() {
        let s = Semantics::text_field().label("Name");
        assert_eq!(
            aria_attrs(&s),
            vec![("aria-label".to_string(), "Name".to_string())]
        );
    }

    #[test]
    fn g2_roles_map() {
        let b = Semantics::button().label("OK").disabled(true);
        assert_eq!(
            aria_attrs(&b),
            vec![
                ("role".to_string(), "button".to_string()),
                ("aria-label".to_string(), "OK".to_string()),
                ("aria-disabled".to_string(), "true".to_string()),
            ]
        );
        let c = Semantics::checkbox().checked(true).label("T&C");
        assert_eq!(
            aria_attrs(&c),
            vec![
                ("role".to_string(), "checkbox".to_string()),
                ("aria-checked".to_string(), "true".to_string()),
                ("aria-label".to_string(), "T&C".to_string()),
            ]
        );
        let s = Semantics::slider().label("Volume").value_text("50 percent");
        assert_eq!(
            aria_attrs(&s),
            vec![
                ("role".to_string(), "slider".to_string()),
                ("aria-valuetext".to_string(), "50 percent".to_string()),
                ("aria-label".to_string(), "Volume".to_string()),
            ]
        );
        let d = Semantics::dialog().label("Delete file?");
        assert_eq!(
            aria_attrs(&d),
            vec![
                ("role".to_string(), "dialog".to_string()),
                ("aria-label".to_string(), "Delete file?".to_string()),
            ]
        );
        let r = Semantics::radio(true).label("Pro");
        assert_eq!(
            aria_attrs(&r),
            vec![
                ("role".to_string(), "radio".to_string()),
                ("aria-checked".to_string(), "true".to_string()),
                ("aria-label".to_string(), "Pro".to_string()),
            ]
        );
        let u = Semantics::radio(false).label("Free");
        assert_eq!(
            aria_attrs(&u),
            vec![
                ("role".to_string(), "radio".to_string()),
                ("aria-checked".to_string(), "false".to_string()),
                ("aria-label".to_string(), "Free".to_string()),
            ]
        );
        let t = Semantics::tab(true).label("Profile");
        assert_eq!(
            aria_attrs(&t),
            vec![
                ("role".to_string(), "tab".to_string()),
                ("aria-selected".to_string(), "true".to_string()),
                ("aria-label".to_string(), "Profile".to_string()),
            ]
        );
        let l = Semantics::tab_list();
        assert_eq!(
            aria_attrs(&l),
            vec![("role".to_string(), "tablist".to_string())]
        );
        let cb = Semantics::combobox().label("Pro");
        assert_eq!(
            aria_attrs(&cb),
            vec![
                ("role".to_string(), "combobox".to_string()),
                ("aria-label".to_string(), "Pro".to_string()),
            ]
        );
        let p = Semantics::progressbar("68 percent").label("Storage");
        assert_eq!(
            aria_attrs(&p),
            vec![
                ("role".to_string(), "progressbar".to_string()),
                ("aria-valuetext".to_string(), "68 percent".to_string()),
                ("aria-label".to_string(), "Storage".to_string()),
            ]
        );
    }

    #[test]
    fn phase36_tree_menuitem_roles_map() {
        let t = Semantics::tree().label("Files");
        assert_eq!(
            aria_attrs(&t),
            vec![
                ("role".to_string(), "tree".to_string()),
                ("aria-label".to_string(), "Files".to_string()),
            ]
        );
        let ti = Semantics::tree_item(true).label("src");
        assert_eq!(
            aria_attrs(&ti),
            vec![
                ("role".to_string(), "treeitem".to_string()),
                ("aria-selected".to_string(), "true".to_string()),
                ("aria-label".to_string(), "src".to_string()),
            ]
        );
        let m = Semantics::menu_item().label("Copy");
        assert_eq!(
            aria_attrs(&m),
            vec![
                ("role".to_string(), "menuitem".to_string()),
                ("aria-label".to_string(), "Copy".to_string()),
            ]
        );
    }

    #[test]
    fn phase36_validation_and_range_attrs() {
        let s = Semantics::text_field()
            .label("Age")
            .invalid(true)
            .required(true)
            .error_message("err-age");
        assert_eq!(
            aria_attrs(&s),
            vec![
                ("aria-invalid".to_string(), "true".to_string()),
                ("aria-required".to_string(), "true".to_string()),
                ("aria-errormessage".to_string(), "err-age".to_string()),
                ("aria-label".to_string(), "Age".to_string()),
            ]
        );
        // False/absent validation emits nothing.
        let plain = Semantics::text_field().label("Age");
        assert_eq!(
            aria_attrs(&plain),
            vec![("aria-label".to_string(), "Age".to_string())]
        );
        let r = Semantics::slider()
            .label("Volume")
            .value_text("50 percent")
            .value_num(50.0)
            .min_value(0.0)
            .max_value(100.0);
        assert_eq!(
            aria_attrs(&r),
            vec![
                ("role".to_string(), "slider".to_string()),
                ("aria-valuetext".to_string(), "50 percent".to_string()),
                ("aria-valuenow".to_string(), "50".to_string()),
                ("aria-valuemin".to_string(), "0".to_string()),
                ("aria-valuemax".to_string(), "100".to_string()),
                ("aria-label".to_string(), "Volume".to_string()),
            ]
        );
        // Fractional values keep their fraction.
        let f = Semantics::slider().value_num(0.5);
        assert!(aria_attrs(&f).contains(&("aria-valuenow".to_string(), "0.5".to_string())));
    }
}
