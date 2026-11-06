//! Debounced analyze-on-type.
//!
//! Typing produces one `didChange` per keystroke, and analyzing a large buffer on every
//! keystroke would stall the editor. The dispatch loop therefore only queues the edited
//! URI here; a dedicated thread collects a burst of edits, waits for a quiet period and
//! then hands the URIs back through a channel the main loop `select!`s on. The analysis
//! always reads the current buffer when the URI comes back, so the last edit wins.

use std::collections::HashSet;
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use lsp_types::Uri;

/// How long the debouncer waits after the last edit before asking for a re-analysis.
/// 250 ms is below the threshold where a person notices the lag and above a normal
/// typing burst, so a whole word costs one analysis instead of one per keystroke.
pub const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(250);

/// The editing side of the debouncer plus the channel its results come back on.
pub struct Debouncer {
    edits: Sender<Uri>,
    results: Receiver<Uri>,
}

impl Debouncer {
    pub fn new() -> Self {
        Self::with_interval(DEBOUNCE_INTERVAL)
    }

    /// The interval is a parameter so the coalescing thread can be tested without
    /// waiting the production 250 ms.
    pub fn with_interval(interval: Duration) -> Self {
        let (edits, edit_receiver) = unbounded();
        let (result_sender, results) = unbounded();
        std::thread::spawn(move || coalesce(edit_receiver, result_sender, interval));
        Self { edits, results }
    }

    /// Queues one edit; a burst of calls results in one analysis per URI.
    pub fn schedule(&self, uri: Uri) {
        if self.edits.send(uri).is_err() {
            log::debug!("debouncer thread is gone; dropping edit");
        }
    }

    /// A clone of the result channel, so the dispatch loop can `select!` on it.
    pub fn results(&self) -> Receiver<Uri> {
        self.results.clone()
    }
}

/// Waits for the first edit, then keeps resetting the timer until `interval` passes with
/// no further edit. A URI edited several times in the burst is reported once, with the
/// text it has when the burst ends.
fn coalesce(edits: Receiver<Uri>, results: Sender<Uri>, interval: Duration) {
    while let Ok(first) = edits.recv() {
        let mut pending = HashSet::from([first]);
        loop {
            match edits.recv_timeout(interval) {
                Ok(uri) => {
                    pending.insert(uri);
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    flush(&results, pending);
                    return;
                }
            }
        }
        if !flush(&results, pending) {
            return;
        }
    }
}

/// Sends every pending URI; returns `false` when the server side is gone.
fn flush(results: &Sender<Uri>, pending: HashSet<Uri>) -> bool {
    pending.into_iter().all(|uri| results.send(uri).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Instant;

    fn uri(name: &str) -> Uri {
        format!("file:///tmp/fixture/{name}")
            .parse()
            .expect("valid uri")
    }

    fn debouncer() -> Debouncer {
        Debouncer::with_interval(Duration::from_millis(30))
    }

    #[test]
    fn a_burst_of_edits_produces_one_result_after_the_quiet_period() {
        let debouncer = debouncer();
        let results = debouncer.results();
        let file = uri("a.ts");

        let started = Instant::now();
        for _ in 0..5 {
            debouncer.schedule(file.clone());
        }

        assert_eq!(
            results.recv_timeout(Duration::from_secs(2)).expect("one result"),
            file
        );
        assert!(started.elapsed() >= Duration::from_millis(30));
        assert!(matches!(
            results.recv_timeout(Duration::from_millis(100)),
            Err(RecvTimeoutError::Timeout)
        ));
    }

    #[test]
    fn distinct_documents_are_each_reported_once() {
        let debouncer = debouncer();
        let results = debouncer.results();
        let first = uri("a.ts");
        let second = uri("b.ts");

        debouncer.schedule(first.clone());
        debouncer.schedule(second.clone());
        debouncer.schedule(first.clone());

        let mut seen = vec![
            results.recv_timeout(Duration::from_secs(2)).expect("first result"),
            results.recv_timeout(Duration::from_secs(2)).expect("second result"),
        ];
        seen.sort_by(|left, right| left.as_str().cmp(right.as_str()));

        assert_eq!(seen, vec![first, second]);
    }
}
