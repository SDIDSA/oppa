//! Real dylib swap (M2b proof of lock #14): builds the hot-fixture cdylib
//! twice (v1 default, v2 `--features v2`), performs an actual unload /
//! reload through `HotRegistry`, and proves adopt-across-unload,
//! manifest-scan discovery (`NewOnly`), and handler re-resolution with
//! real code pages.
//!
//! Single-root host (M2 reconciler): only `Counter` is mounted here —
//! eviction is covered headless in `reload_cycle.rs`. Escape hatch for
//! exotic CI: `OPPA_SKIP_REAL_DYLIB=1` (documented, default runs).
//!
//! The test links the fixture rlib for TYPES and the mount-time render
//! fn; post-swap runs resolve the LOADED dylib's code by symbol — stale
//! pointers would execute unloaded pages (loud crash, not a silent pass).

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;

use oppa::{ComponentHost, Event, EventKind, HandlerId, OpaqueProps, SymbolHash};
use oppa_reload::{ComponentSource, DylibSource, HotRegistry};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixture")
        .join("hot-fixture")
}

/// Builds the fixture cdylib (`features`: "" or "v2") and stages it under
/// a versioned filename (v1 and v2 must coexist as files). The artifact
/// path is discovered from cargo's JSON output — the fixture builds into
/// the enclosing target dir, which varies by environment.
///
/// Profile match (round 6.1, release verification): the harness's
/// rlib↔dylib crossings rely on `TypeId` equality for same-crate
/// types (mount fallback, pre-swap `set_props`, drain glue) — and
/// `TypeId`s diverge across profiles. A release test with debug
/// dylibs panics in the props guard (same names, different ids —
/// the guard working as designed). The fixture therefore builds
/// with the test's own profile (`cfg!(debug_assertions)` reads it
/// at compile time — no env plumbing through the child cargo).
fn build_fixture(features: &str, tag: &str) -> PathBuf {
    let dir = fixture_dir();
    // Touch: cargo emits no compiler-artifact JSON for a fresh build,
    // and the artifact path is only discoverable from that output.
    let lib_rs = dir.join("src").join("lib.rs");
    let now = std::time::SystemTime::now();
    let touched = std::fs::File::options()
        .append(true)
        .open(&lib_rs)
        .and_then(|f| f.set_modified(now))
        .is_ok();
    assert!(touched, "can touch fixture lib.rs");
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .arg("--message-format=json")
        .current_dir(&dir);
    if !features.is_empty() {
        cmd.arg("--features").arg(features);
    }
    if !cfg!(debug_assertions) {
        cmd.arg("--release");
    }
    let output = cmd.output().expect("cargo builds the hot fixture");
    assert!(
        output.status.success(),
        "fixture build ({tag}) failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let built = artifact_path(&stdout).expect("fixture cdylib artifact in cargo JSON");
    assert!(built.exists(), "fixture artifact missing: {built:?}");

    let staged = std::env::temp_dir().join(format!(
        "oppa_hot_swap_{}_{tag}{}",
        std::process::id(),
        std::env::consts::DLL_SUFFIX
    ));
    std::fs::copy(&built, &staged).expect("stage fixture dylib");
    staged
}

/// Scans cargo `--message-format=json` lines for the hot-fixture cdylib
/// artifact (no serde — a targeted string scan over `filenames`).
fn artifact_path(json_lines: &str) -> Option<PathBuf> {
    let suffix = std::env::consts::DLL_SUFFIX;
    for line in json_lines.lines() {
        if !line.contains("\"name\":\"hot_fixture\"") || !line.contains("compiler-artifact") {
            continue;
        }
        // …"filenames":["C:\…\hot_fixture.dll", …]…
        let mut rest = line;
        while let Some(at) = rest.find("\"filenames\":[") {
            rest = &rest[at + 13..];
            let end = rest.find(']')?;
            for file in rest[..end].split(',') {
                let file = file.trim().trim_matches('"');
                if file.ends_with(suffix) && !file.ends_with(".dll.lib") {
                    return Some(PathBuf::from(file));
                }
            }
        }
    }
    None
}

#[test]
fn real_dylib_swap_adopts_across_unload() {
    if std::env::var("OPPA_SKIP_REAL_DYLIB").is_ok() {
        eprintln!("OPPA_SKIP_REAL_DYLIB set — skipping");
        return;
    }
    let v1_path = build_fixture("", "v1");
    let v2_path = build_fixture("v2", "v2");

    let host = ComponentHost::new();
    let rt = host.runtime();
    let counter = rt.signal(10u32);
    let probe = rt.signal(0u32);
    // Mount through the rlib copy's types/fn; post-swap runs resolve the
    // loaded dylib by symbol (stale pointers would crash or misbehave).
    let handle = host.mount(
        "Counter",
        hot_fixture::CounterProps {
            label: Arc::from("counter"),
            initial: 7,
            counter: counter.clone(),
            probe: probe.clone(),
        },
        hot_fixture::Counter,
    );
    host.run_until_idle();
    assert_eq!(probe.get(), 17);

    // v1 loaded: manifest holds Counter + OldOnly.
    let v1 = unsafe { DylibSource::load(&v1_path).expect("load v1 dylib") };
    let symbols: Vec<SymbolHash> = v1.entries().iter().map(|d| d.symbol).collect();
    assert!(symbols.contains(&SymbolHash::of("Counter")));
    assert!(symbols.contains(&SymbolHash::of("OldOnly")));
    let mut reg = HotRegistry::new(host.clone());
    reg.install(Box::new(v1));

    // Drive the author-owned signal, then props, pre-swap.
    counter.set(41);
    host.run_until_idle();
    assert_eq!(probe.get(), 48);
    handle.set_props(hot_fixture::CounterProps {
        label: Arc::from("counter"),
        initial: 100,
        counter: counter.clone(),
        probe: probe.clone(),
    });
    host.run_until_idle();
    assert_eq!(probe.get(), 48); // local keeps 7

    // Real swap: v1 retired (stays mapped — M2b retire model), v2
    // loaded + rescanned.
    let v2 = unsafe { DylibSource::load(&v2_path).expect("load v2 dylib") };
    let report = reg.reload_to(Box::new(v2));
    assert!(report.ok(), "real swap evicted: {:?}", report.evicted);
    assert_eq!(report.drained, 1);
    assert_eq!(report.adopted, 1);
    assert_eq!(report.retired_images, 1);

    // Edited v2 body re-seeds `local` to 100; the author-owned counter
    // signal survives the swap (41). This ran the LOADED v2 pages —
    // stale v1 code would have kept 7 (probe 48, not 141).
    assert_eq!(probe.get(), 141);
    assert_eq!(counter.get(), 41);

    // Dependency tracking survives the image boundary (per-runtime run
    // stacks, not per-image TLS): a post-swap counter write re-runs the
    // v2 component. Without it the probe would freeze at 141.
    counter.set(42);
    host.run_until_idle();
    assert_eq!(probe.get(), 142);

    // Manifest-scan discovery: NewOnly appears, OldOnly is gone.
    let symbols = reg.current_symbols();
    assert!(symbols.contains(&SymbolHash::of("NewOnly")));
    assert!(symbols.contains(&SymbolHash::of("Counter")));
    assert!(!symbols.contains(&SymbolHash::of("OldOnly")));

    // Mount the discovered component through its SCANNED render entry
    // (fresh host: M2 reconciler is single-root). The test-constructed
    // props carry the rlib `TypeId`; the dylib render needs the dylib
    // stamp — so they cross through the entry's adopt glue (same path as
    // swap adopt: layout equality + type-name check + restamp). Passing
    // rlib-stamped props straight in would (correctly) fail the downcast.
    let host2 = ComponentHost::new();
    let gen = host2.runtime().generation();
    let entry = reg
        .find_entry(SymbolHash::of("NewOnly"))
        .expect("NewOnly scanned");
    let stamped = restamp(&entry, hot_fixture::EmptyProps, gen);
    let inst = host2.mount_erased("NewOnly", stamped, std::rc::Rc::new(entry.render));
    host2.run_until_idle();
    assert!(host2.lookup_child(inst, 0).is_none()); // childless, but alive
    assert_eq!(host2.retained_count(), 1);

    // Handler re-resolution across the real unload.
    let fired = Rc::new(RefCell::new(0u32));
    let id = HandlerId::from_symbol("app.flip.real");
    let fired_old = fired.clone();
    rt.register_handler(id, move || *fired_old.borrow_mut() += 1);
    rt.dispatch(Event {
        kind: EventKind::Press,
        handler: id,
    });
    assert_eq!(*fired.borrow(), 1);
    let fired_new = fired.clone();
    rt.register_handler(id, move || *fired_new.borrow_mut() += 10);
    rt.dispatch(Event {
        kind: EventKind::Press,
        handler: id,
    });
    assert_eq!(*fired.borrow(), 11);

    // Stage cleanup (best-effort; temp dir otherwise).
    let _ = std::fs::remove_file(&v1_path);
    let _ = std::fs::remove_file(&v2_path);
}

/// Re-stamps a test-constructed props value through a scanned entry's
/// adopt glue: boxes the concrete value, hands the thin pointer over
/// with its type name, and takes back freshly-stamped `OpaqueProps`.
/// The name check + same-toolchain layout equality make this sound;
/// a mismatch returns null (loud failure below, never silent
/// reinterpretation, never a cross-boundary panic).
fn restamp<P: Clone + 'static>(
    entry: &oppa::ComponentDesc,
    value: P,
    gen: oppa::HotGeneration,
) -> OpaqueProps {
    let boxed = Box::new(value);
    let drained = oppa::DrainedProps {
        ptr: Box::into_raw(boxed) as *mut std::ffi::c_void,
    };
    let expected =
        std::ffi::CString::new(std::any::type_name::<P>()).expect("type name holds no nul");
    // SAFETY: same-toolchain layout equality + the glue's own name check;
    // the provider image is held alive by the harness.
    let raw = unsafe { (entry.adopt_props)(drained, gen, expected.as_ptr()) };
    assert!(!raw.is_null(), "adopt refused test-constructed props");
    unsafe { *Box::from_raw(raw) }
}
