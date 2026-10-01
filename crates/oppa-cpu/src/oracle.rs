//! Headless image-diff / full-repaint-assert oracle (M4 §3: the fourth
//! pseudo-backend — debug full-repaint + image-diff mode).
//!
//! The oracle paints the incremental plan and the full-repaint plan to two
//! surfaces of one backend and byte-compares the pixmaps. Identical bytes
//! prove the damage discipline rebuilt exactly what a full repaint would
//! have painted. It becomes the permanent CI substrate (M8 consumes it
//! frame-by-frame under transitions).

use oppa::{BackendError, FramePlan, RendererBackend, SurfaceDesc, SurfaceId, TreeDiff};

use crate::backend::CpuBackend;

/// Debug full-repaint + image-diff session over one [`CpuBackend`].
pub struct OracleSession {
    backend: CpuBackend,
    incr: SurfaceId,
    full: SurfaceId,
}

impl OracleSession {
    pub fn new(desc: SurfaceDesc) -> Result<Self, BackendError> {
        let mut backend = CpuBackend::new();
        let incr = backend.create_surface(desc)?;
        let full = backend.create_surface(desc)?;
        Ok(Self {
            backend,
            incr,
            full,
        })
    }

    pub fn backend_mut(&mut self) -> &mut CpuBackend {
        &mut self.backend
    }

    pub fn surfaces(&self) -> (SurfaceId, SurfaceId) {
        (self.incr, self.full)
    }

    /// Commits the same diff history to both arms (the full arm replays
    /// from a fresh display list; the incremental arm splices).
    pub fn commit_all(&mut self, diffs: &[TreeDiff]) -> Result<(), BackendError> {
        for d in diffs {
            self.backend.commit(d)?;
        }
        Ok(())
    }

    /// Paints `incr` on the incremental surface and `full` (with
    /// `full_repaint` set) on the reference surface; returns the count of
    /// differing pixels (0 = identical).
    pub fn assert_paints(
        &mut self,
        incr: &FramePlan,
        full: &FramePlan,
    ) -> Result<usize, BackendError> {
        debug_assert!(full.full_repaint, "oracle reference must be a full repaint");
        self.backend.paint(self.incr, incr)?;
        self.backend.paint(self.full, full)?;
        Ok(self.diff_pixels())
    }

    pub fn diff_pixels(&self) -> usize {
        let (Some(a), Some(b)) = (
            self.backend.pixmap(self.incr),
            self.backend.pixmap(self.full),
        ) else {
            return usize::MAX;
        };
        image_diff_count(a, b)
    }
}

/// Byte-level pixmap compare: count of differing pixels (usize::MAX when
/// sizes differ — never silent equality on mismatched surfaces).
pub fn image_diff_count(a: &tiny_skia::Pixmap, b: &tiny_skia::Pixmap) -> usize {
    if a.width() != b.width() || a.height() != b.height() {
        return usize::MAX;
    }
    a.pixels()
        .iter()
        .zip(b.pixels().iter())
        .filter(|(x, y)| x != y)
        .count()
}
