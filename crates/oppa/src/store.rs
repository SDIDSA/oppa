//! App storage seams (G5 — decisions 216–217): key-value settings +
//! sandboxed files.
//!
//! The v2 handoff verified no fs/http/kv in core and no such deps
//! workspace-wide — every app hand-rolls settings/cache/sync with no
//! seam. This module is the seam: two sync-first traits, two
//! in-memory references, and one std-backed FS for native targets.
//!
//! Design (see decisions 216–217 in `docs/04-planning/state.md`):
//!
//! - **Sync-first (216).** Unlike clipboard reads (async on web —
//!   hence request/poll, decision 209), storage has a sync option on
//!   every platform: `localStorage` on web, `std::fs` on native. So
//!   both traits are plain sync `Result` — no poll machinery is
//!   repeated. Async-only backends (IndexedDB, OPFS) are deferred
//!   with the bridge sketched (OQ-G5-3), not silently dropped.
//! - **Bytes, flat keys.** [`KvStore`] values are `Vec<u8>` (backends
//!   encode for string-only stores — the wasm `localStorage` backend
//!   UTF-8-checks loudly); keys are flat non-empty strings (empty
//!   refused loudly; hierarchical separators are app convention, not
//!   framework syntax).
//! - **Lexical jail.** [`FsSandbox`] paths are relative, `..`-free,
//!   and joined under the sandbox root — absolute paths and `..`
//!   refuse loudly as `InvalidPath`. The jail is lexical (symlinks
//!   inside the sandbox can still point out — documented bound,
//!   OQ-G5-4), never a silent escape.
//! - **Loud failures.** [`StoreError`] names every refusal;
//!   `Backend(String)` carries the OS message. Missing reads are
//!   `Ok(None)` / `Err(NotFound)` (query, not failure — same split
//!   as the clipboard's empty-`Ok(None)`).
//!
//! Out of scope: network fetch (OQ-G5-5), shell data-dir exposure
//! (roots are app-chosen `PathBuf`s this round — per-platform
//! conventions live in the decision doc), symlinks, file locking,
//! watching.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use crate::reactive::{untrack, Runtime};
use crate::worker::TaskScope;

/// Storage failure (loud by construction — see
/// `docs/05-implementation/error-handling.md`).
#[derive(Clone, Debug, PartialEq)]
pub enum StoreError {
    /// No backend for this target yet (e.g. FS on web).
    Unsupported(&'static str),
    /// Read of a missing key/path (query outcome, not a failure —
    /// callers branch on it, never unwrap past it).
    NotFound(String),
    /// Empty KV key, or absolute/`..`-carrying FS path.
    InvalidKey(String),
    /// The OS/backend call failed; the string is its message.
    Backend(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Unsupported(who) => {
                write!(f, "{who} has no storage backend — refusal, never silent")
            }
            StoreError::NotFound(key) => write!(f, "no such key/path: {key}"),
            StoreError::InvalidKey(key) => write!(f, "invalid key/path: {key:?}"),
            StoreError::Backend(msg) => write!(f, "storage backend failed: {msg}"),
        }
    }
}

impl std::error::Error for StoreError {}

// ---------------------------------------------------------------------------
// Key-value settings
// ---------------------------------------------------------------------------

/// Synchronous key-value settings backend (decision 216).
pub trait KvStore {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError>;
    fn set(&mut self, key: &str, value: Vec<u8>) -> Result<(), StoreError>;
    fn remove(&mut self, key: &str) -> Result<(), StoreError>;
    fn clear(&mut self) -> Result<(), StoreError>;
}

/// Headless/test KV: a `HashMap` with the key rule enforced.
#[derive(Clone, Debug, Default)]
pub struct InMemoryKv {
    map: HashMap<String, Vec<u8>>,
}

impl InMemoryKv {
    pub fn new() -> Self {
        Self::default()
    }
}

fn check_key(key: &str) -> Result<(), StoreError> {
    if key.is_empty() {
        return Err(StoreError::InvalidKey(key.to_string()));
    }
    Ok(())
}

impl KvStore for InMemoryKv {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        check_key(key)?;
        Ok(self.map.get(key).cloned())
    }

    fn set(&mut self, key: &str, value: Vec<u8>) -> Result<(), StoreError> {
        check_key(key)?;
        self.map.insert(key.to_string(), value);
        Ok(())
    }

    fn remove(&mut self, key: &str) -> Result<(), StoreError> {
        check_key(key)?;
        self.map.remove(key);
        Ok(())
    }

    fn clear(&mut self) -> Result<(), StoreError> {
        self.map.clear();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Sandboxed files
// ---------------------------------------------------------------------------

/// Synchronous sandboxed-file backend (decision 217): every path
/// resolves lexically under the sandbox root (`..`/absolute refused).
pub trait FsSandbox {
    fn read(&self, path: &str) -> Result<Vec<u8>, StoreError>;
    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError>;
    fn remove(&mut self, path: &str) -> Result<(), StoreError>;
    fn exists(&self, path: &str) -> Result<bool, StoreError>;
    /// Relative paths under `prefix` (empty prefix lists all), sorted.
    fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError>;
}

/// Joins `rel` under `root` after the lexical check (decision 217).
fn jailed(root: &Path, rel: &str) -> Result<PathBuf, StoreError> {
    if rel.is_empty() {
        return Err(StoreError::InvalidKey(rel.to_string()));
    }
    let path = Path::new(rel);
    if path.is_absolute() {
        return Err(StoreError::InvalidKey(rel.to_string()));
    }
    for comp in path.components() {
        match comp {
            Component::CurDir | Component::Normal(_) => {}
            // `..`, prefixes, and root dirs escape the jail — refused.
            _ => return Err(StoreError::InvalidKey(rel.to_string())),
        }
    }
    Ok(root.join(path))
}

/// Headless/test FS: a `HashMap` of relative path → bytes.
#[derive(Clone, Debug, Default)]
pub struct InMemoryFs {
    files: HashMap<String, Vec<u8>>,
}

impl InMemoryFs {
    pub fn new() -> Self {
        Self::default()
    }

    fn check(&self, path: &str) -> Result<(), StoreError> {
        jailed(Path::new(""), path).map(|_| ())
    }
}

impl FsSandbox for InMemoryFs {
    fn read(&self, path: &str) -> Result<Vec<u8>, StoreError> {
        self.check(path)?;
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(path.to_string()))
    }

    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.check(path)?;
        self.files.insert(path.to_string(), bytes.to_vec());
        Ok(())
    }

    fn remove(&mut self, path: &str) -> Result<(), StoreError> {
        self.check(path)?;
        self.files.remove(path);
        Ok(())
    }

    fn exists(&self, path: &str) -> Result<bool, StoreError> {
        self.check(path)?;
        Ok(self.files.contains_key(path))
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let mut out: Vec<String> = self
            .files
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        out.sort();
        Ok(out)
    }
}

/// Native FS sandbox over `std::fs` (no new deps — core is std-only).
/// `new` creates the root (loud on failure); parent dirs are created
/// on write (settings/cache writes never pre-create dirs — the seam
/// does it, loudly on failure).
#[derive(Clone, Debug)]
pub struct NativeFs {
    root: PathBuf,
}

impl NativeFs {
    pub fn new(root: PathBuf) -> Result<Self, StoreError> {
        std::fs::create_dir_all(&root)
            .map_err(|e| StoreError::Backend(format!("create sandbox root: {e}")))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl FsSandbox for NativeFs {
    fn read(&self, path: &str) -> Result<Vec<u8>, StoreError> {
        let full = jailed(&self.root, path)?;
        std::fs::read(&full).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound(path.to_string())
            } else {
                StoreError::Backend(format!("read {path}: {e}"))
            }
        })
    }

    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let full = jailed(&self.root, path)?;
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StoreError::Backend(format!("create parent dirs: {e}")))?;
        }
        std::fs::write(&full, bytes)
            .map_err(|e| StoreError::Backend(format!("write {path}: {e}")))?;
        Ok(())
    }

    fn remove(&mut self, path: &str) -> Result<(), StoreError> {
        let full = jailed(&self.root, path)?;
        match std::fs::remove_file(&full) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::Backend(format!("remove {path}: {e}"))),
        }
    }

    fn exists(&self, path: &str) -> Result<bool, StoreError> {
        let full = jailed(&self.root, path)?;
        Ok(full.is_file())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let mut out = Vec::new();
        self.list_walk(&self.root, &PathBuf::new(), prefix, &mut out)?;
        out.sort();
        Ok(out)
    }
}

impl NativeFs {
    fn list_walk(
        &self,
        dir: &Path,
        rel: &Path,
        prefix: &str,
        out: &mut Vec<String>,
    ) -> Result<(), StoreError> {
        let entries =
            std::fs::read_dir(dir).map_err(|e| StoreError::Backend(format!("list: {e}")))?;
        for entry in entries {
            let entry = entry.map_err(|e| StoreError::Backend(format!("list entry: {e}")))?;
            let child_rel = rel.join(entry.file_name());
            let ft = entry
                .file_type()
                .map_err(|e| StoreError::Backend(format!("list file type: {e}")))?;
            if ft.is_dir() {
                self.list_walk(&entry.path(), &child_rel, prefix, out)?;
            } else if ft.is_file() {
                let name = child_rel.to_string_lossy().replace('\\', "/");
                if name.starts_with(prefix) {
                    out.push(name);
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Queryable collections (Round 13.2, decision 309)
// ---------------------------------------------------------------------------

/// Stable row handle: monotonic per collection, never reused
/// (removed ids retire forever — handles stay valid across
/// filter/sort/page and across appends, so selection and
/// virtualized slots can key on them without an ABA hazard).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct RowId(pub u64);

/// One row: stable id + payload snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct Row<T> {
    pub id: RowId,
    pub value: T,
}

/// The keyed-state payload behind a [`Collection`] (Round 23.1,
/// decision 333 — separated residence): `order` is the
/// version-notified structure (commit order + membership —
/// structural readers subscribe here); `values` is the payload
/// truth (silently writable — value writes notify per-row slots,
/// never the version, so updating one row never re-runs its
/// siblings); `slots` are the lazy per-row notify signals
/// (shared across handles through the `Rc`, like the values).
/// Invariant: `values` holds exactly `order`'s ids (structural
/// ops maintain both; granular value writes touch values +
/// slots only).
#[derive(Clone)]
struct CollectionState<T> {
    order: Vec<RowId>,
    values: Rc<RefCell<HashMap<RowId, T>>>,
    slots: Rc<RefCell<HashMap<RowId, crate::reactive::Signal<Option<T>>>>>,
    next_id: u64,
}

impl<T> Default for CollectionState<T> {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            values: Rc::new(RefCell::new(HashMap::new())),
            slots: Rc::new(RefCell::new(HashMap::new())),
            next_id: 0,
        }
    }
}

impl<T: Clone> CollectionState<T> {
    /// Appends values, minting fresh ids in stage order. Returns
    /// the assigned rows (ids included — the caller names them).
    fn ingest(&mut self, values: Vec<T>) -> Vec<Row<T>> {
        let mut out = Vec::with_capacity(values.len());
        for value in values {
            let id = RowId(self.next_id);
            self.next_id += 1;
            self.order.push(id);
            self.values.borrow_mut().insert(id, value.clone());
            out.push(Row { id, value });
        }
        out
    }
}

/// Queryable in-memory collection (Round 13.2, decision 309): rows
/// with stable [`RowId`] handles, filter/sort/page queries, and
/// async appends through a [`CollectionWriter`]. UI-thread owned
/// (like signals — `Clone`, never `Send`); the writer is the
/// `Send` half (the fetch rendezvous pattern: rows cross threads
/// as data, id assignment + signal writes happen on the UI thread
/// in the INPUT drain).
///
/// Reads are tracked (structural mutations invalidate readers;
/// per-row value writes notify only `get_row` readers — Round
/// 23.1, decision 333); hosts must size keyed capacity to cover
/// live collections (one keyed entry each — the M8
/// explicit-capacity discipline; eviction would re-init to empty,
/// so size it, never assume it).
#[derive(Clone)]
pub struct Collection<T> {
    rt: Runtime,
    key: u64,
    marker: std::marker::PhantomData<T>,
}

impl<T: Clone + 'static> Collection<T> {
    /// Opens (or joins, by key) a collection. Keys share the one
    /// global keyed-state namespace — hash readable names (see
    /// [`fetch_key`](crate::fetch::fetch_key)), and never reuse a
    /// live key for a different `T` (the downcast panics loudly).
    pub fn new(rt: &Runtime, key: u64) -> Self {
        Self {
            rt: rt.clone(),
            key,
            marker: std::marker::PhantomData,
        }
    }

    /// The rendezvous key (the writer needs it — see
    /// [`Collection::writer`]).
    pub fn key(&self) -> u64 {
        self.key
    }

    fn signal(&self) -> crate::reactive::Signal<CollectionState<T>> {
        self.rt.keyed_state(self.key, CollectionState::default)
    }

    /// Row count (tracked read).
    pub fn len(&self) -> usize {
        self.signal().get().order.len()
    }

    /// True when no rows are committed (tracked read).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Appends values on the UI thread, minting ids in order.
    /// Returns the assigned rows.
    pub fn ingest(&self, values: Vec<T>) -> Vec<Row<T>> {
        let mut out = Vec::new();
        self.signal().update(|mut s| {
            out = s.ingest(values);
            s
        });
        out
    }

    /// Replaces one row's payload (`false` for unknown ids —
    /// query outcome, never a failure). Coarse broadcast (kept
    /// for bulk/background writes): bumps the version AND syncs
    /// the row's slot, so query readers and granular readers
    /// agree. Prefer [`Collection::update_row`] for interactive
    /// value edits (per-row notify, no fan-out).
    pub fn update(&self, id: RowId, value: T) -> bool {
        let mut hit = false;
        self.signal().update(|s| {
            if s.order.contains(&id) {
                s.values.borrow_mut().insert(id, value.clone());
                if let Some(slot) = s.slots.borrow().get(&id) {
                    slot.set(Some(value));
                }
                hit = true;
            }
            s
        });
        hit
    }

    /// Replaces one row's payload with per-row notify (Round 23.1,
    /// decision 333): writes the value and notifies only that
    /// row's `get_row` readers — the collection version is
    /// untouched, so query readers (windows, counts, filters) and
    /// sibling rows stay quiet. `false` for unknown ids. Callers
    /// move membership (filter/sort/page placement) through
    /// remove + ingest (structural) — `update_row` never
    /// re-resolves query membership, documented, not silent.
    pub fn update_row(&self, id: RowId, value: T) -> bool {
        // Untracked existence read (writers never subscribe —
        // only the row's slot notifies).
        if !untrack(|| self.signal().get().order.contains(&id)) {
            return false;
        }
        let state = untrack(|| self.signal().get());
        state.values.borrow_mut().insert(id, value.clone());
        let slot = {
            let mut slots = state.slots.borrow_mut();
            slots
                .entry(id)
                .or_insert_with(|| self.rt.signal(Some(value.clone())))
                .clone()
        };
        slot.set(Some(value));
        true
    }

    /// Per-row tracked read (Round 23.1, decision 333): a lazy
    /// `Signal<Option<T>>` for exactly this id (`None` when
    /// retired/unknown). Subscribers re-run on this row's writes
    /// (plus structural version reads, like every query reader);
    /// `update_row` notifies only here.
    pub fn get_row(&self, id: RowId) -> crate::reactive::Signal<Option<T>> {
        // The version read subscribes structural re-runs (ingest /
        // remove re-derive rows, like every query reader); value
        // writes arrive through the slot alone.
        let state = self.signal().get();
        if let Some(slot) = state.slots.borrow().get(&id).cloned() {
            return slot;
        }
        let seed = state.values.borrow().get(&id).cloned();
        let slot = self.rt.signal(seed);
        state.slots.borrow_mut().insert(id, slot.clone());
        slot
    }

    /// Removes one row (`false` for unknown ids). The id retires
    /// forever — never reassigned (the no-ABA rule above). Live
    /// slots observe `None` (precisely notified, never silently
    /// stale), then drop.
    pub fn remove(&self, id: RowId) -> bool {
        let mut hit = false;
        self.signal().update(|mut s| {
            let before = s.order.len();
            s.order.retain(|r| *r != id);
            hit = s.order.len() != before;
            if hit {
                s.values.borrow_mut().remove(&id);
                if let Some(slot) = s.slots.borrow_mut().remove(&id) {
                    slot.set(None);
                }
            }
            s
        });
        hit
    }

    /// Drops all rows (ids still never repeat afterwards). Live
    /// slots observe `None`, then drop.
    pub fn clear(&self) {
        self.signal().update(|mut s| {
            s.order.clear();
            s.values.borrow_mut().clear();
            for (_, slot) in s.slots.borrow_mut().drain() {
                slot.set(None);
            }
            s
        });
    }

    /// Payload snapshot for one id (tracked read; `None` for
    /// unknown/retired ids). Reads the payload truth, so granular
    /// `update_row` writes are visible here on the next structural
    /// read.
    pub fn lookup(&self, id: RowId) -> Option<T> {
        let state = self.signal().get();
        if !state.order.contains(&id) {
            return None;
        }
        let value = state.values.borrow().get(&id).cloned();
        value
    }

    /// All committed rows in commit order (tracked snapshot).
    pub fn rows(&self) -> Vec<Row<T>> {
        let state = self.signal().get();
        let values = state.values.borrow();
        state
            .order
            .iter()
            .filter_map(|id| {
                values.get(id).map(|v| Row {
                    id: *id,
                    value: v.clone(),
                })
            })
            .collect()
    }

    /// Filtered/sorted/paged read (tracked — the virtualized feed's
    /// window source): filter keeps commit order, sort is stable,
    /// `total` counts all matches before paging (the list sizes its
    /// extent from it). Values always read fresh (granular
    /// `update_row` writes land here on the next structural read —
    /// membership moves go through remove + ingest). Paging past
    /// the end is a quiet empty page (controlled-contract edge —
    /// same class as unmatched tab signals, documented not
    /// silent).
    pub fn query(&self, q: &CollectionQuery<T>) -> CollectionPage<T> {
        let state = self.signal().get();
        let values = state.values.borrow();
        let join = |id: &RowId| {
            values.get(id).map(|v| Row {
                id: *id,
                value: v.clone(),
            })
        };
        let mut matched: Vec<Row<T>> = match &q.filter {
            Some(f) => state
                .order
                .iter()
                .filter_map(join)
                .filter(|r| f(&r.value))
                .collect(),
            None => state.order.iter().filter_map(join).collect(),
        };
        if let Some(sort) = &q.sort {
            matched.sort_by(|a, b| sort(&a.value, &b.value));
        }
        let total = matched.len();
        let rows = match q.limit {
            Some(limit) => matched.into_iter().skip(q.offset).take(limit).collect(),
            None => matched.into_iter().skip(q.offset).collect(),
        };
        CollectionPage { total, rows }
    }

    /// The `Send` half: submit batches from worker bodies (see
    /// [`CollectionWriter::submit`]).
    pub fn writer(&self) -> CollectionWriter {
        CollectionWriter { key: self.key }
    }

    /// Ingests a batch against a key without a handle (the
    /// paged-fetch rendezvous, Round 13.3 — `spawn_fetch_page`
    /// applies here on the UI thread; reads through any
    /// [`Collection`] on the same key see it). Returns assigned ids.
    pub fn ingest_batch(rt: &Runtime, key: u64, rows: Vec<T>) -> Vec<RowId> {
        let mut out = Vec::new();
        rt.keyed_state::<CollectionState<T>>(key, CollectionState::default)
            .update(|mut s| {
                out = s.ingest(rows).into_iter().map(|r| r.id).collect();
                s
            });
        out
    }
}

/// Keep predicate over row payloads (shared alias — the query
/// struct and the control props name one type, never two
/// spellings of the same closure).
pub type RowFilter<T> = Rc<dyn Fn(&T) -> bool>;

/// Stable ordering over row payloads (applied after filtering).
pub type RowSort<T> = Rc<dyn Fn(&T, &T) -> std::cmp::Ordering>;

/// Filter/sort/page parameters for [`Collection::query`] (all
/// optional except paging — construction-time values, like the
/// M8 window fns; authors re-query per render, so swapping a
/// closure re-derives naturally on the next tracked run).
#[derive(Clone, Default)]
pub struct CollectionQuery<T> {
    /// Keep predicate (commit order preserved for survivors).
    pub filter: Option<RowFilter<T>>,
    /// Stable ordering (applied after filtering).
    pub sort: Option<RowSort<T>>,
    /// Rows to skip (window start).
    pub offset: usize,
    /// Rows to take (`None` takes all).
    pub limit: Option<usize>,
}

/// One query answer: matches before paging + the page itself.
#[derive(Clone, Debug, PartialEq)]
pub struct CollectionPage<T> {
    /// Matches before `offset`/`limit` (the extent source).
    pub total: usize,
    /// The page (stable [`RowId`]s, filter/sort applied).
    pub rows: Vec<Row<T>>,
}

/// The `Send` half of a [`Collection`] (Round 13.2 + 13.1
/// combined): worker bodies submit batches through the task
/// scope; rows cross as `Send` data, id assignment + the signal
/// write run on the UI thread in the INPUT drain (the exact fetch
/// rendezvous — signals never cross threads, locked by
/// construction: this struct holds no signal, only the key).
#[derive(Clone, Copy, Debug)]
pub struct CollectionWriter {
    key: u64,
}

impl CollectionWriter {
    /// Submits a batch from a worker body. The `T` must match the
    /// UI side's `T` for the same key (a mismatch panics loudly in
    /// the drain's downcast — same contract as `keyed_state`).
    pub fn submit<T: Clone + Send + 'static>(&self, scope: &TaskScope, rows: Vec<T>) {
        let key = self.key;
        scope.submit(move |rt| {
            rt.keyed_state::<CollectionState<T>>(key, CollectionState::default)
                .update(|mut s| {
                    s.ingest(rows);
                    s
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kv_round_trip_remove_clear() {
        let mut kv = InMemoryKv::new();
        assert_eq!(kv.get("theme").expect("reads"), None);
        kv.set("theme", b"dark".to_vec()).expect("writes");
        assert_eq!(kv.get("theme").expect("reads"), Some(b"dark".to_vec()));
        kv.remove("theme").expect("removes");
        assert_eq!(kv.get("theme").expect("reads"), None);
        kv.set("a", b"1".to_vec()).expect("writes");
        kv.clear().expect("clears");
        assert_eq!(kv.get("a").expect("reads"), None);
    }

    #[test]
    fn kv_rejects_empty_key_loudly() {
        let mut kv = InMemoryKv::new();
        assert_eq!(
            kv.set("", b"x".to_vec()),
            Err(StoreError::InvalidKey(String::new()))
        );
        assert_eq!(
            kv.get(""),
            Err(StoreError::InvalidKey(String::new())),
            "reads refuse too — never a silent default"
        );
    }

    #[test]
    fn memfs_round_trip_list_exists() {
        let mut fs = InMemoryFs::new();
        assert!(!fs.exists("c/a.txt").expect("reads"));
        fs.write("c/a.txt", b"hello").expect("writes");
        fs.write("c/b.txt", b"world").expect("writes");
        fs.write("other.txt", b"!").expect("writes");
        assert_eq!(fs.read("c/a.txt").expect("reads"), b"hello");
        assert!(fs.exists("c/a.txt").expect("reads"));
        assert_eq!(fs.list("c/").expect("lists"), vec!["c/a.txt", "c/b.txt"]);
        assert_eq!(fs.list("").expect("lists").len(), 3);
        fs.remove("c/a.txt").expect("removes");
        assert_eq!(
            fs.read("c/a.txt"),
            Err(StoreError::NotFound("c/a.txt".to_string()))
        );
    }

    #[test]
    fn memfs_rejects_escapes_loudly() {
        let mut fs = InMemoryFs::new();
        for bad in ["", "../evil", "a/../../evil", "/abs", "C:/x"] {
            assert_eq!(
                fs.write(bad, b"x"),
                Err(StoreError::InvalidKey(bad.to_string())),
                "{bad:?} must not enter the jail"
            );
            assert_eq!(fs.read(bad), Err(StoreError::InvalidKey(bad.to_string())));
        }
    }

    /// Real-OS round-trip in a unique temp subdir (created + removed by
    /// the test — the sandbox root never touches the user's tree).
    #[test]
    fn nativefs_tempdir_round_trip_and_jail() {
        let root = std::env::temp_dir().join(format!(
            "oppa-store-probe-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut fs = NativeFs::new(root.clone()).expect("root creates");
        fs.write("settings/theme.txt", b"dark").expect("writes");
        assert_eq!(fs.read("settings/theme.txt").expect("reads"), b"dark");
        assert!(fs.exists("settings/theme.txt").expect("reads"));
        assert_eq!(
            fs.list("settings/").expect("lists"),
            vec!["settings/theme.txt"]
        );
        assert_eq!(
            fs.read("missing.txt"),
            Err(StoreError::NotFound("missing.txt".to_string()))
        );
        assert_eq!(
            fs.write("../escape", b"x"),
            Err(StoreError::InvalidKey("../escape".to_string())),
            "the OS backend enforces the same jail"
        );
        assert!(fs.root() == root.as_path());
        std::fs::remove_dir_all(&root).expect("test cleans its sandbox");
    }

    // ------------------------------------------------------------------
    // Round 13.2 (decision 309): queryable collections
    // ------------------------------------------------------------------

    fn test_collection(rt: &crate::reactive::Runtime) -> Collection<String> {
        Collection::new(rt, crate::fetch::fetch_key("test:collection"))
    }

    #[test]
    fn collection_ingest_assigns_stable_ids_and_lookup() {
        let rt = crate::reactive::Runtime::new();
        let c = test_collection(&rt);
        assert!(c.is_empty());
        let rows = c.ingest(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert_eq!(
            rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![RowId(0), RowId(1), RowId(2)]
        );
        assert_eq!(c.len(), 3);
        assert_eq!(c.lookup(RowId(1)), Some("b".to_string()));
        assert_eq!(c.lookup(RowId(99)), None, "unknown ids miss quietly");
        // Commit order is ingest order.
        assert_eq!(
            c.rows().iter().map(|r| r.value.clone()).collect::<Vec<_>>(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn collection_query_filters_sorts_pages_with_total() {
        let rt = crate::reactive::Runtime::new();
        let c = test_collection(&rt);
        c.ingest((0..10).map(|i| format!("item {i:02}")).collect::<Vec<_>>());
        // Filter evens, sort descending, second page of 2.
        let page = c.query(&CollectionQuery {
            filter: Some(Rc::new(|v: &String| {
                v.split_whitespace()
                    .last()
                    .is_some_and(|n| n.parse::<u32>().unwrap_or(1) % 2 == 0)
            })),
            sort: Some(Rc::new(|a: &String, b: &String| b.cmp(a))),
            offset: 2,
            limit: Some(2),
        });
        assert_eq!(page.total, 5, "five evens match before paging");
        assert_eq!(
            page.rows
                .iter()
                .map(|r| r.value.clone())
                .collect::<Vec<_>>(),
            vec!["item 04".to_string(), "item 02".to_string()],
            "descending, offset 2, limit 2"
        );
        // Paging past the end is a quiet empty page.
        let past = c.query(&CollectionQuery {
            filter: None,
            sort: None,
            offset: 100,
            limit: Some(10),
        });
        assert_eq!((past.total, past.rows.len()), (10, 0));
    }

    #[test]
    fn collection_update_remove_retire_ids_forever() {
        let rt = crate::reactive::Runtime::new();
        let c = test_collection(&rt);
        c.ingest(vec!["a".to_string(), "b".to_string()]);
        assert!(c.update(RowId(0), "A".to_string()));
        assert_eq!(c.lookup(RowId(0)), Some("A".to_string()));
        assert!(
            !c.update(RowId(99), "x".to_string()),
            "unknown update misses"
        );
        assert!(c.remove(RowId(0)));
        assert!(!c.remove(RowId(0)), "double remove misses");
        assert_eq!(c.lookup(RowId(0)), None, "removed stays gone");
        // Ids never repeat (no ABA for slot/selection keys).
        let rows = c.ingest(vec!["d".to_string()]);
        assert_eq!(rows[0].id, RowId(2), "retired 0 is never reassigned");
        c.clear();
        assert!(c.is_empty());
        let rows = c.ingest(vec!["e".to_string()]);
        assert_eq!(rows[0].id, RowId(3), "clear keeps the monotonic clock");
    }

    #[test]
    fn collection_row_ids_survive_resort_and_page() {
        let rt = crate::reactive::Runtime::new();
        let c = test_collection(&rt);
        c.ingest(vec!["c".to_string(), "a".to_string(), "b".to_string()]);
        let asc = c.query(&CollectionQuery {
            filter: None,
            sort: Some(Rc::new(|a: &String, b: &String| a.cmp(b))),
            offset: 0,
            limit: None,
        });
        assert_eq!(
            asc.rows
                .iter()
                .map(|r| (r.id, r.value.clone()))
                .collect::<Vec<_>>(),
            vec![
                (RowId(1), "a".to_string()),
                (RowId(2), "b".to_string()),
                (RowId(0), "c".to_string()),
            ],
            "sort reorders, ids ride along"
        );
    }

    /// Round 13.1 → 13.2 combined: a worker body submits a batch
    /// through the writer; id assignment + the signal write run on
    /// the UI thread in the INPUT drain (signals never cross
    /// threads — the writer holds no signal, only the key).
    #[test]
    fn collection_async_submit_visible_after_drain() {
        let host = crate::component::ComponentHost::new();
        let rt = host.runtime();
        let c: Collection<String> =
            Collection::new(&rt, crate::fetch::fetch_key("test:collection-async"));
        assert!(c.is_empty());
        let writer = c.writer();
        rt.spawn_task(move |scope| {
            writer.submit(&scope, vec!["x".to_string(), "y".to_string()]);
        });
        // Poll the executor like every worker test (1 task run).
        let mut waited = 0;
        loop {
            let stats = rt.stats();
            if stats.tasks_done + stats.tasks_dropped >= 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
            waited += 1;
            assert!(waited < 10_000, "executor task never resolved");
        }
        host.run_until_idle();
        // The drain assigned ids and bumped the version (the signal
        // delta readers observe).
        assert_eq!(c.len(), 2);
        assert_eq!(
            c.rows()
                .iter()
                .map(|r| (r.id, r.value.clone()))
                .collect::<Vec<_>>(),
            vec![(RowId(0), "x".to_string()), (RowId(1), "y".to_string())]
        );
    }

    // -- Round 23.1 (decision 333): per-row granular subscriptions --

    fn granular_collection(rt: &crate::reactive::Runtime) -> Collection<String> {
        Collection::new(rt, crate::fetch::fetch_key("test:collection-granular"))
    }

    /// `get_row` seeds `Some` for live rows and `None` for unknown
    /// ids; `update_row` refreshes the slot (headless value
    /// half — render counts ride the controls verify).
    #[test]
    fn get_row_seeds_and_update_row_refreshes() {
        let rt = crate::reactive::Runtime::new();
        let c = granular_collection(&rt);
        let rows = c.ingest(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(c.get_row(rows[0].id).get(), Some("a".to_string()));
        assert_eq!(c.get_row(RowId(99)).get(), None, "unknown seeds None");
        assert!(c.update_row(rows[1].id, "B".to_string()));
        assert_eq!(c.get_row(rows[1].id).get(), Some("B".to_string()));
        assert_eq!(c.lookup(rows[1].id), Some("B".to_string()));
        assert!(!c.update_row(RowId(99), "z".to_string()), "unknown misses");
        // Queries read fresh values without any version traffic.
        let page = c.query(&CollectionQuery {
            filter: None,
            sort: None,
            offset: 0,
            limit: None,
        });
        assert_eq!(
            page.rows
                .iter()
                .map(|r| r.value.clone())
                .collect::<Vec<_>>(),
            vec!["a".to_string(), "B".to_string()]
        );
    }

    /// Removal and clear observe `None` through live slots (never
    /// silently stale), then drop them.
    #[test]
    fn remove_and_clear_observe_none() {
        let rt = crate::reactive::Runtime::new();
        let c = granular_collection(&rt);
        let rows = c.ingest(vec!["a".to_string(), "b".to_string()]);
        let slot_a = c.get_row(rows[0].id);
        let slot_b = c.get_row(rows[1].id);
        assert!(c.remove(rows[0].id));
        assert_eq!(slot_a.get(), None, "removed rows read None");
        assert_eq!(c.get_row(rows[0].id).get(), None, "re-seeds None");
        assert_eq!(slot_b.get(), Some("b".to_string()), "siblings untouched");
        c.clear();
        assert_eq!(slot_b.get(), None, "cleared rows read None");
        assert!(c.is_empty());
        assert_eq!(c.lookup(rows[1].id), None);
    }

    /// Coarse `update` keeps broadcasting (version) while syncing
    /// the slot, so granular and structural readers agree.
    #[test]
    fn coarse_update_syncs_slot() {
        let rt = crate::reactive::Runtime::new();
        let c = granular_collection(&rt);
        let rows = c.ingest(vec!["a".to_string()]);
        let slot = c.get_row(rows[0].id);
        assert!(c.update(rows[0].id, "A".to_string()));
        assert_eq!(slot.get(), Some("A".to_string()));
        assert_eq!(c.lookup(rows[0].id), Some("A".to_string()));
        assert!(!c.update(RowId(99), "z".to_string()));
    }
}
