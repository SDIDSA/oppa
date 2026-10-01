//! Zero-stdout diagnostic log (Phase 37b, decision 363 — G17):
//! a fixed-capacity ring buffer for framework diagnostics. Nothing
//! here ever prints (no `println!`/`eprintln!` on any path —
//! test-rig output stays parseable, device logs stay structured);
//! hosts drain entries for assertions, harnesses, and platform
//! sinks. Overwrite-oldest when full (a full log drops the oldest,
//! never the newest — diagnostics describe the present, and counts
//! stay exact through [`RingLog::dropped`]).

use std::collections::VecDeque;

/// Diagnostic severity (ordered — filters take a floor).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// One diagnostic entry: sequence number (monotonic per log),
/// severity, and message.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct LogEntry {
    pub seq: u64,
    pub level: LogLevel,
    pub message: String,
}

/// Fixed-capacity ring buffer (default 512 — see
/// [`RingLog::new`]). `Clone` shares nothing (entries clone —
/// snapshots, never aliases).
#[derive(Clone, Debug, Default)]
pub struct RingLog {
    entries: VecDeque<LogEntry>,
    capacity: usize,
    next_seq: u64,
    dropped: u64,
}

impl RingLog {
    /// Empty log holding at most `capacity` entries (`capacity == 0`
    /// refuses loudly — a zero-capacity log silently drops
    /// everything, never built silently).
    pub fn new(capacity: usize) -> Self {
        assert!(
            capacity > 0,
            "RingLog capacity must be > 0, got {capacity} — refused, never silent"
        );
        Self {
            entries: VecDeque::with_capacity(capacity.min(1024)),
            capacity,
            next_seq: 0,
            dropped: 0,
        }
    }

    /// Default log (512 entries — room for a session's diagnostics
    /// without unbounded growth; override with
    /// [`RingLog::new`]).
    pub fn default_log() -> Self {
        Self::new(512)
    }

    /// Pushes one entry (overwrite-oldest when full — counts through
    /// [`RingLog::dropped`]).
    pub fn push(&mut self, level: LogLevel, message: impl Into<String>) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
            self.dropped += 1;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push_back(LogEntry {
            seq,
            level,
            message: message.into(),
        });
    }

    /// Live entry count (never exceeds capacity).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries overwritten so far (exact — the ring never silently
    /// loses count).
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Entries at or above `floor`, in order (non-destructive —
    /// draining is [`RingLog::take`]).
    pub fn entries_at_or_above(&self, floor: LogLevel) -> Vec<LogEntry> {
        self.entries
            .iter()
            .filter(|e| e.level >= floor)
            .cloned()
            .collect()
    }

    /// Takes all entries, leaving the log empty (sequence numbers
    /// keep counting — takes never rewind).
    pub fn take(&mut self) -> Vec<LogEntry> {
        std::mem::take(&mut self.entries).into_iter().collect()
    }

    /// Takes entries at or above `floor`, keeping the rest in place
    /// with their sequence numbers (filters never destroy, takes
    /// never rewind).
    pub fn take_at_or_above(&mut self, floor: LogLevel) -> Vec<LogEntry> {
        let mut out = Vec::new();
        let mut rest = VecDeque::with_capacity(self.entries.len());
        for entry in std::mem::take(&mut self.entries) {
            if entry.level >= floor {
                out.push(entry);
            } else {
                rest.push_back(entry);
            }
        }
        self.entries = rest;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_overwrites_oldest_and_counts() {
        let mut log = RingLog::new(2);
        log.push(LogLevel::Info, "a");
        log.push(LogLevel::Warn, "b");
        log.push(LogLevel::Error, "c");
        assert_eq!(log.len(), 2);
        assert_eq!(log.dropped(), 1);
        let entries = log.entries_at_or_above(LogLevel::Debug);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].message, "b");
        assert_eq!(entries[1].message, "c");
        assert!(entries[0].seq < entries[1].seq);
        // Floor filters below-threshold entries.
        assert_eq!(log.entries_at_or_above(LogLevel::Error).len(), 1);
        // Take drains without rewinding.
        let taken = log.take();
        assert_eq!(taken.len(), 2);
        assert!(log.is_empty());
        assert_eq!(log.dropped(), 1);
        log.push(LogLevel::Debug, "d");
        assert_eq!(log.take()[0].seq, 3);
    }

    #[test]
    #[should_panic(expected = "capacity must be")]
    fn zero_capacity_refuses_loudly() {
        let _ = RingLog::new(0);
    }

    #[test]
    fn take_at_or_above_keeps_rest_with_seqs() {
        let mut log = RingLog::new(8);
        log.push(LogLevel::Debug, "d");
        log.push(LogLevel::Warn, "w");
        log.push(LogLevel::Error, "e");
        let taken = log.take_at_or_above(LogLevel::Warn);
        assert_eq!(taken.len(), 2);
        assert_eq!(taken[0].seq, 1);
        assert_eq!(taken[1].seq, 2);
        assert_eq!(log.len(), 1, "below-floor entry stays");
        assert_eq!(log.take()[0].seq, 0, "sequence preserved");
    }
}
