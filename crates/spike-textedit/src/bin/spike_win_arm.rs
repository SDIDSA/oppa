//! The §9.2 spike's Windows-GPU arm runner (Windows-only rig; the body
//! lives in `_impl/` so the workspace Linux gate compiles this stub).
//! Emits:
//! - `spike/corpus.json` — the shared rig (strings + cluster tables + IME
//!   scenario scripts + editing-op suites) for the Web-DOM arm;
//! - `spike/results/windows.json` — this arm's raw results.
//!
//! Round 19.2 (workspace Linux gate): the windows-only body moved to
//! `_impl/` behind a `#[cfg(windows)]` module; non-Windows targets get a
//! loud note instead of a compile error. Behavior on Windows unchanged.

#[cfg(windows)]
mod imp {
    include!("_impl/spike_win_arm.rs");
}

#[cfg(windows)]
fn main() {
    imp::main();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("spike_win_arm: Windows-only spike rig (spike/)");
}
