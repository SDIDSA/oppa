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
//! | `checked` (any role but `TextField`) | `aria-checked="true/false"` |
//! | `selected` (any role but `TextField`) | `aria-selected="true/false"` |
//! | `value_text` (any role but `TextField`, G2) | `aria-valuetext="…"` |
//! | `label` (any role) | `aria-label="…"` |
//! | `disabled = true` (any role) | `aria-disabled="true"` (+ native `disabled` on `<input>`) |
//!
//! Absent optionals and `disabled = false` emit nothing (no
//! `aria-checked="false"` noise for plain labels — the dump stays
//! minimal like the M4 `SemanticsDiff` dump).

use oppa::{Role, Semantics};

/// ARIA attributes for one semantics payload, in deterministic order
/// (role, checked, selected, valuetext, label, disabled).
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
    }
    if let Some(label) = &s.label {
        out.push(("aria-label".to_string(), label.to_string()));
    }
    if s.disabled {
        out.push(("aria-disabled".to_string(), "true".to_string()));
    }
    out
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
}
