// ---------------------------------------------------------------------------
// Caret geometry (visual lines, word classes, hit math)
// ---------------------------------------------------------------------------

/// One visual line for caret travel (Round 22.2): byte span plus
/// the logical-order cluster x-map (`(start, end, x, width)` in
/// device px, composite-relative).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VisualLine {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) clusters: Vec<(usize, usize, f32, f32)>,
}

/// Index of the visual line containing `byte` (Round 22.2): the
/// last span starting at or before it, except a wrap-boundary
/// caret (previous byte is not `\n`) belongs to the ending line
/// (end-of-line affinity); hard starts after `\n` open the next.
/// `byte == len` belongs to the last span.
pub(crate) fn line_index_containing(lines: &[VisualLine], composite: &str, byte: usize) -> usize {
    let byte = byte.min(composite.len());
    let mut idx = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.start > byte {
            break;
        }
        idx = i;
    }
    // Wrap-end affinity: a caret exactly on a wrap break (not
    // after `\n`, not the text end) renders at the ending line.
    if byte > 0
        && byte < composite.len()
        && !composite[..byte].ends_with('\n')
        && lines.get(idx).is_some_and(|l| l.start == byte)
        && idx > 0
    {
        idx -= 1;
    }
    idx
}

pub(crate) fn line_containing<'a>(
    lines: &'a [VisualLine],
    composite: &str,
    byte: usize,
) -> &'a VisualLine {
    let idx = line_index_containing(lines, composite, byte);
    &lines[idx.min(lines.len().saturating_sub(1))]
}

/// Caret x for `byte` inside its line (Round 22.2): the
/// containing cluster's leading edge (mid-cluster bytes snap by
/// midpoint, the shared tie rule); past the last cluster, the
/// line end; cluster-less (blank) lines sit at their start (x
/// origin — the line has no advances of its own).
pub(crate) fn x_in_line(line: &VisualLine, byte: usize) -> f32 {
    let mut x = 0.0;
    for &(s, e, cx, w) in &line.clusters {
        if byte <= s {
            return cx;
        }
        if byte < e {
            let mid = s + (e - s) / 2;
            return if byte < mid { cx } else { cx + w };
        }
        x = cx + w;
    }
    x
}

/// Byte in `line` nearest device-px `x` (Round 22.2): cluster
/// starts by midpoint, the line start before the first, the
/// content end past the last (a trailing `\n` belongs to the
/// break, never the caret — column affinity clamps, never
/// wraps).
pub(crate) fn byte_at_x(line: &VisualLine, composite: &str, x: f32) -> usize {
    let end = content_end(line, composite);
    if line.clusters.is_empty() {
        return line.start.min(end);
    }
    let first = &line.clusters[0];
    if x < first.2 {
        return line.start.min(first.0);
    }
    for &(s, e, cx, w) in &line.clusters {
        if x < cx + w / 2.0 {
            return s.clamp(line.start, end);
        }
        if x < cx + w {
            return e.min(end);
        }
    }
    end
}

/// Content end of a span (Round 22.2): the span end, unless the
/// span closes with `\n` (hard break) — then the newline byte,
/// which belongs to the break, is excluded.
pub(crate) fn content_end(line: &VisualLine, composite: &str) -> usize {
    if line.end > line.start
        && line.end <= composite.len()
        && composite.as_bytes().get(line.end - 1) == Some(&b'\n')
    {
        line.end - 1
    } else {
        line.end
    }
}

/// Hard-line fallback without a shaper (Round 22.2, graceful
/// decision-207 rule): previous/next `\n` boundaries with
/// character-column affinity (counts, not advances).
pub(crate) fn hard_line_byte(composite: &str, caret: usize, dir: i32, x: f32) -> usize {
    let caret = caret.min(composite.len());
    // Hard line spans.
    let mut starts = vec![0usize];
    for (i, c) in composite.char_indices() {
        if c == '\n' {
            starts.push(i + 1);
        }
    }
    let mut li = 0;
    for (i, &s) in starts.iter().enumerate() {
        if s > caret {
            break;
        }
        li = i;
    }
    // Column of the caret within its line (chars), or the x
    // column when travelling (callers pass the visual x; without
    // advances the char column stands in).
    let col = if x <= 0.0 {
        composite[starts[li]..caret].chars().count()
    } else {
        x as usize
    };
    let target = if dir < 0 {
        if li == 0 {
            return starts[0];
        }
        li - 1
    } else {
        if li + 1 >= starts.len() {
            return composite.len();
        }
        li + 1
    };
    let base = starts[target];
    let end = if target + 1 < starts.len() {
        // Exclude the line's own `\n`.
        starts[target + 1] - 1
    } else {
        composite.len()
    };
    let mut byte = base;
    for _ in 0..col {
        let next = composite[byte..end.min(composite.len())]
            .chars()
            .next()
            .map(|c| byte + c.len_utf8())
            .unwrap_or(end);
        if next > end {
            break;
        }
        byte = next;
    }
    byte.min(end)
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum WordClass {
    Word,
    Ideograph,
    /// Emoji run (Round 31, decision 347): Extended_Pictographic
    /// clusters + ZWJ/VS16 glue select and step as one unit.
    /// Regional indicators pair separately (Round 32, decision 348).
    Emoji,
    /// Regional-indicator pair unit (Round 32, decision 348 --
    /// UAX #29 WB15/WB16): consecutive RIs pair from the last
    /// non-RI (or string start); a lone RI is its own unit.
    Regional,
    Separator,
}

pub(crate) fn word_class(c: char) -> WordClass {
    if is_ideographish(c) {
        WordClass::Ideograph
    } else if c.is_alphanumeric() {
        WordClass::Word
    } else if is_regional(c) {
        WordClass::Regional
    } else if is_emoji_joiner(c) {
        // After alphanumeric on purpose: only current separators
        // change class (enclosed alphanumerics keep Word).
        WordClass::Emoji
    } else {
        WordClass::Separator
    }
}

fn is_ideographish(c: char) -> bool {
    let u = c as u32;
    (0x3400..=0x4DBF).contains(&u)
        || (0x4E00..=0x9FFF).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
        || (0x3040..=0x30FF).contains(&u) // hiragana + katakana
        || (0xAC00..=0xD7AF).contains(&u) // hangul
        || u >= 0x20000
}

/// Regional indicator for flag pairing (Round 32, decision 348).
fn is_regional(c: char) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&(c as u32))
}

/// Pictograph unit for word runs (Round 31, decision 347 --
/// hand ranges, the is_ideographish precedent; core stays
/// zero-dependency). Pragmatic Extended_Pictographic cover plus
/// the ZWJ/VS16 glue: contiguous emoji select and step as one
/// unit, and ZWJ keeps the run open across join sequences
/// (family emoji select whole). Lone joiners select alone
/// (harmless -- invisible chars). Regional indicators live in
/// `is_regional` (pair grouping, Round 32); CJK radicals/strokes
/// are untouched (outside these ranges, still separators).
fn is_emoji_joiner(c: char) -> bool {
    let u = c as u32;
    u == 0x00A9 // (c)
        || u == 0x00AE // (r)
        || u == 0x200D // ZWJ
        || u == 0xFE0F // VS16
        || u == 0x2122 // (tm)
        || (0x203C..=0x2049).contains(&u) // bang/query marks
        || (0x2190..=0x21FF).contains(&u) // arrows
        || (0x2300..=0x23FF).contains(&u) // misc technical
        || (0x2600..=0x27BF).contains(&u) // symbols + dingbats
        || (0x2B00..=0x2BFF).contains(&u) // more symbols
        || (0x1F000..=0x1F1E5).contains(&u) // emoticons to symbols
        || (0x1F200..=0x1FAFF).contains(&u) // ideograph supplements to extended-A
}
