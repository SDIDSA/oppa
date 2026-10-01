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
/// Decision 352 (Phase 36 PR1) adds Tree + TreeItem for the Tree
/// control (expanded/selected signals, chevrons, arrow keys) and
/// MenuItem for menu rows (migrated off `ListItem` so AT-action
/// patterns attach to the real affordance: Invoke on UIA,
/// `menu item` on DOM/AT-SPI).
/// Decision 352 (Phase 36 PR1) adds form-validation + numeric-range
/// payload state (G7/G18): `invalid`/`required`/`error_message`
/// (validators stay app-side; the payload only announces) and
/// `value_num`/`min_value`/`max_value` (the numeric half of the
/// Slider/ProgressBar announcement — `value_text` stays the human
/// half; backends expose both, never one silently).
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
    /// Expandable hierarchy container (Phase 36 PR1, decision 352):
    /// the Tree control's root. Label (+ `disabled`); no
    /// checked/selected/value states on the container itself — each
    /// row carries `TreeItem` (see [`Semantics::tree_item`]).
    Tree,
    /// Hierarchy row (Phase 36 PR1, decision 352): label +
    /// `.selected(state)` (+ `disabled`); expansion rides the
    /// UIA ExpandCollapse pattern and `aria-expanded` on DOM
    /// (driven by the control's expanded signal — no payload bool,
    /// so visual state cannot drift from announced state).
    TreeItem,
    /// Menu row (Phase 36 PR1, decision 352): label (+ `disabled`);
    /// the MenuItem control carries it (migrated off `ListItem`).
    /// Activation is the press handler, exposed as UIA Invoke and
    /// AT-SPI `click` — the first role beyond Button with an
    /// Invoke-class action.
    MenuItem,
}

/// Numeric range value as exact f32 bits (the [`Px`](crate::style::Px)
/// precedent: styles hash by exact bits so payloads stay
/// `Eq + Hash`). Slider/ProgressBar bounds (`min_value`/`max_value`)
/// and current (`value_num`) — the machine half of the range
/// announcement (`value_text` stays the human half).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Num(u32);

impl Num {
    pub fn of(v: f32) -> Self {
        Self(v.to_bits())
    }

    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
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
    /// Form-validation state (Phase 36 PR1, decision 352 — G7): the
    /// control sets these from its `invalid`/`required` props;
    /// validators stay app-side, the payload only announces.
    /// `invalid` renders `aria-invalid` (+ AT-SPI `invalid` state,
    /// UIA `IsRequiredForForm` stays on `required` — no field shares
    /// another's fate). `error_message` names the error node
    /// (`aria-errormessage` on DOM; accessible description
    /// elsewhere). Both default off/empty — absent optionals emit
    /// nothing, like every other payload field.
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<Arc<str>>,
    /// Numeric range bounds (Phase 36 PR1, decision 352 — G18):
    /// `value_num` is the current value, `min_value`/`max_value`
    /// the bounds (DOM `aria-valuenow/min/max`, UIA RangeValue,
    /// AT-SPI Value). `None` emits nothing — the human half
    /// (`value_text`) keeps working alone, and text-less numeric
    /// payloads never invent a human string.
    pub value_num: Option<Num>,
    pub min_value: Option<Num>,
    pub max_value: Option<Num>,
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

    /// Tree-container payload (Phase 36 PR1, decision 352): label
    /// (+ `disabled`); the Tree control's root carries it. No
    /// checked/selected/value states on the container — each row
    /// carries [`Semantics::tree_item`].
    pub fn tree() -> Self {
        Self {
            role: Role::Tree,
            ..Self::default()
        }
    }

    /// Tree-row payload (Phase 36 PR1, decision 352):
    /// `.selected(state)` + label (+ `disabled`); expansion rides
    /// the UIA ExpandCollapse pattern / DOM `aria-expanded` off the
    /// control's expanded signal — never a payload bool, so visual
    /// state cannot drift from announced state.
    pub fn tree_item(selected: bool) -> Self {
        Self {
            role: Role::TreeItem,
            selected: Some(selected),
            ..Self::default()
        }
    }

    /// Menu-row payload (Phase 36 PR1, decision 352): label (+
    /// `disabled`); the MenuItem control carries it. Activation is
    /// the press handler (UIA Invoke / AT-SPI `click`).
    pub fn menu_item() -> Self {
        Self {
            role: Role::MenuItem,
            ..Self::default()
        }
    }

    pub fn value_text(mut self, v: &str) -> Self {
        self.value_text = Some(Arc::from(v));
        self
    }

    /// Form-validation mark (Phase 36 PR1, decision 352 — G7):
    /// failed validation; pairs with [`Semantics::error_message`].
    pub fn invalid(mut self, v: bool) -> Self {
        self.invalid = v;
        self
    }

    /// Form-validation mark (Phase 36 PR1, decision 352 — G7):
    /// the field must be filled before submit.
    pub fn required(mut self, v: bool) -> Self {
        self.required = v;
        self
    }

    /// Form-validation mark (Phase 36 PR1, decision 352 — G7): the
    /// error node's identity (DOM `aria-errormessage`; accessible
    /// description on other legs). Validators stay app-side.
    pub fn error_message(mut self, v: &str) -> Self {
        self.error_message = Some(Arc::from(v));
        self
    }

    /// Numeric current value (Phase 36 PR1, decision 352 — G18):
    /// DOM `aria-valuenow`, UIA RangeValue `Value`, AT-SPI Value
    /// current. Pairs with [`Semantics::min_value`] /
    /// [`Semantics::max_value`]; `value_text` stays the human half.
    pub fn value_num(mut self, v: f32) -> Self {
        self.value_num = Some(Num::of(v));
        self
    }

    /// Numeric range floor (Phase 36 PR1, decision 352 — G18).
    pub fn min_value(mut self, v: f32) -> Self {
        self.min_value = Some(Num::of(v));
        self
    }

    /// Numeric range ceiling (Phase 36 PR1, decision 352 — G18).
    pub fn max_value(mut self, v: f32) -> Self {
        self.max_value = Some(Num::of(v));
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

    #[test]
    fn phase36_tree_menuitem_shapes() {
        let t = Semantics::tree().label("Files");
        assert_eq!(t.role, Role::Tree);
        assert_eq!(t.label.as_deref(), Some("Files"));
        let ti = Semantics::tree_item(true).label("src");
        assert_eq!((ti.role, ti.selected), (Role::TreeItem, Some(true)));
        let m = Semantics::menu_item().label("Copy").disabled(true);
        assert_eq!(m.role, Role::MenuItem);
        assert!(m.disabled);
    }

    #[test]
    fn phase36_validation_and_range_shapes() {
        let s = Semantics::text_field()
            .label("Age")
            .invalid(true)
            .required(true)
            .error_message("err-age");
        assert!(s.invalid);
        assert!(s.required);
        assert_eq!(s.error_message.as_deref(), Some("err-age"));
        // Absent by default — no noise for plain fields.
        let plain = Semantics::text_field();
        assert!(!plain.invalid);
        assert!(!plain.required);
        assert_eq!(plain.error_message, None);
        assert_eq!(plain.value_num, None);
        let r = Semantics::slider()
            .label("Volume")
            .value_text("50 percent")
            .value_num(50.0)
            .min_value(0.0)
            .max_value(100.0);
        assert_eq!(r.value_num, Some(Num::of(50.0)));
        assert_eq!(r.min_value, Some(Num::of(0.0)));
        assert_eq!(r.max_value, Some(Num::of(100.0)));
        // Bit-exact round-trip (the Px precedent).
        assert_eq!(r.value_num.expect("set").get(), 50.0);
    }
}
