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
        path: "web/bootstrap.js",
        contents: include_str!("../../../templates/hello-web/web/bootstrap.js"),
    },
];

/// Template source names the embedded lists must mirror (the
/// completeness test walks these — `target/` and `Cargo.lock`
/// excluded, like the lists).
pub const TEMPLATE_SOURCES: &[(&str, Template)] = &[
    ("hello-desktop", Template::Desktop),
    ("hello-web", Template::Web),
];

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
            format!("oppa-web = {{ path = \"{root}/crates/oppa-web\" }}\nwasm-bindgen = \"0.2\"\n")
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
/// READMEs carry the full build recipes).
pub fn next_steps(template: Template) -> &'static str {
    match template {
        Template::Desktop => "cd into your project and `cargo run`",
        Template::Web => {
            "cd into your project, `cargo build --release --target wasm32-unknown-unknown`, then follow README.md (wasm-bindgen + serve)"
        }
    }
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
        assert!(web.contains("wasm-bindgen"), "{web}");
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
        // Web scaffolds its shell too (bootstrap stem renamed).
        let web_dest = root.join("demo-web");
        new_project(&web_dest, Template::Web, "demo-web", &checkout).expect("web scaffolds");
        let bootstrap =
            std::fs::read_to_string(web_dest.join("web/bootstrap.js")).expect("bootstrap reads");
        assert!(
            bootstrap.contains("./pkg/demo_web.js"),
            "wasm stem follows the package: {bootstrap}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
