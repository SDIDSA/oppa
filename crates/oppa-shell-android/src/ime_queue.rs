//! JNI text-entry queue (Round 3.1): the Android `InputConnection`
//! proxy calls into Rust on the IME thread; the event pump drains on
//! the loop thread. std-only, no JNI linkage — the `extern` entry
//! points in `oppa-android-app` push here, the pump drains here, so
//! every rule below is host-testable.
//!
//! Ordering is FIFO per push call (one `InputConnection` callback =
//! one item; the proxy never batches). The queue is unbounded —
//! callbacks are keystroke-rate, the pump drains per frame, and a
//! cap would silently drop text (loud rule: never lose a commit).

use std::collections::VecDeque;
use std::sync::Mutex;

/// One JNI text entry (mirrors the `InputConnection` primitives the
/// proxy forwards — nothing else crosses).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeTextItem {
    /// `commitText` payload (replacement selection is applied by the
    /// session insert downstream — the queue carries bytes only).
    CommitText(String),
    /// `deleteSurroundingText` char counts.
    DeleteSurrounding { before_chars: u32, after_chars: u32 },
}

/// Thread-safe FIFO from the IME thread to the pump.
#[derive(Debug, Default)]
pub struct ImeTextQueue {
    items: Mutex<VecDeque<ImeTextItem>>,
}

impl ImeTextQueue {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(VecDeque::new()),
        }
    }

    /// Pushes one entry (the JNI callback path — never blocks; a
    /// poisoned mutex fails loudly instead of swallowing text).
    pub fn push(&self, item: ImeTextItem) {
        self.items
            .lock()
            .expect("ime text queue mutex poisoned")
            .push_back(item);
    }

    /// Pushes a commit (the `onCommitText` native entry's body).
    pub fn push_commit(&self, text: String) {
        self.push(ImeTextItem::CommitText(text));
    }

    /// Pushes a surrounding delete (the `onDeleteSurroundingText`
    /// native entry's body).
    pub fn push_delete(&self, before_chars: u32, after_chars: u32) {
        self.push(ImeTextItem::DeleteSurrounding {
            before_chars,
            after_chars,
        });
    }

    /// Drains everything in FIFO order (the pump path — one drain
    /// per frame, then each item becomes shell intake).
    pub fn drain(&self) -> Vec<ImeTextItem> {
        self.items
            .lock()
            .expect("ime text queue mutex poisoned")
            .drain(..)
            .collect()
    }

    /// Pending count (diagnostics/tests — the pump never branches
    /// on it).
    pub fn len(&self) -> usize {
        self.items
            .lock()
            .expect("ime text queue mutex poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_order_across_kinds() {
        let q = ImeTextQueue::new();
        assert!(q.is_empty());
        q.push_commit("ni".to_string());
        q.push_delete(1, 0);
        q.push_commit("\u{4F60}".to_string());
        assert_eq!(q.len(), 3);
        assert_eq!(
            q.drain(),
            vec![
                ImeTextItem::CommitText("ni".to_string()),
                ImeTextItem::DeleteSurrounding {
                    before_chars: 1,
                    after_chars: 0,
                },
                ImeTextItem::CommitText("\u{4F60}".to_string()),
            ]
        );
        assert!(q.is_empty());
        assert!(q.drain().is_empty(), "empty drain is empty, not an error");
    }

    #[test]
    fn concurrent_pushes_land_exactly_once() {
        let q = std::sync::Arc::new(ImeTextQueue::new());
        let mut handles = Vec::new();
        for t in 0..8 {
            let q = q.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..50 {
                    q.push_commit(format!("t{t}-{i}"));
                }
            }));
        }
        for h in handles {
            h.join().expect("push thread joins");
        }
        let items = q.drain();
        assert_eq!(items.len(), 400, "no push lost, none duplicated");
        let mut texts: Vec<String> = items
            .into_iter()
            .map(|item| match item {
                ImeTextItem::CommitText(t) => t,
                ImeTextItem::DeleteSurrounding { .. } => panic!("only commits pushed"),
            })
            .collect();
        texts.sort();
        assert_eq!(texts[0], "t0-0");
        assert_eq!(texts[399], "t7-9");
    }
}
