//! `cargo oppa add-font` — vendor an app font (Round 27.2, decision 500).
//!
//! Takes a TrueType/OpenType font file (static or variable) and
//! produces everything an app needs to render it through the
//! Round-27.1 app-fonts channel: instanced static weights,
//! a license copy, and a generated `src/fonts.rs` module holding the
//! `include_bytes!` faces (`FAMILY` + `FACES` constants feed
//! `WindowOptions::with_app_fonts` on desktop and the 6-argument
//! `export_app!` on web).
//!
//! No new product dependencies: sfnt parsing (magic, table
//! directory, Unicode family name, OS/2 weight, `fvar` detection) is
//! deliberately hand-rolled and bounds-checked here, so the CLI never
//! depends on its own font stack to vendor one. Variable-weight
//! instancing shells out to the industry tool (`fonttools`, via a
//! local Python) — refused loudly with install instructions when
//! absent, never guessed.
//!
//! Loud-refusal contract (the scaffolder rule): missing/unreadable
//! input, non-sfnt bytes, woff/woff2 (needs decompression the CLI
//! will not guess at), TTC collections, bad `--weights`, a `--family`
//! that disagrees with the name table, existing outputs without
//! `--force`, and a missing app layout all refuse with actionable
//! text. Licensing is reported, never decided: a sibling OFL/LICENSE
//! file is copied when found, otherwise the report carries a
//! prominent confirm-before-shipping warning.

use std::path::{Path, PathBuf};

/// Parsed sfnt facts the command acts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFacts {
    /// Typographic family (name ID 16) else family (name ID 1),
    /// Unicode records only.
    pub family: String,
    /// `true` when the `fvar` table carries a `wght` axis (weights
    /// come from instancing, never from the file as-is — a pinned
    /// `fvar` with only decorative axes like `HEXP` still counts as
    /// static).
    pub variable: bool,
    /// OS/2.usWeightClass (default-instance weight for variables).
    pub weight: u16,
}

/// `cargo oppa add-font` arguments (parsed by
/// [`parse_add_font_args`], executed by [`add_font`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddFontArgs {
    pub font_file: PathBuf,
    pub dir: PathBuf,
    pub weights: Vec<u16>,
    pub family: Option<String>,
    pub force: bool,
}

/// What `add_font` wrote (main prints the wiring report from this).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddFontReport {
    pub family: String,
    pub weights: Vec<u16>,
    pub files: Vec<PathBuf>,
    pub module: PathBuf,
    pub license_copied: bool,
}

/// Default instancing set (the weights the sera app uses — Regular,
/// SemiBold, Bold).
pub const DEFAULT_WEIGHTS: &[u16] = &[400, 600, 700];

/// Parses `add-font <font-file> [dir] [--weights a,b,c] [--family
/// Name] [--force]`. The optional directory is the second positional;
/// anything else positional refuses loudly.
pub fn parse_add_font_args(argv: &[String]) -> Result<AddFontArgs, String> {
    let mut positionals: Vec<String> = Vec::new();
    let mut weights: Option<Vec<u16>> = None;
    let mut family: Option<String> = None;
    let mut force = false;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--weights" => {
                i += 1;
                let raw = argv.get(i).ok_or_else(|| {
                    "--weights needs a value (e.g. --weights 400,600,700)".to_string()
                })?;
                weights = Some(parse_weights(raw)?);
            }
            "--family" => {
                i += 1;
                let raw = argv
                    .get(i)
                    .ok_or_else(|| "--family needs a value (the font family)".to_string())?;
                if raw.trim().is_empty() {
                    return Err("--family needs a non-empty value".to_string());
                }
                if family.is_some() {
                    return Err("--family passed twice — pass exactly one".to_string());
                }
                family = Some(raw.to_string());
            }
            "--force" => force = true,
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag {flag:?}"));
            }
            positional => {
                if positionals.len() >= 2 {
                    return Err(format!("unexpected argument {positional:?}"));
                }
                positionals.push(positional.to_string());
            }
        }
        i += 1;
    }
    let font_file = positionals
        .first()
        .ok_or_else(|| "no font file — try `cargo oppa add-font <font-file> [dir]`".to_string())?;
    Ok(AddFontArgs {
        font_file: PathBuf::from(font_file),
        dir: positionals
            .get(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
        weights: weights.unwrap_or_else(|| DEFAULT_WEIGHTS.to_vec()),
        family,
        force,
    })
}

/// Parses `400,600,700` into weights (each 100–900 in steps of 100 —
/// the sane static set; instancing pins exact hundreds, and the
/// generated file/const names only exist for those).
pub fn parse_weights(raw: &str) -> Result<Vec<u16>, String> {
    let mut out = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        let w: u16 = part.parse().map_err(|_| {
            format!("--weights {part:?} is not a number (e.g. --weights 400,600,700)")
        })?;
        if !(100..=900).contains(&w) || !w.is_multiple_of(100) {
            return Err(format!(
                "--weights {w} is not a standard weight (100–900 in steps of 100)"
            ));
        }
        if out.contains(&w) {
            return Err(format!("--weights lists {w} twice — list each weight once"));
        }
        out.push(w);
    }
    if out.is_empty() {
        return Err("--weights needs at least one weight".to_string());
    }
    out.sort_unstable();
    Ok(out)
}

/// Standard weight number → file/const name.
pub fn weight_name(weight: u16) -> &'static str {
    match weight {
        100 => "Thin",
        200 => "ExtraLight",
        300 => "Light",
        400 => "Regular",
        500 => "Medium",
        600 => "SemiBold",
        700 => "Bold",
        800 => "ExtraBold",
        900 => "Black",
        _ => "Unknown",
    }
}

fn be_u16(bytes: &[u8], at: usize) -> Result<u16, String> {
    bytes
        .get(at..at + 2)
        .ok_or_else(|| "font file truncated in table header".to_string())
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
}

fn table_range(bytes: &[u8], tag: &[u8; 4]) -> Result<Option<(usize, usize)>, String> {
    if bytes.len() < 12 {
        return Err("font file too small for an sfnt header".to_string());
    }
    let count = be_u16(bytes, 4)? as usize;
    for k in 0..count {
        let base = 12 + k * 16;
        let entry = bytes
            .get(base..base + 16)
            .ok_or_else(|| "font file truncated in the table directory".to_string())?;
        if &entry[0..4] == tag {
            let offset = u32::from_be_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
            let length = u32::from_be_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
            if offset
                .checked_add(length)
                .is_none_or(|end| end > bytes.len())
            {
                return Err(format!(
                    "font table {} runs past the file end — refused, never a half-read font",
                    String::from_utf8_lossy(tag)
                ));
            }
            return Ok(Some((offset, length)));
        }
    }
    Ok(None)
}

/// Reads sfnt facts from raw font bytes: magic-validated, Unicode
/// family name, OS/2 weight, `fvar` presence. woff/woff2/TTC refuse
/// loudly with what to do instead.
pub fn read_font_facts(bytes: &[u8], what: &str) -> Result<FontFacts, String> {
    if bytes.len() < 12 {
        return Err(format!("{what} is too small for a font file"));
    }
    match &bytes[0..4] {
        [0x00, 0x01, 0x00, 0x00] | [0x74, 0x72, 0x75, 0x65] | b"OTTO" => {}
        b"wOFF" | b"wOF2" => {
            return Err(format!(
                "{what} is a compressed woff/woff2 container — decompress it first (e.g. `fonttools ttLib.woff2 decompress`, needs the brotli package), then pass the TTF/OTF"
            ));
        }
        b"ttcf" => {
            return Err(format!(
                "{what} is a TrueType collection — split it first (e.g. `fonttools ttc2ttf`), then pass one face"
            ));
        }
        _ => {
            return Err(format!(
                "{what} is not a TrueType/OpenType font (bad sfnt magic)"
            ));
        }
    }
    let variable = has_wght_axis(bytes, what)?;
    let (name_off, name_len) = table_range(bytes, b"name")?
        .ok_or_else(|| format!("{what} has no name table — refused, never an unnamed font"))?;
    let name = &bytes[name_off..name_off + name_len];
    let family = unicode_family_name(name, what)?;
    let (os2_off, os2_len) = table_range(bytes, b"OS/2")?
        .ok_or_else(|| format!("{what} has no OS/2 table — refused, never an unweighed font"))?;
    if os2_len < 6 {
        return Err(format!("{what} OS/2 table too small for usWeightClass"));
    }
    let weight = u16::from_be_bytes([bytes[os2_off + 4], bytes[os2_off + 5]]);
    if !(1..=1000).contains(&weight) {
        return Err(format!(
            "{what} reports usWeightClass {weight} (expected 1–1000)"
        ));
    }
    // CFF-flavored OpenType shapes but never rasterizes on the
    // CPU/GPU backends (TrueType outlines only) — proceed, but say
    // so loudly; the report repeats it.
    if &bytes[0..4] == b"OTTO" {
        eprintln!(
            "cargo oppa: WARNING: {what} is CFF-flavored OpenType — shaping works, but the CPU/GPU backends rasterize TrueType outlines only (use a TTF build of the family for pixels)"
        );
    }
    Ok(FontFacts {
        family,
        variable,
        weight,
    })
}

/// `true` when the `fvar` table lists a `wght` axis (fully pinned
/// files keep decorative axes — those still count as static).
fn has_wght_axis(bytes: &[u8], what: &str) -> Result<bool, String> {
    let Some((off, len)) = table_range(bytes, b"fvar")? else {
        return Ok(false);
    };
    if len < 16 {
        return Err(format!("{what} fvar table truncated"));
    }
    // fvar: Fixed version (4), offsetToData u16 (+4),
    // countSizePairs u16 (+6), axisCount u16 (+8), axisSize u16
    // (+10); instance fields follow (unread — axis presence is the
    // only question here).
    let count = be_u16(bytes, off + 8)? as usize;
    let axis_off = be_u16(bytes, off + 4)? as usize;
    let axis_size = be_u16(bytes, off + 10)? as usize;
    if axis_size < 20 {
        return Err(format!("{what} fvar axis size {axis_size} too small"));
    }
    for k in 0..count {
        let base = off + axis_off + k * axis_size;
        let tag = bytes
            .get(base..base + 4)
            .ok_or_else(|| format!("{what} fvar axis {k} truncated"))?;
        if tag == b"wght" {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Unicode family name: typographic family (ID 16) wins over family
/// (ID 1); Windows Unicode records win over legacy Mac ones (which
/// need a MacRoman decoder the CLI will not carry — refused loudly
/// instead).
fn unicode_family_name(name: &[u8], what: &str) -> Result<String, String> {
    if name.len() < 6 {
        return Err(format!("{what} name table truncated"));
    }
    let count = be_u16(name, 2)? as usize;
    let storage = be_u16(name, 4)? as usize;
    // (rank, units): ID 16 outranks ID 1; Windows records (3,*)
    // outrank bare-Unicode (0,*) leftovers at equal IDs.
    let mut best: Option<(u8, Vec<u16>)> = None;
    for k in 0..count {
        let base = 6 + k * 12;
        let rec = name
            .get(base..base + 12)
            .ok_or_else(|| format!("{what} name table truncated at record {k}"))?;
        let platform = u16::from_be_bytes([rec[0], rec[1]]);
        let encoding = u16::from_be_bytes([rec[2], rec[3]]);
        let name_id = u16::from_be_bytes([rec[6], rec[7]]);
        if name_id != 1 && name_id != 16 {
            continue;
        }
        let is_unicode = platform == 0 || (platform == 3 && (encoding == 1 || encoding == 10));
        if !is_unicode {
            continue;
        }
        let len = u16::from_be_bytes([rec[8], rec[9]]) as usize;
        let off = u16::from_be_bytes([rec[10], rec[11]]) as usize;
        let bytes = name
            .get(storage + off..storage + off + len)
            .ok_or_else(|| format!("{what} name record {k} runs past the string storage"))?;
        if bytes.len() % 2 != 0 {
            return Err(format!("{what} name record {k} has odd UTF-16 length"));
        }
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        let rank = if name_id == 16 { 0 } else { 1 };
        // Windows (3,*) outranks bare Unicode (0,*) at equal IDs.
        let rank = if platform == 3 { rank } else { rank + 2 };
        let replace = match &best {
            None => true,
            Some((best_rank, _)) => rank < *best_rank,
        };
        if replace {
            best = Some((rank, units));
        }
    }
    let (_, units) = best.ok_or_else(|| {
        format!("{what} carries no Unicode family name (IDs 1/16, platforms 0/3) — Mac-Roman-only fonts are refused, re-export with Unicode names")
    })?;
    String::from_utf16(&units)
        .map_err(|_| format!("{what} family name is not valid UTF-16"))
        .and_then(|s| {
            let s = s.trim().to_string();
            if s.is_empty() {
                Err(format!("{what} family name is empty"))
            } else {
                Ok(s)
            }
        })
}

/// `Family Name` → `FAMILY_NAME` (const/file-stem safe).
pub fn const_stem(family: &str) -> String {
    let mut out = String::with_capacity(family.len());
    for ch in family.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// `Family Name` → `FamilyName` (file-stem safe).
pub fn file_stem(family: &str) -> String {
    family
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Finds a Python able to run fonttools (`python` first, then the
/// Windows `py -3` launcher). Refuses loudly with install
/// instructions — instancing never falls back to a guess.
fn fonttools_probe() -> Result<(&'static str, bool), String> {
    for (prog, dash3) in [("python", false), ("py", true)] {
        let mut cmd = std::process::Command::new(prog);
        if dash3 {
            cmd.arg("-3");
        }
        let probe = cmd
            .arg("-c")
            .arg("import fontTools.varLib.instancer")
            .output();
        match probe {
            Ok(out) if out.status.success() => return Ok((prog, dash3)),
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(format!("cannot probe {prog} for fonttools: {e}"));
            }
        }
    }
    Err("fonttools not found — install it for variable-font instancing (`pip install fonttools`, needs Python 3), or pass a static font instead".to_string())
}

/// Instances one static weight out of a variable font into `out`
/// (fonttools, pinned `wght`), then verifies the output parses and
/// reports the requested OS/2 weight.
fn instance_weight(input: &Path, weight: u16, out: &Path, what: &str) -> Result<(), String> {
    let (prog, dash3) = fonttools_probe()?;
    let mut cmd = std::process::Command::new(prog);
    if dash3 {
        cmd.arg("-3");
    }
    let status = cmd
        .arg("-m")
        .arg("fontTools.varLib.instancer")
        .arg(input)
        .arg(format!("wght={weight}"))
        .arg("-o")
        .arg(out)
        .status()
        .map_err(|e| format!("cannot run fonttools instancer: {e}"))?;
    if !status.success() {
        return Err(format!(
            "fonttools failed instancing wght={weight} from {what} (exit {status})"
        ));
    }
    let bytes =
        std::fs::read(out).map_err(|e| format!("cannot read instanced wght={weight}: {e}"))?;
    let facts = read_font_facts(&bytes, &format!("instanced wght={weight}"))?;
    if facts.weight != weight {
        return Err(format!(
            "instanced wght={weight} reports usWeightClass {} — refused, never a misweighed face",
            facts.weight
        ));
    }
    Ok(())
}

/// Sibling license candidates, in preference order (family-specific
/// `OFL-<Stem>.txt` is tried first by the caller).
const LICENSE_NAMES: &[&str] = &[
    "OFL.txt",
    "LICENSE.txt",
    "LICENSE",
    "LICENCE.txt",
    "LICENCE",
    "LICENSE.md",
];

/// Copies a sibling license next to the input into `fonts/`. Order:
/// the exact `OFL-<Stem>.txt` Google-Fonts sibling shape, then any
/// sibling naming both OFL and the family stem (first sorted hit —
/// e.g. `Poppins-OFL.txt`, while a stray `OFL-Other.txt` never
/// launders the wrong license in), then the generic names. Returns
/// the copied file name, or `None` when nothing was found (the caller
/// warns — licensing stays the developer's call, never the tool's).
fn copy_license(input: &Path, fonts_dir: &Path, family: &str) -> Result<Option<String>, String> {
    let parent = input
        .parent()
        .ok_or_else(|| "font path has no parent directory".to_string())?;
    let stem_lower = file_stem(family).to_ascii_lowercase();
    let mut candidates: Vec<String> = vec![format!("OFL-{}.txt", file_stem(family))];
    if let Ok(entries) = std::fs::read_dir(parent) {
        let mut ofl: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| {
                let lower = n.to_ascii_lowercase();
                lower.contains("ofl") && lower.contains(&stem_lower)
            })
            .collect();
        ofl.sort();
        candidates.extend(ofl);
    }
    candidates.extend(LICENSE_NAMES.iter().map(|s| s.to_string()));
    for name in candidates {
        let src = parent.join(&name);
        if !src.is_file() {
            continue;
        }
        let dest = fonts_dir.join(format!("LICENSE-{}.txt", file_stem(family)));
        let bytes = std::fs::read(&src)
            .map_err(|e| format!("cannot read license {}: {e}", src.display()))?;
        std::fs::write(&dest, bytes)
            .map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
        return Ok(Some(
            dest.file_name()
                .expect("license dest has a name")
                .to_string_lossy()
                .to_string(),
        ));
    }
    Ok(None)
}

/// One scanned face on disk (for `src/fonts.rs` regeneration).
struct ScannedFace {
    file_name: String,
    family: String,
    weight: u16,
}

/// Regenerates `src/fonts.rs` by scanning `fonts/` (every
/// `.ttf`/`.otf` parses — a corrupt sibling refuses loudly instead
/// of silently dropping a face). Whole-file rewrite with a do-not-edit
/// header, families in sorted order, weights ascending.
fn regenerate_module(app_dir: &Path) -> Result<PathBuf, String> {
    let fonts_dir = app_dir.join("fonts");
    let src_dir = app_dir.join("src");
    if !src_dir.is_dir() {
        return Err(format!(
            "{} has no src/ — add-font targets an app crate layout",
            app_dir.display()
        ));
    }
    let mut faces: Vec<ScannedFace> = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&fonts_dir)
        .map_err(|e| format!("cannot list {}: {e}", fonts_dir.display()))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("cannot list {}: {e}", fonts_dir.display()))?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with(".ttf") || lower.ends_with(".otf")) {
            continue;
        }
        let bytes = std::fs::read(entry.path())
            .map_err(|e| format!("cannot read {}: {e}", entry.path().display()))?;
        let facts = read_font_facts(&bytes, &format!("fonts/{name}"))?;
        faces.push(ScannedFace {
            file_name: name,
            family: facts.family,
            weight: facts.weight,
        });
    }
    if faces.is_empty() {
        return Err(format!(
            "{} has no font faces — nothing to generate",
            fonts_dir.display()
        ));
    }
    faces.sort_by(|a, b| {
        a.family
            .cmp(&b.family)
            .then(a.weight.cmp(&b.weight))
            .then(a.file_name.cmp(&b.file_name))
    });
    let mut families: Vec<String> = Vec::new();
    for face in &faces {
        if !families.contains(&face.family) {
            families.push(face.family.clone());
        }
    }
    let mut out = String::from(
        "//! Bundled app fonts (generated by `cargo oppa add-font` — do not edit by hand;\n//! re-run the command to regenerate).\n\n",
    );
    for family in &families {
        let stem = const_stem(family);
        if stem.is_empty() {
            return Err(format!(
                "family {family:?} has no ASCII-alphanumeric stem for const names"
            ));
        }
        out.push_str(&format!(
            "/// {family} family name as the name table reports it.\n"
        ));
        out.push_str(&format!("pub const {stem}_FAMILY: &str = {family:?};\n\n"));
        for face in faces.iter().filter(|f| &f.family == family) {
            let wname = weight_name(face.weight);
            out.push_str(&format!(
                "/// {family} {wname} ({}) font bytes.\n",
                face.weight
            ));
            out.push_str(&format!(
                "pub const {stem}_{}: &[u8] = include_bytes!(\"../fonts/{}\");\n\n",
                wname.to_ascii_uppercase(),
                face.file_name
            ));
        }
        out.push_str(&format!(
            "/// {family} faces in weight order — the `from_bytes_with_chain` input\n/// (desktop `WindowOptions::with_app_fonts`, web `export_app!` font arm).\n"
        ));
        out.push_str(&format!("pub const {stem}_FACES: &[(&str, &[u8])] = &[\n"));
        for face in faces.iter().filter(|f| &f.family == family) {
            let wname = weight_name(face.weight);
            out.push_str(&format!(
                "    ({:?}, {stem}_{}),\n",
                face.file_name,
                wname.to_ascii_uppercase()
            ));
        }
        out.push_str("];\n\n");
    }
    let module = src_dir.join("fonts.rs");
    std::fs::write(&module, out).map_err(|e| format!("cannot write {}: {e}", module.display()))?;
    Ok(module)
}

/// Adds a font to the app at `args.dir` (font bytes → `fonts/`,
/// license copy-or-warn, `src/fonts.rs` regeneration). Returns the
/// wiring report the CLI prints.
pub fn add_font(args: &AddFontArgs) -> Result<AddFontReport, String> {
    if !args.dir.join("Cargo.toml").is_file() {
        return Err(format!(
            "{} has no Cargo.toml — add-font targets an app crate directory",
            args.dir.display()
        ));
    }
    let input_bytes = std::fs::read(&args.font_file)
        .map_err(|e| format!("cannot read font file {}: {e}", args.font_file.display()))?;
    let input_name = args
        .font_file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "font file".to_string());
    let facts = read_font_facts(&input_bytes, &input_name)?;
    if let Some(want) = &args.family {
        if !want.eq_ignore_ascii_case(&facts.family) {
            return Err(format!(
                "--family {want:?} does not match the font's name-table family {:?} — pass the table family (shaping resolves that string, never the override)",
                facts.family
            ));
        }
    }
    let family = facts.family.clone();
    let stem = file_stem(&family);
    if stem.is_empty() {
        return Err(format!(
            "family {family:?} has no ASCII-alphanumeric file stem"
        ));
    }
    let fonts_dir = args.dir.join("fonts");
    std::fs::create_dir_all(&fonts_dir)
        .map_err(|e| format!("cannot create {}: {e}", fonts_dir.display()))?;
    // Resolve the per-weight sources: instanced staging files for
    // variable inputs, the input itself for a matching static. The
    // staging dir lives inside fonts/ (always writable where the
    // outputs go — never the system temp dir) and is removed after.
    let staging_root = fonts_dir.join(format!(".staging-{}", std::process::id()));
    let mut sources: Vec<(u16, PathBuf)> = Vec::new();
    if facts.variable {
        std::fs::create_dir_all(&staging_root)
            .map_err(|e| format!("cannot create staging dir: {e}"))?;
        for weight in &args.weights {
            let out = staging_root.join(format!("{stem}-{}.ttf", weight_name(*weight)));
            instance_weight(&args.font_file, *weight, &out, &input_name)?;
            sources.push((*weight, out));
        }
    } else {
        if weight_name(facts.weight) == "Unknown" {
            return Err(format!(
                "{input_name} is a static face with non-standard usWeightClass {} — add-font names the standard hundred set only",
                facts.weight
            ));
        }
        if args.weights.len() != 1 || args.weights[0] != facts.weight {
            return Err(format!(
                "{input_name} is a static {} face — request exactly [--weights {}] (variable inputs instance any set)",
                facts.weight, facts.weight
            ));
        }
        sources.push((facts.weight, args.font_file.clone()));
    }
    // Stage bytes through memory (temp files may live beside the
    // inputs; the fallback holds the input bytes for statics).
    let mut staged: Vec<(u16, Vec<u8>)> = Vec::new();
    for (weight, path) in &sources {
        let bytes =
            std::fs::read(path).map_err(|e| format!("cannot read staged wght={weight}: {e}"))?;
        // Re-parse: instancer output is trusted, never assumed.
        let staged_facts = read_font_facts(&bytes, &format!("staged wght={weight}"))?;
        if staged_facts.variable {
            return Err(format!(
                "staged wght={weight} is still variable — refused, never a variable face in fonts/"
            ));
        }
        staged.push((*weight, bytes));
    }
    let _ = std::fs::remove_dir_all(&staging_root);
    // Commit: refuse existing outputs without --force (all of them,
    // before writing any), then write.
    let mut files: Vec<PathBuf> = Vec::new();
    for (weight, _) in &staged {
        let dest = fonts_dir.join(format!("{stem}-{}.ttf", weight_name(*weight)));
        if dest.exists() && !args.force {
            return Err(format!(
                "{} exists — pass --force to overwrite",
                dest.display()
            ));
        }
        files.push(dest);
    }
    for ((_, bytes), dest) in staged.iter().zip(files.iter()) {
        std::fs::write(dest, bytes).map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
    }
    let license = copy_license(&args.font_file, &fonts_dir, &family)?;
    let module = regenerate_module(&args.dir)?;
    Ok(AddFontReport {
        family,
        weights: args.weights.clone(),
        files,
        module,
        license_copied: license.is_some(),
    })
}

/// The post-add wiring report (exact snippets — the module, desktop,
/// and web legs differ, so all three print).
pub fn wiring_report(report: &AddFontReport) -> String {
    let stem = const_stem(&report.family);
    let weights = report
        .weights
        .iter()
        .map(|w| w.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let mut lines = vec![
        format!(
            "cargo oppa: added {} ({}) — {} face(s)",
            report.family,
            weights,
            report.files.len()
        ),
        String::new(),
    ];
    for file in &report.files {
        lines.push(format!("  wrote {}", file.display()));
    }
    lines.push(format!("  generated {}", report.module.display()));
    lines.push(String::new());
    if report.license_copied {
        lines.push(
            "  license copied beside the faces (keep it — redistribution needs it).".to_string(),
        );
    } else {
        lines.push(
            "  WARNING: no license file found beside the input — confirm you may ship this font"
                .to_string(),
        );
        lines.push("  and add its license under fonts/ before distributing your app.".to_string());
    }
    lines.push(String::new());
    lines.push("  wire it up:".to_string());
    lines.push("    1. add `mod fonts;` to your crate root (once).".to_string());
    lines.push(format!(
        "    2. desktop: `.with_app_fonts(fonts::{stem}_FAMILY, fonts::{stem}_FACES)` on `WindowOptions`."
    ));
    lines.push(format!(
        "    3. web: the 6-argument `export_app!` arm with `fonts::{stem}_FAMILY`, `fonts::{stem}_FACES`, and a display face."
    ));
    lines.push(
        "  note: a new typeface remeasures every text box — re-check pixel targets".to_string(),
    );
    lines.push("  (card heights, centering) under the new metrics after swapping.".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use oppa::text::TextService;

    /// Repo font files (oppa-fonts) as parse fixtures — no Python,
    /// no network, deterministic.
    fn fixture(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../oppa-fonts/fonts")
            .join(name);
        std::fs::read(&path)
            .unwrap_or_else(|e| panic!("cannot read test fixture {}: {e}", path.display()))
    }

    #[test]
    fn weights_parse_and_refuse_loudly() {
        assert_eq!(
            parse_weights("400,600,700").expect("ok"),
            vec![400, 600, 700]
        );
        assert_eq!(parse_weights("700").expect("ok"), vec![700]);
        assert_eq!(
            parse_weights(" 700 , 400 ").expect("ok"),
            vec![400, 700],
            "whitespace + sort"
        );
        for bad in ["", "abc", "450", "50", "1000", "400,400", "400,abc"] {
            assert!(parse_weights(bad).is_err(), "{bad:?} refuses");
        }
    }

    #[test]
    fn arg_shapes_parse() {
        let argv = ["add-font".to_string(), "pop.ttf".to_string()];
        let args = parse_add_font_args(&argv[1..]).expect("ok");
        assert_eq!(args.dir, PathBuf::from("."));
        assert_eq!(args.weights, vec![400, 600, 700]);
        assert!(!args.force);

        let argv = [
            "pop.ttf",
            "my-app",
            "--weights",
            "500",
            "--family",
            "Poppins",
            "--force",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
        let args = parse_add_font_args(&argv).expect("ok");
        assert_eq!(args.dir, PathBuf::from("my-app"));
        assert_eq!(args.weights, vec![500]);
        assert_eq!(args.family.as_deref(), Some("Poppins"));
        assert!(args.force);

        assert!(parse_add_font_args(&[]).is_err(), "no file refuses");
        let extra = ["a.ttf", "b", "c"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        assert!(
            parse_add_font_args(&extra).is_err(),
            "third positional refuses"
        );
        let flag = ["a.ttf", "--nope"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        assert!(parse_add_font_args(&flag).is_err(), "unknown flag refuses");
    }

    #[test]
    fn static_facts_read_off_repo_fonts() {
        let deutils = read_font_facts(&fixture("DejaVuSans.ttf"), "DejaVuSans.ttf").expect("ok");
        assert_eq!(deutils.family, "DejaVu Sans");
        assert!(!deutils.variable);
        assert_eq!(deutils.weight, 400);

        let bold = read_font_facts(&fixture("ReadexPro-Bold.ttf"), "bold").expect("ok");
        assert_eq!(bold.family, "Readex Pro");
        assert!(!bold.variable, "pinned HEXP-only fvar counts as static");
        assert_eq!(bold.weight, 700);
    }

    #[test]
    fn variable_facts_detect_wght_axis() {
        let var = read_font_facts(&fixture("ReadexPro-Variable.ttf"), "variable").expect("ok");
        assert_eq!(var.family, "Readex Pro");
        assert!(var.variable, "wght axis present");
    }

    #[test]
    fn containers_refuse_with_instructions() {
        assert!(read_font_facts(b"wOF2junkjunk", "x.woff2").is_err());
        assert!(read_font_facts(b"ttcfjunkjunk", "x.ttc").is_err());
        assert!(read_font_facts(b"nope", "x.ttf").is_err());
        assert!(read_font_facts(b"\x00\x01\x00\x00short", "x.ttf").is_err());
    }

    #[test]
    fn stems_sanitize() {
        assert_eq!(const_stem("Readex Pro"), "READEX_PRO");
        assert_eq!(const_stem("IBM Plex Sans"), "IBM_PLEX_SANS");
        assert_eq!(file_stem("Readex Pro"), "ReadexPro");
        assert_eq!(weight_name(600), "SemiBold");
    }

    fn scratch_app(tag: &str) -> PathBuf {
        // Under target/ (always writable — the harness temp dir is
        // off-limits in sandboxed runs).
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("add-font-tests-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).expect("scratch src");
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"t\"\n")
            .expect("scratch manifest");
        dir
    }

    #[test]
    fn static_font_vends_faces_and_module() {
        let dir = scratch_app("static");
        let input = dir.join("DejaVuSans.ttf");
        std::fs::write(&input, fixture("DejaVuSans.ttf")).expect("stage input");
        let args = AddFontArgs {
            font_file: input,
            dir: dir.clone(),
            weights: vec![400],
            family: None,
            force: false,
        };
        let report = add_font(&args).expect("static adds");
        assert_eq!(report.family, "DejaVu Sans");
        assert!(dir.join("fonts/DejaVuSans-Regular.ttf").is_file());
        let module = std::fs::read_to_string(dir.join("src/fonts.rs")).expect("module written");
        assert!(module.contains("DEJAVU_SANS_FAMILY"), "{module}");
        assert!(module.contains("DEJAVU_SANS_REGULAR"), "{module}");
        assert!(module.contains("DEJAVU_SANS_FACES"), "{module}");
        // No license beside the input: warned, not refused.
        assert!(!report.license_copied);
        // Generated faces must shape, not just parse (dev-deps prove
        // the framework path the runners will take).
        let faces: Vec<(String, Vec<u8>)> = ["DejaVuSans-Regular.ttf"]
            .iter()
            .map(|name| {
                (
                    name.to_string(),
                    std::fs::read(dir.join("fonts").join(name)).expect("read face"),
                )
            })
            .collect();
        let borrowed: Vec<(&str, &[u8])> = faces
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
            .collect();
        let (svc, _) = oppa_text_rustybuzz::RustybuzzService::from_bytes_with_chain(&borrowed, &[])
            .expect("generated faces load");
        let run = svc
            .shape("Ag", &oppa::text::TextStyle::new("DejaVu Sans", 20.0))
            .expect("generated face shapes");
        assert!(!run.glyphs.is_empty(), "shaped glyphs out");
        // Second run without --force refuses on the existing face.
        assert!(add_font(&args).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn family_mismatch_and_layout_refuse() {
        let dir = scratch_app("mismatch");
        let input = dir.join("DejaVuSans.ttf");
        std::fs::write(&input, fixture("DejaVuSans.ttf")).expect("stage input");
        let base = AddFontArgs {
            font_file: input,
            dir: dir.clone(),
            weights: vec![400],
            family: Some("Poppins".to_string()),
            force: false,
        };
        assert!(add_font(&base).is_err(), "--family mismatch refuses");
        let wrong_weights = AddFontArgs {
            weights: vec![700],
            family: None,
            ..base.clone()
        };
        assert!(
            add_font(&wrong_weights).is_err(),
            "static 400 with --weights 700 refuses"
        );
        let no_layout = AddFontArgs {
            dir: PathBuf::from("/no/such/app"),
            ..base.clone()
        };
        assert!(add_font(&no_layout).is_err(), "missing Cargo.toml refuses");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn license_prefers_family_ofl_sibling() {
        let dir = scratch_app("license");
        // A stray other-family OFL must never launder in, while the
        // family-mentioning sibling wins over the generic fallback.
        std::fs::write(dir.join("OFL-ReadexPro.txt"), "wrong family").expect("stage stray");
        std::fs::write(dir.join("Poppins-OFL.txt"), "ofl text").expect("stage sibling");
        std::fs::write(dir.join("LICENSE.txt"), "generic").expect("stage generic");
        let fonts = dir.join("fonts");
        std::fs::create_dir_all(&fonts).expect("fonts dir");
        let got = copy_license(&dir.join("in.ttf"), &fonts, "Poppins").expect("ok");
        assert_eq!(got.as_deref(), Some("LICENSE-Poppins.txt"));
        assert_eq!(
            std::fs::read(fonts.join("LICENSE-Poppins.txt")).expect("read"),
            b"ofl text"
        );
        // Nothing relevant: warned path, not an error.
        let empty = scratch_app("license-empty");
        let fonts = empty.join("fonts");
        std::fs::create_dir_all(&fonts).expect("fonts dir");
        assert!(copy_license(&empty.join("in.ttf"), &fonts, "Poppins")
            .expect("ok")
            .is_none());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }

    /// End to end over the variable file (needs fonttools on PATH —
    /// a loud skip-pass when absent, never a silent hole: CI without
    /// Python still proves everything else).
    #[test]
    fn variable_font_instances_requested_weights() {
        if fonttools_probe().is_err() {
            eprintln!("add-font e2e: fonttools absent — skipping instancing proof");
            return;
        }
        let dir = scratch_app("variable");
        let input = dir.join("ReadexPro-Variable.ttf");
        std::fs::write(&input, fixture("ReadexPro-Variable.ttf")).expect("stage input");
        // Sibling license proves the copy leg.
        std::fs::write(dir.join("OFL.txt"), "sibling license").expect("stage license");
        let args = AddFontArgs {
            font_file: input,
            dir: dir.clone(),
            weights: vec![400, 700],
            family: None,
            force: false,
        };
        let report = add_font(&args).expect("variable adds");
        assert_eq!(report.family, "Readex Pro");
        assert!(report.license_copied);
        assert!(dir.join("fonts/ReadexPro-Regular.ttf").is_file());
        assert!(dir.join("fonts/ReadexPro-Bold.ttf").is_file());
        assert!(dir.join("fonts/LICENSE-ReadexPro.txt").is_file());
        for (bytes, weight) in [
            (
                std::fs::read(dir.join("fonts/ReadexPro-Regular.ttf")).expect("read"),
                400,
            ),
            (
                std::fs::read(dir.join("fonts/ReadexPro-Bold.ttf")).expect("read"),
                700,
            ),
        ] {
            let facts = read_font_facts(&bytes, "instanced").expect("parses");
            assert_eq!(facts.weight, weight);
            assert!(!facts.variable);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
