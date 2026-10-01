//! M10 — the Android platform shell.
//!
//! Scope: the headless-testable contract core — input classification
//! into the **shared** [`InputEvent`](oppa::InputEvent) pipeline (the
//! same host router Win32 feeds, no per-platform fork), the lifecycle
//! state machine with the retained-graph-relevant rules, and the
//! restart-only reload path (locked #16). Everything here is std +
//! `oppa`; there is no NDK/JNI linkage in this crate.
//!
//! Deliberately NOT here (platform-track follow-up, needs NDK +
//! device): the `NativeActivity`/JNI glue that calls `push_event`,
//! the wgpu-Android surface creation, the platform `TextService`
//! slice, and Android's accessibility service (explicitly out of
//! this milestone's AT-SPI scope — see `oppa-atspi` docs). The JNI
//! layer will be a thin forwarder over [`AndroidShell::push_event`]
//! and [`AndroidLifecycle::transition`]; no classification logic
//! lives there, so this crate's tests cover the contract.
//!
//! Hot reload on Android is **restart-only** (locked #16, re-confirmed
//! M10 against current AOSP sepolicy: W^X `neverallow`s on app-home
//! execute still stand — see the M10 ROUNDS entry). This crate
//! therefore has **no dependency on `oppa-reload`** (no dylib path
//! exists here by construction); relaunch builds a fresh host, which
//! is the cold-start path, with no scheduler work.
//!
//! Round 3.1 adds the soft-keyboard intake half that stays
//! headless-provable: text commit/surrounding-delete events and
//! commands ([`events`]), the IME visibility policy + focused-
//! session helpers ([`shell`]), and the JNI text-entry queue
//! ([`ime_queue`] — the `extern` entry points in the app crate push
//! here). The JNI muscle (`showSoftInput`/`hideSoftInputFromWindow`
//! calls, native-method registration) lives in `oppa-android-app`
//! where `jni` + `AndroidApp` exist — this crate keeps no NDK/JNI
//! linkage.

pub mod app_dirs;
pub mod events;
pub mod ime_queue;
pub mod lifecycle;
pub mod shell;

pub use app_dirs::{dirs_record, resolve_app_dirs, validate_dirs, AppDirs, DirSource};
pub use events::{AndroidCmd, AndroidEvent, ShellError};
pub use ime_queue::{ImeTextItem, ImeTextQueue};
pub use lifecycle::{AndroidLifecycle, LifecycleError, LifecycleState};
pub use shell::{
    commit_text_to_focused, delete_surrounding_to_focused, field_event, AndroidShell, ImeRequest,
    ShellConfig,
};
