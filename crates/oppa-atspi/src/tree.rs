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

use oppa::{NodeId, Semantics, SemanticsDiff};

use super::roles::{atspi_role, atspi_states};

/// One mirrored accessible: what an AT client would read.
#[derive(Clone, Debug, PartialEq)]
pub struct AtspiNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub role: &'static str,
    pub name: Option<String>,
    pub states: Vec<&'static str>,
    pub bounds: (f32, f32, f32, f32),
}

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
}

impl AtspiTree {
    pub fn new() -> Self {
        Self::default()
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
        if s.label.is_some() {
            self.events.push(AtspiEvent::NameChanged { node });
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
        if old.name.as_deref() != new.label.as_deref() {
            self.events.push(AtspiEvent::NameChanged { node });
        }
        if old.bounds != bounds {
            self.events.push(AtspiEvent::BoundsMoved { node });
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
    }
}
