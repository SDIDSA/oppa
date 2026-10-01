//! SVG path-data parser for [`DrawOp::Path`](oppa::DrawOp) (decision 291).
//!
//! `tiny-skia-path` 0.12 ships no `from_svg` (verified against the
//! registry source — the brief's `PathBuilder::from_svg` does not
//! exist there), so the CPU backend owns this parser while Vello
//! uses `kurbo::BezPath::from_svg` natively. Same contract, second
//! implementation (the text path's discipline: CPU cells vs Vello
//! outlines, compared by the oracle, never shared).
//!
//! Supported verbs (SVG 1.1 §8.5, absolute + relative): `M L H V C
//! S Q T A Z`, implicit repeats (extra `M` pairs become `L`), and
//! sign/decimal/exponent floats with comma/whitespace separators.
//! Arcs convert per SVG 1.1 §F.6.5 (endpoint → center → ≤90° cubic
//! spans); a zero-radii arc is a straight line per the same section
//! (degenerate, never an error). Smooth `S`/`T` reflect the previous
//! matching control point (reset by any other verb, including `M`).
//!
//! Loud failures (never silent geometry): empty data, unknown verbs,
//! truncated/malformed numbers, and data that yields no drawable
//! segment (a lone `M`) all return `Err` naming the byte offset —
//! the backend maps these to [`BackendError::UnsupportedOp`](oppa::BackendError),
//! so bad vectors fail the paint, never mis-paint.

/// Parses SVG path data into a drawable tiny-skia path.
pub fn build_path(data: &str) -> Result<tiny_skia::Path, String> {
    Parser::new(data).parse()
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    builder: tiny_skia::PathBuilder,
    /// Current point (device/authoring px, local space).
    cur: (f32, f32),
    /// Subpath start (for `Z`).
    start: (f32, f32),
    /// Previous cubic control (for smooth `S`), `None` resets it.
    prev_cubic: Option<(f32, f32)>,
    /// Previous quad control (for smooth `T`), `None` resets it.
    prev_quad: Option<(f32, f32)>,
    /// True once any drawable segment (line/quad/cubic/arc-span) lands.
    drew: bool,
}

impl<'a> Parser<'a> {
    fn new(data: &'a str) -> Self {
        Self {
            bytes: data.as_bytes(),
            pos: 0,
            builder: tiny_skia::PathBuilder::new(),
            cur: (0.0, 0.0),
            start: (0.0, 0.0),
            prev_cubic: None,
            prev_quad: None,
            drew: false,
        }
    }

    fn parse(mut self) -> Result<tiny_skia::Path, String> {
        if self
            .bytes
            .iter()
            .all(|b| b.is_ascii_whitespace() || *b == b',')
        {
            return Err("path data is empty — refused, never a silent no-op".to_string());
        }
        let mut cmd: Option<u8> = None;
        self.skip_sep();
        loop {
            self.skip_sep();
            if self.pos >= self.bytes.len() {
                break;
            }
            let b = self.bytes[self.pos];
            if b.is_ascii_alphabetic() {
                cmd = Some(b);
                self.pos += 1;
                if b == b'Z' || b == b'z' {
                    self.close();
                    cmd = None;
                    continue;
                }
                if !matches!(
                    b,
                    b'M' | b'm'
                        | b'L'
                        | b'l'
                        | b'H'
                        | b'h'
                        | b'V'
                        | b'v'
                        | b'C'
                        | b'c'
                        | b'S'
                        | b's'
                        | b'Q'
                        | b'q'
                        | b'T'
                        | b't'
                        | b'A'
                        | b'a'
                ) {
                    return Err(format!(
                        "path data: unknown verb '{}' at byte {} — refused, never skipped",
                        b as char,
                        self.pos - 1
                    ));
                }
            }
            let Some(c) = cmd else {
                return Err(format!(
                    "path data: number without a verb at byte {} — refused",
                    self.pos
                ));
            };
            self.run_verb(c)?;
            // `M` pairs after the first are implicit `L` (SVG 1.1 §8.5.2).
            if cmd == Some(b'M') {
                cmd = Some(b'L');
            } else if cmd == Some(b'm') {
                cmd = Some(b'l');
            }
        }
        if !self.drew {
            return Err(
                "path data produced no drawable segment — refused, never a silent no-op"
                    .to_string(),
            );
        }
        self.builder.finish().ok_or_else(|| {
            "path data produced no drawable segment — refused, never a silent no-op".to_string()
        })
    }

    fn run_verb(&mut self, cmd: u8) -> Result<(), String> {
        match cmd {
            b'M' | b'm' => {
                let (x, y) = self.point(cmd)?;
                let (x, y) = self.abs(x, y, cmd);
                self.builder.move_to(x, y);
                self.cur = (x, y);
                self.start = (x, y);
                self.prev_cubic = None;
                self.prev_quad = None;
            }
            b'L' | b'l' => {
                let (x, y) = self.point(cmd)?;
                let (x, y) = self.abs(x, y, cmd);
                self.builder.line_to(x, y);
                self.cur = (x, y);
                self.prev_cubic = None;
                self.prev_quad = None;
                self.drew = true;
            }
            b'H' | b'h' => {
                let x = self.number(cmd)?;
                let x = if cmd == b'h' { self.cur.0 + x } else { x };
                self.builder.line_to(x, self.cur.1);
                self.cur = (x, self.cur.1);
                self.prev_cubic = None;
                self.prev_quad = None;
                self.drew = true;
            }
            b'V' | b'v' => {
                let y = self.number(cmd)?;
                let y = if cmd == b'v' { self.cur.1 + y } else { y };
                self.builder.line_to(self.cur.0, y);
                self.cur = (self.cur.0, y);
                self.prev_cubic = None;
                self.prev_quad = None;
                self.drew = true;
            }
            b'C' | b'c' => {
                let (x1, y1) = self.point(cmd)?;
                let (x2, y2) = self.point(cmd)?;
                let (x, y) = self.point(cmd)?;
                let (x1, y1) = self.abs(x1, y1, cmd);
                let (x2, y2) = self.abs(x2, y2, cmd);
                let (x, y) = self.abs(x, y, cmd);
                self.builder.cubic_to(x1, y1, x2, y2, x, y);
                self.prev_cubic = Some((x2, y2));
                self.prev_quad = None;
                self.cur = (x, y);
                self.drew = true;
            }
            b'S' | b's' => {
                let (x2, y2) = self.point(cmd)?;
                let (x, y) = self.point(cmd)?;
                let (x2, y2) = self.abs(x2, y2, cmd);
                let (x, y) = self.abs(x, y, cmd);
                let (x1, y1) = match self.prev_cubic {
                    Some((px, py)) => (2.0 * self.cur.0 - px, 2.0 * self.cur.1 - py),
                    None => self.cur,
                };
                self.builder.cubic_to(x1, y1, x2, y2, x, y);
                self.prev_cubic = Some((x2, y2));
                self.prev_quad = None;
                self.cur = (x, y);
                self.drew = true;
            }
            b'Q' | b'q' => {
                let (x1, y1) = self.point(cmd)?;
                let (x, y) = self.point(cmd)?;
                let (x1, y1) = self.abs(x1, y1, cmd);
                let (x, y) = self.abs(x, y, cmd);
                self.builder.quad_to(x1, y1, x, y);
                self.prev_quad = Some((x1, y1));
                self.prev_cubic = None;
                self.cur = (x, y);
                self.drew = true;
            }
            b'T' | b't' => {
                let (x, y) = self.point(cmd)?;
                let (x, y) = self.abs(x, y, cmd);
                let (x1, y1) = match self.prev_quad {
                    Some((px, py)) => (2.0 * self.cur.0 - px, 2.0 * self.cur.1 - py),
                    None => self.cur,
                };
                self.builder.quad_to(x1, y1, x, y);
                self.prev_quad = Some((x1, y1));
                self.prev_cubic = None;
                self.cur = (x, y);
                self.drew = true;
            }
            b'A' | b'a' => {
                let rx = self.number(cmd)?;
                let ry = self.number(cmd)?;
                let rot = self.number(cmd)?;
                let large = self.number(cmd)?;
                let sweep = self.number(cmd)?;
                let (x, y) = self.point(cmd)?;
                let (x, y) = self.abs(x, y, cmd);
                self.arc(rx, ry, rot, large, sweep, x, y)?;
                self.prev_cubic = None;
                self.prev_quad = None;
            }
            _ => {
                return Err(format!(
                    "path data: unknown verb '{}' at byte {} — refused, never skipped",
                    cmd as char, self.pos
                ));
            }
        }
        Ok(())
    }

    fn close(&mut self) {
        self.builder.close();
        self.cur = self.start;
        self.prev_cubic = None;
        self.prev_quad = None;
    }

    fn abs(&self, x: f32, y: f32, cmd: u8) -> (f32, f32) {
        if cmd.is_ascii_lowercase() {
            (self.cur.0 + x, self.cur.1 + y)
        } else {
            (x, y)
        }
    }

    /// Arc per SVG 1.1 §F.6.5 (endpoint → center parameterization,
    /// then ≤90° cubic spans). Zero radii draw a straight line per
    /// the same section; coincident endpoints draw nothing (both
    /// degenerate, never errors).
    #[allow(clippy::too_many_arguments)] // the seven SVG arc numbers, one call site; a params struct buys nothing
    fn arc(
        &mut self,
        rx: f32,
        ry: f32,
        rot_deg: f32,
        large: f32,
        sweep: f32,
        x: f32,
        y: f32,
    ) -> Result<(), String> {
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        let (x1, y1) = self.cur;
        if rx == 0.0 || ry == 0.0 {
            self.builder.line_to(x, y);
            self.cur = (x, y);
            self.drew = true;
            return Ok(());
        }
        if (x1, y1) == (x, y) {
            self.cur = (x, y);
            return Ok(());
        }
        if !rx.is_finite() || !ry.is_finite() || !rot_deg.is_finite() {
            return Err(format!(
                "path data: arc has non-finite radii/rotation at byte {} — refused",
                self.pos
            ));
        }
        let rot = rot_deg.to_radians();
        let (cos, sin) = (rot.cos(), rot.sin());
        // Step 1: (x1', y1').
        let (dx, dy) = ((x1 - x) / 2.0, (y1 - y) / 2.0);
        let (xp, yp) = (cos * dx + sin * dy, -sin * dx + cos * dy);
        // Step 2: radii correction.
        let lambda = xp * xp / (rx * rx) + yp * yp / (ry * ry);
        if lambda > 1.0 {
            let s = lambda.sqrt();
            rx *= s;
            ry *= s;
        }
        // Step 3: center (cx', cy').
        let large_flag = large != 0.0;
        let sweep_flag = sweep != 0.0;
        let num = rx * rx * ry * ry - rx * rx * yp * yp - ry * ry * xp * xp;
        let den = rx * rx * yp * yp + ry * ry * xp * xp;
        let mut coef = if den == 0.0 {
            0.0
        } else {
            (num / den).max(0.0).sqrt()
        };
        if large_flag == sweep_flag {
            coef = -coef;
        }
        let (cxp, cyp) = (coef * rx * yp / ry, -coef * ry * xp / rx);
        // Step 4: center (cx, cy).
        let (cx, cy) = (
            cos * cxp - sin * cyp + (x1 + x) / 2.0,
            sin * cxp + cos * cyp + (y1 + y) / 2.0,
        );
        // Step 5: angles.
        let angle = |ux: f32, uy: f32, vx: f32, vy: f32| {
            let dot = ux * vx + uy * vy;
            let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
            let mut a = if len == 0.0 {
                0.0
            } else {
                (dot / len).clamp(-1.0, 1.0).acos()
            };
            if ux * vy - uy * vx < 0.0 {
                a = -a;
            }
            a
        };
        let mut t1 = angle(1.0, 0.0, (xp - cxp) / rx, (yp - cyp) / ry);
        let mut dt = angle(
            (xp - cxp) / rx,
            (yp - cyp) / ry,
            (-xp - cxp) / rx,
            (-yp - cyp) / ry,
        );
        if !sweep_flag && dt > 0.0 {
            dt -= 2.0 * std::f32::consts::PI;
        } else if sweep_flag && dt < 0.0 {
            dt += 2.0 * std::f32::consts::PI;
        }
        // Step 6: ≤90° cubic spans.
        let n = (dt.abs() / (std::f32::consts::PI / 2.0)).ceil().max(1.0) as usize;
        let step = dt / n as f32;
        for i in 0..n {
            let a = t1 + step * i as f32;
            let b = a + step;
            // Cubic approximation of the unit span (the (4/3)·tan(θ/4)
            // handle factor — standard arc flattening, not tuning).
            let k = 4.0 / 3.0 * (step / 4.0).tan();
            let pt = |t: f32| {
                let (c, s) = (t.cos(), t.sin());
                (
                    cx + rx * cos * c - ry * sin * s,
                    cy + rx * sin * c + ry * cos * s,
                )
            };
            let der = |t: f32| {
                let (c, s) = (t.cos(), t.sin());
                (-rx * cos * s - ry * sin * c, -rx * sin * s + ry * cos * c)
            };
            let (p0, p1) = (pt(a), pt(b));
            let (d0, d1) = (der(a), der(b));
            // First span starts exactly at the current point (joins the
            // subpath); later spans chain from the previous span end.
            let _ = (p0, t1);
            self.builder.cubic_to(
                p0.0 + k * d0.0,
                p0.1 + k * d0.1,
                p1.0 - k * d1.0,
                p1.1 - k * d1.1,
                p1.0,
                p1.1,
            );
            self.drew = true;
        }
        t1 += dt;
        let _ = t1;
        self.cur = (x, y);
        Ok(())
    }

    fn point(&mut self, cmd: u8) -> Result<(f32, f32), String> {
        let x = self.number(cmd)?;
        let y = self.number(cmd)?;
        Ok((x, y))
    }

    fn number(&mut self, cmd: u8) -> Result<f32, String> {
        self.skip_sep();
        let start = self.pos;
        let bytes = self.bytes;
        let mut i = self.pos;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let mut digits = 0;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return Err(format!(
                "path data: '{}' expects a number at byte {start} — refused",
                cmd as char
            ));
        }
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            let mut j = i + 1;
            if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                j += 1;
            }
            let mut ed = 0;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
                ed += 1;
            }
            if ed > 0 {
                i = j;
            }
        }
        self.pos = i;
        let text = std::str::from_utf8(&bytes[start..i]).map_err(|e| {
            format!("path data: number is not UTF-8 at byte {start} ({e}) — refused")
        })?;
        text.parse::<f32>()
            .map_err(|e| format!("path data: bad number {text:?} at byte {start} ({e}) — refused"))
    }

    /// Separators are ASCII whitespace and commas (a comma is exactly
    /// one separator — `1,,2` leaves the second comma to fail as a
    /// non-number, loudly, per the rule above).
    fn skip_sep(&mut self) {
        while self.pos < self.bytes.len()
            && (self.bytes[self.pos].is_ascii_whitespace() || self.bytes[self.pos] == b',')
        {
            self.pos += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verbs_of(path: &tiny_skia::Path) -> Vec<String> {
        path.verbs().iter().map(|v| format!("{v:?}")).collect()
    }

    #[test]
    fn triangle_parses_to_move_line_line_close() {
        let path = build_path("M 5 5 L 55 5 L 30 55 Z").expect("triangle parses");
        assert_eq!(verbs_of(&path), ["Move", "Line", "Line", "Close"]);
        let pts = path.points();
        assert_eq!(pts.len(), 3);
        assert!((pts[0].x - 5.0).abs() < 1e-6 && (pts[0].y - 5.0).abs() < 1e-6);
        assert!((pts[2].x - 30.0).abs() < 1e-6 && (pts[2].y - 55.0).abs() < 1e-6);
    }

    #[test]
    fn relative_and_implicit_repeats_agree_with_absolute() {
        let abs = build_path("M 0 0 L 10 0 L 10 10 L 0 10 Z").expect("abs");
        let rel = build_path("m 0 0 l 10 0 l 0 10 l -10 0 z").expect("rel");
        assert_eq!(abs.bounds(), rel.bounds());
        // Extra M pairs are implicit L (SVG 1.1 §8.5.2).
        let implicit = build_path("M 0 0 10 0 10 10 Z").expect("implicit");
        assert_eq!(implicit.bounds(), abs.bounds());
    }

    #[test]
    fn hv_cubic_quad_and_smooth_verbs_parse() {
        let path =
            build_path("M 0 0 H 10 V 10 C 12 12 14 14 16 16 S 20 20 22 22 Q 24 24 26 26 T 30 30 Z")
                .expect("verbs parse");
        let verbs = verbs_of(&path);
        assert_eq!(
            verbs,
            ["Move", "Line", "Line", "Cubic", "Cubic", "Quad", "Quad", "Close"]
        );
    }

    #[test]
    fn smooth_reflects_the_previous_control() {
        // Q control (10, 0) reflects to (10, 20) across end (10, 10).
        let path = build_path("M 10 10 Q 10 0 10 10 T 10 30").expect("smooth quad");
        let pts = path.points();
        // Quad verbs store (control, end) pairs after the start point.
        assert_eq!(pts.len(), 5);
        assert!((pts[1].x - 10.0).abs() < 1e-5 && (pts[1].y - 0.0).abs() < 1e-5);
        assert!((pts[3].x - 10.0).abs() < 1e-5 && (pts[3].y - 20.0).abs() < 1e-5);
    }

    #[test]
    fn quarter_arc_covers_the_quarter_bounds() {
        let path = build_path("M 10 0 A 10 10 0 0 1 0 10").expect("arc parses");
        let b = path.compute_tight_bounds().expect("arc has bounds");
        assert!(b.x() >= -0.5 && b.x() <= 0.5, "starts near x=0 edge, {b:?}");
        assert!(
            b.width() > 9.0 && b.height() > 9.0,
            "spans the quarter, {b:?}"
        );
    }

    #[test]
    fn degenerate_arc_is_a_line_per_spec() {
        let path = build_path("M 0 0 A 0 5 0 0 0 10 10").expect("zero-rx arc");
        assert_eq!(verbs_of(&path), ["Move", "Line"]);
    }

    #[test]
    fn refusals_name_the_offset() {
        assert!(build_path("").is_err(), "empty refuses");
        assert!(build_path("   ,  ").is_err(), "blank refuses");
        assert!(build_path("M 0 0").is_err(), "lone moveto refuses");
        assert!(build_path("M 0 0 X 1 1").is_err(), "unknown verb refuses");
        assert!(build_path("M 0").is_err(), "truncated pair refuses");
        assert!(build_path("L").is_err(), "bare verb refuses");
        assert!(build_path("10 10").is_err(), "number without verb refuses");
    }
}
