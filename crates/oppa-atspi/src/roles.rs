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
//! | `Tree` (decision 352) | role `tree`, no capability states (rows carry `tree item` + `selectable`) |
//! | `TreeItem` (decision 352) | role `tree item` + `selectable` always present |
//! | `MenuItem` (decision 352) | role `menu item`, no capability states (activation is the `click` action — see `atspi_actions`) |
//! | `checked = Some(true)` (any role but `TextField`) | state `checked` |
//! | `checked = Some(false)` | state absent (`checkable` still tells the AT the affordance exists) |
//! | `selected = Some(true)` (any role but `TextField`) | state `selected` |
//! | `invalid = true` (decision 352, any role incl. `TextField`) | state `invalid` (G7 validation — validators stay app-side) |
//! | `label` (any role) | accessible name (no state — carried on the node) |
//! | `disabled = true` (any role) | `enabled` + `sensitive` absent |
//! | `disabled = false` | `enabled` + `sensitive` present |
//!
//! Actions (decision 352 — G18): [`atspi_actions`] names the AT-SPI
//! Action names per role (`click` on Button/MenuItem — the only two
//! Invoke-class affordances); everything else carries none. Values
//! (decision 352 — G18): [`atspi_value`] is the numeric Value triple
//! `(current, min, max)` for Slider/ProgressBar — `None` (no value
//! interface) unless `value_num` is set; `value_text` stays the
//! human half and never invents a number.
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
        // Hierarchy container + row (decision 352 — the Tree control;
        // canonical at-spi2-core names with the same space spelling as
        // "toggle button" / "check box" above).
        Role::Tree => "tree",
        Role::TreeItem => "tree item",
        // Menu row (decision 352 — migrated off `ListItem` so the
        // `click` action attaches to the real affordance).
        Role::MenuItem => "menu item",
        Role::Generic => "filler",
    }
}

/// AT-SPI Action names for one semantics payload (decision 352 —
/// G18): `click` on the two Invoke-class affordances
/// (Button/MenuItem); every other role carries no actions (empty,
/// never a silent no-op — invocation of an unlisted name refuses
/// loudly with `UnknownAction`).
pub fn atspi_actions(s: &Semantics) -> Vec<&'static str> {
    match s.role {
        Role::Button | Role::MenuItem => vec!["click"],
        _ => Vec::new(),
    }
}

/// AT-SPI numeric Value triple `(current, min, max)` for one
/// semantics payload (decision 352 — G18): `Some` exactly when
/// `value_num` is set (Slider/ProgressBar shape — bounds default to
/// `0.0`/`100.0` when unset, the Slider-control contract); `None`
/// means no value interface (never an invented number — the human
/// half `value_text` keeps working alone).
pub fn atspi_value(s: &Semantics) -> Option<(f64, f64, f64)> {
    let current = s.value_num?.get() as f64;
    let min = s.min_value.map(|n| n.get() as f64).unwrap_or(0.0);
    let max = s.max_value.map(|n| n.get() as f64).unwrap_or(100.0);
    Some((current, min, max))
}

/// at-spi2 state set for one semantics payload, in deterministic
/// order (capability, value, invalid, sensitivity).
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
        Role::TreeItem => {
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
        // Dialog, Status, RadioButton, Tab, TabList, ComboBox,
        // ProgressBar, Tree, and MenuItem ride here (G2 + decisions
        // 241/244/245/247/251/337/352): their roles name the
        // affordance; Checkbox has its own arm above, ListItem and
        // TreeItem their `selectable` arms above.
        Role::Generic
        | Role::Button
        | Role::Slider
        | Role::Dialog
        | Role::Status
        | Role::RadioButton
        | Role::Tab
        | Role::TabList
        | Role::ComboBox
        | Role::ProgressBar
        | Role::Tree
        | Role::MenuItem => {
            if s.checked == Some(true) {
                out.push("checked");
            }
            if s.selected == Some(true) {
                out.push("selected");
            }
        }
    }
    // Validation mark (decision 352 — G7): every role including
    // text fields (TextInput/TextArea carry it); true-only, like
    // `disabled` (absent/false emits nothing).
    if s.invalid {
        out.push("invalid");
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

    #[test]
    fn phase36_tree_menuitem_roles_map() {
        let t = Semantics::tree().label("Files");
        assert_eq!(atspi_role(&t), "tree");
        assert_eq!(atspi_states(&t), vec!["enabled", "sensitive"]);
        let ti = Semantics::tree_item(true).label("src");
        assert_eq!(atspi_role(&ti), "tree item");
        assert_eq!(
            atspi_states(&ti),
            vec!["selectable", "selected", "enabled", "sensitive"]
        );
        let m = Semantics::menu_item().label("Copy");
        assert_eq!(atspi_role(&m), "menu item");
        assert_eq!(atspi_states(&m), vec!["enabled", "sensitive"]);
    }

    #[test]
    fn phase36_actions_and_values() {
        // Actions: the two Invoke-class affordances only.
        assert_eq!(super::atspi_actions(&Semantics::button()), vec!["click"]);
        assert_eq!(super::atspi_actions(&Semantics::menu_item()), vec!["click"]);
        assert!(super::atspi_actions(&Semantics::switch()).is_empty());
        assert!(super::atspi_actions(&Semantics::list_item()).is_empty());
        assert!(super::atspi_actions(&Semantics::tree_item(false)).is_empty());
        // Values: numeric triple exactly when `value_num` is set.
        let r = Semantics::slider()
            .value_num(50.0)
            .min_value(0.0)
            .max_value(100.0);
        assert_eq!(super::atspi_value(&r), Some((50.0, 0.0, 100.0)));
        // Unset bounds default to the Slider contract (0–100).
        let bare = Semantics::slider().value_num(25.0);
        assert_eq!(super::atspi_value(&bare), Some((25.0, 0.0, 100.0)));
        // No number, no value interface (human half alone is fine).
        assert_eq!(super::atspi_value(&Semantics::slider()), None);
        assert_eq!(
            super::atspi_value(&Semantics::slider().value_text("50 percent")),
            None
        );
    }

    #[test]
    fn phase36_invalid_state_reports_on_any_role() {
        let field = Semantics::text_field().label("Age").invalid(true);
        assert_eq!(
            atspi_states(&field),
            vec!["editable", "invalid", "enabled", "sensitive"]
        );
        let plain = Semantics::text_field();
        assert_eq!(
            atspi_states(&plain),
            vec!["editable", "enabled", "sensitive"]
        );
        let check = Semantics::checkbox().checked(true).invalid(true);
        assert_eq!(
            atspi_states(&check),
            vec!["checkable", "checked", "invalid", "enabled", "sensitive"]
        );
    }
}
