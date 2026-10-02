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

/// Phase 37b (decision 364): `persisted_string` writes through
/// (signal + store agree synchronously), reseeds across hosts
/// from the same store (fresh boot reads committed values), and
/// falls back to initial with a `Warn` diagnostic on corrupt
/// bytes (boot never crashes on bad settings).
#[test]
fn persisted_string_writes_through_and_reseeds() {
    use crate::component::{ComponentHost, Ctx, Props};
    use crate::vnode::{Div, VNode};
    use std::cell::RefCell;
    use std::rc::Rc;
    #[derive(Clone)]
    struct PersistProbe {
        store: Rc<RefCell<dyn KvStore>>,
        stash: Rc<RefCell<Option<Persisted<String>>>>,
    }
    impl Props for PersistProbe {}
    fn persist_render(ctx: &Ctx, p: &PersistProbe) -> VNode {
        // The store handle is app-owned (props); the seed
        // discipline (store wins when present and decodable)
        // lives in `persisted_string`.
        let handle = ctx.persisted_string("theme", "light", p.store.clone());
        *p.stash.borrow_mut() = Some(handle);
        Div("probe").build()
    }
    let store: Rc<RefCell<dyn KvStore>> = Rc::new(RefCell::new(InMemoryKv::new()));
    let stash: Rc<RefCell<Option<Persisted<String>>>> = Rc::new(RefCell::new(None));
    let host = ComponentHost::new();
    host.mount(
        "Persist",
        PersistProbe {
            store: store.clone(),
            stash: stash.clone(),
        },
        persist_render,
    );
    host.run_until_idle();
    let handle = stash.borrow().clone().expect("handle");
    assert_eq!(handle.get(), "light", "empty store seeds initial");
    handle.set("dark".to_string());
    assert_eq!(
        store.borrow().get("theme").expect("reads"),
        Some(b"dark".to_vec()),
        "write-through hits the store synchronously"
    );
    // Fresh boot (new host, same store) reseeds committed values.
    let stash2: Rc<RefCell<Option<Persisted<String>>>> = Rc::new(RefCell::new(None));
    let host2 = ComponentHost::new();
    host2.mount(
        "Persist",
        PersistProbe {
            store: store.clone(),
            stash: stash2.clone(),
        },
        persist_render,
    );
    host2.run_until_idle();
    assert_eq!(
        stash2.borrow().clone().expect("handle").get(),
        "dark",
        "reseed reads committed values"
    );
    // Corrupt bytes fall back with a Warn (never a crash).
    store
        .borrow_mut()
        .set("theme", vec![0xFF, 0xFE])
        .expect("writes");
    let stash3: Rc<RefCell<Option<Persisted<String>>>> = Rc::new(RefCell::new(None));
    let host3 = ComponentHost::new();
    host3.mount(
        "Persist",
        PersistProbe {
            store: store.clone(),
            stash: stash3.clone(),
        },
        persist_render,
    );
    host3.run_until_idle();
    assert_eq!(
        stash3.borrow().clone().expect("handle").get(),
        "light",
        "corrupt seeds initial"
    );
    assert_eq!(host3.diag_len(), 1, "fallback is observable");
    let warns = host3.take_diag_logs(crate::diag::LogLevel::Warn);
    assert_eq!(warns.len(), 1);
    assert!(warns[0].message.contains("undecodable"));
}

/// Phase 37b: failing stores refuse loudly on write (never
/// silent loss — the signal holds the value, the panic names
/// the key).
#[test]
#[should_panic(expected = "persisted set")]
fn persisted_write_failure_panics_loudly() {
    use crate::component::ComponentHost;
    struct FailKv;
    impl KvStore for FailKv {
        fn get(&self, _key: &str) -> Result<Option<Vec<u8>>, StoreError> {
            Ok(None)
        }
        fn set(&mut self, _key: &str, _value: Vec<u8>) -> Result<(), StoreError> {
            Err(StoreError::Backend("disk gone".to_string()))
        }
        fn remove(&mut self, _key: &str) -> Result<(), StoreError> {
            Ok(())
        }
        fn clear(&mut self) -> Result<(), StoreError> {
            Ok(())
        }
    }
    let store: Rc<RefCell<dyn KvStore>> = Rc::new(RefCell::new(FailKv));
    let host = ComponentHost::new();
    let sig = host.runtime().signal("v".to_string());
    let persisted = Persisted::new(sig, "k", |s: &String| s.as_bytes().to_vec(), store);
    persisted.set("w".to_string());
}

/// Phase 37b (decision 364): collections seed from the store on
/// a fresh runtime (ids re-mint, values survive) and write
/// through every mutation path (ingest/update/remove/clear);
/// corrupt snapshots seed empty and are replaced on the next
/// write (last-writer-wins, stated).
#[test]
fn collection_persist_seeds_and_writes_through() {
    use crate::component::ComponentHost;
    let encode = |rows: &[String]| rows.join("\n").into_bytes();
    let decode = |bytes: &[u8]| {
        String::from_utf8(bytes.to_vec())
            .map(|s| {
                if s.is_empty() {
                    Vec::new()
                } else {
                    s.split('\n').map(|r| r.to_string()).collect()
                }
            })
            .unwrap_or_default()
    };
    let store: Rc<RefCell<dyn KvStore>> = Rc::new(RefCell::new(InMemoryKv::new()));
    let host = ComponentHost::new();
    let c: Collection<String> = Collection::new(
        &host.runtime(),
        crate::fetch::fetch_key("test:persist-coll"),
    );
    let report = c
        .persist("tasks", encode, decode, store.clone())
        .expect("persists");
    assert_eq!(report.seeded_rows, 0, "no snapshot seeds nothing");
    c.ingest(vec!["a".to_string(), "b".to_string()]);
    assert_eq!(
        store.borrow().get("tasks").expect("reads"),
        Some(b"a\nb".to_vec()),
        "ingest writes through"
    );
    let rows = c.rows();
    assert!(c.update(rows[0].id, "A".to_string()));
    assert_eq!(
        store.borrow().get("tasks").expect("reads"),
        Some(b"A\nb".to_vec()),
        "update writes through"
    );
    assert!(c.remove(rows[1].id));
    assert_eq!(
        store.borrow().get("tasks").expect("reads"),
        Some(b"A".to_vec()),
        "remove writes through"
    );
    c.clear();
    assert_eq!(
        store.borrow().get("tasks").expect("reads"),
        Some(Vec::new()),
        "clear writes through (empty snapshot, not deletion)"
    );
    // Fresh runtime, same store: values reseed (ids re-mint).
    let host2 = ComponentHost::new();
    store
        .borrow_mut()
        .set("tasks", b"x\ny".to_vec())
        .expect("writes");
    let c2: Collection<String> = Collection::new(
        &host2.runtime(),
        crate::fetch::fetch_key("test:persist-coll"),
    );
    let report2 = c2
        .persist("tasks", encode, decode, store.clone())
        .expect("persists");
    assert_eq!(report2.seeded_rows, 2);
    assert_eq!(
        c2.rows()
            .iter()
            .map(|r| r.value.clone())
            .collect::<Vec<_>>(),
        vec!["x".to_string(), "y".to_string()],
        "values survive, ids re-mint"
    );
    // Corrupt snapshots seed empty (bytes replaced on next write).
    store
        .borrow_mut()
        .set("tasks", vec![0xFF, 0xFE])
        .expect("writes");
    let host3 = ComponentHost::new();
    let c3: Collection<String> = Collection::new(
        &host3.runtime(),
        crate::fetch::fetch_key("test:persist-coll"),
    );
    let report3 = c3
        .persist("tasks", encode, decode, store.clone())
        .expect("persists");
    assert_eq!(report3.seeded_rows, 0, "corrupt seeds empty");
    assert!(c3.is_empty());
    c3.ingest(vec!["z".to_string()]);
    assert_eq!(
        store.borrow().get("tasks").expect("reads"),
        Some(b"z".to_vec()),
        "next write replaces corrupt bytes"
    );
}

/// Phase 38d (decision 373, G23): app names that could escape
/// the root refuse before any env is read (no env writes here —
/// validation is env-free, so parallel tests stay deterministic).
#[test]
fn app_data_dir_refuses_root_escapes() {
    assert_eq!(app_data_dir(""), Err(StoreError::InvalidKey(String::new())));
    for bad in [
        "a/b",
        "a\\b",
        "..",
        "../evil",
        "a/../evil",
        "/abs",
        "C:/x",
        "C:",
    ] {
        assert_eq!(
            app_data_dir(bad),
            Err(StoreError::InvalidKey(bad.to_string())),
            "{bad:?} must not become a root segment"
        );
    }
}

/// Phase 38d (decision 373, G23): the resolved dir ends with
/// the app name (env reads only — no mutation, so parallel
/// tests stay deterministic; home variables are set on every
/// native target, wasm refuses by cfg and never runs here).
#[test]
fn app_data_dir_ends_with_app_name() {
    let dir = app_data_dir("oppa-probe").expect("data dir resolves natively");
    assert_eq!(
        dir.file_name().and_then(|s| s.to_str()),
        Some("oppa-probe"),
        "the app name is the final segment: {}",
        dir.display()
    );
}
