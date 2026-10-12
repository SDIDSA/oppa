//! Desktop entry (Windows + Linux, same source): the launch style
//! rides `--style=` (default `extended`), so one binary exercises
//! all four chromes. Run with `cargo run -p stylebook` or
//! `cargo run -p stylebook -- --style transparent`.

use oppa::WindowStyle;
use oppa_app::{run_desktop_with, WindowOptions};
use stylebook::{ChromeHooks, StylebookProps, StylebookRoot};

const STYLES: &str = "decorated|undecorated|transparent|extended";

/// Parses one `--style=` argument (pure — headless-testable):
/// `None` (flag absent) means `Extended`. Anything else is a loud
/// `Err` naming the valid spellings (never a silent default — an
/// unknown chrome must not boot looking like a choice).
fn parse_style_arg(arg: Option<&str>) -> Result<WindowStyle, String> {
    match arg {
        None => Ok(WindowStyle::Extended),
        Some("decorated") => Ok(WindowStyle::Decorated),
        Some("undecorated") => Ok(WindowStyle::Undecorated),
        Some("transparent") => Ok(WindowStyle::Transparent),
        Some("extended") => Ok(WindowStyle::Extended),
        Some(other) => Err(format!(
            "stylebook: unknown style {other:?} (expected {STYLES})"
        )),
    }
}

fn main() {
    let mut style_arg: Option<String> = None;
    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--style=") {
            style_arg = Some(value.to_string());
        } else if arg == "--help" || arg == "-h" {
            println!("stylebook: cargo run -p stylebook -- --style=<{STYLES}> (default extended)");
            return;
        } else {
            eprintln!("stylebook: unknown argument {arg:?} (expected --style=<{STYLES}>)");
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
    let hooks = ChromeHooks::default();
    let props = StylebookProps {
        style,
        hooks: hooks.clone(),
    };
    let title = format!("Stylebook — {}", stylebook::style_title(style));
    if let Err(e) = run_desktop_with(
        WindowOptions::new(&title, 640, 440).with_window_style(style),
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
