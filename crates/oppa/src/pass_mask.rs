use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Sub, SubAssign};

/// Per-node dirty flags for the pass pipeline (DESIGN §2.2, §9.1):
/// STRUCTURE | STYLE | LAYOUT | PAINT | TEXT | SEMANTICS.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct PassMask(u16);

impl PassMask {
    pub const EMPTY: PassMask = PassMask(0);
    pub const STRUCTURE: PassMask = PassMask(1 << 0);
    pub const STYLE: PassMask = PassMask(1 << 1);
    pub const LAYOUT: PassMask = PassMask(1 << 2);
    pub const PAINT: PassMask = PassMask(1 << 3);
    pub const TEXT: PassMask = PassMask(1 << 4);
    pub const SEMANTICS: PassMask = PassMask(1 << 5);
    pub const ALL: PassMask = PassMask(0b11_1111);

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn from_bits(bits: u16) -> Self {
        PassMask(bits)
    }

    pub const fn contains(self, other: PassMask) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: PassMask) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn set(&mut self, flag: PassMask) {
        self.0 |= flag.0;
    }

    pub fn clear(&mut self, flag: PassMask) {
        self.0 &= !flag.0;
    }

    pub fn without(self, flag: PassMask) -> PassMask {
        PassMask(self.0 & !flag.0)
    }

    pub fn names(self) -> Vec<&'static str> {
        let all: [(PassMask, &str); 6] = [
            (Self::STRUCTURE, "STRUCTURE"),
            (Self::STYLE, "STYLE"),
            (Self::LAYOUT, "LAYOUT"),
            (Self::PAINT, "PAINT"),
            (Self::TEXT, "TEXT"),
            (Self::SEMANTICS, "SEMANTICS"),
        ];
        all.iter()
            .filter(|(f, _)| self.contains(*f))
            .map(|(_, n)| *n)
            .collect()
    }
}

impl BitOr for PassMask {
    type Output = PassMask;
    fn bitor(self, rhs: PassMask) -> PassMask {
        PassMask(self.0 | rhs.0)
    }
}

impl BitOrAssign for PassMask {
    fn bitor_assign(&mut self, rhs: PassMask) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for PassMask {
    type Output = PassMask;
    fn bitand(self, rhs: PassMask) -> PassMask {
        PassMask(self.0 & rhs.0)
    }
}

impl BitAndAssign for PassMask {
    fn bitand_assign(&mut self, rhs: PassMask) {
        self.0 &= rhs.0;
    }
}

impl Sub for PassMask {
    type Output = PassMask;
    fn sub(self, rhs: PassMask) -> PassMask {
        PassMask(self.0 & !rhs.0)
    }
}

impl SubAssign for PassMask {
    fn sub_assign(&mut self, rhs: PassMask) {
        self.0 &= !rhs.0;
    }
}

impl fmt::Display for PassMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            write!(f, "EMPTY")
        } else {
            write!(f, "{}", self.names().join("|"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_ops() {
        let mut m = PassMask::EMPTY;
        assert!(m.is_empty());
        m.set(PassMask::STYLE | PassMask::PAINT);
        assert_eq!(m.to_string(), "STYLE|PAINT");
        assert!(m.contains(PassMask::STYLE));
        assert!(m.contains(PassMask::STYLE | PassMask::PAINT));
        assert!(!m.contains(PassMask::TEXT));
        m.clear(PassMask::STYLE);
        assert_eq!(m, PassMask::PAINT);
        m.set(PassMask::LAYOUT);
        assert_eq!(m - PassMask::PAINT, PassMask::LAYOUT);
        assert!(m.intersects(PassMask::LAYOUT));
        assert!(!m.intersects(PassMask::SEMANTICS));
    }

    #[test]
    fn all_mask_contains_every_flag() {
        for f in [
            PassMask::STRUCTURE,
            PassMask::STYLE,
            PassMask::LAYOUT,
            PassMask::PAINT,
            PassMask::TEXT,
            PassMask::SEMANTICS,
        ] {
            assert!(PassMask::ALL.contains(f));
            assert_eq!(f.names().len(), 1);
        }
        assert!(PassMask::ALL.contains(PassMask::ALL));
        assert!(PassMask::EMPTY.is_empty());
    }
}
