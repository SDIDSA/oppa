//! The M1-remainder host: one editable single-line field in a REAL Win32
//! window with REAL OS IME wiring — the vehicle for the manual real-IME
//! pass (DESIGN §2.3, REPORT.md finding #5). Windows-only rig.
//!
//! Round 19.2 (workspace Linux gate): the windows-only body moved to
//! `_impl/` behind a `#[cfg(windows)]` module; non-Windows targets get a
//! loud note instead of a compile error. Behavior on Windows is unchanged
//! (the body's `#![cfg(windows)]` is inert inside the gated module).

#[cfg(windows)]
mod imp {
    include!("_impl/spike_ime_shell.rs");
}

#[cfg(windows)]
fn main() {
    let _ = imp::main();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("spike_ime_shell: Windows-only spike rig (docs/11-experiments/)");
}
