//! UAX #14 line-break opportunities behind the [`BreakSource`]
//! trait (v2 item 2, decisions 192-193).
//!
//! Decision 192 locks the boundary: break tables live here, never
//! in `oppa` core (core keeps its M0 zero-dependency invariant --
//! `[dependencies]` empty). Decision 193 adopts `unicode-linebreak`
//! (lighter than `icu_segmenter`) with zero tailorings: this crate
//! filters its `Allowed` offsets to the [`BreakSource`] contract
//! (sorted, unique, within `(0, len)`, soft breaks only) and
//! `layout_text` consumes them the way it consumes `ShapedRun`s.
//!
//! Non-ASCII test corpus is `\u{...}` escapes (decision 45).

use oppa::text::BreakSource;
use unicode_linebreak::{linebreaks, BreakOpportunity};

/// The UAX #14 Unicode version this crate conforms to (re-exported
/// for corpus provenance headers).
pub use unicode_linebreak::UNICODE_VERSION;

/// Zero-tailoring UAX #14 break source: `Allowed` offsets from
/// [`linebreaks`], minus the endpoints. `Mandatory` offsets (`\n`
/// and friends) are excluded by contract -- hard breaks stay
/// cluster-driven in `layout_text`.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnicodeBreakSource;

impl UnicodeBreakSource {
    pub fn new() -> Self {
        Self
    }

    /// Soft-break-after byte offsets in `text` (the [`BreakSource`]
    /// contract, callable without a receiver for tests).
    pub fn soft_breaks(text: &str) -> Vec<usize> {
        let len = text.len();
        let mut out: Vec<usize> = linebreaks(text)
            .filter(|(off, kind)| *kind == BreakOpportunity::Allowed && *off > 0 && *off < len)
            .map(|(off, _)| off)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

impl BreakSource for UnicodeBreakSource {
    fn opportunities(&self, text: &str) -> Vec<usize> {
        Self::soft_breaks(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn breaks(text: &str) -> Vec<usize> {
        UnicodeBreakSource::soft_breaks(text)
    }

    #[test]
    fn space_creates_one_opportunity_after_it() {
        // "hello world": the space is byte 5, break after it is 6.
        assert_eq!(breaks("hello world"), vec![6]);
    }

    #[test]
    fn empty_and_single_word_have_no_opportunities() {
        assert!(breaks("").is_empty());
        assert!(breaks("word").is_empty());
    }

    #[test]
    fn hard_breaks_are_excluded_newline_stays_cluster_driven() {
        // "a\nb": UAX reports Mandatory after \n and at end -- neither
        // is a soft opportunity.
        assert!(breaks("a\nb").is_empty());
        assert!(breaks("a\nb\n").is_empty());
    }

    #[test]
    fn cjk_chars_break_between_every_pair() {
        // Three CJK chars (3 bytes each): breaks after each but the last.
        let text = "\u{65E5}\u{672C}\u{8A9E}";
        assert_eq!(text.len(), 9);
        assert_eq!(breaks(text), vec![3, 6]);
    }

    #[test]
    fn space_run_offers_one_break_after_the_run() {
        // "a  b": UAX offers NO break between consecutive spaces --
        // one break after the run (offset 3). The layout trim rule
        // strips the whole run's spaces at that break, never strands one.
        assert_eq!(breaks("a  b"), vec![3]);
    }

    #[test]
    fn arabic_words_break_at_spaces() {
        // "word1 word2" shape with Arabic letters (escapes per decision 45).
        let text = "\u{645}\u{631}\u{62D}\u{628}\u{627} \u{627}\u{644}\u{639}\u{627}\u{644}\u{645}";
        let space = text.find(' ').expect("space");
        assert_eq!(breaks(text), vec![space + 1]);
    }

    #[test]
    fn contract_holds_offsets_sorted_unique_interior_char_boundaries() {
        let texts = [
            "hello world, again and again.",
            "\u{65E5}\u{672C}\u{8A9E} test \u{645}\u{631}\u{62D}\u{628}\u{627}",
            "a  b   c",
            "one-two three/four",
        ];
        for text in texts {
            let got = breaks(text);
            let mut sorted = got.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(got, sorted, "sorted+unique for {text:?}");
            for off in &got {
                assert!(*off > 0 && *off < text.len(), "interior for {text:?}");
                assert!(text.is_char_boundary(*off), "char boundary for {text:?}");
            }
        }
    }

    #[test]
    fn url_breaks_after_slash_runs() {
        // "See https://x.io/a/b end": S0 e1 e2 SP3 h4..s8 :9 /10 /11
        // x12 .13 i14 o15 /16 a17 /18 b19 SP20 e21 n22 d23.
        // UAX SY breaks after `/` (one break after the `//` run, the
        // same run-pattern as spaces); no break around `:` or `.`.
        assert_eq!(breaks("See https://x.io/a/b end"), vec![4, 12, 17, 19, 21]);
    }

    #[test]
    fn unicode_version_is_pinned_for_provenance() {
        // Triple (major, minor, update); floor-pinned so a UAX data
        // bump is a visible corpus event, not a silent drift.
        assert!(UNICODE_VERSION.0 >= 15, "{UNICODE_VERSION:?}");
    }
}
