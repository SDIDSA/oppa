//! M10 close-v1 — the Windows UIA provider.
//!
//! Serves the retained tree's [`Semantics`](oppa::Semantics) through
//! real UIA interfaces (`IRawElementProviderSimple` +
//! `IRawElementProviderFragment` + Toggle/SelectionItem/Value
//! patterns): the toggle and list-item semantics locked (#3, M2)
//! are queryable by any UIA client — not just present in the
//! retained tree. Total mapping table (every payload field has
//! exactly one fate, the aria.rs discipline):
//!
//! | Payload | UIA |
//! |---|
//! | `Switch` | `CheckBox` control type + Toggle pattern |
//! | `ListItem` | `ListItem` control type + SelectionItem pattern |
//! | `TextField` | `Edit` control type + Value pattern (gated — see below) |
//! | `Generic` | `Group` control type, no pattern |
//! | `checked` | `IToggleProvider::ToggleState` (None → Indeterminate) + readable `ToggleState` property |
//! | `selected` | `ISelectionItemProvider::IsSelected` |
//! | `label` | `Name` property |
//! | `disabled` | `IsEnabled` property (inverted) |
//! | bounds | `BoundingRectangle` (fragment + property) |
//!
//! Scope, stated: **query path + toggle/select actions + event
//! raising mechanics.** `Toggle()` and `Select()` drive back into
//! the framework through callbacks the host loop installs (proven
//! end-to-end in `tests/uia_emit.rs`: AT action → framework press
//! → re-read flips). The Value pattern serves only with an
//! installed value callback (the M1 editor seam — the test stubs
//! session content). UIA **event raising** is proven in
//! `tests/uia_events.rs`: an HWND-hosted provider plus a real
//! `CUIAutomation` client pump observes a framework toggle flip as
//! a property-changed event (old/new values + sender). The HWND in
//! that test is test scope — production hosting belongs with the
//! shell window (decision 149); what the provider owns for it is
//! the opt-in `OppaProvider::set_host_hwnd` token (unset stays loud
//! `E_NOTIMPL`), the `AdviseEvents` capability both UIA paths
//! require, and host-token propagation across derived fragments.
//! `SetFocus` likewise reports not-implemented (focus follows
//! click in v1, M5).

#[cfg(windows)]
pub mod provider;
#[cfg(windows)]
pub mod tree;

#[cfg(windows)]
pub use provider::{uia_control_type, OppaProvider, UiaAction};
#[cfg(windows)]
pub use tree::{UiaNode, UiaTree};
