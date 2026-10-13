//! Transform bake (decisions 511 + 517): node paint transforms folded into
//! op geometry at plan-build time, with rotation riding per-op.
//!
//! The shared builder emits each node's own ops contiguously, so
//! folding the node's accumulated scale+translate affine into those
//! ops is splice-safe (per-node runs carry their baked numbers — no
//! stack pairing to break across incremental rebuilds) and agrees by
//! construction on CPU and GPU (both consume the same plan — zero
//! raster changes on either backend for ST). Identity is a strict no-op, so
//! untransformed plans stay byte-identical.
//!
//! Rotation (decision 517) does NOT bake into geometry — the
//! [`PaintXf`] residual (`full ∘ st⁻¹`, identity for ST-only chains)
//! stamps onto each op in the same per-node range (same splice
//! safety) and backends apply it at paint time (CPU temp-layer
//! detour for the manual helpers, GPU instance rotation, DOM
//! `rotate()`).
//!
//! Canonical semantics (SPEC §Transitions): scale about the
//! committed-box center, then translate — baked here as a concrete
//! [`AffineST`](oppa::AffineST) the walker accumulates down the tree.
//! Radii, circle radii, shadow blurs, and stroke widths scale by
//! `max(sx, sy)` (non-uniform corners/ellipses are unrepresentable in
//! the op set — the max keeps coverage; stated, deterministic).
//! Glyph advances scale by `sx` while line metrics scale by `sy`
//! (anamorphic text renders correctly from the same numbers on both
//! backends; the uniform case scales everything by one factor).
//! Path data rescales about the local origin through
//! [`scale_path_data`] (translation cancels out of the data — only
//! the op origin moves).

use oppa::{AffineST, DrawOp, PaintXf};

/// Fold `acc` into a node's freshly emitted ops (the
/// `plan.ops[start..]` range of one `emit_node_ops` call) and stamp
/// the rotation residual (decision 517). Identity ST returns without
/// touching geometry; identity residual stamps nothing (unrotated
/// plans stay byte-identical — the stamp is a no-op there).
pub fn bake_ops(ops: &mut [DrawOp], acc: AffineST, residual: PaintXf) {
    if !residual.is_identity() {
        for op in ops.iter_mut() {
            op.set_xf(residual);
        }
    }
    if acc.is_identity() {
        return;
    }
    for op in ops {
        bake_op(op, acc);
    }
}

fn bake_op(op: &mut DrawOp, acc: AffineST) {
    let (sx, sy) = (acc.sx, acc.sy);
    // Scalar px under non-uniform scale (radii, circles, blurs,
    // strokes): the max keeps coverage — stated, deterministic.
    let maxk = sx.max(sy);
    let mx = |v: f32| sx * v + acc.tx;
    let my = |v: f32| sy * v + acc.ty;
    match op {
        DrawOp::Rect { x, y, w, h, .. } => {
            *x = mx(*x);
            *y = my(*y);
            *w *= sx;
            *h *= sy;
        }
        DrawOp::RRect {
            x,
            y,
            w,
            h,
            radius,
            radii,
            ..
        } => {
            *x = mx(*x);
            *y = my(*y);
            *w *= sx;
            *h *= sy;
            *radius *= maxk;
            if let Some(rs) = radii {
                for r in rs.iter_mut() {
                    *r *= maxk;
                }
            }
        }
        DrawOp::Circle { cx, cy, r, .. } => {
            *cx = mx(*cx);
            *cy = my(*cy);
            *r *= maxk;
        }
        DrawOp::Shadow {
            x,
            y,
            w,
            h,
            dx,
            dy,
            blur_radius,
            ..
        } => {
            *x = mx(*x);
            *y = my(*y);
            *w *= sx;
            *h *= sy;
            *dx *= sx;
            *dy *= sy;
            *blur_radius *= maxk;
        }
        DrawOp::Text {
            x,
            y,
            line_height,
            baseline,
            em_size,
            glyphs,
            ..
        } => {
            *x = mx(*x);
            *y = my(*y);
            *line_height *= sy;
            *baseline *= sy;
            *em_size *= sy;
            for g in glyphs.iter_mut() {
                g.x *= sx;
                g.advance *= sx;
            }
        }
        DrawOp::RImg { x, y, w, h, .. } => {
            *x = mx(*x);
            *y = my(*y);
            *w *= sx;
            *h *= sy;
        }
        DrawOp::Path {
            x,
            y,
            width,
            height,
            data,
            stroke,
            ..
        } => {
            *x = mx(*x);
            *y = my(*y);
            *width *= sx;
            *height *= sy;
            if sx != 1.0 || sy != 1.0 {
                *data = scale_path_data(data, sx, sy).into();
            }
            if let Some(s) = stroke {
                s.width *= maxk;
            }
        }
        DrawOp::PushClip { x, y, w, h } => {
            *x = mx(*x);
            *y = my(*y);
            *w *= sx;
            *h *= sy;
        }
        DrawOp::PushLayer { .. } | DrawOp::Pop => {}
    }
}

/// Command-aware SVG coordinate scaling (decision 511): every
/// coordinate scales by its axis (`sx` for x, `sy` for y — `H` takes
/// `sx`, `V` takes `sy`); arc flags and the x-axis rotation pass
/// through untouched (scaling flags would corrupt them — the
/// `scale_svg_path` uniform wart, not repeated here); arc radii scale
/// with their axes (non-uniform arc distortion is the stated
/// approximation — deterministic, backend-agreeing). Unknown command
/// letters refuse loudly (malformed data, never a dropped segment).
/// Numbers format like [`scale_svg_path`](oppa::scale_svg_path)
/// (`{:.4}`, trimmed) so backends parse the same string.
pub fn scale_path_data(d: &str, sx: f32, sy: f32) -> String {
    #[derive(Clone, Copy)]
    enum Tok {
        Cmd(char),
        Num(f32),
    }
    // Tokenize: command letters vs numbers (exponents ride the
    // number — a bare `e` is a malformed command, refused below).
    let bytes = d.as_bytes();
    let mut toks: Vec<Tok> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() || b == b',' {
            i += 1;
        } else if b.is_ascii_alphabetic() {
            if (b == b'e' || b == b'E') && !toks.is_empty() {
                // An exponent tail the number parser declined (no
                // digit follows) — malformed, never skipped.
                panic!("plan: path data has a bare exponent at byte {i} in {d:?} — malformed data, never a dropped segment");
            }
            // A leading `e` cannot be an exponent — refused as an
            // unknown command below.
            toks.push(Tok::Cmd(b as char));
            i += 1;
        } else if b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'+' {
            let start = i;
            if b == b'-' || b == b'+' {
                i += 1;
            }
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                let mut j = i + 1;
                if j < bytes.len() && (bytes[j] == b'-' || bytes[j] == b'+') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j].is_ascii_digit() {
                    i = j;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let s = std::str::from_utf8(&bytes[start..i]).unwrap_or("0");
            toks.push(Tok::Num(s.parse().unwrap_or(0.0)));
        } else {
            i += 1;
        }
    }
    // Walk commands; each number takes its axis scale by position.
    // Group lengths: M/L/T pairs, H/V singles, C sextuples, S/Q
    // quadruples, A septuples (rx, ry, rot, large, sweep, x, y).
    let mut out = String::with_capacity(d.len());
    let mut cmd: Option<char> = None;
    let mut pos = 0usize;
    let mut first = true;
    let push = |out: &mut String, first: &mut bool, s: &str| {
        if !*first {
            out.push(' ');
        }
        *first = false;
        out.push_str(s);
    };
    let fmt = |v: f32| {
        let f = format!("{:.4}", v);
        f.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    for t in toks {
        match t {
            Tok::Cmd(c) => {
                if !"MmLlHhVvCcSsQqTtAaZz".contains(c) {
                    panic!("plan: unknown path command {c:?} in {d:?} — SVG paths take M L H V C S Q T A Z only, never a silent skip");
                }
                cmd = Some(c);
                pos = 0;
                push(&mut out, &mut first, &c.to_string());
            }
            Tok::Num(v) => {
                let c = cmd.unwrap_or_else(|| {
                    panic!("plan: leading number {v} before any command in {d:?} — malformed data, never a dropped segment")
                });
                let k = match c {
                    'M' | 'm' | 'L' | 'l' | 'T' | 't' => {
                        // Pairs; post-M pairs are implicit linetos
                        // (same pair rule — no mode switch needed).
                        let k = if pos.is_multiple_of(2) { sx } else { sy };
                        pos += 1;
                        k
                    }
                    'H' | 'h' => {
                        pos += 1;
                        sx
                    }
                    'V' | 'v' => {
                        pos += 1;
                        sy
                    }
                    'C' | 'c' => {
                        let k = if pos.is_multiple_of(2) { sx } else { sy };
                        pos = (pos + 1) % 6;
                        k
                    }
                    'S' | 's' | 'Q' | 'q' => {
                        let k = if pos.is_multiple_of(2) { sx } else { sy };
                        pos = (pos + 1) % 4;
                        k
                    }
                    'A' | 'a' => {
                        let k = match pos {
                            0 => sx,
                            1 => sy,
                            5 => sx,
                            6 => sy,
                            _ => 1.0,
                        };
                        pos = (pos + 1) % 7;
                        k
                    }
                    'Z' | 'z' => {
                        panic!("plan: number {v} inside a close command in {d:?} — malformed data, never a dropped segment")
                    }
                    _ => unreachable!(),
                };
                push(&mut out, &mut first, &fmt(v * k));
            }
        }
    }
    out
}
