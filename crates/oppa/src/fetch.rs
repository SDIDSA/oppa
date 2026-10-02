//! Blessed fetch→render (G7 — decisions 220–221).
//!
//! The executor (`ctx.spawn`, generation-tagged submits, INPUT drain)
//! existed, but only as tribal knowledge in a test comment
//! (`reload_cycle.rs`: "tasks cannot capture signals (`!Send`) —
//! rendezvous through `keyed_state`"). This module turns that into
//! the blessed app-level async pattern: one state shape plus one
//! driver, shared by native and web.
//!
//! Design (see decisions 220–221; rationale in git history):
//!
//! - **One state shape.** [`FetchState<T>`] (`Idle/Loading/Ready/
//!   Failed`) lives in a keyed signal ([`Ctx::fetch_state`]) —
//!   components render it like any other state (match + VNode).
//! - **Native driver.** [`Ctx::spawn_fetch`] sets `Loading`
//!   synchronously, runs `fetch` on the executor thread, and submits
//!   `Ready/Failed` through the `keyed_state` rendezvous (the only
//!   `Send`-safe path to UI state — signals never cross threads,
//!   locked by doctest). Generation tags ride along, so a result
//!   landing after a swap is discarded, never applied half-swapped
//!   (§9.6, proven by M9). [`Ctx::spawn_fetch_with_retry`] (Round
//!   13.1) adds a retry budget: up to N total tries in one task run,
//!   exhaustion surfacing a distinct `Failed("<last> (retry budget
//!   exhausted after N attempts)")`.
//! - **Preparation stages.** Every submit — task or fetch — walks
//!   `Queued → Prepared → Ready → Done` (Round 13.1, decision 308):
//!   [`Ctx::prepare_task`] parks bodies with dependencies until
//!   every dep id reads `Done`, then promotes and schedules on the
//!   single worker thread (FIFO, id-ordered promotion — the walk is
//!   total, therefore testable). Deps gate order, never success.
//! - **Wasm driver.** No threads on wasm — `spawn_task`/`spawn_fetch`
//!   refuse loudly there (explicit panic, not the cryptic OS stub).
//!   The platform binding resolves the promise and writes the *same*
//!   keyed signal from the UI thread, then requests a frame: same
//!   shape, mechanism-only difference (the verdict-(b) split applied
//!   to async).
//! - **Key namespacing.** Keyed state is one global u64 namespace per
//!   runtime — [`fetch_key`] hashes a `"route:name"` string (FNV-1a,
//!   the handler-id hash), so two features never collide silently.
//!
//! Out of scope: network backends themselves (OQ-G5-5 owns the socket;
//! [`Fetcher`] is the seam — scripted doubles, app closures, and the
//! platform binding meet here, never a built-in client); progress
//! reporting (OQ-G7-3); a reload-harness example (G14 owns app/reload
//! wiring).
//!
//! Phase 37b (decision 362 — G16) closes the two named gaps:
//! [`TaskId`] cancellation (parked/queued tasks never run — the
//! generation-discard rule already covered retired results; explicit
//! cancel covers live ones) and the pluggable [`Fetcher`] trait.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::hash::fnv1a64;

/// Remote-data lifecycle for one fetch (clone it in/out of its keyed
/// signal — the stack never schedules by itself).
#[derive(Clone, Debug, PartialEq)]
pub enum FetchState<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

/// Namespaces a fetch key: `fetch_key("settings:avatar")` (FNV-1a over
/// the string — deterministic across runs, same hash as handler ids).
pub fn fetch_key(name: &str) -> u64 {
    fnv1a64(name.as_bytes())
}

/// Page-state key for a paged collection load (Round 13.3,
/// decision 310): `FetchState<Vec<T>>` lives here (Loading →
/// Ready(page rows) / Failed), so each page tracks independently
/// while rows stream into the shared collection.
pub fn page_key(collection_key: u64, page: usize) -> u64 {
    fetch_key(&format!("page:{collection_key}:{page}"))
}

/// Load-generation key for a paged load (Round 13.3): a keyed u64
/// bumped per load; stale applies discard against it (the
/// start/resolve guard shape, per page instead of per field).
pub fn page_gen_key(collection_key: u64, page: usize) -> u64 {
    fetch_key(&format!("page-gen:{collection_key}:{page}"))
}

/// Pluggable fetch backend (Phase 37b, decision 362 — G16): the seam
/// OQ-G5-5 left open. `fetch` runs on the executor thread (blocking
/// is expected — never touch reactive state here; return the bytes
/// or the reason). Implementations: [`ScriptedFetcher`] (test
/// doubles), app closures via [`ClosureFetcher`] (the native hook —
/// wrap any blocking client), and the wasm platform binding (which
/// resolves promises outside threads and writes the keyed signal
/// directly — the G7 verdict-(b) split, not this trait).
pub trait Fetcher: Send + Sync + 'static {
    /// Human name for diagnostics (the decision-220 instrument —
    /// counted, never inferred, so backends name themselves).
    fn name(&self) -> &'static str;
    /// Fetches `url` off-thread: `Ok(body)` or `Err(reason)`.
    fn fetch(&self, url: &str) -> Result<String, String>;
}

/// Scripted fetch double (Phase 37b): route table for tests and
/// previews — exact-URL match, `Err("unscripted url: {url}")` for
/// misses (loud, never a silent empty). Clone shares the table
/// (scripts stage mid-test through the shared handle).
#[derive(Clone, Default)]
pub struct ScriptedFetcher {
    routes: Arc<Mutex<HashMap<String, Result<String, String>>>>,
}

impl ScriptedFetcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Scripts one URL (replaces any previous script for it — last
    /// wins, stated).
    pub fn script(&self, url: &str, result: Result<String, String>) {
        self.routes
            .lock()
            .expect("script table lock")
            .insert(url.to_string(), result);
    }
}

impl Fetcher for ScriptedFetcher {
    fn name(&self) -> &'static str {
        "scripted"
    }

    fn fetch(&self, url: &str) -> Result<String, String> {
        self.routes
            .lock()
            .expect("script table lock")
            .get(url)
            .cloned()
            .unwrap_or_else(|| Err(format!("unscripted url: {url}")))
    }
}

/// Closure fetch backend (Phase 37b — the native hook): wraps any
/// `Fn(&str) -> Result<String, String> + Send + Sync` (a blocking
/// HTTP client, a file reader, a test closure) as a [`Fetcher`].
pub struct ClosureFetcher<F>(pub &'static str, pub F)
where
    F: Fn(&str) -> Result<String, String> + Send + Sync + 'static;

impl<F> Fetcher for ClosureFetcher<F>
where
    F: Fn(&str) -> Result<String, String> + Send + Sync + 'static,
{
    fn name(&self) -> &'static str {
        self.0
    }

    fn fetch(&self, url: &str) -> Result<String, String> {
        (self.1)(url)
    }
}

#[cfg(test)]
mod tests;
