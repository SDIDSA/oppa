//! `cargo oppa new` — the Oppa scaffolder (Phase 37c, decision 365).
//!
//! Copies `templates/hello-desktop` (default) or `templates/hello-web`
//! out of the checkout into a new directory, renames the package to
//! the directory name, and points the three path dependencies at an
//! Oppa checkout (the compile-time checkout by default — correct
//! when run from source; `--oppa-path` overrides). Refusals are
//! loud: existing non-empty destinations, invalid package names,
//! unknown flags, and missing checkouts never scaffold half.
//!
//! Template contents embed via `include_str!` (deterministic — the
//! tool never depends on its working directory). The embedded list
//! is pinned by [`TEMPLATE_FILES`] and proven complete by
//! `embedded_list_matches_templates` (new template files fail the
//! test until the list names them — misses are loud, never silent).

use std::path::{Path, PathBuf};

/// One embedded template file: path inside the scaffold + contents.
pub struct TemplateFile {
    /// Scaffold-relative path (`Cargo.toml`, `src/main.rs`, …).
    pub path: &'static str,
    /// File contents (template spellings — `new_project` rewrites
    /// names and dependency paths per scaffold).
    pub contents: &'static str,
}

/// The template kind (`--desktop` default, `--web`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Template {
    Desktop,
    Web,
}

/// Embedded desktop files (mirrors `templates/hello-desktop` minus
/// `target/` and `Cargo.lock` — build outputs regenerate on first
/// build, never scaffolded).
pub const DESKTOP_FILES: &[TemplateFile] = &[
    TemplateFile {
        path: "Cargo.toml",
        contents: include_str!("../../../templates/hello-desktop/Cargo.toml"),
    },
    TemplateFile {
        path: "src/main.rs",
        contents: include_str!("../../../templates/hello-desktop/src/main.rs"),
    },
    TemplateFile {
        path: "README.md",
        contents: include_str!("../../../templates/hello-desktop/README.md"),
    },
];

/// Embedded web files (mirrors `templates/hello-web` minus
/// `target/` and `Cargo.lock`).
pub const WEB_FILES: &[TemplateFile] = &[
    TemplateFile {
        path: "Cargo.toml",
        contents: include_str!("../../../templates/hello-web/Cargo.toml"),
    },
    TemplateFile {
        path: "src/lib.rs",
        contents: include_str!("../../../templates/hello-web/src/lib.rs"),
    },
    TemplateFile {
        path: "README.md",
        contents: include_str!("../../../templates/hello-web/README.md"),
    },
    TemplateFile {
        path: "web/index.html",
        contents: include_str!("../../../templates/hello-web/web/index.html"),
    },
    TemplateFile {
        path: "web/oppa-bootstrap.js",
        contents: include_str!("../../../templates/hello-web/web/oppa-bootstrap.js"),
    },
    TemplateFile {
        path: "web/manifest.json",
        contents: include_str!("../../../templates/hello-web/web/manifest.json"),
    },
    TemplateFile {
        path: "web/sw.js",
        contents: include_str!("../../../templates/hello-web/web/sw.js"),
    },
];

/// Canonical browser rig, embedded (the single copy every page
/// rides — `web/oppa-bootstrap.js` in dev rigs, templates, and
/// generated app shells alike). Deterministic: no checkout lookup
/// at runtime, and `embedded_bootstrap_matches_sources` fails loudly
/// on drift.
pub const BOOTSTRAP_JS: &str = include_str!("../../../crates/oppa-web/web/oppa-bootstrap.js");

/// Canonical Android java stubs, embedded (per-app shells render
/// these with the app package + lib substituted — same rule as the
/// bootstrap: `embedded_java_matches_sources` fails loudly on
/// drift).
pub const ANDROID_JAVA_UI: &str =
    include_str!("../../../crates/oppa-android-runner/java/OppaUi.java");
pub const ANDROID_JAVA_IME: &str =
    include_str!("../../../crates/oppa-android-runner/java/OppaIme.java");

/// Boot placeholder resources (static — the window paints this
/// before the first frame, so cold start shows app chrome at
/// window-open instead of black).
pub const ANDROID_RES_COLORS: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <color name=\"oppa_boot\">#ffffff</color>\n</resources>\n";
pub const ANDROID_RES_THEMES: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <style name=\"OppaBoot\" parent=\"@android:style/Theme.NoTitleBar\">\n        <item name=\"android:windowBackground\">@color/oppa_boot</item>\n    </style>\n</resources>\n";

/// Template source names the embedded lists must mirror (the
/// completeness test walks these — `target/` and `Cargo.lock`
/// excluded, like the lists).
pub const TEMPLATE_SOURCES: &[(&str, Template)] = &[
    ("hello-desktop", Template::Desktop),
    ("hello-web", Template::Web),
];

/// wasm-bindgen pin for web scaffolds (mirrors
/// `templates/hello-web/Cargo.toml` — a floating range would resolve
/// past the installed CLI on fresh scaffolds and `serve` would
/// refuse; the mirror test keeps this, the template, and the
/// workspace lockfile in sync).
pub const WEB_BINDGEN_PIN: &str = "=0.2.128";

/// The checkout this tool built from (the templates' home —
/// default `--oppa-path`, correct when run from source).
pub fn compile_time_checkout() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent() // crates/
        .and_then(|p| p.parent()) // checkout root
        .expect("cargo-oppa lives in <checkout>/crates/cargo-oppa")
        .to_path_buf()
}

/// Validates a scaffold package name (cargo's rules, subset):
/// non-empty, lowercase alphanumeric/`-`/`_`, never starting with a
/// digit or `-`. Refuses loudly (a bad name fails pages later at
/// `cargo build` — here, at scaffold time).
pub fn validate_name(name: &str) -> Result<String, String> {
    if name.is_empty() {
        return Err("project name is empty — pass `cargo oppa new <dir>`".to_string());
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => {
            return Err(format!(
                "project name {name:?} must start with a lowercase letter or `_` (cargo rules)"
            ));
        }
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(format!(
            "project name {name:?} must be lowercase alphanumeric, `-`, or `_` (cargo rules)"
        ));
    }
    Ok(name.to_string())
}

/// The `-`→`_` form (Rust identifiers, wasm pkg stems).
pub fn snake_name(name: &str) -> String {
    name.replace('-', "_")
}

/// Validates an Oppa checkout path (must contain
/// `crates/oppa/Cargo.toml` — scaffolding against a non-checkout
/// writes broken path deps, never silently).
pub fn validate_checkout(path: &Path) -> Result<PathBuf, String> {
    let manifest = path.join("crates/oppa/Cargo.toml");
    if !manifest.is_file() {
        return Err(format!(
            "oppa checkout at {} has no crates/oppa/Cargo.toml — pass --oppa-path <checkout>",
            path.display()
        ));
    }
    Ok(path.to_path_buf())
}

/// Renders one template file for a scaffold: renames the package,
/// points path deps at `checkout`, and rewrites the wasm pkg stem
/// (web only). Returns the scaffold-relative path + bytes.
pub fn render_file(
    template: Template,
    file: &TemplateFile,
    name: &str,
    checkout: &Path,
) -> (String, Vec<u8>) {
    let snake = snake_name(name);
    let mut out = file.contents.to_string();
    if file.path == "Cargo.toml" {
        out = render_manifest(template, name, checkout);
    }
    // Rewrite the copy-out paragraph BEFORE the identifier
    // renames below (which would otherwise eat the match): the
    // generated README's copy-out line is template provenance, not
    // instructions (you are already here — asserted by
    // `rendered_scaffold_has_no_copy_out_or_relative_climbs`).
    // Template-relative doc links (`](../../docs/…`) rebase at the
    // checkout too (scaffolds live outside the repo — a stale
    // relative climb would 404 silently, never loudly).
    if file.path == "README.md" {
        let (from_kebab, _) = match template {
            Template::Desktop => ("hello-desktop", "hello_desktop"),
            Template::Web => ("hello-web", "hello_web"),
        };
        let root = checkout.display().to_string().replace('\\', "/");
        out = out
            .replace(
                &format!("cp -r templates/{from_kebab} ~/my-app"),
                "# scaffolded here by `cargo oppa new` (no copy needed)",
            )
            .replace(
                &format!("cp -r templates/{from_kebab} ~/my-web-app"),
                "# scaffolded here by `cargo oppa new` (no copy needed)",
            )
            .replace("](../../", &format!("]({root}/"));
    }
    // Crate renames (both spellings — kebab in prose/manifests,
    // snake in module stems).
    let (from_kebab, from_snake) = match template {
        Template::Desktop => ("hello-desktop", "hello_desktop"),
        Template::Web => ("hello-web", "hello_web"),
    };
    out = out.replace(from_kebab, name).replace(from_snake, &snake);
    (file.path.to_string(), out.into_bytes())
}

/// Renders the manifest: package name + absolute path deps at
/// `checkout` (relative `../../crates/*` only builds inside this
/// repo — scaffolds live outside it, so the deps name the checkout
/// explicitly, never a stale relative climb).
fn render_manifest(template: Template, name: &str, checkout: &Path) -> String {
    let root = checkout.display().to_string().replace('\\', "/");
    let (kind, bin_or_lib) = match template {
        Template::Desktop => (
            "Minimal Oppa desktop app",
            "[[bin]]\nname = \"__NAME__\"\npath = \"src/main.rs\"\n",
        ),
        Template::Web => (
            "Minimal Oppa web app",
            "[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\npath = \"src/lib.rs\"\n",
        ),
    };
    let third_dep = match template {
        Template::Desktop => format!("oppa-app = {{ path = \"{root}/crates/oppa-app\" }}\n"),
        Template::Web => {
            format!("oppa-web = {{ path = \"{root}/crates/oppa-web\" }}\nwasm-bindgen = \"{WEB_BINDGEN_PIN}\"\n")
        }
    };
    format!(
        "[package]\nname = \"__NAME__\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"MIT OR Apache-2.0\"\ndescription = \"{kind} (scaffolded by `cargo oppa new`)\"\n\n{bin_or_lib}\n[dependencies]\noppa = {{ path = \"{root}/crates/oppa\" }}\noppa-controls = {{ path = \"{root}/crates/oppa-controls\" }}\n{third_dep}\n[workspace]\n",
    )
    .replace("__NAME__", name)
}

/// Scaffolds a project: validates, refuses non-empty destinations
/// loudly (never merges into existing work), writes rendered files.
/// Returns the written paths (for the next-steps report).
pub fn new_project(
    dest: &Path,
    template: Template,
    name: &str,
    checkout: &Path,
) -> Result<Vec<PathBuf>, String> {
    let name = validate_name(name)?;
    let checkout = validate_checkout(checkout)?;
    if dest.exists() {
        let occupied = std::fs::read_dir(dest)
            .map_err(|e| format!("cannot read {}: {e}", dest.display()))?
            .next()
            .is_some();
        if occupied {
            return Err(format!(
                "destination {} exists and is not empty — scaffold into an empty directory (never merge)",
                dest.display()
            ));
        }
    }
    let files = match template {
        Template::Desktop => DESKTOP_FILES,
        Template::Web => WEB_FILES,
    };
    let mut written = Vec::with_capacity(files.len());
    for file in files {
        let (rel, bytes) = render_file(template, file, &name, &checkout);
        let path = dest.join(&rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::write(&path, &bytes)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

/// Next steps after scaffolding (printed by the CLI — the template
/// READMEs carry the full recipes).
pub fn next_steps(template: Template) -> &'static str {
    match template {
        Template::Desktop => "cd into your project and `cargo run`",
        Template::Web => {
            "cd into your project and `cargo oppa serve` (builds the wasm, runs wasm-bindgen, serves, opens the browser)"
        }
    }
}

/// Options for `cargo oppa serve` (one-command web loop).
pub struct ServeOptions {
    /// Project directory (holds `Cargo.toml` + `web/index.html`).
    pub dir: PathBuf,
    /// Loopback port to serve on.
    pub port: u16,
    /// `true` = dev profile (fast builds); `false` = release.
    pub dev: bool,
    /// Skip opening the system browser (print the URL only).
    pub no_browser: bool,
}

/// Content type for a served path (dev server — the full matrix the
/// shell needs; unknown suffixes refuse loudly, never guessed).
pub fn content_type_for(path: &str) -> Result<&'static str, String> {
    let suffix = path.rsplit('.').next().unwrap_or_default();
    match suffix {
        "html" => Ok("text/html; charset=utf-8"),
        "js" => Ok("text/javascript; charset=utf-8"),
        "mjs" => Ok("text/javascript; charset=utf-8"),
        "wasm" => Ok("application/wasm"),
        "json" => Ok("application/json"),
        "css" => Ok("text/css"),
        "svg" => Ok("image/svg+xml"),
        "png" => Ok("image/png"),
        "ico" => Ok("image/x-icon"),
        other => Err(format!(
            "refusing to serve {path:?} (unknown suffix {other:?} — add it to content_type_for)"
        )),
    }
}

/// Parses `wasm-bindgen --version` output (`wasm-bindgen 0.2.128`).
pub fn parse_bindgen_cli_version(output: &str) -> Result<String, String> {
    let version = output
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("cannot parse wasm-bindgen version from {output:?}"))?;
    if version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        Ok(version.to_string())
    } else {
        Err(format!("cannot parse wasm-bindgen version from {output:?}"))
    }
}

/// Scans a flat `"key":"value"` JSON string field (`cargo
/// metadata` output is one line — a full JSON crate for two fields
/// would be a new dependency; standard escapes decode, anything
/// else refuses loudly instead of guessing a path).
pub fn json_string_field(text: &str, key: &str) -> Option<String> {
    let start = text.find(key)?;
    let rest = text[start + key.len()..].trim_start();
    let body = rest.strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = body.chars();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                _ => return None,
            },
            c => out.push(c),
        }
    }
}

/// Resolves build paths that move under workspaces: `cargo
/// metadata` names the real `target_directory` + `workspace_root`
/// (a member like `crates/oppa-web` builds into the workspace root
/// and shares its lockfile — never its own dir). Refuses loudly
/// when metadata fails.
pub fn cargo_paths(dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    let out = std::process::Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version")
        .arg("1")
        .current_dir(dir)
        .output()
        .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err("cargo metadata failed — is this a Rust project?".to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let target = json_string_field(&text, "\"target_directory\"")
        .ok_or_else(|| "cargo metadata hides target_directory".to_string())?;
    let root = json_string_field(&text, "\"workspace_root\"")
        .ok_or_else(|| "cargo metadata hides workspace_root".to_string())?;
    Ok((PathBuf::from(target), PathBuf::from(root)))
}

/// Reads the `wasm-bindgen` crate version from a `Cargo.lock` body.
pub fn lockfile_bindgen_version(lock: &str) -> Result<String, String> {
    let mut in_package = false;
    let mut is_bindgen = false;
    for line in lock.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            in_package = true;
            is_bindgen = false;
        } else if in_package && line == "name = \"wasm-bindgen\"" {
            is_bindgen = true;
        } else if in_package && is_bindgen && line.starts_with("version = \"") {
            let version = line
                .strip_prefix("version = \"")
                .and_then(|v| v.strip_suffix('"'))
                .ok_or_else(|| "cannot parse wasm-bindgen version line".to_string())?;
            return Ok(version.to_string());
        }
    }
    Err("Cargo.lock has no wasm-bindgen package — build the project first".to_string())
}

/// Reads the `[package] name` from a manifest body.
pub fn package_name_from_manifest(manifest: &str) -> Result<String, String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package && line.starts_with("name") {
            let name = line
                .split_once('=')
                .map(|(_, v)| v.trim().trim_matches('"').to_string())
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "cannot parse [package] name".to_string())?;
            return Ok(name);
        }
    }
    Err("manifest has no [package] name".to_string())
}

/// Resolves an HTTP request target to a file under `root`: `/`
/// serves `index.html`, queries strip, `..` escapes refuse loudly
/// (never serve outside the web dir).
pub fn resolve_request(root: &Path, target: &str) -> Result<PathBuf, String> {
    let path = target.split('?').next().unwrap_or("/");
    let path = if path == "/" { "/index.html" } else { path };
    let mut out = root.to_path_buf();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        if part == ".." {
            return Err(format!("refusing escape {target:?}"));
        }
        out.push(part);
    }
    if !out.is_file() {
        return Err(format!("no such file {target:?}"));
    }
    Ok(out)
}

/// Runs one dev-server connection: parses the request line, serves
/// the file (GET) or its headers (HEAD), then closes. Errors become
/// status lines on the wire (loud in the browser, never hangs).
fn serve_connection(stream: &mut std::net::TcpStream, root: &Path) -> Result<(), String> {
    use std::io::{BufRead, BufReader, Write};
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|e| format!("cannot clone stream: {e}"))?,
    );
    let mut request = String::new();
    reader
        .read_line(&mut request)
        .map_err(|e| format!("cannot read request: {e}"))?;
    let mut parts = request.split_whitespace();
    let (method, target) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    if method != "GET" && method != "HEAD" {
        let body = "method not allowed (GET/HEAD only)";
        write!(
            stream,
            "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .map_err(|e| format!("cannot write 405: {e}"))?;
        return Ok(());
    }
    let (status, ctype, bytes) = match resolve_request(root, target).and_then(|path| {
        let ctype = content_type_for(&path.display().to_string())?;
        Ok((path, ctype.to_string()))
    }) {
        Ok((path, ctype)) => match std::fs::read(&path) {
            Ok(bytes) => ("HTTP/1.1 200 OK", ctype, bytes),
            Err(e) => {
                let body = format!("cannot read {}: {e}", path.display());
                (
                    "HTTP/1.1 500 Internal Server Error",
                    "text/plain".to_string(),
                    body.into_bytes(),
                )
            }
        },
        Err(why) => {
            let body = format!("oppa serve: {why}");
            (
                "HTTP/1.1 404 Not Found",
                "text/plain".to_string(),
                body.into_bytes(),
            )
        }
    };
    // Dev loop: never cache (a stale wasm under a fresh page is the
    // classic mixed-behavior trap — the browser revalidates all).
    let head = format!(
        "{status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    stream
        .write_all(head.as_bytes())
        .map_err(|e| format!("cannot write head: {e}"))?;
    if method == "GET" {
        stream
            .write_all(&bytes)
            .map_err(|e| format!("cannot write body: {e}"))?;
    }
    Ok(())
}

/// Launch target for `check` / `build` / `run` (exactly one per
/// invocation — ambiguous targets refuse loudly, never first-wins).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Desktop,
    Web,
    Android,
}

impl Target {
    /// Parses `--desktop` / `--web` / `--android`.
    pub fn parse(flag: &str) -> Result<Self, String> {
        match flag {
            "--desktop" => Ok(Target::Desktop),
            "--web" => Ok(Target::Web),
            "--android" => Ok(Target::Android),
            _ => Err(format!(
                "unknown target {flag:?} — pass exactly one of --desktop --web --android"
            )),
        }
    }
}

/// Validates an app dir has a manifest plus Rust sources (the
/// `check` floor — targets add their own entries below).
pub fn validate_layout(dir: &Path) -> Result<(), String> {
    if !dir.join("Cargo.toml").is_file() {
        return Err(format!(
            "no Cargo.toml in {} — run in an app dir (try `cargo oppa new <dir>`)",
            dir.display()
        ));
    }
    let src = dir.join("src");
    let has_sources = src.is_dir()
        && std::fs::read_dir(&src)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.path().extension().is_some_and(|x| x == "rs"))
            })
            .unwrap_or(false);
    if !has_sources {
        return Err(format!(
            "no src/*.rs in {} — nothing to build",
            dir.display()
        ));
    }
    Ok(())
}

/// Validates the desktop entries (`src/main.rs` native binary).
pub fn validate_desktop_layout(dir: &Path) -> Result<(), String> {
    validate_layout(dir)?;
    if !dir.join("src").join("main.rs").is_file() {
        return Err(format!(
            "no src/main.rs in {} — not a desktop app",
            dir.display()
        ));
    }
    Ok(())
}

/// Validates the web entries (wasm lib + standard shell — the exact
/// files the web build consumes; a bespoke shell refuses loudly
/// here, never half-builds).
pub fn validate_web_layout(dir: &Path) -> Result<(), String> {
    validate_layout(dir)?;
    for rel in ["src/lib.rs", "web/index.html", "web/oppa-bootstrap.js"] {
        if !dir.join(rel).is_file() {
            return Err(format!(
                "no {rel} in {} — not a standard web app",
                dir.display()
            ));
        }
    }
    Ok(())
}

/// `cargo oppa check`: layout validation plus `cargo check` on host
/// (all targets) and, when the crate has a lib, on wasm and both
/// Android ABIs too (check never links — no NDK needed). Fast and
/// side-effect-free (no artifacts, no servers).
pub fn check_app(dir: &Path) -> Result<(), String> {
    validate_layout(dir)?;
    let mut jobs: Vec<Vec<&str>> = vec![vec!["check", "--all-targets"]];
    if dir.join("src").join("lib.rs").is_file() {
        jobs.push(vec!["check", "--target", "wasm32-unknown-unknown", "--lib"]);
        for row in ANDROID_ABIS {
            let target: &str = row.0;
            jobs.push(vec!["check", "--target", target, "--lib"]);
        }
    }
    for args in &jobs {
        let status = std::process::Command::new("cargo")
            .args(args)
            .current_dir(dir)
            .status()
            .map_err(|e| format!("cannot run cargo check: {e}"))?;
        if !status.success() {
            return Err(format!(
                "cargo check failed ({} — see output above)",
                args.join(" ")
            ));
        }
    }
    println!("oppa check: {} clean", dir.display());
    Ok(())
}

/// Android ABI rows the CLI builds: Rust target, JNI ABI dir, and
/// the NDK LLVM linker triple (API level rides [`ANDROID_API`]).
pub const ANDROID_ABIS: &[(&str, &str, &str)] = &[
    (
        "aarch64-linux-android",
        "arm64-v8a",
        "aarch64-linux-android",
    ),
    ("x86_64-linux-android", "x86_64", "x86_64-linux-android"),
];

/// NDK platform level the linkers target (the repo's minSdk —
/// matches the proof APK flow).
pub const ANDROID_API: &str = "35";

/// NDK LLVM prebuilt dir per host OS (loud refusal elsewhere —
/// never a guessed path).
pub fn ndk_host_tag() -> Result<&'static str, String> {
    match std::env::consts::OS {
        "windows" => Ok("windows-x86_64"),
        "linux" => Ok("linux-x86_64"),
        "macos" => Ok("darwin-x86_64"),
        other => Err(format!("no NDK host toolchain for {other}")),
    }
}

/// Parses `--variant debug|release` to the `dev` flag (`--dev`
/// is the same as `--variant debug`). Anything else refuses loudly
/// — variants are exactly these two, never a prefix or a guess.
pub fn parse_variant(raw: &str) -> Result<bool, String> {
    match raw {
        "debug" => Ok(true),
        "release" => Ok(false),
        _ => Err(format!(
            "--variant {raw:?} is not a variant (debug|release)"
        )),
    }
}

/// NDK LLVM bin tool by bare name (`llvm-strip`, `llvm-readobj` —
/// `.exe` on Windows, bare elsewhere; absence refuses loudly).
pub fn ndk_bin(ndk: &Path, name: &str) -> Result<PathBuf, String> {
    let file = if std::env::consts::OS == "windows" {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let path = ndk
        .join("toolchains")
        .join("llvm")
        .join("prebuilt")
        .join(ndk_host_tag()?)
        .join("bin")
        .join(&file);
    if !path.is_file() {
        return Err(format!(
            "no {file} in {} — reinstall the NDK LLVM toolchain",
            ndk.display()
        ));
    }
    Ok(path)
}

/// Linker binary inside an NDK root for a triple
/// (`aarch64-linux-android35-clang[.cmd]`).
pub fn ndk_linker(ndk: &Path, triple: &str) -> Result<PathBuf, String> {
    let exe = if std::env::consts::OS == "windows" {
        ".cmd"
    } else {
        ""
    };
    let path = ndk
        .join("toolchains")
        .join("llvm")
        .join("prebuilt")
        .join(ndk_host_tag()?)
        .join("bin")
        .join(format!("{triple}{ANDROID_API}-clang{exe}"));
    if !path.is_file() {
        return Err(format!(
            "no linker {} in {} — install the NDK LLVM toolchain",
            path.display(),
            ndk.display()
        ));
    }
    Ok(path)
}

/// Picks the highest NDK version under `<sdk>/ndk` (version-dir
/// ordering — a tie or absence refuses loudly, never first-found).
pub fn find_ndk_in(sdk: &Path) -> Result<PathBuf, String> {
    let ndk_root = sdk.join("ndk");
    let entries = std::fs::read_dir(&ndk_root)
        .map_err(|_| format!("no ndk/ in {} — install the NDK (r29+)", sdk.display()))?;
    let mut versions: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .collect();
    versions.sort();
    versions.pop().map(|v| ndk_root.join(v)).ok_or_else(|| {
        format!(
            "no versioned NDK in {} — install the NDK (r29+)",
            ndk_root.display()
        )
    })
}

/// Locates the Android SDK (explicit env first, well-known homes
/// after — nothing guessed beyond those, absence refuses with the
/// install pointer).
pub fn find_sdk() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(path) = std::env::var(var) {
            candidates.push(PathBuf::from(path));
        }
    }
    #[cfg(target_os = "windows")]
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join("Android").join("Sdk"));
    }
    #[cfg(target_os = "macos")]
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(home).join("Library/Android/sdk"));
    }
    #[cfg(target_os = "linux")]
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(home).join("Android/Sdk"));
    }
    for path in candidates {
        if path.join("platform-tools").is_dir() {
            return Ok(path);
        }
    }
    Err("no Android SDK found (ANDROID_HOME nor the default home) — install Android Studio or the cmdline-tools + platform-tools + NDK, then retry".to_string())
}

/// Attached-device serials from `adb devices -l` (ready `device`
/// state only — unauthorized/offline entries never count as a
/// launch target). Missing adb refuses loudly with the install
/// pointer instead of failing obscurely at spawn.
pub fn adb_devices() -> Result<Vec<String>, String> {
    let exe = if std::env::consts::OS == "windows" {
        "adb.exe"
    } else {
        "adb"
    };
    let adb = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join(exe))
        .find(|p| p.is_file())
        .or_else(|| {
            find_sdk()
                .ok()
                .map(|sdk| sdk.join("platform-tools").join(exe))
        })
        .filter(|p| p.is_file())
        .ok_or_else(|| {
            "no adb found (PATH nor SDK platform-tools) — install Android platform-tools"
                .to_string()
        })?;
    let out = std::process::Command::new(&adb)
        .arg("devices")
        .arg("-l")
        .output()
        .map_err(|e| format!("cannot run {}: {e}", adb.display()))?;
    Ok(parse_adb_devices(&String::from_utf8_lossy(&out.stdout)))
}

/// Parses `adb devices -l` output (pure for tests — header skipped,
/// only second-column `device` lines count).
pub fn parse_adb_devices(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            match (parts.next(), parts.next()) {
                (Some(serial), Some("device")) => Some(serial.to_string()),
                _ => None,
            }
        })
        .collect()
}

/// Parses one `export_app!(Class, "Title", root)` invocation from a
/// `src/lib.rs` body (the shape [`export_app`](oppa_web::export_app)
/// owns — anything else refuses loudly with the expected shape,
/// never a guessed class name).
pub fn parse_export_app(src: &str) -> Result<(String, String), String> {
    let shape = "export_app!(ClassName, \"Title\", root_fn)";
    let start = src
        .find("export_app!")
        .ok_or_else(|| format!("no export_app! in src/lib.rs — expected {shape}"))?;
    let rest = src[start + "export_app!".len()..].trim_start();
    let args = rest
        .strip_prefix('(')
        .ok_or_else(|| format!("malformed export_app! — expected {shape}"))?;
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = args.chars().peekable();
    let mut closed = false;
    while let Some(c) = chars.next() {
        match c {
            ')' => {
                closed = true;
                break;
            }
            ',' => {
                parts.push(current.trim().to_string());
                current = String::new();
            }
            '"' => {
                current.push(c);
                let mut escaped = false;
                for d in chars.by_ref() {
                    current.push(d);
                    if escaped {
                        escaped = false;
                    } else if d == '\\' {
                        escaped = true;
                    } else if d == '"' {
                        break;
                    }
                }
            }
            _ => current.push(c),
        }
    }
    if !closed {
        return Err(format!("malformed export_app! — expected {shape}"));
    }
    if parts.len() != 2 {
        return Err(format!("malformed export_app! — expected {shape}"));
    }
    let class = parts[0].clone();
    if class.is_empty() || !class.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("malformed export_app! class — expected {shape}"));
    }
    let title = parts[1]
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .ok_or_else(|| format!("malformed export_app! title — expected {shape}"))?;
    Ok((class, title.to_string()))
}

/// Renders the standard `web/index.html` (inline boot call naming
/// the package's bundle + class — the only per-app facts a page
/// needs; anything richer stays hand-owned and is never
/// overwritten).
pub fn render_index_html(name: &str, title: &str, class: &str, snake: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\" />\n<link rel=\"icon\" href=\"data:,\" />\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n<meta name=\"theme-color\" content=\"#2266cc\" />\n<link rel=\"manifest\" href=\"./manifest.json\" />\n<title>{title} — {name}</title>\n<style>\n  body {{ margin: 0; background: #eee; font-family: sans-serif; }}\n  html, body {{ height: 100%; }}\n  body {{ display: flex; flex-direction: column; height: 100vh; overflow: hidden; }}\n  #oppa-status {{ flex: 0 0 auto; padding: 8px 12px; color: #333; }}\n  #oppa-root {{ flex: 1 1 auto; width: 100%; min-height: 0; background: #fff; }}\n</style>\n</head>\n<body>\n<div id=\"oppa-status\">booting…</div>\n<div id=\"oppa-root\" tabindex=\"0\"></div>\n<script type=\"module\">\nimport {{ bootOppa }} from \"./oppa-bootstrap.js\";\n\n// Generated by `cargo oppa` (build/run/serve) from src/lib.rs\n// (`export_app!`) + Cargo.toml — do not edit by hand: missing\n// files regenerate, present files stay yours.\n//\n// The scene container flex-fills the viewport below the status\n// line (decision 470): the module viewport follows this box\n// through the bootstrap ResizeObserver. Pin to fixed px here for\n// a fixed-size app.\nawait bootOppa({{\n  module: \"./pkg/{snake}.js\",\n  className: \"{class}\",\n  statusReady: \"ready\",\n  pwa: \"./sw.js\",\n}});\n</script>\n</body>\n</html>\n"
    )
}

/// Renders the standard PWA manifest (app identity from the
/// package; icons stay app branding, none ships).
pub fn render_manifest_json(name: &str, title: &str) -> String {
    format!(
        "{{\n  \"name\": \"{title} — {name}\",\n  \"short_name\": \"{name}\",\n  \"start_url\": \"./index.html\",\n  \"scope\": \"./\",\n  \"display\": \"standalone\",\n  \"background_color\": \"#ffffff\",\n  \"theme_color\": \"#2266cc\",\n  \"description\": \"Oppa web app.\"\n}}\n"
    )
}

/// Renders the standard service worker (cache-first shell over the
/// package's bundle; versioned `CACHE` per release like the
/// template).
pub fn render_sw_js(name: &str, snake: &str) -> String {
    format!(
        "// Generated by `cargo oppa` (build/run/serve) — do not edit\n// by hand: missing files regenerate, present files stay yours.\nconst CACHE = '{name}-v1';\nconst ASSETS = [\n  './index.html',\n  './manifest.json',\n  './oppa-bootstrap.js',\n  './pkg/{snake}.js',\n  './pkg/{snake}_bg.wasm',\n];\n"
    ) + "self.addEventListener('install', (event) => {\n  event.waitUntil(\n    caches.open(CACHE).then((cache) => cache.addAll(ASSETS)).then(() => self.skipWaiting()),\n  );\n});\n\nself.addEventListener('activate', (event) => {\n  event.waitUntil(\n    caches\n      .keys()\n      .then((keys) =>\n        Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))),\n      )\n      .then(() => self.clients.claim()),\n  );\n});\n\nself.addEventListener('fetch', (event) => {\n  if (event.request.method !== 'GET') {\n    return;\n  }\n  event.respondWith(\n    caches.match(event.request).then((hit) => hit || fetch(event.request)),\n  );\n});\n"
}

/// Writes bytes only when missing or changed (the launch-time
/// cache — identical bytes skip the write; every write reports).
fn write_cached(path: &Path, bytes: &[u8], what: &str) -> Result<(), String> {
    let current = std::fs::read(path).ok();
    if current.as_deref() == Some(bytes) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let fresh = !path.exists();
    std::fs::write(path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    println!(
        "oppa web shell: {} {what} ({})",
        if fresh { "generated" } else { "refreshed" },
        path.display()
    );
    Ok(())
}

/// Generates the web shell at launch time: `oppa-bootstrap.js`
/// always syncs to the embedded canonical copy (byte-compared —
/// identical bytes skip the write); `index.html`, `manifest.json`,
/// and `sw.js` generate only when missing (present files stay
/// hand-owned, never overwritten — custom pages are safe). A
/// present `index.html` naming a different bundle refuses loudly
/// (stale shell would 404, never serves broken).
pub fn sync_web_shell(dir: &Path) -> Result<(), String> {
    let manifest = dir.join("Cargo.toml");
    let manifest_text = std::fs::read_to_string(&manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    let name = package_name_from_manifest(&manifest_text)?;
    let snake = snake_name(&name);
    let lib = dir.join("src").join("lib.rs");
    let src = std::fs::read_to_string(&lib).map_err(|_| {
        format!(
            "no src/lib.rs in {} — only lib crates serve web",
            dir.display()
        )
    })?;
    let (class, title) = parse_export_app(&src)?;
    let web = dir.join("web");
    write_cached(
        &web.join("oppa-bootstrap.js"),
        BOOTSTRAP_JS.as_bytes(),
        "bootstrap",
    )?;
    let index = web.join("index.html");
    if !index.is_file() {
        write_cached(
            &index,
            render_index_html(&name, &title, &class, &snake).as_bytes(),
            "index",
        )?;
    } else {
        let html = std::fs::read_to_string(&index)
            .map_err(|e| format!("cannot read {}: {e}", index.display()))?;
        if !html.contains(&format!("./pkg/{snake}.js")) {
            return Err(format!(
                "web/index.html names no ./pkg/{snake}.js — stale shell (delete it to regenerate, never serve broken)"
            ));
        }
    }
    let manifest_json = web.join("manifest.json");
    if !manifest_json.is_file() {
        write_cached(
            &manifest_json,
            render_manifest_json(&name, &title).as_bytes(),
            "manifest",
        )?;
    }
    let sw = web.join("sw.js");
    if !sw.is_file() {
        write_cached(&sw, render_sw_js(&name, &snake).as_bytes(), "worker")?;
    }
    Ok(())
}

/// Builds the wasm lib + runs the pinned wasm-bindgen (shared by
/// `build --web` and `run --web`). Returns the `web/pkg` dir.
pub fn build_web_app(dir: &Path, dev: bool) -> Result<PathBuf, String> {
    sync_web_shell(dir)?;
    validate_web_layout(dir)?;
    // wasm-bindgen CLI first (fast refusal beats a long build into
    // a missing tool).
    let cli_out = std::process::Command::new("wasm-bindgen")
        .arg("--version")
        .output()
        .map_err(|_| {
            "wasm-bindgen not on PATH — install it (cargo install wasm-bindgen-cli --version <your lockfile's wasm-bindgen>)".to_string()
        })?;
    if !cli_out.status.success() {
        return Err("`wasm-bindgen --version` failed — reinstall wasm-bindgen-cli".to_string());
    }
    let cli_version = parse_bindgen_cli_version(&String::from_utf8_lossy(&cli_out.stdout))?;
    // Build the lib only (bins are native entries — a desktop
    // `main` never compiles to wasm; the web artifact is the
    // cdylib).
    let profile = if dev { "dev" } else { "release" };
    let mut build = std::process::Command::new("cargo");
    build
        .arg("build")
        .arg("--lib")
        .arg("--target")
        .arg("wasm32-unknown-unknown")
        .current_dir(dir);
    if !dev {
        build.arg("--release");
    }
    let status = build
        .status()
        .map_err(|e| format!("cannot run cargo build: {e}"))?;
    if !status.success() {
        return Err(format!(
            "wasm {profile} build failed (see cargo output above)"
        ));
    }
    // Bindgen (CLI pin must match the lockfile pin — mismatches
    // corrupt the glue, never warn-and-continue). Workspace members
    // share the workspace lockfile and target dir (never the
    // member's own).
    let manifest = dir.join("Cargo.toml");
    let (target_dir, workspace_root) = cargo_paths(dir)?;
    let lock = std::fs::read_to_string(workspace_root.join("Cargo.lock"))
        .map_err(|_| "no Cargo.lock — the build produced nothing".to_string())?;
    let lock_version = lockfile_bindgen_version(&lock)?;
    if lock_version != cli_version {
        return Err(format!(
            "wasm-bindgen-cli {cli_version} != lockfile {lock_version} — cargo install wasm-bindgen-cli --version {lock_version}"
        ));
    }
    let manifest_text = std::fs::read_to_string(&manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    let name = package_name_from_manifest(&manifest_text)?;
    let snake = snake_name(&name);
    let wasm = target_dir.join(format!(
        "wasm32-unknown-unknown/{}/{snake}.wasm",
        if dev { "debug" } else { "release" }
    ));
    if !wasm.is_file() {
        return Err(format!(
            "build left no {} — the crate-type must include cdylib",
            wasm.display()
        ));
    }
    let pkg = dir.join("web").join("pkg");
    let status = std::process::Command::new("wasm-bindgen")
        .arg(&wasm)
        .arg("--target")
        .arg("web")
        .arg("--out-dir")
        .arg(&pkg)
        .status()
        .map_err(|e| format!("cannot run wasm-bindgen: {e}"))?;
    if !status.success() {
        return Err("wasm-bindgen failed (see output above)".to_string());
    }
    Ok(pkg)
}

/// Builds the app lib for both Android ABIs with the NDK LLVM
/// linkers and stages per-ABI `.so` files under `android/libs/`
/// (the exact binaries [`assemble_android_apk`] packages — assembly,
/// signing, and install build on this step). Returns the staged
/// `.so` paths. Toolchain and targets refuse loudly (missing NDK,
/// missing Rust target, link failure) — never a half-staged dir.
pub fn build_android_app(dir: &Path, dev: bool) -> Result<Vec<PathBuf>, String> {
    validate_layout(dir)?;
    if !dir.join("src").join("lib.rs").is_file() {
        return Err(format!(
            "no src/lib.rs in {} — only lib crates build Android .so files",
            dir.display()
        ));
    }
    let sdk = find_sdk()?;
    let ndk = find_ndk_in(&sdk)?;
    let manifest_text = std::fs::read_to_string(dir.join("Cargo.toml"))
        .map_err(|e| format!("cannot read {}/Cargo.toml: {e}", dir.display()))?;
    let snake = snake_name(&package_name_from_manifest(&manifest_text)?);
    let profile = if dev { "dev" } else { "release" };
    let mut staged = Vec::new();
    for row in ANDROID_ABIS {
        let (target, jni_abi, triple): (&str, &str, &str) = (row.0, row.1, row.2);
        let linker = ndk_linker(&ndk, triple)?;
        let mut build = std::process::Command::new("cargo");
        build
            .arg("build")
            .arg("--lib")
            .arg("--target")
            .arg(target)
            .current_dir(dir)
            .env(
                format!(
                    "CARGO_TARGET_{}_LINKER",
                    target.to_uppercase().replace('-', "_")
                ),
                &linker,
            );
        if !dev {
            build.arg("--release");
        }
        let status = build
            .status()
            .map_err(|e| format!("cannot run cargo build: {e}"))?;
        if !status.success() {
            return Err(format!(
                "Android {target} {profile} build failed (see cargo output above)"
            ));
        }
        let (target_dir, _) = cargo_paths(dir)?;
        // Rust cdylib names are `lib<name>.so` on every host (the
        // prefix rides the target, never the host).
        let built = target_dir.join(format!(
            "{target}/{}/lib{snake}.so",
            if dev { "debug" } else { "release" }
        ));
        if !built.is_file() {
            return Err(format!(
                "build left no {} — the crate-type must include cdylib",
                built.display()
            ));
        }
        let dest = dir
            .join("android")
            .join("libs")
            .join(jni_abi)
            .join(format!("lib{snake}.so"));
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::copy(&built, &dest)
            .map_err(|e| format!("cannot stage {}: {e}", dest.display()))?;
        // Release strips the staged copy (symbol tables are
        // developer data — the `target/` originals keep them for
        // crash symbolication, the shipped bytes never carry them).
        // Debug keeps full symbols (the debug path is the debug
        // path — never a silent strip).
        if !dev {
            let strip = ndk_bin(&ndk, "llvm-strip")?;
            let status = std::process::Command::new(&strip)
                .arg(&dest)
                .status()
                .map_err(|e| format!("cannot run llvm-strip: {e}"))?;
            if !status.success() {
                return Err(format!("llvm-strip failed on {}", dest.display()));
            }
            println!("oppa build: android stripped ({})", dest.display());
        }
        println!("oppa build: android {target} ok ({})", dest.display());
        staged.push(dest);
    }
    Ok(staged)
}

/// Assembles a signed release APK for the app (the `build
/// --android` artifact): shell sync → NDK `.so` set → aapt2 link →
/// javac → d8 → zipalign → apksigner. Every tool refuses loudly
/// when missing; every stage checks its output (a half-built APK
/// never reports success). Returns the APK path. Host-verified by
/// the caller (`verify_android_apk`); on-device boot stays the
/// manual leg until hardware proves it.
pub fn assemble_android_apk(dir: &Path, dev: bool) -> Result<PathBuf, String> {
    let (package, snake) = sync_android_shell(dir)?;
    build_android_app(dir, dev)?;
    let sdk = find_sdk()?;
    let android_jar = sdk
        .join("platforms")
        .join(format!("android-{ANDROID_TARGET_SDK}"))
        .join("android.jar");
    if !android_jar.is_file() {
        return Err(format!(
            "no {} — install the Android {} platform",
            android_jar.display(),
            ANDROID_TARGET_SDK
        ));
    }
    let aapt2 = sdk_tool(&sdk, "aapt2")?;
    let profile = if dev { "debug" } else { "release" };
    let android = dir.join("android");
    let work = android.join("apk-work");
    if work.is_dir() {
        std::fs::remove_dir_all(&work)
            .map_err(|e| format!("cannot clear {}: {e}", work.display()))?;
    }
    for sub in ["gen", "classes", "dex", "compiled"] {
        std::fs::create_dir_all(work.join(sub))
            .map_err(|e| format!("cannot create {}: {e}", work.display()))?;
    }
    // Compile + link: res/ values (boot placeholder theme) compile
    // first, then manifest + platform + resources link to the base
    // APK + R.java. Debug links `--debug-mode` (the manifest must
    // not hardcode debuggable — lint forbids it on release — so the
    // flag carries it).
    let compile = std::process::Command::new(&aapt2)
        .arg("compile")
        .arg("--dir")
        .arg(android.join("res"))
        .arg("-o")
        .arg(work.join("compiled.zip"))
        .output()
        .map_err(|e| format!("cannot run aapt2: {e}"))?;
    if !compile.status.success() {
        return Err(format!(
            "aapt2 compile failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        ));
    }
    let base_apk = work.join("base.apk");
    let mut link_cmd = std::process::Command::new(&aapt2);
    link_cmd
        .arg("link")
        .arg("-o")
        .arg(&base_apk)
        .arg("-I")
        .arg(&android_jar)
        .arg("--manifest")
        .arg(android.join("AndroidManifest.xml"))
        .arg("-R")
        .arg(work.join("compiled.zip"))
        .arg("--java")
        .arg(work.join("gen"))
        .arg("--min-sdk-version")
        .arg(ANDROID_MIN_SDK)
        .arg("--target-sdk-version")
        .arg(ANDROID_TARGET_SDK)
        .arg("--auto-add-overlay");
    if dev {
        link_cmd.arg("--debug-mode");
    }
    let link = link_cmd
        .output()
        .map_err(|e| format!("cannot run aapt2: {e}"))?;
    if !link.status.success() {
        return Err(format!(
            "aapt2 link failed: {}",
            String::from_utf8_lossy(&link.stderr)
        ));
    }
    // Java: R.java + stubs → classes (`--release` forbids
    // `-bootclasspath`, so android.* resolves off the classpath).
    let javac = jdk_tool("javac")?;
    let mut sources = Vec::new();
    collect_files(&work.join("gen"), "java", &mut sources)?;
    collect_files(&android.join("java"), "java", &mut sources)?;
    if sources.is_empty() {
        return Err("no java sources (gen + stubs) — nothing to compile".to_string());
    }
    let mut javac_cmd = std::process::Command::new(&javac);
    javac_cmd
        .arg("--release")
        .arg("17")
        .arg("--class-path")
        .arg(&android_jar)
        .arg("-d")
        .arg(work.join("classes"));
    for source in &sources {
        javac_cmd.arg(source);
    }
    let javac_out = javac_cmd
        .output()
        .map_err(|e| format!("cannot run javac: {e}"))?;
    if !javac_out.status.success() {
        return Err(format!(
            "javac failed: {}",
            String::from_utf8_lossy(&javac_out.stderr)
        ));
    }
    // Dex: classes → single classes.dex.
    let d8 = sdk_tool(&sdk, "d8")?;
    let mut classes = Vec::new();
    collect_files(&work.join("classes"), "class", &mut classes)?;
    let mut d8_args: Vec<&str> = vec![
        "--lib",
        android_jar
            .to_str()
            .ok_or_else(|| "android.jar path is not UTF-8 — move the SDK".to_string())?,
        "--min-api",
        ANDROID_MIN_SDK,
        "--output",
    ];
    let dex_dir = work.join("dex").display().to_string();
    d8_args.push(&dex_dir);
    let d8_class_args: Vec<String> = classes.iter().map(|p| p.display().to_string()).collect();
    let mut d8_all: Vec<&str> = d8_args;
    for class in &d8_class_args {
        d8_all.push(class);
    }
    let d8_out = run_sdk_tool(&d8, &d8_all)?;
    if !d8_out.status.success() {
        return Err(format!(
            "d8 failed: {}",
            String::from_utf8_lossy(&d8_out.stderr)
        ));
    }
    // Zip: base entries + dex (deflated) + native libs (stored —
    // page alignment later demands it).
    let unsigned = work.join("app-unsigned.apk");
    assemble_zip(&base_apk, &work.join("dex"), dir, &unsigned)?;
    // Align (page-aligns the stored .so files) then sign.
    let zipalign = sdk_tool(&sdk, "zipalign")?;
    let aligned = work.join("app-aligned.apk");
    let align = std::process::Command::new(&zipalign)
        .arg("-p")
        .arg("-f")
        .arg("4")
        .arg(&unsigned)
        .arg(&aligned)
        .output()
        .map_err(|e| format!("cannot run zipalign: {e}"))?;
    if !align.status.success() {
        return Err(format!(
            "zipalign failed: {}",
            String::from_utf8_lossy(&align.stderr)
        ));
    }
    let keystore = android.join("debug.keystore");
    if !keystore.is_file() {
        let keytool = jdk_tool("keytool")?;
        let gen = std::process::Command::new(&keytool)
            .arg("-genkeypair")
            .arg("-keystore")
            .arg(&keystore)
            .arg("-alias")
            .arg("androiddebugkey")
            .arg("-storepass")
            .arg("android")
            .arg("-keypass")
            .arg("android")
            .arg("-keyalg")
            .arg("RSA")
            .arg("-keysize")
            .arg("2048")
            .arg("-validity")
            .arg("10950")
            .arg("-dname")
            .arg("CN=Android Debug,O=Android,C=US")
            .output()
            .map_err(|e| format!("cannot run keytool: {e}"))?;
        if !gen.status.success() {
            return Err(format!(
                "keytool failed: {}",
                String::from_utf8_lossy(&gen.stderr)
            ));
        }
        println!(
            "oppa android: generated dev keystore {}",
            keystore.display()
        );
    }
    let apk_name = format!("{snake}-{profile}.apk");
    let apk = android.join(&apk_name);
    let apksigner = sdk_tool(&sdk, "apksigner")?;
    let sign = run_sdk_tool(
        &apksigner,
        &[
            "sign",
            "--ks",
            keystore
                .to_str()
                .ok_or_else(|| "keystore path is not UTF-8 — move the project".to_string())?,
            "--ks-pass",
            "pass:android",
            "--key-pass",
            "pass:android",
            "--out",
            apk.to_str()
                .ok_or_else(|| "apk path is not UTF-8 — move the project".to_string())?,
            aligned
                .to_str()
                .ok_or_else(|| "aligned path is not UTF-8 — move the project".to_string())?,
        ],
    )?;
    if !sign.status.success() {
        return Err(format!(
            "apksigner failed: {}",
            String::from_utf8_lossy(&sign.stderr)
        ));
    }
    verify_android_apk(&sdk, &apk, &package)?;
    println!("oppa build: android apk ok ({})", apk.display());
    Ok(apk)
}

/// Collects files with an extension under a dir (deterministic
/// order — APK bytes stay reproducible run to run).
pub fn collect_files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries =
            std::fs::read_dir(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == ext) {
                out.push(path);
            }
        }
    }
    Ok(())
}

/// Zips the APK: base entries verbatim, `classes.dex` deflated,
/// native `.so` files stored (page alignment later demands raw
/// bytes). No compression surprises — kinds are explicit per entry.
pub fn assemble_zip(
    base_apk: &Path,
    dex_dir: &Path,
    app_dir: &Path,
    out: &Path,
) -> Result<(), String> {
    let base =
        std::fs::read(base_apk).map_err(|e| format!("cannot read {}: {e}", base_apk.display()))?;
    let mut zip = zip::ZipWriter::new(
        std::fs::File::create(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?,
    );
    let mut reader = std::io::Cursor::new(base);
    let mut base_zip =
        zip::ZipArchive::new(&mut reader).map_err(|e| format!("bad base APK: {e}"))?;
    for i in 0..base_zip.len() {
        let mut entry = base_zip
            .by_index(i)
            .map_err(|e| format!("bad base entry: {e}"))?;
        let options =
            zip::write::SimpleFileOptions::default().compression_method(entry.compression());
        zip.start_file(entry.name(), options)
            .map_err(|e| format!("zip entry {}: {e}", entry.name()))?;
        std::io::copy(&mut entry, &mut zip)
            .map_err(|e| format!("zip copy {}: {e}", entry.name()))?;
    }
    let mut dexes = Vec::new();
    collect_files(dex_dir, "dex", &mut dexes)?;
    if dexes.is_empty() {
        return Err("no classes.dex from d8 — nothing to package".to_string());
    }
    for dex in dexes {
        let bytes =
            std::fs::read(&dex).map_err(|e| format!("cannot read {}: {e}", dex.display()))?;
        zip.start_file(
            "classes.dex",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(|e| format!("zip classes.dex: {e}"))?;
        use std::io::Write;
        zip.write_all(&bytes)
            .map_err(|e| format!("zip classes.dex body: {e}"))?;
    }
    let libs = app_dir.join("android").join("libs");
    let mut sos: Vec<(&str, PathBuf)> = Vec::new();
    for abi in ["arm64-v8a", "x86_64"] {
        let mut found = Vec::new();
        if libs.join(abi).is_dir() {
            collect_files(&libs.join(abi), "so", &mut found)?;
        }
        for so_file in found {
            sos.push((abi, so_file));
        }
    }
    if sos.is_empty() {
        return Err("no staged .so files — build the ABIs first".to_string());
    }
    for (abi, so_file) in sos {
        let name = so_file
            .file_name()
            .ok_or_else(|| "nameless .so".to_string())?
            .to_string_lossy()
            .into_owned();
        let bytes = std::fs::read(&so_file)
            .map_err(|e| format!("cannot read {}: {e}", so_file.display()))?;
        zip.start_file(
            format!("lib/{abi}/{name}"),
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .map_err(|e| format!("zip {name}: {e}"))?;
        use std::io::Write;
        zip.write_all(&bytes)
            .map_err(|e| format!("zip {name} body: {e}"))?;
    }
    zip.finish()
        .map_err(|e| format!("cannot finish {}: {e}", out.display()))?;
    Ok(())
}

/// Host-verifies a signed APK: alignment check, signature check,
/// and badging assertions (package, version, both native ABIs, a
/// launchable activity). Anything off refuses loudly with the tool
/// output — an unverified APK never reports success.
pub fn verify_android_apk(sdk: &Path, apk: &Path, package: &str) -> Result<(), String> {
    let zipalign = sdk_tool(sdk, "zipalign")?;
    let aligned = std::process::Command::new(&zipalign)
        .arg("-c")
        .arg("4")
        .arg(apk)
        .output()
        .map_err(|e| format!("cannot run zipalign: {e}"))?;
    if !aligned.status.success() {
        return Err(format!(
            "zipalign -c failed: {}",
            String::from_utf8_lossy(&aligned.stderr)
        ));
    }
    let apksigner = sdk_tool(sdk, "apksigner")?;
    let verified = run_sdk_tool(
        &apksigner,
        &[
            "verify",
            apk.to_str()
                .ok_or_else(|| "apk path is not UTF-8 — move the project".to_string())?,
        ],
    )?;
    if !verified.status.success() {
        return Err(format!(
            "apksigner verify failed: {}",
            String::from_utf8_lossy(&verified.stderr)
        ));
    }
    let aapt2 = sdk_tool(sdk, "aapt2")?;
    let badging = std::process::Command::new(&aapt2)
        .arg("dump")
        .arg("badging")
        .arg(apk)
        .output()
        .map_err(|e| format!("cannot run aapt2: {e}"))?;
    if !badging.status.success() {
        return Err(format!(
            "aapt dump badging failed: {}",
            String::from_utf8_lossy(&badging.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&badging.stdout);
    for needle in [
        format!("package: name='{package}'"),
        "launchable-activity:".to_string(),
        "'arm64-v8a'".to_string(),
        "'x86_64'".to_string(),
    ] {
        if !text.contains(&needle) {
            return Err(format!("badging misses {needle}:\n{text}"));
        }
    }
    println!("oppa build: apk verified ({})", apk.display());
    Ok(())
}

/// Android SDK levels the generated shell targets (mirrors the
/// proof manifest — the NDK linkers pin API [`ANDROID_API`]).
pub const ANDROID_MIN_SDK: &str = "29";
pub const ANDROID_TARGET_SDK: &str = "36";

/// Parses one `android_main!(root::path, "com.example.app")`
/// invocation from a `src/lib.rs` body (the shape
/// `oppa_android_runner::android_main!` owns — anything else
/// refuses loudly with the expected shape, never a guessed root).
pub fn parse_android_main(src: &str) -> Result<(String, String), String> {
    let shape = "android_main!(root::path, \"com.example.app\")";
    let start = src
        .find("android_main!")
        .ok_or_else(|| format!("no android_main! in src/lib.rs — expected {shape}"))?;
    let rest = src[start + "android_main!".len()..].trim_start();
    let args = rest
        .strip_prefix('(')
        .ok_or_else(|| format!("malformed android_main! — expected {shape}"))?;
    let close = args
        .find(')')
        .ok_or_else(|| format!("malformed android_main! — expected {shape}"))?;
    let mut parts = args[..close].split(',');
    let root = parts
        .next()
        .map(str::trim)
        .filter(|s| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
        })
        .ok_or_else(|| format!("malformed android_main! root — expected {shape}"))?;
    let package = parts
        .next()
        .map(str::trim)
        .and_then(|s| s.strip_prefix('"'))
        .and_then(|s| s.strip_suffix('"'))
        .filter(|s| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        })
        .ok_or_else(|| format!("malformed android_main! package — expected {shape}"))?;
    Ok((root.to_string(), package.to_string()))
}

/// Renders `AndroidManifest.xml` (proof-manifest shape: fullscreen
/// NativeActivity + GLES feature, min/target SDK pinned — no picker
/// trampoline; pickers need their activity, a follow-up).
pub fn render_android_manifest(
    package: &str,
    label: &str,
    lib_name: &str,
    version_name: &str,
) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\"\n    package=\"{package}\"\n    android:versionCode=\"1\"\n    android:versionName=\"{version_name}\">\n    <uses-sdk android:minSdkVersion=\"{}\" android:targetSdkVersion=\"{}\" />\n    <uses-feature android:glEsVersion=\"0x00030000\" android:required=\"true\" />\n    <application android:label=\"{label}\"\n        android:hasCode=\"true\"\n        android:extractNativeLibs=\"true\">\n        <activity android:name=\"android.app.NativeActivity\"\n            android:exported=\"true\"\n            android:theme=\"@style/OppaBoot\"\n            android:configChanges=\"orientation|keyboardHidden|screenSize\">\n            <meta-data android:name=\"android.app.lib_name\" android:value=\"{lib_name}\" />\n            <intent-filter>\n                <action android:name=\"android.intent.action.MAIN\" />\n                <category android:name=\"android.intent.category.LAUNCHER\" />\n            </intent-filter>\n        </activity>\n    </application>\n</manifest>\n",
        ANDROID_MIN_SDK, ANDROID_TARGET_SDK
    )
}

/// Renders one java stub (package + lib substituted — asserts no
/// tokens survive, so a template edit without substitution fails
/// here, never as a broken class on device).
pub fn render_java(
    template: &str,
    package: &str,
    lib: &str,
    class: &str,
) -> Result<String, String> {
    if !template.contains("__PACKAGE__") {
        return Err(format!(
            "java template for {class} lost its __PACKAGE__ token"
        ));
    }
    let mut out = template.replace("__PACKAGE__", package);
    if template.contains("__LIB__") {
        if lib.is_empty() {
            return Err(format!("java template for {class} needs a lib name"));
        }
        out = out.replace("__LIB__", lib);
    }
    if out.contains("__PACKAGE__") || out.contains("__LIB__") {
        return Err(format!("unsubstituted tokens in {class}"));
    }
    Ok(out)
}

/// Generates the Android shell at build time: the manifest plus
/// the two java stubs under the package path (fully derived from
/// the manifest package and `android_main!` — always synced,
/// byte-compared, never hand-edited).
pub fn sync_android_shell(dir: &Path) -> Result<(String, String), String> {
    let manifest_text = std::fs::read_to_string(dir.join("Cargo.toml"))
        .map_err(|e| format!("cannot read {}/Cargo.toml: {e}", dir.display()))?;
    let name = package_name_from_manifest(&manifest_text)?;
    let version = manifest_text
        .lines()
        .map(str::trim)
        .find_map(|line| {
            line.strip_prefix("version")
                .and_then(|v| v.split_once('=').map(|(_, v)| v.trim().trim_matches('"')))
        })
        .filter(|v| !v.is_empty())
        .unwrap_or("0.1.0")
        .to_string();
    let snake = snake_name(&name);
    let lib_rs = std::fs::read_to_string(dir.join("src").join("lib.rs")).map_err(|_| {
        format!(
            "no src/lib.rs in {} — only lib crates build Android",
            dir.display()
        )
    })?;
    let (_, package) = parse_android_main(&lib_rs)?;
    let android = dir.join("android");
    // Framework-derived files always sync (byte-compared — the
    // manifest and java stubs carry no app content beyond the
    // package + lib names, so a renderer fix lands on the next
    // build instead of rotting in a stale copy).
    let manifest_path = android.join("AndroidManifest.xml");
    write_cached(
        &manifest_path,
        render_android_manifest(&package, &name, &snake, &version).as_bytes(),
        "manifest",
    )?;
    let java_dir = android.join("java").join(package.replace('.', "/"));
    for (class, template) in [("OppaUi", ANDROID_JAVA_UI), ("OppaIme", ANDROID_JAVA_IME)] {
        let rendered = render_java(template, &package, &snake, class)?;
        write_cached(
            &java_dir.join(format!("{class}.java")),
            rendered.as_bytes(),
            "java",
        )?;
    }
    // Boot placeholder resources (static boot chrome — the window
    // paints this before the first frame).
    let res_values = android.join("res").join("values");
    write_cached(
        &res_values.join("colors.xml"),
        ANDROID_RES_COLORS.as_bytes(),
        "res",
    )?;
    write_cached(
        &res_values.join("themes.xml"),
        ANDROID_RES_THEMES.as_bytes(),
        "res",
    )?;
    Ok((package, snake))
}

/// Tool file inside an SDK dir (`build-tools/<ver>/<tool>` —
/// highest version wins; absence refuses loudly with the install
/// pointer).
pub fn sdk_tool(sdk: &Path, tool: &str) -> Result<PathBuf, String> {
    let dir = sdk.join("build-tools");
    let mut versions: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|_| {
            format!(
                "no build-tools in {} — install Android build-tools",
                sdk.display()
            )
        })?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .collect();
    versions.sort();
    let version = versions.pop().ok_or_else(|| {
        format!(
            "no build-tools in {} — install Android build-tools",
            sdk.display()
        )
    })?;
    // `d8`/`apksigner` ship as `.bat` launchers on Windows
    // (CreateProcess cannot spawn them directly — `run_sdk_tool`
    // routes through `cmd /C`); prefer `.exe` where both exist.
    for candidate in [
        dir.join(&version).join(format!("{tool}.exe")),
        dir.join(&version).join(tool),
        dir.join(&version).join(format!("{tool}.bat")),
    ] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "no {tool} in build-tools/{version} — reinstall Android build-tools"
    ))
}

/// Runs a `.bat`-shipped SDK tool on Windows through `cmd /C`
/// (direct spawn cannot execute batch files); elsewhere (and for
/// real exes) spawns directly.
pub fn run_sdk_tool(tool: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    let is_bat = tool
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("bat"));
    let mut cmd = if is_bat {
        let mut cmd = std::process::Command::new("cmd");
        cmd.arg("/C");
        let mut full = tool.display().to_string();
        for arg in args {
            full.push(' ');
            full.push_str(arg);
        }
        cmd.arg(full);
        cmd
    } else {
        let mut cmd = std::process::Command::new(tool);
        cmd.args(args);
        cmd
    };
    cmd.output()
        .map_err(|e| format!("cannot run {}: {e}", tool.display()))
}

/// JDK `javac`/`keytool` (JAVA_HOME first, then PATH — absence
/// refuses loudly with the install pointer).
pub fn jdk_tool(name: &str) -> Result<PathBuf, String> {
    let exe = if std::env::consts::OS == "windows" {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let path = PathBuf::from(java_home).join("bin").join(&exe);
        if path.is_file() {
            return Ok(path);
        }
    }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join(&exe))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("no {name} found (JAVA_HOME nor PATH) — install a JDK (17+)"))
}

/// `cargo oppa build`: artifacts per target, no launching.
/// Desktop builds the host binary; web builds the bindgen bundle
/// into `web/pkg`; Android assembles a signed release APK
/// (`assemble_android_apk`).
pub fn build_app(dir: &Path, target: Target, dev: bool) -> Result<(), String> {
    match target {
        Target::Desktop => {
            validate_desktop_layout(dir)?;
            let mut build = std::process::Command::new("cargo");
            build.arg("build").current_dir(dir);
            if !dev {
                build.arg("--release");
            }
            let status = build
                .status()
                .map_err(|e| format!("cannot run cargo build: {e}"))?;
            if !status.success() {
                return Err("desktop build failed (see cargo output above)".to_string());
            }
            println!("oppa build: desktop ok ({})", dir.display());
            Ok(())
        }
        Target::Web => {
            build_web_app(dir, dev)?;
            println!(
                "oppa build: web ok ({})",
                dir.join("web").join("pkg").display()
            );
            Ok(())
        }
        Target::Android => {
            let apk = assemble_android_apk(dir, dev)?;
            println!("oppa build: android apk ok ({})", apk.display());
            Ok(())
        }
    }
}

/// `cargo oppa run`: `build` plus launch per target. Desktop runs
/// the host binary (blocks); web serves `web/` and opens the
/// browser (blocks until Ctrl-C); Android assembles the APK and
/// then needs a device — without one it refuses loudly naming the
/// APK with the manual install lines (with one attached,
/// install/launch stays manual until a device proves it).
pub fn run_app(
    dir: &Path,
    target: Target,
    port: u16,
    dev: bool,
    no_browser: bool,
) -> Result<(), String> {
    match target {
        Target::Desktop => {
            validate_desktop_layout(dir)?;
            let mut run = std::process::Command::new("cargo");
            run.arg("run").current_dir(dir);
            if !dev {
                run.arg("--release");
            }
            let status = run
                .status()
                .map_err(|e| format!("cannot run cargo run: {e}"))?;
            if !status.success() {
                return Err("desktop run failed (see output above)".to_string());
            }
            Ok(())
        }
        Target::Web => serve_web(&ServeOptions {
            dir: dir.to_path_buf(),
            port,
            dev,
            no_browser,
        }),
        Target::Android => {
            let apk = assemble_android_apk(dir, dev)?;
            let apk_arg = apk.display().to_string();
            match adb_devices() {
                Ok(serials) if !serials.is_empty() => Err(format!(
                    "device(s) attached ({}) but install/launch is not implemented — install by hand: adb install -r {}",
                    serials.join(", "),
                    apk_arg
                )),
                Ok(_) => Err(format!(
                    "no device attached — install by hand when one is: adb install -r {apk_arg} (built {apk_arg})"
                )),
                Err(why) => Err(format!(
                    "{why} — install by hand when adb sees a device: adb install -r {apk_arg} (built {apk_arg})"
                )),
            }
        }
    }
}

/// Builds the wasm, runs wasm-bindgen, serves `web/`, opens the
/// browser: the web equivalent of desktop `cargo run`. Blocks until
/// Ctrl-C (one app per invocation — a second serve refuses the busy
/// port loudly, never steals it).
pub fn serve_web(opts: &ServeOptions) -> Result<(), String> {
    build_web_app(&opts.dir, opts.dev)?;
    // Serve.
    let root = opts.dir.join("web");
    let listener = std::net::TcpListener::bind(("127.0.0.1", opts.port)).map_err(|e| {
        format!(
            "cannot serve on 127.0.0.1:{} ({e}) — pick another with --port",
            opts.port
        )
    })?;
    let url = format!("http://127.0.0.1:{}/index.html", opts.port);
    println!("oppa serve: {url} (Ctrl-C to stop)");
    if !opts.no_browser {
        open_browser(&url);
    }
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let root = root.clone();
                std::thread::spawn(move || {
                    if let Err(e) = serve_connection(&mut stream, &root) {
                        eprintln!("oppa serve: {e}");
                    }
                });
            }
            Err(e) => eprintln!("oppa serve: accept failed: {e}"),
        }
    }
    Ok(())
}

/// Opens the URL in the system browser (best-effort — the URL
/// prints regardless, so a weird platform never blocks serving).
fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .status();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).status();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).status();
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let _ = url;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("cargo-oppa-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn names_validate_like_cargo() {
        assert_eq!(validate_name("my-app").expect("ok"), "my-app");
        assert_eq!(validate_name("my_app2").expect("ok"), "my_app2");
        assert!(validate_name("").is_err());
        assert!(validate_name("My-App").is_err());
        assert!(validate_name("9lives").is_err());
        assert!(validate_name("-dash").is_err());
        assert!(validate_name("white space").is_err());
        assert_eq!(snake_name("my-app"), "my_app");
    }

    #[test]
    fn manifest_renders_names_and_absolute_deps() {
        let checkout = PathBuf::from("/x/oppa");
        let desktop = render_manifest(Template::Desktop, "my-app", &checkout);
        assert!(desktop.contains("name = \"my-app\""), "{desktop}");
        assert!(
            desktop.contains("path = \"/x/oppa/crates/oppa-app\""),
            "{desktop}"
        );
        assert!(!desktop.contains("../.."), "no relative climbs: {desktop}");
        let web = render_manifest(Template::Web, "my-web", &checkout);
        assert!(web.contains("name = \"my-web\""), "{web}");
        assert!(
            web.contains(&format!("wasm-bindgen = \"{WEB_BINDGEN_PIN}\"")),
            "web pins the CLI-matching bindgen: {web}"
        );
        assert!(!web.contains("oppa-app"), "{web}");
    }

    #[test]
    fn checkout_validation_names_the_missing_manifest() {
        let err = validate_checkout(Path::new("/no/such/checkout")).unwrap_err();
        assert!(err.contains("crates/oppa/Cargo.toml"), "{err}");
    }

    /// Phase 37c: the embedded lists mirror the templates (new
    /// template files fail here until embedded — misses are loud).
    #[test]
    fn embedded_lists_mirror_templates() {
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = here
            .parent()
            .and_then(|p| p.parent())
            .expect("checkout root");
        for (source, template) in TEMPLATE_SOURCES {
            let dir = root.join("templates").join(source);
            let mut on_disk = Vec::new();
            collect_template_files(&dir, &dir, &mut on_disk);
            on_disk.sort();
            let embedded: Vec<String> = match template {
                Template::Desktop => DESKTOP_FILES.iter().map(|f| f.path.to_string()).collect(),
                Template::Web => WEB_FILES.iter().map(|f| f.path.to_string()).collect(),
            };
            let mut embedded_sorted = embedded.clone();
            embedded_sorted.sort();
            assert_eq!(
                on_disk, embedded_sorted,
                "embedded list must mirror templates/{source} (minus target/ + Cargo.lock)"
            );
        }
    }

    fn collect_template_files(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let entries = std::fs::read_dir(dir).expect("template dir reads");
        for entry in entries {
            let entry = entry.expect("template entry reads");
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "target" || name == "Cargo.lock" {
                continue;
            }
            if path.is_dir() {
                collect_template_files(base, &path, out);
            } else {
                out.push(
                    path.strip_prefix(base)
                        .expect("template prefix strips")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    #[test]
    fn rendered_scaffold_has_no_copy_out_or_relative_climbs() {
        for (template, files) in [
            (Template::Desktop, DESKTOP_FILES),
            (Template::Web, WEB_FILES),
        ] {
            for file in files {
                let (rel, bytes) = render_file(template, file, "demo-app", Path::new("/x/oppa"));
                let text = String::from_utf8(bytes).expect("utf8 templates");
                assert!(
                    !text.contains("cp -r templates/"),
                    "{rel} still tells authors to copy it out"
                );
                assert!(
                    !text.contains("../../crates"),
                    "{rel} still climbs relatively"
                );
            }
        }
    }

    /// Phase 37c: end-to-end scaffold into a temp dir (refuses the
    /// second run — destinations never merge).
    #[test]
    fn new_project_scaffolds_and_refuses_nonempty() {
        let root = temp_root("scaffold");
        let dest = root.join("demo-app");
        let checkout = compile_time_checkout();
        let written =
            new_project(&dest, Template::Desktop, "demo-app", &checkout).expect("scaffolds");
        assert_eq!(written.len(), DESKTOP_FILES.len());
        let manifest = std::fs::read_to_string(dest.join("Cargo.toml")).expect("manifest reads");
        assert!(manifest.contains("name = \"demo-app\""), "{manifest}");
        assert!(
            manifest.contains(&checkout.display().to_string().replace('\\', "/")),
            "deps point at the checkout: {manifest}"
        );
        assert!(dest.join("src/main.rs").is_file());
        // Second run refuses (never merge into existing work).
        let err = new_project(&dest, Template::Desktop, "demo-app", &checkout).unwrap_err();
        assert!(err.contains("not empty"), "{err}");
        // Web scaffolds its shell too (pkg stem renamed in the
        // inline boot call).
        let web_dest = root.join("demo-web");
        new_project(&web_dest, Template::Web, "demo-web", &checkout).expect("web scaffolds");
        let index = std::fs::read_to_string(web_dest.join("web/index.html")).expect("index reads");
        assert!(
            index.contains("./pkg/demo_web.js"),
            "wasm stem follows the package: {index}"
        );
        assert!(
            web_dest.join("web/oppa-bootstrap.js").is_file(),
            "shared bootstrap scaffolds"
        );
        assert!(
            !web_dest.join("web/bootstrap.js").exists(),
            "no per-app bootstrap copy"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn serve_content_types_name_the_shell() {
        assert_eq!(
            content_type_for("index.html").expect("html"),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            content_type_for("pkg/demo_web.js").expect("js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type_for("pkg/demo_web_bg.wasm").expect("wasm"),
            "application/wasm"
        );
        assert_eq!(
            content_type_for("manifest.json").expect("json"),
            "application/json"
        );
        assert!(content_type_for("pkg/blob.exe").is_err());
    }

    #[test]
    fn serve_versions_parse_and_match() {
        assert_eq!(
            parse_bindgen_cli_version("wasm-bindgen 0.2.128").expect("cli"),
            "0.2.128"
        );
        assert!(parse_bindgen_cli_version("wasm-bindgen").is_err());
        let lock = "[[package]]\nname = \"wasm-bindgen\"\nversion = \"0.2.128\"\n";
        assert_eq!(lockfile_bindgen_version(lock).expect("lock"), "0.2.128");
        assert!(lockfile_bindgen_version("[[package]]\nname = \"other\"\n").is_err());
    }

    #[test]
    fn serve_names_parse_from_manifests() {
        let manifest = "[package]\nname = \"demo-web\"\nversion = \"0.1.0\"\n\n[dependencies]\nname = \"nope\"\n";
        assert_eq!(
            package_name_from_manifest(manifest).expect("name"),
            "demo-web"
        );
        assert!(package_name_from_manifest("[dependencies]\n").is_err());
    }

    #[test]
    fn serve_requests_resolve_inside_web() {
        let root = temp_root("serve-resolve");
        let web = root.join("web");
        std::fs::create_dir_all(&web).expect("web dir");
        std::fs::write(web.join("index.html"), "<html></html>").expect("index");
        std::fs::write(web.join("app.js"), "x").expect("js");
        assert_eq!(
            resolve_request(&web, "/").expect("slash"),
            web.join("index.html")
        );
        assert_eq!(
            resolve_request(&web, "/app.js?v=2").expect("query strips"),
            web.join("app.js")
        );
        assert!(resolve_request(&web, "/../Cargo.toml").is_err());
        assert!(resolve_request(&web, "/missing.html").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn serve_json_fields_decode_escapes() {
        let meta = "{\"target_directory\":\"C:\\\\Users\\\\me\\\\app\\\\target\",\"workspace_root\":\"C:\\\\x\"}";
        assert_eq!(
            json_string_field(meta, "\"target_directory\"").expect("target"),
            "C:\\Users\\me\\app\\target"
        );
        assert_eq!(
            json_string_field(meta, "\"workspace_root\"").expect("root"),
            "C:\\x"
        );
        assert!(json_string_field(meta, "\"missing\"").is_none());
        assert!(json_string_field("{\"a\":\"\\u1234\"}", "\"a\"").is_none());
    }

    #[test]
    fn serve_cargo_paths_name_this_workspace() {
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let (target, root) = cargo_paths(&here).expect("metadata reads");
        assert!(target.ends_with("target"), "{target:?}");
        assert!(root.join("Cargo.toml").is_file(), "{root:?}");
    }

    #[test]
    fn android_rows_name_abis_linkers_and_adb_shapes() {
        assert_eq!(ANDROID_ABIS.len(), 2);
        assert_eq!(ANDROID_ABIS[0].1, "arm64-v8a");
        assert_eq!(ANDROID_ABIS[1].1, "x86_64");
        let ndk = PathBuf::from("/sdk/ndk/29.0.14206865");
        let linker = ndk_linker(&ndk, "aarch64-linux-android").unwrap_err();
        assert!(linker.contains("aarch64-linux-android35-clang"), "{linker}");
        assert!(find_ndk_in(&PathBuf::from("/no/sdk")).is_err());
        let devices = "List of devices attached\nemulator-5554\tdevice product:sdk model:emu\nXX\tunauthorized\n\n";
        assert_eq!(
            parse_adb_devices(devices),
            vec!["emulator-5554".to_string()]
        );
        assert!(parse_adb_devices("List of devices attached\n\n").is_empty());
    }

    #[test]
    fn embedded_bootstrap_matches_sources() {
        // The embedded rig, the template copy, and the canonical
        // file are one fact in three places (a fix in any one fails
        // here until all three move).
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = here
            .parent()
            .and_then(|p| p.parent())
            .expect("checkout root");
        let canonical = std::fs::read_to_string(root.join("crates/oppa-web/web/oppa-bootstrap.js"))
            .expect("canonical bootstrap reads");
        assert_eq!(BOOTSTRAP_JS, canonical, "embedded rig matches canonical");
        let templated =
            std::fs::read_to_string(root.join("templates/hello-web/web/oppa-bootstrap.js"))
                .expect("template bootstrap reads");
        assert_eq!(templated, canonical, "template copy matches canonical");
    }

    #[test]
    fn export_app_parses_class_and_title() {
        let (class, title) =
            parse_export_app("use x;\nexport_app!(LoginWebApp, \"Login\", app::login_root);")
                .expect("parses");
        assert_eq!(class, "LoginWebApp");
        assert_eq!(title, "Login");
        assert!(parse_export_app("fn main() {}").is_err());
        assert!(parse_export_app("export_app!(NoTitle);").is_err());
        assert!(parse_export_app("export_app!(Bad-Name, \"T\", r);").is_err());
    }

    #[test]
    fn shell_sync_generates_caches_and_never_clobbers() {
        let root = temp_root("shell-sync");
        let dir = root.join("app");
        std::fs::create_dir_all(dir.join("src")).expect("src");
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"demo-web\"\n")
            .expect("manifest");
        std::fs::write(
            dir.join("src/lib.rs"),
            "export_app!(DemoWebApp, \"Demo\", app::root);",
        )
        .expect("lib");
        sync_web_shell(&dir).expect("generates");
        for rel in [
            "web/oppa-bootstrap.js",
            "web/index.html",
            "web/manifest.json",
            "web/sw.js",
        ] {
            assert!(dir.join(rel).is_file(), "{rel} generated");
        }
        let index = std::fs::read_to_string(dir.join("web/index.html")).expect("index reads");
        assert!(index.contains("./pkg/demo_web.js"), "{index}");
        assert!(index.contains("DemoWebApp"), "{index}");
        assert_eq!(
            std::fs::read_to_string(dir.join("web/oppa-bootstrap.js")).expect("rig reads"),
            BOOTSTRAP_JS,
            "rig is the canonical copy"
        );
        // Second sync is a clean no-op over generated files.
        sync_web_shell(&dir).expect("idempotent");
        // A tampered rig refreshes to canonical; a custom page stays.
        std::fs::write(dir.join("web/oppa-bootstrap.js"), "tampered").expect("tamper");
        std::fs::write(dir.join("web/index.html"), "<custom></custom>").expect("custom");
        assert!(sync_web_shell(&dir).is_err(), "stale index refuses");
        std::fs::write(
            dir.join("web/index.html"),
            "<div>custom ./pkg/demo_web.js</div>",
        )
        .expect("custom");
        sync_web_shell(&dir).expect("custom page stays");
        assert_eq!(
            std::fs::read_to_string(dir.join("web/oppa-bootstrap.js")).expect("rig reads"),
            BOOTSTRAP_JS,
            "tampered rig refreshed"
        );
        assert!(
            std::fs::read_to_string(dir.join("web/index.html"))
                .expect("index reads")
                .contains("custom"),
            "custom page untouched"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The web bindgen pin is one fact in three places: the
    /// scaffolder constant, the template manifest, and the
    /// workspace lockfile (a bump in any one fails here until all
    /// three move — fresh scaffolds resolve exactly the installed
    /// CLI, so `serve` never trips on skew).
    #[test]
    fn web_bindgen_pin_mirrors_template_and_lockfile() {
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = here
            .parent()
            .and_then(|p| p.parent())
            .expect("checkout root");
        let template = std::fs::read_to_string(root.join("templates/hello-web/Cargo.toml"))
            .expect("template manifest reads");
        assert!(
            template.contains(&format!("wasm-bindgen = \"{WEB_BINDGEN_PIN}\"")),
            "template pins {WEB_BINDGEN_PIN}: {template}"
        );
        let lock = std::fs::read_to_string(root.join("Cargo.lock")).expect("workspace lock reads");
        let pinned = WEB_BINDGEN_PIN.strip_prefix('=').expect("pin is exact");
        assert_eq!(
            lockfile_bindgen_version(&lock).expect("lock names bindgen"),
            pinned,
            "workspace lock matches the pin"
        );
    }

    #[test]
    fn targets_parse_exactly_once() {
        assert_eq!(Target::parse("--desktop").expect("d"), Target::Desktop);
        assert_eq!(Target::parse("--web").expect("w"), Target::Web);
        assert_eq!(Target::parse("--android").expect("a"), Target::Android);
        assert!(Target::parse("--webapp").is_err());
        assert!(Target::parse("web").is_err());
    }

    #[test]
    fn variants_parse_to_dev_flag() {
        assert!(parse_variant("debug").expect("debug"));
        assert!(!parse_variant("release").expect("release"));
        assert!(parse_variant("rel").is_err());
        assert!(parse_variant("Debug").is_err());
    }

    #[test]
    fn layouts_refuse_non_apps_loudly() {
        let root = temp_root("layout");
        assert!(validate_layout(&root.join("missing")).is_err());
        assert!(validate_desktop_layout(&root.join("missing")).is_err());
        assert!(validate_web_layout(&root.join("missing")).is_err());
        // Manifest without sources refuses.
        let empty = root.join("empty");
        std::fs::create_dir_all(&empty).expect("dir");
        std::fs::write(empty.join("Cargo.toml"), "[package]\n").expect("manifest");
        assert!(validate_layout(&empty).is_err());
        // Desktop needs main.rs; web needs lib.rs + the standard shell.
        let desk = root.join("desk");
        std::fs::create_dir_all(desk.join("src")).expect("src");
        std::fs::write(desk.join("Cargo.toml"), "[package]\n").expect("manifest");
        std::fs::write(desk.join("src/main.rs"), "fn main(){}").expect("main");
        validate_desktop_layout(&desk).expect("desktop ok");
        assert!(validate_web_layout(&desk).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn android_main_parses_root_and_package() {
        let (root, package) =
            parse_android_main("use x;\nandroid_main!(app::login_root, \"com.oppa.login\");")
                .expect("parses");
        assert_eq!(root, "app::login_root");
        assert_eq!(package, "com.oppa.login");
        assert!(parse_android_main("fn main() {}").is_err());
        assert!(parse_android_main("android_main!(only_root);").is_err());
        assert!(parse_android_main("android_main!(r, \"bad package!\");").is_err());
    }

    #[test]
    fn android_manifest_names_package_lib_and_sdk() {
        let xml = render_android_manifest("com.oppa.login", "login", "login", "0.1.0");
        assert!(xml.contains("package=\"com.oppa.login\""), "{xml}");
        assert!(
            xml.contains("android.app.lib_name\" android:value=\"login\""),
            "{xml}"
        );
        assert!(xml.contains("minSdkVersion=\"29\""), "{xml}");
        assert!(xml.contains("targetSdkVersion=\"36\""), "{xml}");
        assert!(xml.contains("android.intent.action.MAIN"), "{xml}");
        assert!(!xml.contains("OppaPickerActivity"), "{xml}");
    }

    #[test]
    fn android_java_substitutes_package_and_lib() {
        let ui =
            render_java(ANDROID_JAVA_UI, "com.oppa.login", "login", "OppaUi").expect("renders");
        assert!(ui.contains("package com.oppa.login;"), "{ui}");
        assert!(
            !ui.contains("__PACKAGE__") && !ui.contains("__LIB__"),
            "{ui}"
        );
        let ime =
            render_java(ANDROID_JAVA_IME, "com.oppa.login", "login", "OppaIme").expect("renders");
        assert!(ime.contains("package com.oppa.login;"), "{ime}");
        assert!(
            !ime.contains("loadLibrary(\""),
            "no loader call — the activity loader owns the .so, RegisterNatives binds directly"
        );
        assert!(render_java("no tokens here", "p", "l", "C").is_err());
    }

    #[test]
    fn embedded_java_matches_sources() {
        // Same one-fact rule as the bootstrap: the embedded java
        // matches the runner templates byte-for-byte (the proof's
        // Gradle copies evolve separately — device-pending paths
        // the runner had to leave behind stay documented there).
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = here
            .parent()
            .and_then(|p| p.parent())
            .expect("checkout root");
        for (embedded, file) in [
            (ANDROID_JAVA_UI, "OppaUi.java"),
            (ANDROID_JAVA_IME, "OppaIme.java"),
        ] {
            let canonical =
                std::fs::read_to_string(root.join("crates/oppa-android-runner/java").join(file))
                    .expect("runner java reads");
            assert_eq!(embedded, canonical, "{file} embedded matches");
        }
    }

    #[test]
    fn android_res_carries_boot_placeholder() {
        assert!(ANDROID_RES_COLORS.contains("oppa_boot"), "boot color");
        assert!(ANDROID_RES_THEMES.contains("OppaBoot"), "boot style");
        assert!(
            ANDROID_RES_THEMES.contains("android:windowBackground"),
            "placeholder hook"
        );
    }
}
