//! `cargo-oppa` — the `cargo oppa` plugin binary (thin CLI over
//! the [`cargo_oppa`] library: argument parsing, loud usage errors,
//! and the next-steps report).

use cargo_oppa::{
    compile_time_checkout, new_project, next_steps, serve_web, validate_checkout, ServeOptions,
    Template,
};
use std::path::PathBuf;

fn usage() -> String {
    "\
cargo-oppa: verify, build, and launch Oppa apps per target

USAGE:
    cargo oppa new <dir> [--desktop|--web] [--oppa-path <checkout>]
    cargo oppa check [dir]
    cargo oppa build [dir] --desktop|--web|--android [--dev]
    cargo oppa run [dir] --desktop|--web|--android [--port <n>] [--dev] [--no-browser]
    cargo oppa serve [dir] [--port <n>] [--dev] [--no-browser]

    check       verify an app dir (layout + host, wasm, Android checks)
    build       artifacts per target, no launching (Android: signed
                release APK into android/)
    run         build plus launch per target (exactly one target flag;
                Android needs a device or refuses loudly naming the
                APK with the manual install lines)
    serve       the web express lane: exactly `run --web`

    --desktop   scaffold templates/hello-desktop (default);
                `build`/`run` target flag alongside --web/--android
    --web       scaffold templates/hello-web; `build`/`run` target
    --android   `build`/`run` target (signed release APK into
                android/; install/launch needs a device or refuses
                loudly naming the APK)
    --oppa-path point path deps at this checkout (default: the checkout
                this tool built from)
    --port      loopback port for web run/serve (default 8932)
    --dev       dev profile, fast builds (exactly `--variant debug`)
    --variant   debug|release (default release: optimized Rust,
                `--debug-mode` off, stripped native code; debug:
                unoptimized, debuggable, full symbols)
    --no-browser
                print the URL without opening the browser (web-only)

EXAMPLES:
    cargo oppa new ~/my-app
    cargo oppa new ~/my-web-app --web
    cargo oppa check ~/my-app
    cargo oppa build ~/my-app --desktop
    cargo oppa run ~/my-app --desktop
    cargo oppa run ~/my-web-app --web
    cargo oppa serve"
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
        if rest == "serve" {
            return run_serve(&argv[1..]);
        }
        if rest == "check" {
            return run_check(&argv[1..]);
        }
        if rest == "build" {
            return run_build(&argv[1..]);
        }
        if rest == "run" {
            return run_run(&argv[1..]);
        }
        return Err(if args.len() <= 1 {
            "no subcommand — try `new`, `check`, `build`, `run`, or `serve`".to_string()
        } else {
            format!("unknown subcommand {rest:?} — try `new`, `check`, `build`, `run`, or `serve`")
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

/// `cargo oppa serve [dir] [--port <n>] [--dev|--variant <v>] [--no-browser]`.
/// The web express lane: exactly `run --web`.
fn run_serve(argv: &[String]) -> Result<(), String> {
    let mut dir = PathBuf::from(".");
    let mut dir_set = false;
    let mut port: u16 = 8932;
    let mut dev = false;
    let mut dev_flag = false;
    let mut variant: Option<bool> = None;
    let mut no_browser = false;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--port" => {
                i += 1;
                let raw = argv
                    .get(i)
                    .ok_or_else(|| "--port needs a value".to_string())?;
                port = raw
                    .parse::<u16>()
                    .map_err(|_| format!("--port {raw:?} is not a port (1-65535)"))?;
            }
            "--dev" => {
                dev_flag = true;
                dev = true;
            }
            "--variant" => {
                i += 1;
                let raw = argv
                    .get(i)
                    .ok_or_else(|| "--variant needs a value (debug|release)".to_string())?;
                let v = cargo_oppa::parse_variant(raw)?;
                if variant.is_some() {
                    return Err("--variant passed twice — pass exactly one variant".to_string());
                }
                variant = Some(v);
                dev = v;
            }
            "--no-browser" => no_browser = true,
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag {flag:?}"));
            }
            positional => {
                if dir_set {
                    return Err(format!("unexpected argument {positional:?}"));
                }
                dir_set = true;
                dir = PathBuf::from(positional);
            }
        }
        i += 1;
    }
    if dev_flag && variant == Some(false) {
        return Err("--dev conflicts with --variant release".to_string());
    }
    serve_web(&ServeOptions {
        dir,
        port,
        dev,
        no_browser,
    })
}

/// `cargo oppa check [dir]`.
fn run_check(argv: &[String]) -> Result<(), String> {
    let mut dir = PathBuf::from(".");
    let mut dir_set = false;
    for arg in argv {
        if arg.starts_with('-') {
            return Err(format!("unknown flag {arg:?}"));
        }
        if dir_set {
            return Err(format!("unexpected argument {arg:?}"));
        }
        dir_set = true;
        dir = PathBuf::from(arg);
    }
    cargo_oppa::check_app(&dir)
}

/// Shared `[dir] --target [--dev|--variant] [--port] [--no-browser]`
/// parse for `build` and `run` (`--port` / `--no-browser` are
/// web-only and refuse loudly elsewhere; `--dev` is exactly
/// `--variant debug`, and contradicting the two refuses loudly).
struct LaunchArgs {
    dir: PathBuf,
    target: cargo_oppa::Target,
    port: u16,
    dev: bool,
    no_browser: bool,
}

fn parse_launch(argv: &[String]) -> Result<LaunchArgs, String> {
    let mut dir = PathBuf::from(".");
    let mut dir_set = false;
    let mut target: Option<cargo_oppa::Target> = None;
    let mut port: u16 = 8932;
    let mut dev = false;
    let mut dev_flag = false;
    let mut variant: Option<bool> = None;
    let mut no_browser = false;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            flag @ ("--desktop" | "--web" | "--android") => {
                if target.is_some() {
                    return Err(
                        "conflicting targets — pass exactly one of --desktop --web --android"
                            .to_string(),
                    );
                }
                target = Some(cargo_oppa::Target::parse(flag)?);
            }
            "--port" => {
                i += 1;
                let raw = argv
                    .get(i)
                    .ok_or_else(|| "--port needs a value".to_string())?;
                port = raw
                    .parse::<u16>()
                    .map_err(|_| format!("--port {raw:?} is not a port (1-65535)"))?;
            }
            "--dev" => {
                dev_flag = true;
                dev = true;
            }
            "--variant" => {
                i += 1;
                let raw = argv
                    .get(i)
                    .ok_or_else(|| "--variant needs a value (debug|release)".to_string())?;
                let v = cargo_oppa::parse_variant(raw)?;
                if variant.is_some() {
                    return Err("--variant passed twice — pass exactly one variant".to_string());
                }
                variant = Some(v);
                dev = v;
            }
            "--no-browser" => no_browser = true,
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag {flag:?}"));
            }
            positional => {
                if dir_set {
                    return Err(format!("unexpected argument {positional:?}"));
                }
                dir_set = true;
                dir = PathBuf::from(positional);
            }
        }
        i += 1;
    }
    let target = target
        .ok_or_else(|| "no target — pass exactly one of --desktop --web --android".to_string())?;
    if dev_flag && variant == Some(false) {
        return Err("--dev conflicts with --variant release".to_string());
    }
    Ok(LaunchArgs {
        dir,
        target,
        port,
        dev,
        no_browser,
    })
}

/// `cargo oppa build [dir] --desktop|--web|--android [--dev]`.
fn run_build(argv: &[String]) -> Result<(), String> {
    let args = parse_launch(argv)?;
    cargo_oppa::build_app(&args.dir, args.target, args.dev)
}

/// `cargo oppa run [dir] --desktop|--web|--android
/// [--port <n>] [--dev] [--no-browser]`.
fn run_run(argv: &[String]) -> Result<(), String> {
    let args = parse_launch(argv)?;
    if args.target != cargo_oppa::Target::Web {
        if argv.contains(&"--port".to_string()) {
            return Err("--port is web-only".to_string());
        }
        if argv.contains(&"--no-browser".to_string()) {
            return Err("--no-browser is web-only".to_string());
        }
    }
    cargo_oppa::run_app(&args.dir, args.target, args.port, args.dev, args.no_browser)
}
