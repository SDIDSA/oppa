//! [`Semantics`](oppa::Semantics) → at-spi2 roles and states (M10).
//!
//! The mapping (total, documented — role and state names are the
//! at-spi2-core canonical strings: roles per the 2011 ATK-consistency
//! fix — "toggle button", "list item", "filler", "entry" — and the
//! `atspi` Rust crate's `ROLE_NAMES`; states per the at-spi2
//! `AtspiStateType` nicks — "checked", "selected", "selectable",
//! "enabled", "sensitive"):
//!
//! | Payload | AT-SPI |
//! |---|---|
//! | `Switch` | role `toggle button` + `checkable` always present |
//! | `ListItem` | role `list item` + `selectable` always present |
//! | `TextField` | role `entry` (+ `editable` — the verdict-(b) field is an editor) |
//! | `Generic` | role `filler`, no capability states |
//! | `Button` (G2) | role `push button`, no capability states |
//! | `Checkbox` (G2) | role `check box` + `checkable` always present |
//! | `Slider` (G2) | role `slider`, no capability states (numeric value is OQ-G2-2) |
//! | `Dialog` (decision 241) | role `dialog`, no capability states |
//! | `RadioButton` (decision 244) | role `radio button`, `selected` reports (checked rides the same state) |
//! | `Tab` (decision 245) | role `page tab`, `selected` reports |
//! | `TabList` (decision 245) | role `page tab list`, no capability states |
//! | `ComboBox` (decision 247) | role `combo box`, no capability states (per-option `selected` rides `list item`) |
//! | `ProgressBar` (decision 251) | role `progress bar`, no capability states (percentage text is OQ-G2-2, like Slider) |
//! | `Status` (decision 337) | role `notification`, no capability states (the Toast message; announced, never focused) |
//! | `checked = Some(true)` (any role but `TextField`) | state `checked` |
//! | `checked = Some(false)` | state absent (`checkable` still tells the AT the affordance exists) |
//! | `selected = Some(true)` (any role but `TextField`) | state `selected` |
//! | `label` (any role) | accessible name (no state — carried on the node) |
//! | `disabled = true` (any role) | `enabled` + `sensitive` absent |
//! | `disabled = false` | `enabled` + `sensitive` present |
//!
//! Absent optionals emit nothing (no state noise for plain labels —
//! the event log stays minimal like the M4 `SemanticsDiff` dump).

use oppa::{Role, Semantics};

/// at-spi2 role name for one semantics payload.
pub fn atspi_role(s: &Semantics) -> &'static str {
    match s.role {
        Role::Switch => "toggle button",
        Role::ListItem => "list item",
        // Round 5.1: multi-line is an ATK text detail, not a role
        // split — same "entry" contract as single-line fields.
        Role::TextField | Role::TextArea => "entry",
        Role::Button => "push button",
        Role::Checkbox => "check box",
        Role::Slider => "slider",
        Role::Dialog => "dialog",
        // Spelled with a space per the at-spi2-core canonical names
        // (same source as "toggle button" / "check box" above —
        // decision 244 overrode the brief's underscore form).
        Role::RadioButton => "radio button",
        Role::Tab => "page tab",
        Role::TabList => "page tab list",
        // Canonical "progress bar" (space), same at-spi2-core source
        // as the names above — the brief's form already matches.
        Role::ProgressBar => "progress bar",
        // Canonical "combo box" (space), same at-spi2-core source as
        // the names above — the brief's form already matches, no
        // override needed.
        Role::ComboBox => "combo box",
        // ATK "notification": the transient-message role (decision 337 —
        // the Toast card; announced by the AT, never focused).
        Role::Status => "notification",
        Role::Generic => "filler",
    }
}

/// at-spi2 state set for one semantics payload, in deterministic
/// order (capability, value, sensitivity).
pub fn atspi_states(s: &Semantics) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    match s.role {
        Role::Switch => {
            out.push("checkable");
            if s.checked == Some(true) {
                out.push("checked");
            }
            if s.selected == Some(true) {
                out.push("selected");
            }
        }
        Role::Checkbox => {
            out.push("checkable");
            if s.checked == Some(true) {
                out.push("checked");
            }
            if s.selected == Some(true) {
                out.push("selected");
            }
        }
        Role::ListItem => {
            out.push("selectable");
            if s.selected == Some(true) {
                out.push("selected");
            }
            if s.checked == Some(true) {
                out.push("checked");
            }
        }
        Role::TextField | Role::TextArea => {
            // State owned by the native editor (mirrors aria.rs:
            // no checked/selected emitted for text fields).
            out.push("editable");
        }
        // Capability-free role: a set value still reports (total —
        // no payload field is ever dropped silently). Button, Slider,
        // Dialog, Status, RadioButton, Tab, TabList, ComboBox, and
        // ProgressBar ride here (G2 + decisions 241/244/245/247/251/337):
        // their roles name the affordance; Checkbox has its own arm
        // above.
        Role::Generic
        | Role::Button
        | Role::Slider
        | Role::Dialog
        | Role::Status
        | Role::RadioButton
        | Role::Tab
        | Role::TabList
        | Role::ComboBox
        | Role::ProgressBar => {
            if s.checked == Some(true) {
                out.push("checked");
            }
            if s.selected == Some(true) {
                out.push("selected");
            }
        }
    }
    if !s.disabled {
        out.push("enabled");
        out.push("sensitive");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_payload_maps_like_the_m5_dump() {
        let s = Semantics::switch().checked(true).label("Wi-Fi");
        assert_eq!(atspi_role(&s), "toggle button");
        assert_eq!(
            atspi_states(&s),
            vec!["checkable", "checked", "enabled", "sensitive"]
        );
    }

    #[test]
    fn unchecked_switch_keeps_capability_without_value() {
        let s = Semantics::switch().checked(false);
        assert_eq!(atspi_states(&s), vec!["checkable", "enabled", "sensitive"]);
    }

    #[test]
    fn list_item_and_disabled_shapes() {
        let s = Semantics::list_item().selected(true);
        assert_eq!(atspi_role(&s), "list item");
        assert_eq!(
            atspi_states(&s),
            vec!["selectable", "selected", "enabled", "sensitive"]
        );
        let off = Semantics::list_item().selected(false).disabled(true);
        assert_eq!(atspi_states(&off), vec!["selectable"]);
        let plain = Semantics::list_item();
        assert_eq!(
            atspi_states(&plain),
            vec!["selectable", "enabled", "sensitive"]
        );
    }

    #[test]
    fn text_field_is_an_editable_entry() {
        let s = Semantics::text_field().label("Name");
        assert_eq!(atspi_role(&s), "entry");
        assert_eq!(atspi_states(&s), vec!["editable", "enabled", "sensitive"]);
    }

    #[test]
    fn generic_is_a_bare_filler() {
        assert_eq!(atspi_role(&Semantics::default()), "filler");
        assert_eq!(
            atspi_states(&Semantics::default()),
            vec!["enabled", "sensitive"]
        );
    }

    #[test]
    fn g2_roles_map() {
        let b = Semantics::button().label("OK").disabled(true);
        assert_eq!(atspi_role(&b), "push button");
        assert_eq!(atspi_states(&b), Vec::<&str>::new());
        let c = Semantics::checkbox().checked(true);
        assert_eq!(atspi_role(&c), "check box");
        assert_eq!(
            atspi_states(&c),
            vec!["checkable", "checked", "enabled", "sensitive"]
        );
        let s = Semantics::slider().label("Volume").value_text("50 percent");
        assert_eq!(atspi_role(&s), "slider");
        assert_eq!(atspi_states(&s), vec!["enabled", "sensitive"]);
        let d = Semantics::dialog().label("Delete file?");
        assert_eq!(atspi_role(&d), "dialog");
        assert_eq!(atspi_states(&d), vec!["enabled", "sensitive"]);
        let r = Semantics::radio(true).label("Pro");
        assert_eq!(atspi_role(&r), "radio button");
        assert_eq!(atspi_states(&r), vec!["selected", "enabled", "sensitive"]);
        let u = Semantics::radio(false).label("Free");
        assert_eq!(atspi_states(&u), vec!["enabled", "sensitive"]);
        let t = Semantics::tab(true).label("Profile");
        assert_eq!(atspi_role(&t), "page tab");
        assert_eq!(atspi_states(&t), vec!["selected", "enabled", "sensitive"]);
        let l = Semantics::tab_list();
        assert_eq!(atspi_role(&l), "page tab list");
        assert_eq!(atspi_states(&l), vec!["enabled", "sensitive"]);
        let cb = Semantics::combobox().label("Pro");
        assert_eq!(atspi_role(&cb), "combo box");
        assert_eq!(atspi_states(&cb), vec!["enabled", "sensitive"]);
        let p = Semantics::progressbar("68 percent").label("Storage");
        assert_eq!(atspi_role(&p), "progress bar");
        assert_eq!(atspi_states(&p), vec!["enabled", "sensitive"]);
    }
}
