//! Android file-open-by-bytes (decision 423): pure SAF option
//! mapping + pending/resolve rendezvous over
//! [`PickedFile`](oppa::PickedFile).
//!
//! No JNI here (std + `oppa` only — the `app_dirs` precedent):
//! the app crate fires `ACTION_OPEN_DOCUMENT` on the UI thread and
//! feeds `ContentResolver` bytes into
//! [`AndroidFileOpener::resolve_bytes`]. Intent launch/result
//! routing stays device-side (Blocked — needs `startActivity` +
//! `onActivityResult` plumbing that does not exist yet); this
//! module owns the host-provable half — option mapping,
//! generation-gated resolve (stale discards, never half-swaps),
//! supersede, dismissal-is-data, level-triggered re-poll.

use oppa::{FileOpener, FilePickerOptions, PickError, PickedFile};

/// SAF open-document action (fired device-side with the spec
/// below — kept here so the intent string lives in one place).
pub const ACTION_OPEN_DOCUMENT: &str = "android.intent.action.OPEN_DOCUMENT";
/// Openable category (files as streams — directories never arrive
/// through this action, stated).
pub const CATEGORY_OPENABLE: &str = "android.intent.category.OPENABLE";

/// Pure SAF open spec (what the JNI bridge puts into the intent):
/// `title` rides `EXTRA_TITLE`, `patterns` ride `EXTRA_MIME_TYPES`
/// after the bridge's extension→MIME translation (this layer never
/// invents MIME mappings — patterns pass through verbatim),
/// `allow_multiple` rides `EXTRA_ALLOW_MULTIPLE`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SafOpenSpec {
    pub title: String,
    pub patterns: Vec<String>,
    pub allow_multiple: bool,
}

/// Maps open options to the SAF spec (pure — host-tested): title
/// passes through, filter patterns flatten in order, multi-select
/// rides the flag. `initial_dir` is a hint the SAF action ignores
/// (stated, never silently honored).
pub fn saf_spec_for(options: &FilePickerOptions) -> SafOpenSpec {
    SafOpenSpec {
        title: options.title.clone(),
        patterns: options
            .filters
            .iter()
            .flat_map(|f| f.patterns.iter().cloned())
            .collect(),
        allow_multiple: options.multiple,
    }
}

/// Android bytes opener: pending/resolve rendezvous over the
/// [`FileOpener`] request/poll shape (the fetch generation
/// precedent — resolve carries its generation, stale results
/// discard to `None`-equivalent `false`, never half-swapped
/// state). `!Send` like every UI-thread type.
#[derive(Debug, Default)]
pub struct AndroidFileOpener {
    generation: u64,
    pending: Option<u64>,
    last: Option<Result<Vec<PickedFile>, PickError>>,
    seen_specs: Vec<SafOpenSpec>,
}

impl AndroidFileOpener {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current generation (minted per request — the JNI bridge tags
    /// its intent/requestCode with this so results route back).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Outstanding request generation, if a picker is open.
    pub fn pending_generation(&self) -> Option<u64> {
        self.pending
    }

    /// Specs received so far, in order (tests prove filters,
    /// titles, and multi-select reached the bridge).
    pub fn seen_specs(&self) -> &[SafOpenSpec] {
        &self.seen_specs
    }

    /// Resolves the outstanding request from the device bridge
    /// (ContentResolver bytes or dismissal `Ok(vec![])` / failure
    /// `Err`). Stale generations discard (`false`, state
    /// untouched); the current generation applies, clears pending,
    /// and latches the level-triggered last result (`true`).
    pub fn resolve_bytes(
        &mut self,
        generation: u64,
        result: Result<Vec<PickedFile>, PickError>,
    ) -> bool {
        if self.pending != Some(generation) {
            return false;
        }
        self.pending = None;
        self.last = Some(result);
        true
    }
}

impl FileOpener for AndroidFileOpener {
    fn request_bytes(&mut self, options: FilePickerOptions) {
        // Supersede: a second request replaces the first (the
        // `FileDialog` rule — stale resolves discard by
        // generation, so no orphan picker can settle twice).
        self.generation += 1;
        self.pending = Some(self.generation);
        self.seen_specs.push(saf_spec_for(&options));
    }

    fn poll_bytes(&mut self) -> Option<Result<Vec<PickedFile>, PickError>> {
        if self.pending.is_some() {
            return None;
        }
        self.last.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oppa::FileFilter;

    fn opts() -> FilePickerOptions {
        FilePickerOptions {
            title: "Open".to_string(),
            filters: vec![FileFilter {
                name: "Images".to_string(),
                patterns: vec!["*.png".to_string(), "*.jpg".to_string()],
            }],
            multiple: true,
            initial_dir: None,
        }
    }

    #[test]
    fn spec_carries_title_patterns_and_multiple() {
        let spec = saf_spec_for(&opts());
        assert_eq!(spec.title, "Open");
        assert_eq!(
            spec.patterns,
            vec!["*.png".to_string(), "*.jpg".to_string()]
        );
        assert!(spec.allow_multiple);
        assert_eq!(ACTION_OPEN_DOCUMENT, "android.intent.action.OPEN_DOCUMENT");
        assert_eq!(CATEGORY_OPENABLE, "android.intent.category.OPENABLE");
    }

    #[test]
    fn pending_poll_is_none_and_resolve_settles() {
        let mut d = AndroidFileOpener::new();
        d.request_bytes(opts());
        let gen = d.generation();
        assert_eq!(d.pending_generation(), Some(gen));
        assert_eq!(d.poll_bytes(), None, "open picker stays None while open");
        assert_eq!(d.seen_specs().len(), 1);
        let files = vec![PickedFile::new("a.png".to_string(), vec![1, 2])];
        assert!(d.resolve_bytes(gen, Ok(files.clone())));
        assert_eq!(d.pending_generation(), None);
        assert_eq!(d.poll_bytes(), Some(Ok(files)));
        // Level-triggered: re-poll repeats the last result.
        assert_eq!(
            d.poll_bytes().expect("settled"),
            Ok(vec![PickedFile::new("a.png".to_string(), vec![1, 2])])
        );
    }

    #[test]
    fn stale_resolve_discards_and_supersede_wins() {
        let mut d = AndroidFileOpener::new();
        d.request_bytes(FilePickerOptions::default());
        let first = d.generation();
        d.request_bytes(FilePickerOptions::default());
        let second = d.generation();
        assert_ne!(first, second);
        assert!(!d.resolve_bytes(first, Ok(vec![])), "stale discards");
        assert_eq!(d.poll_bytes(), None, "stale never settles");
        assert!(
            d.resolve_bytes(second, Ok(vec![])),
            "current applies (dismissed)"
        );
        assert_eq!(d.poll_bytes(), Some(Ok(vec![])));
    }

    #[test]
    fn backend_failure_resolves_loudly() {
        let mut d = AndroidFileOpener::new();
        d.request_bytes(FilePickerOptions::default());
        let gen = d.generation();
        assert!(d.resolve_bytes(gen, Err(PickError::Backend("resolver died".to_string()))));
        assert_eq!(
            d.poll_bytes(),
            Some(Err(PickError::Backend("resolver died".to_string())))
        );
    }
}
