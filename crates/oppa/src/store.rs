//! App storage seams (G5 — decisions 216–217): key-value settings +
//! sandboxed files.
//!
//! The v2 handoff verified no fs/http/kv in core and no such deps
//! workspace-wide — every app hand-rolls settings/cache/sync with no
//! seam. This module is the seam: two sync-first traits, two
//! in-memory references, and one std-backed FS for native targets.
//!
//! Design (see decisions 216–217; rationale in git history):
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
//!   OQ-G5-4), never a silent escape: sandbox roots must be
//!   canonicalized by the app when link traversal matters
//!   (`std::fs::canonicalize` at open — the seam checks syntax,
//!   the OS resolves links, stated split, never conflated).
//! - **Per-platform roots.** [`app_data_dir`] resolves the OS data
//!   dir for an app name (`%APPDATA%` on Windows, macOS
//!   `Library/Application Support`, `$XDG_DATA_HOME` or
//!   `~/.local/share` elsewhere; wasm refuses loudly — no
//!   filesystem, `localStorage` covers it). Pair it with
//!   [`NativeFs`] for the settings/cache home (G23).
//! - **Loud failures.** [`StoreError`] names every refusal;
//!   `Backend(String)` carries the OS message. Missing reads are
//!   `Ok(None)` / `Err(NotFound)` (query, not failure — same split
//!   as the clipboard's empty-`Ok(None)`).
//!
//! Out of scope: network fetch (OQ-G5-5), symlinks beyond the
//! lexical bound above, file locking, watching.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use crate::reactive::{untrack, Runtime, Signal};
use crate::worker::TaskScope;

/// Storage failure (loud by construction — see
/// `docs/CONTRIBUTING.md`).
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

// ---------------------------------------------------------------------------
// Per-platform data dir (G23)
// ---------------------------------------------------------------------------

/// Resolves the OS app-data dir for `app_name` (G23 — the
/// settings/cache home to pair with [`NativeFs`]).
///
/// Conventions (no new deps — `std::env` only):
/// Windows reads `%APPDATA%`, macOS joins
/// `~/Library/Application Support`, other native targets read
/// `$XDG_DATA_HOME` else `~/.local/share`. wasm refuses loudly
/// ([`StoreError::Unsupported`] — no filesystem there;
/// `localStorage` covers it, stated).
///
/// `app_name` is a single path segment (empty names, separators,
/// drive prefixes, and `..` refuse loudly as `InvalidKey` —
/// a jail escape at the root would defeat [`FsSandbox`]).
/// Missing home variables surface as `Backend` (environment
/// failure, never a silent fallback dir).
pub fn app_data_dir(app_name: &str) -> Result<PathBuf, StoreError> {
    if app_name.is_empty() {
        return Err(StoreError::InvalidKey(app_name.to_string()));
    }
    if app_name.contains(['/', '\\'])
        || app_name.contains("..")
        || Path::new(app_name).is_absolute()
        || app_name.contains(':')
    {
        return Err(StoreError::InvalidKey(app_name.to_string()));
    }
    if cfg!(target_arch = "wasm32") {
        return Err(StoreError::Unsupported("app_data_dir on wasm"));
    }
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| StoreError::Backend("APPDATA is not set".to_string()))?
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| StoreError::Backend("HOME is not set".to_string()))?;
        home.join("Library/Application Support")
    } else {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            let xdg = PathBuf::from(xdg);
            if xdg.is_absolute() {
                xdg
            } else {
                return Err(StoreError::InvalidKey(format!(
                    "XDG_DATA_HOME is not absolute: {}",
                    xdg.display()
                )));
            }
        } else {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or_else(|| StoreError::Backend("HOME is not set".to_string()))?;
            home.join(".local/share")
        }
    };
    Ok(base.join(app_name))
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
    /// Write-through persistence hook (Phase 37b, decision 364):
    /// installed by [`Collection::persist`], fired by
    /// [`CollectionState::note_write`] after every mutation path
    /// (ingest/update/update_row/remove/clear, writer submits,
    /// fetch-page applies — the state level catches them all,
    /// never one method's twin). `None` is the unpersisted norm.
    persister: Option<Rc<PersistHook<T>>>,
}

/// One collection's write-through hook: snapshot under `key` after
/// every mutation (shared through the state — every handle on the
/// key writes through the same hook, last install wins).
struct PersistHook<T> {
    key: String,
    encode: EncodeRows<T>,
    store: Rc<RefCell<dyn KvStore>>,
}

/// Row-snapshot encoder (whole collection, commit order → bytes).
type EncodeRows<T> = Rc<dyn Fn(&[T]) -> Vec<u8>>;

/// Value encoder (one persisted signal value → bytes).
type EncodeValue<T> = Rc<dyn Fn(&T) -> Vec<u8>>;

impl<T> Default for CollectionState<T> {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            values: Rc::new(RefCell::new(HashMap::new())),
            slots: Rc::new(RefCell::new(HashMap::new())),
            next_id: 0,
            persister: None,
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

    /// Fires the write-through hook after a mutation (Phase 37b):
    /// snapshots values in commit order, encodes, stores. A store
    /// failure panics loudly (silent loss is never an option —
    /// the in-memory rows stay, so the panic names the key and the
    /// retry is the next mutation).
    fn note_write(&self) {
        let Some(hook) = self.persister.clone() else {
            return;
        };
        let values = self.values.borrow();
        let ordered: Vec<T> = self
            .order
            .iter()
            .filter_map(|id| values.get(id).cloned())
            .collect();
        drop(values);
        let bytes = (hook.encode)(&ordered);
        hook.store
            .borrow_mut()
            .set(&hook.key, bytes)
            .unwrap_or_else(|e| {
                panic!(
                    "collection write-through on {:?} failed: {e} — refused, never silent loss",
                    hook.key
                )
            });
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
            s.note_write();
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
            s.note_write();
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
        state.note_write();
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
            s.note_write();
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
            s.note_write();
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
                s.note_write();
                s
            });
        out
    }

    /// Write-through persistence for a collection (Phase 37b,
    /// decision 364): seeds from `store` under `key` when this
    /// collection is empty (fresh boot — row ids re-mint, values
    /// survive; identity never crosses a restart, stated), then
    /// installs the write-through hook (every later mutation —
    /// ingest/update/remove/clear, writer submits, fetch-page
    /// applies — snapshots values in commit order through
    /// `encode`). Reinstalls replace (last wins, stated).
    ///
    /// Backend read failures return `Err` loudly (nothing seeds —
    /// the collection stays as it was). Corrupt snapshots seed
    /// empty (`decode` is infallible — authors map corruption to
    /// `vec![]`; the bytes are replaced on the next write,
    /// last-writer-wins, stated).
    pub fn persist(
        &self,
        key: &str,
        encode: impl Fn(&[T]) -> Vec<u8> + 'static,
        decode: impl Fn(&[u8]) -> Vec<T> + 'static,
        store: Rc<RefCell<dyn KvStore>>,
    ) -> Result<PersistReport, StoreError> {
        let mut seeded_rows = 0usize;
        if self.is_empty() {
            if let Some(bytes) = store.borrow().get(key)? {
                let rows = decode(&bytes);
                seeded_rows = rows.len();
                // Persister installs after (seeding never
                // writes back — no echo, no error mask).
                self.ingest(rows);
            }
        }
        let hook = PersistHook {
            key: key.to_string(),
            encode: Rc::new(encode),
            store,
        };
        self.signal().update(|mut s| {
            s.persister = Some(Rc::new(hook));
            s
        });
        Ok(PersistReport { seeded_rows })
    }
}

/// What one [`Collection::persist`] call seeded (plain data —
/// test-assertable, never a silent empty).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PersistReport {
    /// Rows decoded from the store (0 = no snapshot, or corrupt —
    /// see `persist`).
    pub seeded_rows: usize,
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
                    s.note_write();
                    s
                });
        });
    }
}

/// Signal-backed persisted value (Phase 37b, decision 364):
/// reads track through the inner signal (bodies render it like any
/// state); writes go to the signal AND the store synchronously
/// (write-through — a crash loses nothing committed). Created per
/// run through [`Ctx::persisted`](crate::component::Ctx::persisted)
/// (cheap handle clone — the signal persists in the instance).
#[derive(Clone)]
pub struct Persisted<T: Clone + 'static> {
    signal: Signal<T>,
    key: String,
    store: Rc<RefCell<dyn KvStore>>,
    encode: EncodeValue<T>,
}

impl<T: Clone + 'static> Persisted<T> {
    /// Builds the handle (see
    /// [`Ctx::persisted`](crate::component::Ctx::persisted) — bodies
    /// never call this directly; the seed discipline lives there).
    pub fn new(
        signal: Signal<T>,
        key: &str,
        encode: impl Fn(&T) -> Vec<u8> + 'static,
        store: Rc<RefCell<dyn KvStore>>,
    ) -> Self {
        Self {
            signal,
            key: key.to_string(),
            store,
            encode: Rc::new(encode),
        }
    }

    /// Tracked read (subscribes like the inner signal).
    pub fn get(&self) -> T {
        self.signal.get()
    }

    /// Write-through set: signal first, then the store. A store
    /// failure panics loudly (data loss is never silent — the
    /// signal already holds the value, so the panic names the key
    /// and the backend error, and the next run re-reads the
    /// in-memory value).
    pub fn set(&self, value: T) {
        let bytes = (self.encode)(&value);
        self.signal.set(value);
        self.store
            .borrow_mut()
            .set(&self.key, bytes)
            .unwrap_or_else(|e| {
                panic!(
                    "persisted set on {:?} failed: {e} — refused, never silent loss",
                    self.key
                )
            });
    }

    /// The inner signal (effect wiring, snapshots — same value).
    pub fn signal(&self) -> Signal<T> {
        self.signal.clone()
    }

    /// The store key (diagnostics).
    pub fn key(&self) -> &str {
        &self.key
    }
}

#[cfg(test)]
mod tests;
