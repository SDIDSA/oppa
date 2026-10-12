//! Shipped control catalog (G2 — decisions 212–214; TextInput —
//! decision 240; Modal — decision 241, closing OQ-G2-3's overlay half;
//! Radio/RadioGroup — 244; Tabs — 245; Select — 247;
//! ProgressBar/Badge — 251).
//!
//! Button, Checkbox, Toggle, Slider, TextInput, Modal, Radio,
//! RadioGroup, Tabs, Select, ProgressBar, and Badge as plain component
//! functions over the existing vocabulary
//! (`Div`/`Row`/`Text`/`TextField` + handlers + `Semantics` +
//! author-owned signals). No new [`Tag`](oppa::Tag), no reconciler /
//!   layout / backend changes — controls compose what the framework
//!   already proves (decision 212).
//!
//! Conventions (decision 213):
//!
//! - **Controlled state.** Stateful controls take author-owned
//!   [`Signal`](oppa::Signal)s and flip them on press (the same
//!   controlled pattern as [`EditSession`](oppa::EditSession) content,
//!   locked #24). There are no uncontrolled variants in v1 (OQ-G2-4).
//! - **Use through `ctx.child`.** Each control is a `fn(&Ctx, &P) ->
//!   VNode` shaped for [`Ctx::child`](oppa::Ctx::child), so every
//!   instance owns its hover/press/focus flags and hot-reload identity
//!   (`"oppa::Button"` + slot key). Embedding the returned `VNode`
//!   directly in one-control bodies works; two stateful controls in one
//!   body need `ctx.child` (shared flags otherwise — the M8/F6 rule).
//! - **Press-only interactions.** Buttons flip/activate on press
//!   (pointer, Enter/Space on the focused control — the M5 router);
//!   the Slider is decrement/track/increment (drag + arrow keys need
//!   router drag events + shell arrow classification — OQ-G2-1).
//! - **Disabled drops the handler.** A disabled control carries
//!   `Semantics::disabled(true)`, dims, and attaches no press handler —
//!   so it also leaves the Tab order (v1 focusable means press-owner,
//!   decision 96). Never a silent no-op handler: the refusal is
//!   structural.
//! - **Explicit sizes.** Controls ship default sizes in px (buttons
//!   96×32, boxes 20×20, slider 160×32, text inputs 200×32, dialog
//!   cards 360 wide, progress bars 160×12, badges 24 high) so
//!   headless trees get hit boxes without a text service; authors
//!   override through the props' `width`/`height`.
//!
//! Controls: Button, Checkbox, Toggle, Slider, TextInput, TextArea,
//! Modal, Radio, RadioGroup, Tabs, Select, ProgressBar, Badge,
//! Toast, Tooltip, Menu, ContextMenu, VirtualList, DataGrid,
//! Scrollbar, ErrorBoundary, Tree, Splitter, DatePicker, Toolbar,
//! Menubar, FilePicker, RichTextView, ImageView, CanvasView,
//! NavHost, HeaderBar, plus the `Uncontrolled*` self-managed companions.
//!
//! Per-control behavior is documented in this crate; catalog status
//! lives in `docs/STATE.md` (Done).
//!
//! Cookbook (G10 — the three additions the page skipped; each names
//! its precedent, the rule of the page):
//!
//! - **Text-field binding.** Controlled state in, committed edits
//!   out: `let name = ctx.signal(SharedString::from(""))` renders
//!   through `TextInputProps::new("Name", name.clone())`, and
//!   `on_change` writes the store back (`Change<SharedString>` —
//!   the `TaskStudio` inspector's `title_commit` is the shipped
//!   spelling: guard on inequality, then `update_row`). The
//!   [`EditSession`](oppa::EditSession) owns caret/selection/undo
//!   across renders; validators stay app-side, `invalid` +
//!   `error_message` only announce (decision 366).
//! - **`VirtualList` / `Collection` paging.** Data lives in an
//!   author-owned [`Collection`](oppa::Collection) (stable
//!   [`RowId`](oppa::RowId)s, filter/sort/page queries with match
//!   totals); [`VirtualList`] windows it through [`vlist_window`]
//!   (binary-searched tops + overscan — never division) with per-slot
//!   `child_keyed` recycling; background pages stream in through the
//!   [`CollectionWriter`](oppa::CollectionWriter) rendezvous (rows
//!   cross threads as data, ids assign on the UI thread) or
//!   `spawn_fetch_page` (same drain). [`DataGrid`] is the same feed
//!   with columns; [`Tree`] the same feed flattened hierarchically.
//! - **Images and menus.** Static pixels deposit once via
//!   [`ImageCache::insert_pixels`](oppa::ImageCache::insert_pixels)
//!   (exact `w × h × 4` RGBA8, loud otherwise) and render through
//!   [`ImageView`] (alt names it); `key_of` reverses ids for the DOM
//!   leg. Menus compose [`MenuItem`] rows in a [`Menu`] portal
//!   (container-owned focus, highlight cursor, hit-test activation)
//!   wrapped by [`ContextMenu`] (cursor-anchored), [`Select`]
//!   (box-anchored), or [`Menubar`] (bar-anchored titles).
//!
//! Styling (G11 — current, not Proposed): components read
//! `ctx.theme().tokens()` (a tracked read — `host.set_theme(mode)`
//! re-renders every themed control in place, instances and state
//! survive) instead of literals. The token table
//! ([`ThemeTokens`](oppa::ThemeTokens)): `background`, `surface`,
//! `text_primary`, `text_secondary`, `primary`, `primary_pressed`,
//! `border`, `focus_ring`, `disabled`, `error`. Rules: fills ride
//! tokens; contrast ink on saturated accents stays literal white
//! (both palettes keep `primary` saturated); keyboard focus paints
//! a 2px inset `focus_ring` (paint-only — never layout, via
//! `focus_ringed`); invalid fields edge in `error`; enabled
//! clickables show `Pointer`/text fields `Text`, inert controls the
//! platform arrow; decorative literals stay fixed and carry no text
//! (deliberate non-goals, not debt).

// Control components are `CamelCase` functions by framework convention
// (`#[component] fn Name` — M2 authoring surface), hence the crate-level
// allow (same rationale as vnode.rs's `Column::new` surface).
#![allow(non_snake_case)]

/// Unified kitchen sink reference app (Phase 7, decision 276).
pub mod kitchen_sink;
pub mod menu;
pub mod studio;

pub use kitchen_sink::{KitchenSinkApp, KitchenSinkProps, SinkDialogs, SinkTab};
pub use menu::{ContextMenu, ContextMenuProps, Menu, MenuItem, MenuItemProps, MenuProps};
pub mod bars;
pub mod button;
pub mod checkbox;
pub mod common;
pub mod datagrid;
pub mod date_picker;
pub mod error_boundary;
pub mod header_bar;
pub mod login;
pub mod modal;
pub mod progress_bar;
pub mod radio;
pub mod scrollbar;
pub mod select;
pub mod slider;
pub mod splitter;
pub mod tabs;
pub mod text_area;
pub mod text_input;
pub mod toast;
pub mod toggle;
pub mod tooltip;
pub mod tree;
pub mod views;
pub mod virtual_list;

pub use bars::*;
pub use button::*;
pub use checkbox::*;
pub use common::*;
pub use datagrid::*;
pub use date_picker::*;
pub use error_boundary::*;
pub use header_bar::*;
pub use login::*;
pub use modal::*;
pub use progress_bar::*;
pub use radio::*;
pub use scrollbar::*;
pub use select::*;
pub use slider::*;
pub use splitter::*;
pub use tabs::*;
pub use text_area::*;
pub use text_input::*;
pub use toast::*;
pub use toggle::*;
pub use tooltip::*;
pub use tree::*;
pub use views::*;
pub use virtual_list::*;

#[cfg(test)]
mod tests;
