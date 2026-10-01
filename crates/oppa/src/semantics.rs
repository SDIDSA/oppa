//! Accessibility payloads, inline on the node (DESIGN §4.1, locked #3).
//!
//! Semantics live in the retained node and diff like style: role/state
//! cannot drift from visuals because they are written in the same expression
//! that produces the visuals. The per-platform *emitters* (UIA / AT-SPI /
//! ARIA) are M10 scope; in M2 the payload is produced, retained, and diffed
//! (`SEMANTICS` dirty flag into the A11Y stub phase).

use std::sync::Arc;

/// The node role — the closed-set-tag-aligned subset M2 needs. More roles
/// arrive with real widgets (M5); the shape (plain data, diffed) is final.
/// G2 adds Button/Checkbox/Slider for the shipped control catalog
/// (decision 214): Checkbox reuses `checked`, Slider announces through
/// `value_text` (see below), Button carries label (+ `disabled`).
/// Decision 241 adds Dialog for the Modal control: label, no states.
/// Decision 244 adds RadioButton for the Radio control: label +
/// `selected` (single-choice; the group owns exclusivity).
/// Decision 245 adds Tab + TabList for the Tabs control: tabs carry
/// label + `selected`, the list is the container role.
/// Decision 247 adds ComboBox for the Select control: label (the
/// current selection's text) + `disabled`; per-option `selected`
/// rides the existing ListItem payload on the option rows.
/// Decision 251 adds ProgressBar for the ProgressBar control:
/// label + `value_text` (human percentage, e.g. `"68 percent"` —
/// the Slider announcement shape).
/// Decision 337 adds Status for the Toast control: label (the
/// message), no states — announced politely, never focused.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Role {
    #[default]
    Generic,
    Switch,
    ListItem,
    TextField,
    /// Multi-line editable field (Round 5.1): same presenter-owned
    /// editing contract as `TextField` (real `<textarea>` on DOM),
    /// distinguished so routers and backends can diverge (Enter
    /// inserts a newline instead of activating; geometry
    /// auto-heights with content).
    TextArea,
    Button,
    Checkbox,
    Slider,
    Dialog,
    /// Transient non-modal message (Round 25.2, decision 337): the
    /// Toast control's card carries it (label = message). Announced
    /// politely, never focused — no checked/selected/value states.
    Status,
    RadioButton,
    Tab,
    TabList,
    ComboBox,
    ProgressBar,
}

/// Inline semantics payload: `.semantics(Semantics::switch()...)`.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Semantics {
    pub role: Role,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub disabled: bool,
    pub label: Option<Arc<str>>,
    /// Human-readable value for value-like roles (G2: Slider renders
    /// e.g. `"50 percent"` — announced as `aria-valuetext` on the DOM
    /// leg; AT-SPI/UIA value-pattern exposure is OQ-G2-2). `None` emits
    /// nothing. Plain `String`-shaped (`Arc<str>`) because core is
    /// zero-dependency (no float-to-exact-decimal story — the control
    /// formats the text, the payload carries it opaquely).
    pub value_text: Option<Arc<str>>,
}

impl Semantics {
    pub fn switch() -> Self {
        Self {
            role: Role::Switch,
            ..Self::default()
        }
    }

    pub fn list_item() -> Self {
        Self {
            role: Role::ListItem,
            ..Self::default()
        }
    }

    /// Editable-field payload (M7, decision 113): the behavior flag
    /// (locked #24) the DOM backend recognizes as its verdict-(b)
    /// special case (real `<input>`, presenter-owned editing). Same
    /// diff shape as every payload (`SEMANTICS` flag into A11Y).
    pub fn text_field() -> Self {
        Self {
            role: Role::TextField,
            ..Self::default()
        }
    }

    /// Multi-line editable-field payload (Round 5.1): same contract
    /// as [`Semantics::text_field`] (presenter-owned editing), with
    /// the `TextArea` role so routers insert newlines on Enter and
    /// backends render `<textarea>`.
    pub fn text_area() -> Self {
        Self {
            role: Role::TextArea,
            ..Self::default()
        }
    }

    pub fn checked(mut self, v: bool) -> Self {
        self.checked = Some(v);
        self
    }

    pub fn selected(mut self, v: bool) -> Self {
        self.selected = Some(v);
        self
    }

    pub fn label(mut self, v: &str) -> Self {
        self.label = Some(Arc::from(v));
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    /// Push-button payload (G2): label (+ `disabled`) — activation is
    /// the press handler; no AT-action pattern in v1 (OQ-G2-2).
    pub fn button() -> Self {
        Self {
            role: Role::Button,
            ..Self::default()
        }
    }

    /// Checkbox payload (G2): `.checked(state)` + label (+ `disabled`);
    /// rides the Toggle pattern on UIA like `Switch`.
    pub fn checkbox() -> Self {
        Self {
            role: Role::Checkbox,
            ..Self::default()
        }
    }

    /// Slider payload (G2): label + `.value_text(human value)` (+
    /// `disabled`); value-pattern exposure per backend is OQ-G2-2.
    pub fn slider() -> Self {
        Self {
            role: Role::Slider,
            ..Self::default()
        }
    }

    /// Dialog payload (decision 241): label (+ `disabled`); the Modal
    /// control's card carries it. No checked/selected/value states.
    pub fn dialog() -> Self {
        Self {
            role: Role::Dialog,
            ..Self::default()
        }
    }

    /// Status payload (decision 337): label (the message, + `disabled`
    /// unused); the Toast control's card carries it. Announced
    /// politely, never focused — no checked/selected/value states.
    pub fn status() -> Self {
        Self {
            role: Role::Status,
            ..Self::default()
        }
    }

    /// Radio payload (decision 244): `.selected(state)` + label (+
    /// `disabled`); the Radio control carries it, the RadioGroup owns
    /// mutual exclusion. Rides the SelectionItem pattern on UIA like
    /// `ListItem`.
    pub fn radio(selected: bool) -> Self {
        Self {
            role: Role::RadioButton,
            selected: Some(selected),
            ..Self::default()
        }
    }

    /// Tab payload (decision 245): `.selected(state)` + label (+
    /// `disabled`); one tab button carries it, the Tabs bar carries
    /// the list role. Rides the SelectionItem pattern on UIA like
    /// `ListItem`.
    pub fn tab(selected: bool) -> Self {
        Self {
            role: Role::Tab,
            selected: Some(selected),
            ..Self::default()
        }
    }

    /// Tab-list payload (decision 245): the tab bar's container role.
    /// No checked/selected/value states.
    pub fn tab_list() -> Self {
        Self {
            role: Role::TabList,
            ..Self::default()
        }
    }

    /// Combo-box payload (decision 247): label (the Select control
    /// sets it to the current selection's text) + `disabled`. No
    /// checked/selected/value states on the box itself — each option
    /// row carries `list_item` + `selected` (existing payloads, no
    /// new states invented).
    pub fn combobox() -> Self {
        Self {
            role: Role::ComboBox,
            ..Self::default()
        }
    }

    /// Progress-bar payload (decision 251): the human-readable
    /// percentage text rides `value_text` (the Slider announcement
    /// shape — the control formats it, e.g. `"68 percent"`), so the
    /// constructor takes it as the required payload state (the
    /// radio/tab `selected` precedent). Label (+ `disabled`) chain
    /// as usual; no checked/selected states.
    pub fn progressbar(value_text: &str) -> Self {
        Self {
            role: Role::ProgressBar,
            value_text: Some(Arc::from(value_text)),
            ..Self::default()
        }
    }

    pub fn value_text(mut self, v: &str) -> Self {
        self.value_text = Some(Arc::from(v));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_builder_shape() {
        let s = Semantics::switch()
            .checked(true)
            .label("Wi-Fi")
            .disabled(false);
        assert_eq!(s.role, Role::Switch);
        assert_eq!(s.checked, Some(true));
        assert_eq!(s.label.as_deref(), Some("Wi-Fi"));
    }

    #[test]
    fn g2_builder_shapes() {
        let b = Semantics::button().label("OK").disabled(true);
        assert_eq!(b.role, Role::Button);
        assert!(b.disabled);
        let c = Semantics::checkbox().checked(false).label("T&C");
        assert_eq!((c.role, c.checked), (Role::Checkbox, Some(false)));
        let s = Semantics::slider().label("Volume").value_text("50 percent");
        assert_eq!(s.role, Role::Slider);
        assert_eq!(s.value_text.as_deref(), Some("50 percent"));
        let d = Semantics::dialog().label("Delete file?");
        assert_eq!(d.role, Role::Dialog);
        assert_eq!(d.label.as_deref(), Some("Delete file?"));
        let r = Semantics::radio(true).label("Pro");
        assert_eq!((r.role, r.selected), (Role::RadioButton, Some(true)));
        let t = Semantics::tab(true).label("Profile");
        assert_eq!((t.role, t.selected), (Role::Tab, Some(true)));
        assert_eq!(Semantics::tab_list().role, Role::TabList);
        let cb = Semantics::combobox().label("Pro");
        assert_eq!(cb.role, Role::ComboBox);
        assert_eq!(cb.label.as_deref(), Some("Pro"));
        let p = Semantics::progressbar("68 percent").label("Storage");
        assert_eq!(p.role, Role::ProgressBar);
        assert_eq!(p.value_text.as_deref(), Some("68 percent"));
    }
}
