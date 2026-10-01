/// FNV-1a 64-bit — a hash with cross-version stability (std's `DefaultHasher`
/// does not guarantee stability, and handler ids must be stable symbol hashes
/// per DESIGN §5.1/§5.3: the registry re-resolves the same hash to new code
/// after every hot swap).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct SymbolHash(u64);

impl SymbolHash {
    pub const fn of(name: &str) -> Self {
        Self(fnv1a64(name.as_bytes()))
    }

    pub const fn bits(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for SymbolHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#018x}", self.0)
    }
}

pub const fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(PRIME);
        i += 1;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_across_calls_and_known_value() {
        let a = SymbolHash::of("toggle.on_press");
        let b = SymbolHash::of("toggle.on_press");
        assert_eq!(a, b);
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
        assert_ne!(SymbolHash::of("a"), SymbolHash::of("b"));
        assert_ne!(SymbolHash::of("ab"), SymbolHash::of("ba"));
    }
}
