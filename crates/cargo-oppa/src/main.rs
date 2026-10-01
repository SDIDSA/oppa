//! `cargo-oppa` — the `cargo oppa` plugin binary (thin CLI over
//! the [`cargo_oppa`] library: argument parsing, loud usage errors,
//! and the next-steps report).

use cargo_oppa::{compile_time_checkout, new_project, next_steps, validate_checkout, Template};
use std::path::PathBuf;

fn usage() -> String {
    "\
cargo-oppa: the Oppa scaffolder

USAGE:
    cargo oppa new <dir> [--desktop|--web] [--oppa-path <checkout>]

    --desktop   scaffold templates/hello-desktop (default)
    --web       scaffold templates/hello-web
    --oppa-path point path deps at this checkout (default: the checkout
                this tool built from)

EXAMPLES:
    cargo oppa new ~/my-app
    cargo oppa new ~/my-web-app --web"
        .to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(()) => {}
        Err(message) => {
            eprintln!("cargo oppa: {message}\n\n{}", usage());
            std::process::exit(2);
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    // Invoked as `cargo oppa ...` (cargo passes `oppa` as argv[1])
    // or directly as `cargo-oppa ...` — accept both.
    let mut rest = args.get(1).map(String::as_str).unwrap_or_default();
    let mut argv: &[String] = &args[1..];
    if rest == "oppa" {
        argv = &args[2..];
        rest = argv.first().map(String::as_str).unwrap_or_default();
    }
    if rest != "new" {
        return Err(if args.len() <= 1 {
            "no subcommand — try `new`".to_string()
        } else {
            format!("unknown subcommand {rest:?} — try `new`")
        });
    }
    let mut dir: Option<String> = None;
    let mut template = Template::Desktop;
    let mut template_flag: Option<&str> = None;
    let mut oppa_path: Option<PathBuf> = None;
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            flag @ ("--desktop" | "--web") => {
                // Both flags together refuse loudly (ambiguous
                // template — never last-wins silently).
                if let Some(first) = template_flag {
                    return Err(format!(
                        "conflicting template flags {first:?} and {flag:?} — pass exactly one"
                    ));
                }
                template_flag = Some(flag);
                template = if flag == "--web" {
                    Template::Web
                } else {
                    Template::Desktop
                };
            }
            "--oppa-path" => {
                i += 1;
                oppa_path = Some(PathBuf::from(argv.get(i).ok_or_else(|| {
                    "--oppa-path needs a value (the Oppa checkout)".to_string()
                })?));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag {flag:?}"));
            }
            positional => {
                if dir.is_some() {
                    return Err(format!("unexpected argument {positional:?}"));
                }
                dir = Some(positional.to_string());
            }
        }
        i += 1;
    }
    let dir = dir.ok_or_else(|| "no directory — try `cargo oppa new <dir>`".to_string())?;
    let checkout = match oppa_path {
        Some(path) => validate_checkout(&path)?,
        None => {
            let implied = compile_time_checkout();
            validate_checkout(&implied)?
        }
    };
    // The package name is the directory's own name (cargo
    // convention — `new foo-bar` builds package `foo-bar`).
    let name = PathBuf::from(&dir)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("directory {dir:?} has no usable final component"))?
        .to_string();
    let dest = PathBuf::from(&dir);
    let written = new_project(&dest, template, &name, &checkout)?;
    println!(
        "cargo oppa: scaffolded {name} ({template:?}) at {}",
        dest.display()
    );
    for path in written {
        println!("  created {}", path.display());
    }
    println!("next: {}", next_steps(template));
    Ok(())
}
