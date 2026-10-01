//! Plain-text clipboard (G3 — decisions 209–211).
//!
//! The v2 handoff verified zero clipboard hits tree-wide (core + all
//! shells), blocking G1's usefulness directly. This module is the
//! product seam: a [`Clipboard`] trait every shell implements (or
//! loudly refuses), an [`InMemoryClipboard`] for tests/headless, and
//! the [`EditSession`](crate::editing::EditSession) copy/cut/paste ops
//! that consume it.
//!
//! Design (see decisions 209–211; rationale in git history):
//!
//! - **Async-capable, sync-friendly (209).** `write_text` is fire-and-
//!   forget (every platform can queue a write synchronously, including
//!   the web's `navigator.clipboard.writeText` promise, which a shell
//!   fires without awaiting). Reads split into `request_read` +
//!   `poll_read`: sync platforms resolve immediately (poll returns
//!   `Some` on the first call); async platforms (web) return `None`
//!   until the promise settles. `read_text_now` is the sync convenience
//!   — it returns [`ClipboardError::Pending`] instead of blocking, so a
//!   caller can never hang the UI thread by accident. Plain text only
//!   (rich formats are a later round, not smuggled in).
//! - **Loud failures.** Unsupported shells fail as
//!   `Err(ClipboardError::Unsupported)`, never silent no-ops; backend
//!   failures carry the OS message in `ClipboardError::Backend`.
//! - **Shell seam (210).**
//!   [`PlatformShell::clipboard`](crate::shell::PlatformShell::clipboard)
//!   returns `None` by default (every pre-G3 shell keeps compiling and
//!   refuses loudly through the `None`); shells with a real backend
//!   return `Some`. This round wires Win32 (verifiable on this
//!   machine); Linux/Android/Web are open questions OQ-G3-1..3.

use std::fmt;

/// Clipboard failure (loud by construction — see
/// `docs/CONTRIBUTING.md`).
#[derive(Clone, Debug, PartialEq)]
pub enum ClipboardError {
    /// This shell has no clipboard backend yet (default seam state).
    /// The `&'static str` names the shell (e.g. `"LinuxShell"`).
    Unsupported(&'static str),
    /// An async read was polled before it settled — retry next frame,
    /// never block the UI thread.
    Pending,
    /// The OS call failed; the string is the backend's message.
    Backend(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClipboardError::Unsupported(who) => {
                write!(f, "{who} has no clipboard backend — refusal, never silent")
            }
            ClipboardError::Pending => {
                write!(
                    f,
                    "clipboard read still pending — retry next frame, never block"
                )
            }
            ClipboardError::Backend(msg) => write!(f, "clipboard backend failed: {msg}"),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// Plain-text clipboard backend (decision 209). `!Send` like every
/// UI-thread type (backends call thread-affine OS APIs).
pub trait Clipboard {
    /// Queue a write (fire-and-forget; shells flush synchronously or
    /// fire the platform promise without awaiting — never blocks).
    /// Fails loudly as `Err` when the backend refuses (e.g. another
    /// app holds the Win32 clipboard open — a routine transient the
    /// app retries next frame, never a silent drop).
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError>;

    /// Drop the contents (an empty clipboard reads as `Ok(None)`).
    fn clear(&mut self) -> Result<(), ClipboardError>;

    /// Begin an async read. Sync backends resolve immediately (their
    /// next `poll_read` returns `Some`); async backends (web) fire the
    /// platform promise and settle later.
    fn request_read(&mut self);

    /// Poll the pending read: `None` = not settled yet (async only);
    /// `Some(Ok(text))` = settled (`None` text = empty clipboard);
    /// `Some(Err(e))` = refused/failed loudly. Polling with no
    /// outstanding request re-reads synchronously on sync backends and
    /// returns `Some(Err(Pending))` on async ones that need a request
    /// first — both loud, never a silent empty.
    fn poll_read(&mut self) -> Option<Result<Option<String>, ClipboardError>>;

    /// Sync convenience: `request_read` + one `poll_read`. Returns
    /// `Err(Pending)` on unsettled async reads instead of blocking.
    fn read_text_now(&mut self) -> Result<Option<String>, ClipboardError> {
        self.request_read();
        match self.poll_read() {
            Some(result) => result,
            None => Err(ClipboardError::Pending),
        }
    }

    /// Service peer requests without blocking (X11 selection
    /// ownership — Round 2.2, OQ-G3-1): the OS asks us for our text
    /// when another app pastes while we own the selection. Default:
    /// no-op returning false (most backends are request-driven and
    /// hold no servable state). Returns true when the backend wants
    /// idle wakeups (it owns a servable selection) so runners poll
    /// on a timer instead of spinning. Must never block — a peer
    /// that never answers is its own timeout, never ours.
    fn service(&mut self) -> bool {
        false
    }
}

/// Headless/test clipboard: everything resolves synchronously.
#[derive(Clone, Debug, Default)]
pub struct InMemoryClipboard {
    text: Option<String>,
    outstanding: bool,
}

impl InMemoryClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Preloads the clipboard (test setup).
    pub fn with_text(text: &str) -> Self {
        Self {
            text: Some(text.to_string()),
            outstanding: false,
        }
    }
}

impl Clipboard for InMemoryClipboard {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.text = Some(text.to_string());
        Ok(())
    }

    fn clear(&mut self) -> Result<(), ClipboardError> {
        self.text = None;
        Ok(())
    }

    fn request_read(&mut self) {
        self.outstanding = true;
    }

    fn poll_read(&mut self) -> Option<Result<Option<String>, ClipboardError>> {
        if !self.outstanding {
            // No request: re-read synchronously (sync-backend rule).
            return Some(Ok(self.text.clone()));
        }
        self.outstanding = false;
        Some(Ok(self.text.clone()))
    }

    fn read_text_now(&mut self) -> Result<Option<String>, ClipboardError> {
        Ok(self.text.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_round_trip_and_clear() {
        let mut c = InMemoryClipboard::new();
        assert_eq!(c.read_text_now().expect("empty reads"), None);
        c.write_text("hi").expect("writes");
        assert_eq!(c.read_text_now().expect("reads"), Some("hi".to_string()));
        c.clear().expect("clears");
        assert_eq!(c.read_text_now().expect("cleared reads"), None);
    }

    #[test]
    fn memory_request_then_poll_settles_immediately() {
        let mut c = InMemoryClipboard::with_text("x");
        c.request_read();
        assert_eq!(
            c.poll_read(),
            Some(Ok(Some("x".to_string()))),
            "sync backend settles on first poll"
        );
    }

    /// A minimal async-shaped stub: armed by `request_read`, `None`
    /// until `settle` delivers — the web backend's contract in miniature.
    struct AsyncStub {
        armed: bool,
        settled: Option<Option<String>>,
    }

    impl Clipboard for AsyncStub {
        fn write_text(&mut self, _text: &str) -> Result<(), ClipboardError> {
            Ok(())
        }
        fn clear(&mut self) -> Result<(), ClipboardError> {
            Ok(())
        }
        fn request_read(&mut self) {
            self.armed = true;
        }
        fn poll_read(&mut self) -> Option<Result<Option<String>, ClipboardError>> {
            if !self.armed {
                return Some(Err(ClipboardError::Pending));
            }
            self.settled.clone().map(Ok)
        }
    }

    #[test]
    fn async_shape_pends_then_settles() {
        let mut c = AsyncStub {
            armed: false,
            settled: None,
        };
        // No request: loud Pending, never silent empty.
        assert_eq!(c.poll_read(), Some(Err(ClipboardError::Pending)));
        c.request_read();
        assert_eq!(c.poll_read(), None, "in flight → None, not an error");
        c.settled = Some(Some("late".to_string()));
        assert_eq!(
            c.poll_read(),
            Some(Ok(Some("late".to_string()))),
            "settled value delivered"
        );
        // `read_text_now` never blocks: Pending surfaces as Err.
        c.settled = None;
        assert_eq!(c.read_text_now(), Err(ClipboardError::Pending));
    }
}
