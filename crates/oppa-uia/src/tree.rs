//! Incremental provider-side tree mirror (M10 close-v1): applies
//! each [`SemanticsDiff`](oppa::SemanticsDiff) once, in order, so
//! fragment navigation (parent/children/siblings) answers from live
//! state. Same incremental discipline as `AtspiTree`; UIA event
//! raising is the stated cut, so no event log lives here — only
//! the queryable snapshot.

use std::collections::HashMap;

use oppa::{NodeId, SemanticsDiff};

/// One mirrored element: what the provider reads per node.
#[derive(Clone, Debug, PartialEq)]
pub struct UiaNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub role: oppa::Role,
    pub name: String,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub disabled: bool,
    pub bounds: (f32, f32, f32, f32),
}

/// The mirror: `apply` per commit, queries for provider reads.
#[derive(Clone, Debug, Default)]
pub struct UiaTree {
    nodes: HashMap<NodeId, UiaNode>,
}

impl UiaTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies one commit's diff. `parent_of` resolves hierarchy from
    /// the reconciler (returns `None` for roots).
    pub fn apply(&mut self, diff: &SemanticsDiff, parent_of: &dyn Fn(NodeId) -> Option<NodeId>) {
        for id in &diff.removed {
            self.nodes.remove(id);
        }
        for e in &diff.upserted {
            self.nodes.insert(
                e.node,
                UiaNode {
                    id: e.node,
                    parent: parent_of(e.node),
                    role: e.semantics.role,
                    name: e.semantics.label.as_deref().unwrap_or("").to_string(),
                    checked: e.semantics.checked,
                    selected: e.semantics.selected,
                    disabled: e.semantics.disabled,
                    bounds: (e.x, e.y, e.w, e.h),
                },
            );
        }
    }

    pub fn get(&self, id: NodeId) -> Option<&UiaNode> {
        self.nodes.get(&id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Children of `parent` in id order (fragment FirstChild walks
    /// this; NextSibling indexes into it).
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
    use oppa::{NodeId, Semantics, SemanticsEntry};

    fn entry(node: (u32, u32), s: Semantics) -> SemanticsEntry {
        SemanticsEntry {
            node: NodeId::new(node.0, node.1),
            semantics: s,
            x: 0.0,
            y: 0.0,
            w: 44.0,
            h: 24.0,
        }
    }

    #[test]
    fn apply_upsert_remove_and_navigate() {
        let mut tree = UiaTree::new();
        let diff = SemanticsDiff {
            upserted: vec![
                entry((1, 0), Semantics::switch().checked(false).label("Wi-Fi")),
                entry((2, 0), Semantics::list_item().selected(true).label("Bob")),
            ],
            removed: vec![],
        };
        tree.apply(&diff, &|_| None);
        assert_eq!(tree.len(), 2);
        let sw = tree.get(NodeId::new(1, 0)).expect("switch mirrored");
        assert_eq!(sw.name, "Wi-Fi");
        assert_eq!(sw.checked, Some(false));
        assert_eq!(tree.children_of(None).len(), 2);
        let diff2 = SemanticsDiff {
            upserted: vec![entry(
                (1, 0),
                Semantics::switch().checked(true).label("Wi-Fi"),
            )],
            removed: vec![NodeId::new(2, 0)],
        };
        tree.apply(&diff2, &|_| None);
        assert_eq!(tree.len(), 1);
        assert_eq!(
            tree.get(NodeId::new(1, 0)).expect("live").checked,
            Some(true)
        );
        assert!(tree.get(NodeId::new(2, 0)).is_none());
    }
}
