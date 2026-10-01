//! Incremental accessible-tree mirror (M10): [`SemanticsDiff`] in,
//! at-spi2 events out, queryable tree within.
//!
//! The highest-risk AT-SPI item is incremental-sync semantics (linux
//! overview): an AT client must see `children-changed` only for
//! genuine structural changes and `state-changed`/`property-change`
//! for value changes — never a full-tree resync per commit. This
//! mirror applies each [`SemanticsDiff`](oppa::SemanticsDiff) once,
//! in order, and emits exactly the events the diff justifies:
//!
//! - New id under a known parent → `children-changed:add` once,
//!   then the node's own state/name as `state-changed` /
//!   `property-change` (an AT reading the child right after `add`
//!   sees settled values — the entry is stored before events emit).
//! - Payload or bounds change on a known id → value events only,
//!   never `children-changed` (no resync storms on scroll ticks —
//!   the M8 virtualization sweep is the load case: 20 cells
//!   re-derived per tick must not re-announce the list).
//! - Removal → `children-changed:remove` once; later queries on the
//!   id fail loudly (`UnknownNode`), never alias a recycled id.
//!
//! Hierarchy comes from the caller (`parent_of`): the diff carries
//! ids, the reconciler owns parentage — same split as the DOM
//! backend's retained reads. Event names are the at-spi2 wire names
//! (`object:state-changed:checked`, …).

use std::collections::HashMap;
use std::rc::Rc;

use oppa::{NodeId, Semantics, SemanticsDiff};

use super::roles::{atspi_actions, atspi_role, atspi_states, atspi_value};

/// One mirrored accessible: what an AT client would read.
#[derive(Clone, Debug, PartialEq)]
pub struct AtspiNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub role: &'static str,
    pub name: Option<String>,
    pub states: Vec<&'static str>,
    /// AT-SPI Action names (decision 352 — G18): `click` on
    /// Button/MenuItem, empty elsewhere (no action interface).
    pub actions: Vec<&'static str>,
    /// AT-SPI numeric Value `(current, min, max)` (decision 352 —
    /// G18): `Some` exactly when `value_num` is set; `None` means no
    /// value interface (never an invented number).
    pub value: Option<(f64, f64, f64)>,
    pub bounds: (f32, f32, f32, f32),
}

/// Host-loop action driver (decision 352 — G18): invoked with the
/// mirrored node id and the action name (`"click"`).
pub type AtspiInvokeFn = dyn Fn(NodeId, &str);

/// Host-loop drivers the mirror calls into for AT actions (decision
/// 352 — G18). Uninstalled entries refuse loudly (`NoHandler`),
/// never as silent no-ops. Headless-testable: the test installs a
/// recording closure, no live bus needed.
#[derive(Clone, Default)]
pub struct AtspiAction {
    /// Action invocation for Action-capable nodes (Button/MenuItem
    /// `click` — the test wires this to `inject_input` at the node's
    /// center, the UIA `on_toggle` precedent).
    pub on_invoke: Option<Rc<AtspiInvokeFn>>,
}

impl std::fmt::Debug for AtspiAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AtspiAction")
            .field("on_invoke", &self.on_invoke.is_some())
            .finish()
    }
}

/// Loud invocation failure (never a silent no-op).
#[derive(Clone, Debug, PartialEq)]
pub enum AtspiActionError {
    UnknownNode(NodeId),
    UnknownAction(String),
    NoHandler,
}

impl std::fmt::Display for AtspiActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AtspiActionError::UnknownNode(id) => write!(f, "unknown node {id:?}"),
            AtspiActionError::UnknownAction(name) => write!(f, "unknown action {name}"),
            AtspiActionError::NoHandler => write!(f, "no action handler installed"),
        }
    }
}

impl std::error::Error for AtspiActionError {}

/// Emitter events in commit order (the D-Bus signal vocabulary —
/// names, not framing; framing needs a session bus, gated).
#[derive(Clone, Debug, PartialEq)]
pub enum AtspiEvent {
    ChildrenAdded {
        parent: Option<NodeId>,
        child: NodeId,
    },
    ChildrenRemoved {
        parent: Option<NodeId>,
        child: NodeId,
    },
    StateChangedChecked {
        node: NodeId,
        on: bool,
    },
    StateChangedSelected {
        node: NodeId,
        on: bool,
    },
    StateChangedEnabled {
        node: NodeId,
        on: bool,
    },
    /// Validation mark flipped (decision 352 — G7): the wire name is
    /// `object:state-changed:invalid`.
    StateChangedInvalid {
        node: NodeId,
        on: bool,
    },
    /// Numeric Value changed (decision 352 — G18): the wire name is
    /// `object:property-change:accessible-value`.
    ValueChanged {
        node: NodeId,
    },
    NameChanged {
        node: NodeId,
    },
    BoundsMoved {
        node: NodeId,
    },
}

impl AtspiEvent {
    /// The at-spi2 signal name for this event.
    pub fn dbus_name(&self) -> &'static str {
        match self {
            AtspiEvent::ChildrenAdded { .. } => "object:children-changed:add",
            AtspiEvent::ChildrenRemoved { .. } => "object:children-changed:remove",
            AtspiEvent::StateChangedChecked { .. } => "object:state-changed:checked",
            AtspiEvent::StateChangedSelected { .. } => "object:state-changed:selected",
            AtspiEvent::StateChangedEnabled { .. } => "object:state-changed:enabled",
            AtspiEvent::StateChangedInvalid { .. } => "object:state-changed:invalid",
            AtspiEvent::ValueChanged { .. } => "object:property-change:accessible-value",
            AtspiEvent::NameChanged { .. } => "object:property-change:accessible-name",
            AtspiEvent::BoundsMoved { .. } => "object:bounds-changed",
        }
    }
}

/// The mirror: `apply` per commit, queries for AT reads, `take_events`
/// for the signal log.
#[derive(Clone, Debug, Default)]
pub struct AtspiTree {
    nodes: HashMap<NodeId, AtspiNode>,
    events: Vec<AtspiEvent>,
    actions: AtspiAction,
}

impl AtspiTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs the host-loop action drivers (decision 352). The
    /// mirror never touches framework state directly — invocation
    /// enqueues through these callbacks; the host loop drains them
    /// on the INPUT phase (ADR-0010 single-UI-thread rule).
    pub fn set_actions(&mut self, actions: AtspiAction) {
        self.actions = actions;
    }

    /// Invokes one named action on one mirrored node (decision 352 —
    /// G18, headless-testable, no live bus): unknown ids, unlisted
    /// names, and uninstalled handlers all refuse loudly (never a
    /// silent no-op).
    pub fn invoke_action(&self, id: NodeId, name: &str) -> Result<(), AtspiActionError> {
        let node = self
            .nodes
            .get(&id)
            .ok_or(AtspiActionError::UnknownNode(id))?;
        if !node.actions.contains(&name) {
            return Err(AtspiActionError::UnknownAction(name.to_string()));
        }
        match &self.actions.on_invoke {
            Some(f) => {
                f(id, name);
                Ok(())
            }
            None => Err(AtspiActionError::NoHandler),
        }
    }

    /// Applies one commit's diff. `parent_of` resolves hierarchy from
    /// the reconciler (returns `None` for roots).
    pub fn apply(&mut self, diff: &SemanticsDiff, parent_of: &dyn Fn(NodeId) -> Option<NodeId>) {
        for id in &diff.removed {
            if let Some(node) = self.nodes.remove(id) {
                self.events.push(AtspiEvent::ChildrenRemoved {
                    parent: node.parent,
                    child: *id,
                });
            }
        }
        for e in &diff.upserted {
            let parent = parent_of(e.node);
            let node = AtspiNode {
                id: e.node,
                parent,
                role: atspi_role(&e.semantics),
                name: e.semantics.label.as_deref().map(str::to_string),
                states: atspi_states(&e.semantics),
                actions: atspi_actions(&e.semantics),
                value: atspi_value(&e.semantics),
                bounds: (e.x, e.y, e.w, e.h),
            };
            match self.nodes.get(&e.node) {
                None => {
                    self.nodes.insert(e.node, node);
                    self.events.push(AtspiEvent::ChildrenAdded {
                        parent,
                        child: e.node,
                    });
                    // A fresh child announces its values once (an AT
                    // reading after `add` sees settled state).
                    self.announce_new(&e.semantics, e.node);
                }
                Some(old) => {
                    let old = old.clone();
                    self.nodes.insert(e.node, node);
                    self.emit_value_delta(&old, &e.semantics, e.node, (e.x, e.y, e.w, e.h));
                }
            }
        }
    }

    fn announce_new(&mut self, s: &Semantics, node: NodeId) {
        if s.role != oppa::Role::TextField {
            if s.checked.is_some() {
                self.events.push(AtspiEvent::StateChangedChecked {
                    node,
                    on: s.checked == Some(true),
                });
            }
            if s.selected.is_some() {
                self.events.push(AtspiEvent::StateChangedSelected {
                    node,
                    on: s.selected == Some(true),
                });
            }
        }
        if s.disabled {
            self.events
                .push(AtspiEvent::StateChangedEnabled { node, on: false });
        }
        if s.invalid {
            self.events
                .push(AtspiEvent::StateChangedInvalid { node, on: true });
        }
        if s.label.is_some() {
            self.events.push(AtspiEvent::NameChanged { node });
        }
        if atspi_value(s).is_some() {
            self.events.push(AtspiEvent::ValueChanged { node });
        }
    }

    fn emit_value_delta(
        &mut self,
        old: &AtspiNode,
        new: &Semantics,
        node: NodeId,
        bounds: (f32, f32, f32, f32),
    ) {
        if new.role == oppa::Role::TextField {
            if old.name.as_deref() != new.label.as_deref() {
                self.events.push(AtspiEvent::NameChanged { node });
            }
            if old.bounds != bounds {
                self.events.push(AtspiEvent::BoundsMoved { node });
            }
            let old_invalid = old.states.contains(&"invalid");
            if old_invalid != new.invalid {
                self.events.push(AtspiEvent::StateChangedInvalid {
                    node,
                    on: new.invalid,
                });
            }
            return;
        }
        let old_checked = old.states.contains(&"checked");
        let new_checked = new.checked == Some(true);
        if old_checked != new_checked && new.checked.is_some() {
            self.events.push(AtspiEvent::StateChangedChecked {
                node,
                on: new_checked,
            });
        }
        let old_selected = old.states.contains(&"selected");
        let new_selected = new.selected == Some(true);
        if old_selected != new_selected && new.selected.is_some() {
            self.events.push(AtspiEvent::StateChangedSelected {
                node,
                on: new_selected,
            });
        }
        let old_enabled = old.states.contains(&"enabled");
        let new_enabled = !new.disabled;
        if old_enabled != new_enabled {
            self.events.push(AtspiEvent::StateChangedEnabled {
                node,
                on: new_enabled,
            });
        }
        let old_invalid = old.states.contains(&"invalid");
        if old_invalid != new.invalid {
            self.events.push(AtspiEvent::StateChangedInvalid {
                node,
                on: new.invalid,
            });
        }
        if old.name.as_deref() != new.label.as_deref() {
            self.events.push(AtspiEvent::NameChanged { node });
        }
        if old.bounds != bounds {
            self.events.push(AtspiEvent::BoundsMoved { node });
        }
        let old_value = old.value;
        let new_value = atspi_value(new);
        if old_value != new_value && new_value.is_some() {
            self.events.push(AtspiEvent::ValueChanged { node });
        }
    }

    /// Drains the signal log in commit order.
    pub fn take_events(&mut self) -> Vec<AtspiEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn get(&self, id: NodeId) -> Option<&AtspiNode> {
        self.nodes.get(&id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Children of `parent` in id order (what an AT paginates).
    pub fn children_of(&self, parent: Option<NodeId>) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = self
            .nodes
            .values()
            .filter(|n| n.parent == parent)
            .map(|n| n.id)
            .collect();
        out.sort_by_key(|id| (id.index(), id.generation()));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oppa::Semantics;

    fn entry(node: (u32, u32), s: Semantics) -> oppa::SemanticsEntry {
        oppa::SemanticsEntry {
            node: NodeId::new(node.0, node.1),
            semantics: s,
            x: 0.0,
            y: 0.0,
            w: 44.0,
            h: 24.0,
        }
    }

    fn diff(up: Vec<oppa::SemanticsEntry>, rem: Vec<NodeId>) -> SemanticsDiff {
        SemanticsDiff {
            upserted: up,
            removed: rem,
        }
    }

    #[test]
    fn mount_announces_add_then_values() {
        let mut tree = AtspiTree::new();
        tree.apply(
            &diff(
                vec![entry(
                    (1, 0),
                    Semantics::switch().checked(false).label("Wi-Fi"),
                )],
                vec![],
            ),
            &|_| None,
        );
        let id = NodeId::new(1, 0);
        let node = tree.get(id).expect("mounted node is queryable");
        assert_eq!(node.role, "toggle button");
        assert_eq!(node.name.as_deref(), Some("Wi-Fi"));
        assert!(node.states.contains(&"checkable"));
        assert!(!node.states.contains(&"checked"));
        assert_eq!(
            tree.take_events(),
            vec![
                AtspiEvent::ChildrenAdded {
                    parent: None,
                    child: id
                },
                AtspiEvent::StateChangedChecked {
                    node: id,
                    on: false
                },
                AtspiEvent::NameChanged { node: id },
            ]
        );
        assert_eq!(
            tree.children_of(None),
            vec![id],
            "root parents the mounted node"
        );
    }

    #[test]
    fn value_change_emits_state_only_never_resync() {
        let mut tree = AtspiTree::new();
        let id = NodeId::new(1, 0);
        tree.apply(
            &diff(
                vec![entry((1, 0), Semantics::switch().checked(false))],
                vec![],
            ),
            &|_| None,
        );
        tree.take_events();
        // Flip: same id, new value — one state event, no children-changed.
        tree.apply(
            &diff(
                vec![entry((1, 0), Semantics::switch().checked(true))],
                vec![],
            ),
            &|_| None,
        );
        assert_eq!(
            tree.take_events(),
            vec![AtspiEvent::StateChangedChecked { node: id, on: true }]
        );
        assert!(tree
            .get(id)
            .expect("still live")
            .states
            .contains(&"checked"));
    }

    #[test]
    fn g13_catalog_roles_serve_end_to_end() {
        // G13: the G2 catalog payloads through the live mirror —
        // role names + capability states per leg contract.
        let mut tree = AtspiTree::new();
        tree.apply(
            &diff(
                vec![
                    entry((1, 0), Semantics::button().label("OK")),
                    entry((2, 0), Semantics::checkbox().checked(true)),
                    entry(
                        (3, 0),
                        Semantics::slider().label("Volume").value_text("50 percent"),
                    ),
                ],
                vec![],
            ),
            &|_| None,
        );
        let button = tree.get(NodeId::new(1, 0)).expect("button live");
        assert_eq!(button.role, "push button");
        assert_eq!(button.name.as_deref(), Some("OK"));
        let checkbox = tree.get(NodeId::new(2, 0)).expect("checkbox live");
        assert_eq!(checkbox.role, "check box");
        assert!(checkbox.states.contains(&"checkable"));
        assert!(checkbox.states.contains(&"checked"));
        let slider = tree.get(NodeId::new(3, 0)).expect("slider live");
        assert_eq!(slider.role, "slider");
        assert_eq!(slider.name.as_deref(), Some("Volume"));
    }

    #[test]
    fn removal_announces_once_and_forgets() {
        let mut tree = AtspiTree::new();
        let id = NodeId::new(2, 0);
        tree.apply(
            &diff(vec![entry((2, 0), Semantics::list_item())], vec![]),
            &|_| None,
        );
        tree.take_events();
        tree.apply(&diff(vec![], vec![id]), &|_| None);
        assert_eq!(
            tree.take_events(),
            vec![AtspiEvent::ChildrenRemoved {
                parent: None,
                child: id
            }]
        );
        assert!(tree.get(id).is_none(), "removed ids never alias");
        // Double-removal is silent (diffs never repeat removals).
        tree.apply(&diff(vec![], vec![id]), &|_| None);
        assert!(tree.take_events().is_empty());
    }

    #[test]
    fn event_names_are_the_wire_vocabulary() {
        assert_eq!(
            AtspiEvent::StateChangedChecked {
                node: NodeId::new(0, 0),
                on: true
            }
            .dbus_name(),
            "object:state-changed:checked"
        );
        assert_eq!(
            AtspiEvent::ChildrenAdded {
                parent: None,
                child: NodeId::new(0, 0)
            }
            .dbus_name(),
            "object:children-changed:add"
        );
        assert_eq!(
            AtspiEvent::NameChanged {
                node: NodeId::new(0, 0)
            }
            .dbus_name(),
            "object:property-change:accessible-name"
        );
        assert_eq!(
            AtspiEvent::ValueChanged {
                node: NodeId::new(0, 0)
            }
            .dbus_name(),
            "object:property-change:accessible-value"
        );
        assert_eq!(
            AtspiEvent::StateChangedInvalid {
                node: NodeId::new(0, 0),
                on: true
            }
            .dbus_name(),
            "object:state-changed:invalid"
        );
    }

    #[test]
    fn phase36_slider_value_announces_and_deltas() {
        let mut tree = AtspiTree::new();
        let id = NodeId::new(7, 0);
        tree.apply(
            &diff(
                vec![entry(
                    (7, 0),
                    Semantics::slider().label("Volume").value_num(50.0),
                )],
                vec![],
            ),
            &|_| None,
        );
        let node = tree.get(id).expect("slider live");
        assert_eq!(node.role, "slider");
        assert!(
            node.actions.is_empty(),
            "sliders act through Value, not Action"
        );
        assert_eq!(node.value, Some((50.0, 0.0, 100.0)));
        assert!(tree
            .take_events()
            .contains(&AtspiEvent::ValueChanged { node: id }));
        // Move: same triple shape, new current — one value event, no resync.
        tree.apply(
            &diff(
                vec![entry(
                    (7, 0),
                    Semantics::slider().label("Volume").value_num(75.0),
                )],
                vec![],
            ),
            &|_| None,
        );
        assert_eq!(
            tree.take_events(),
            vec![AtspiEvent::ValueChanged { node: id }]
        );
        assert_eq!(
            tree.get(id).expect("still live").value,
            Some((75.0, 0.0, 100.0))
        );
    }

    #[test]
    fn phase36_button_click_invokes_through_installed_handler() {
        use std::rc::Rc;
        let mut tree = AtspiTree::new();
        let id = NodeId::new(3, 0);
        tree.apply(
            &diff(vec![entry((3, 0), Semantics::button().label("OK"))], vec![]),
            &|_| None,
        );
        assert_eq!(tree.get(id).expect("button live").actions, vec!["click"]);
        // No handler installed: loud, never a silent no-op.
        assert_eq!(
            tree.invoke_action(id, "click"),
            Err(super::AtspiActionError::NoHandler)
        );
        // Unlisted names refuse loudly even with a handler installed.
        tree.set_actions(super::AtspiAction {
            on_invoke: Some(Rc::new(|_, _| {})),
        });
        assert_eq!(
            tree.invoke_action(id, "press"),
            Err(super::AtspiActionError::UnknownAction("press".to_string()))
        );
        // Unknown ids refuse loudly.
        assert_eq!(
            tree.invoke_action(NodeId::new(9, 9), "click"),
            Err(super::AtspiActionError::UnknownNode(NodeId::new(9, 9)))
        );
        // Installed handler fires with (id, name).
        let fired: Rc<std::cell::RefCell<Vec<(NodeId, String)>>> =
            Rc::new(std::cell::RefCell::new(Vec::new()));
        let record = fired.clone();
        tree.set_actions(super::AtspiAction {
            on_invoke: Some(Rc::new(move |got: NodeId, name: &str| {
                record.borrow_mut().push((got, name.to_string()));
            })),
        });
        assert_eq!(tree.invoke_action(id, "click"), Ok(()));
        assert_eq!(*fired.borrow(), vec![(id, "click".to_string())]);
        // Menu rows ride the same action.
        tree.apply(
            &diff(
                vec![entry((4, 0), Semantics::menu_item().label("Copy"))],
                vec![],
            ),
            &|_| None,
        );
        let item = NodeId::new(4, 0);
        assert_eq!(tree.get(item).expect("item live").actions, vec!["click"]);
        assert_eq!(tree.invoke_action(item, "click"), Ok(()));
        assert_eq!(fired.borrow().len(), 2);
    }

    #[test]
    fn phase36_invalid_flip_announces_state() {
        let mut tree = AtspiTree::new();
        let id = NodeId::new(5, 0);
        tree.apply(
            &diff(
                vec![entry((5, 0), Semantics::text_field().label("Age"))],
                vec![],
            ),
            &|_| None,
        );
        assert!(!tree
            .get(id)
            .expect("field live")
            .states
            .contains(&"invalid"));
        tree.take_events();
        tree.apply(
            &diff(
                vec![entry(
                    (5, 0),
                    Semantics::text_field().label("Age").invalid(true),
                )],
                vec![],
            ),
            &|_| None,
        );
        assert_eq!(
            tree.take_events(),
            vec![AtspiEvent::StateChangedInvalid { node: id, on: true }]
        );
        assert!(tree
            .get(id)
            .expect("still live")
            .states
            .contains(&"invalid"));
    }
}
