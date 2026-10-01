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
//! | `required` (decision 352) | `IsRequiredForForm` property |
//! | `invalid` (decision 352) | `IsDataValidForForm` property (inverted) |
//! | `error_message` (decision 352) | `FullDescription` property (`""` when absent) |
//! | `value_num`/`min_value`/`max_value` (decision 352) | `IRangeValueProvider` (`Value`/`Minimum`/`Maximum`; bounds default 0/100; pattern gates on `value_num`) |
//! | `Button`/`MenuItem` (decision 352) | `IInvokeProvider::Invoke` through the host-loop callback |
//! | `Tree`/`TreeItem`/`MenuItem` (decision 352) | `Tree`/`TreeItem`/`MenuItem` control types |
//!
//! Threading contract (decision 352 — the COM RPC-thread →
//! host-loop marshaling design, ADR-0010): UIA invokes provider
//! methods on the COM RPC/STA thread, never the framework UI
//! thread. Provider methods therefore never touch
//! `ComponentHost`/the reconciler directly — they (a) read the
//! snapshot tree (in-proc, owned by the host thread that applies
//! each commit's diff) and (b) drive AT actions through the
//! installed [`UiaAction`](provider::UiaAction) callbacks, which
//! must enqueue non-blocking work the host loop drains on the
//! INPUT phase (single UI thread — no cross-thread wait, never a
//! blocking call back into the framework). Uninstalled callbacks
//! fail `E_NOTIMPL`: AT-driven action without a driver is loud,
//! never a silent no-op.
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
