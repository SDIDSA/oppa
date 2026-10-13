//! Desktop entry (Windows + Linux, same source): the launch style
//! rides `--style=` (default `extended`), so one binary exercises
//! all four chromes; `--native-buttons` declares the native caption
//! cluster in `Extended` mode (default off — drag-only bar).
//! Run with `cargo run -p stylebook` or
//! `cargo run -p stylebook -- --style transparent`.

use oppa_app::{run_desktop_with, WindowOptions};
use stylebook::{
    parse_native_buttons_flag, parse_style_arg, ChromeHooks, StylebookProps, StylebookRoot,
};

const STYLES: &str = "decorated|undecorated|transparent|extended";

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "stylebook: cargo run -p stylebook -- --style=<{STYLES}> [--native-buttons] (defaults: extended, bridged)"
        );
        return;
    }
    let mut style_arg: Option<String> = None;
    let mut args = raw.iter().peekable();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--style=") {
            style_arg = Some(value.to_string());
        } else if arg == "--style" {
            // Space-separated form (`--style extended`): the value
            // must follow as the next argument (missing it is a loud
            // refusal, never a silent default).
            match args.next() {
                Some(value) => style_arg = Some(value.clone()),
                None => {
                    eprintln!("stylebook: --style needs a value (expected --style=<{STYLES}>)");
                    std::process::exit(2);
                }
            }
        } else if arg == "--native-buttons" {
            // Handled below through `parse_native_buttons_flag`.
        } else {
            eprintln!("stylebook: unknown argument {arg:?} (expected --style=<{STYLES}> [--native-buttons])");
            std::process::exit(2);
        }
    }
    let style = match parse_style_arg(style_arg.as_deref()) {
        Ok(style) => style,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    // Decision 485 opt-in demo: native buttons default off here so
    // the bare drag bar shows first; `--native-buttons` declares the
    // caption cluster (the OS answers first on Windows, the wired
    // actions bridge on Linux).
    let native = parse_native_buttons_flag(&raw);
    let hooks = ChromeHooks::default();
    let props = StylebookProps {
        style,
        native,
        hooks: hooks.clone(),
    };
    let title = format!("Stylebook — {}", stylebook::style_title(style));
    if let Err(e) = run_desktop_with(
        WindowOptions::new(&title, 640, 440)
            .with_window_style(style)
            .with_native_caption_buttons(native),
        props,
        StylebookRoot,
        |loop_| {
            // Caption bridge (the studio poll-hook precedent):
            // the HeaderBar buttons arm runner-visible signals and
            // the hook forwards them into the loop-installed
            // `WindowControl` (minimize/maximize) or the close veto
            // path (close) once per pump iteration.
            let poll_hooks = hooks.clone();
            loop_.set_poll_hook(Some(Box::new(move |loop_| {
                let Some((minimize, maximize, close)) = poll_hooks.get() else {
                    return;
                };
                if minimize.get() {
                    minimize.set(false);
                    loop_.minimize();
                }
                if maximize.get() {
                    maximize.set(false);
                    loop_.toggle_maximize();
                }
                if close.get() {
                    close.set(false);
                    loop_.host().request_close();
                }
            })));
        },
    ) {
        eprintln!("stylebook: FATAL: {e}");
        std::process::exit(1);
    }
}
