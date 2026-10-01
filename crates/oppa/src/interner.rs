use std::collections::HashMap;
use std::hash::Hash;

/// Interned style id (DESIGN §2.2, locked #8): typed style structs are
/// interned into a table whose payloads are shared across thousands of
/// nodes; a theme flip becomes a per-node `StyleId` change.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct StyleId(u32);

impl StyleId {
    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for StyleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StyleId({})", self.0)
    }
}

/// Intern table: value -> [`StyleId`] -> value, with structural dedup.
pub struct Interner<T: Eq + Hash + Clone> {
    map: HashMap<T, StyleId>,
    values: Vec<T>,
}

impl<T: Eq + Hash + Clone> Default for Interner<T> {
    fn default() -> Self {
        Self {
            map: HashMap::default(),
            values: Vec::default(),
        }
    }
}

impl<T: Eq + Hash + Clone> Interner<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, value: T) -> StyleId {
        if let Some(&id) = self.map.get(&value) {
            return id;
        }
        let id = StyleId(self.values.len() as u32);
        self.values.push(value.clone());
        self.map.insert(value, id);
        id
    }

    pub fn get(&self, id: StyleId) -> Option<&T> {
        self.values.get(id.0 as usize)
    }

    /// Interner dedup implies `lookup(intern(v)) == Some(id)`.
    pub fn lookup(&self, value: &T) -> Option<StyleId> {
        self.map.get(value).copied()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_dedup_and_roundtrip() {
        let mut t: Interner<String> = Interner::new();
        let a = t.intern("bg:red;radius:12".to_string());
        let b = t.intern("bg:red;radius:12".to_string());
        assert_eq!(a, b, "equal payloads share one id");
        let c = t.intern("bg:blue;radius:12".to_string());
        assert_ne!(a, c);
        assert_eq!(t.len(), 2);
        assert_eq!(t.get(a), Some(&"bg:red;radius:12".to_string()));
        assert_eq!(t.get(StyleId(u32::MAX)), None);
        assert_eq!(t.lookup(&"bg:red;radius:12".to_string()), Some(a));
    }
}
