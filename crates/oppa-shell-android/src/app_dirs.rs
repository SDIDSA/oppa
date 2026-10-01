//! App-private scoped storage (Round 3.4): files/cache directory
//! resolution with fallback, plus round-trip validation.
//!
//! Split where it is provable (the 3.1 `ime_queue` precedent):
//! everything here is pure Rust over `std::path` + the core
//! [`NativeFs`](oppa::store::NativeFs) seam — host-tested, no JNI
//! linkage. The thin JNI getters (`getFilesDir`/`getCacheDir`
//! strings) live in `oppa-android-app`, which feeds their outputs
//! into [`resolve_app_dirs`] and runs [`validate_dirs`] on device.
//!
//! Scoped-storage rules (stated):
//!
//! - `files` holds persistent app data (proof outputs, settings);
//!   `cache` holds regenerable data the OS may reclaim (no
//!   `MediaStore`/shared-storage access in v1 — app-private only,
//!   never a silent shared write).
//! - Resolution order per dir: explicit JNI path first, then the
//!   `internal_data_path` fallback (`files` uses it directly;
//!   `cache` uses `<internal>/cache`), else loud `Err` (no `/tmp`
//!   fallback on device — a missing data dir is a platform fact,
//!   not a default).
//! - Validation round-trips write/read/delete (+ list) through
//!   `NativeFs` in both dirs and reports verbatim (a failing dir
//!   fails the phase loudly — the proof loop never writes outputs
//!   somewhere unverified).

use std::path::PathBuf;

use oppa::store::{FsSandbox, NativeFs};

/// Resolved app-private directories + where each came from (the
/// record path reports sources verbatim — fallback is visible,
/// never silent).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppDirs {
    pub files_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub files_source: DirSource,
    pub cache_source: DirSource,
}

/// Where one resolved dir came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirSource {
    /// Explicit JNI path (`getFilesDir` / `getCacheDir`).
    Jni,
    /// `internal_data_path` fallback (`files` directly,
    /// `cache` as `<internal>/cache`).
    InternalFallback,
}

impl std::fmt::Display for DirSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DirSource::Jni => write!(f, "jni"),
            DirSource::InternalFallback => write!(f, "internal-fallback"),
        }
    }
}

/// Resolves both dirs from explicit JNI paths with the internal
/// fallback (pure — the JNI getters feed `Option<PathBuf>` in;
/// `None`/empty means that getter failed or was skipped).
/// Both missing with no internal path is a loud `Err`.
pub fn resolve_app_dirs(
    internal_data_path: Option<PathBuf>,
    jni_files_dir: Option<PathBuf>,
    jni_cache_dir: Option<PathBuf>,
) -> Result<AppDirs, String> {
    let clean = |p: PathBuf| (!p.as_os_str().is_empty()).then_some(p);
    let jni_files = jni_files_dir.and_then(clean);
    let jni_cache = jni_cache_dir.and_then(clean);
    let internal = internal_data_path.and_then(clean);
    let (files_dir, files_source) = match (jni_files, &internal) {
        (Some(p), _) => (p, DirSource::Jni),
        (None, Some(root)) => (root.clone(), DirSource::InternalFallback),
        (None, None) => {
            return Err(
                "no files dir: JNI getFilesDir failed and no internal_data_path fallback"
                    .to_string(),
            );
        }
    };
    let (cache_dir, cache_source) = match (jni_cache, &internal) {
        (Some(p), _) => (p, DirSource::Jni),
        (None, Some(root)) => (root.join("cache"), DirSource::InternalFallback),
        (None, None) => {
            return Err(
                "no cache dir: JNI getCacheDir failed and no internal_data_path fallback"
                    .to_string(),
            );
        }
    };
    Ok(AppDirs {
        files_dir,
        cache_dir,
        files_source,
        cache_source,
    })
}

/// One-line record for the run log (`files=<path> (<source>)
/// cache=<path> (<source>)`).
pub fn dirs_record(dirs: &AppDirs) -> String {
    format!(
        "files={} ({}) cache={} ({})",
        dirs.files_dir.display(),
        dirs.files_source,
        dirs.cache_dir.display(),
        dirs.cache_source,
    )
}

/// Round-trips both dirs through `NativeFs` (write → read →
/// list → delete → absent) and returns the verbatim record.
/// Any failure is a loud `Err` naming the dir and step (outputs
/// must never land somewhere unverified).
pub fn validate_dirs(dirs: &AppDirs) -> Result<String, String> {
    let files_report = round_trip(&dirs.files_dir, "files")?;
    let cache_report = round_trip(&dirs.cache_dir, "cache")?;
    Ok(format!("storage-ok {files_report} {cache_report}"))
}

fn round_trip(dir: &std::path::Path, tag: &str) -> Result<String, String> {
    let mut fs =
        NativeFs::new(dir.to_path_buf()).map_err(|e| format!("{tag} sandbox root: {e}"))?;
    let probe = "oppa-storage-probe/roundtrip.txt";
    let payload = format!("{tag}-probe").into_bytes();
    fs.write(probe, &payload)
        .map_err(|e| format!("{tag} write: {e}"))?;
    let back = fs.read(probe).map_err(|e| format!("{tag} read: {e}"))?;
    if back != payload {
        return Err(format!("{tag} readback mismatch"));
    }
    let listed = fs
        .list("oppa-storage-probe/")
        .map_err(|e| format!("{tag} list: {e}"))?;
    if !listed.iter().any(|p| p == probe) {
        return Err(format!("{tag} probe missing from list"));
    }
    fs.remove(probe).map_err(|e| format!("{tag} delete: {e}"))?;
    if fs.exists(probe).map_err(|e| format!("{tag} exists: {e}"))? {
        return Err(format!("{tag} probe survives delete"));
    }
    Ok(format!("{tag}=write/read/list/delete-ok"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rooted(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "oppa-appdirs-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn jni_paths_win_internal_falls_back_empty_refuses() {
        let internal = rooted("internal");
        // Explicit JNI paths win outright (no internal needed).
        let dirs = resolve_app_dirs(
            None,
            Some(PathBuf::from("/data/data/pkg/files")),
            Some(PathBuf::from("/data/data/pkg/cache")),
        )
        .expect("jni resolves");
        assert_eq!(dirs.files_source, DirSource::Jni);
        assert_eq!(dirs.cache_source, DirSource::Jni);
        // Missing JNI falls back to internal (cache nests under it).
        let dirs = resolve_app_dirs(None, None, None).expect_err("nothing resolves to nothing");
        assert!(dirs.contains("no files dir"), "{dirs}");
        let dirs = resolve_app_dirs(Some(internal.clone()), None, None).expect("falls back");
        assert_eq!(dirs.files_dir, internal);
        assert_eq!(dirs.files_source, DirSource::InternalFallback);
        assert_eq!(dirs.cache_dir, internal.join("cache"));
        assert_eq!(dirs.cache_source, DirSource::InternalFallback);
        // Mixed: JNI files + fallback cache.
        let dirs = resolve_app_dirs(
            Some(internal.clone()),
            Some(PathBuf::from("/j/files")),
            None,
        )
        .expect("mixes");
        assert_eq!(dirs.files_dir, PathBuf::from("/j/files"));
        assert_eq!(dirs.cache_dir, internal.join("cache"));
        // Empty strings count as missing (a getter that hands back
        // "" is a failed getter, never a root).
        let dirs = resolve_app_dirs(
            Some(internal.clone()),
            Some(PathBuf::from("")),
            Some(PathBuf::from("")),
        )
        .expect("empty jni falls back");
        assert_eq!(dirs.files_source, DirSource::InternalFallback);
    }

    #[test]
    fn validation_round_trips_both_dirs_for_real() {
        let root = rooted("validate");
        let dirs = AppDirs {
            files_dir: root.join("files"),
            cache_dir: root.join("cache"),
            files_source: DirSource::InternalFallback,
            cache_source: DirSource::InternalFallback,
        };
        let report = validate_dirs(&dirs).expect("round-trips");
        assert!(
            report.contains("files=write/read/list/delete-ok"),
            "{report}"
        );
        assert!(
            report.contains("cache=write/read/list/delete-ok"),
            "{report}"
        );
        assert_eq!(
            dirs_record(&dirs),
            format!(
                "files={} (internal-fallback) cache={} (internal-fallback)",
                dirs.files_dir.display(),
                dirs.cache_dir.display(),
            )
        );
        std::fs::remove_dir_all(&root).expect("test cleans its sandbox");
    }

    #[test]
    fn validation_names_the_failing_step() {
        // A file where the sandbox root must go fails loudly at
        // root creation (not somewhere downstream).
        let clutter = rooted("clutter-file");
        std::fs::write(&clutter, b"x").expect("clutter");
        let dirs = AppDirs {
            files_dir: clutter.clone(),
            cache_dir: clutter.join("cache"),
            files_source: DirSource::Jni,
            cache_source: DirSource::Jni,
        };
        let err = validate_dirs(&dirs).expect_err("root is a file");
        assert!(err.contains("files sandbox root"), "{err}");
        std::fs::remove_file(&clutter).expect("test cleans");
    }
}
