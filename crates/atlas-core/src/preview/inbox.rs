//! Pixels have backpressure; terminal bookkeeping never blocks the UI producer.
use super::{PreviewOutcome, PreviewResult};
use crossbeam_channel::{bounded, unbounded, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::Duration;

const READY_CAP: usize = 2;

#[derive(Clone)]
pub(super) struct PreviewSender {
    ready: Sender<PreviewResult>,
    terminal: Sender<PreviewResult>,
}

#[derive(Clone)]
pub struct PreviewInbox {
    ready: Receiver<PreviewResult>,
    terminal: Option<Receiver<PreviewResult>>,
}

pub(super) fn channel() -> (PreviewSender, PreviewInbox) {
    let (ready_tx, ready) = bounded(READY_CAP);
    let (terminal_tx, terminal) = unbounded();
    (
        PreviewSender {
            ready: ready_tx,
            terminal: terminal_tx,
        },
        PreviewInbox {
            ready,
            terminal: Some(terminal),
        },
    )
}

impl PreviewSender {
    pub(super) fn terminal(&self, id: u32, outcome: PreviewOutcome) {
        debug_assert!(!matches!(outcome, PreviewOutcome::Ready(..)));
        let _ = self.terminal.send(PreviewResult { id, outcome });
    }

    pub(super) fn send(&self, result: PreviewResult) -> Result<(), ()> {
        // Only worker threads call this path. A slow UI leaves at most two
        // ready images in the inbox and one held by each of the two workers.
        if matches!(result.outcome, PreviewOutcome::Ready(..)) {
            self.ready.send(result).map_err(|_| ())
        } else {
            self.terminal.send(result).map_err(|_| ())
        }
    }
}

impl PreviewInbox {
    pub fn try_recv(&self) -> Result<PreviewResult, TryRecvError> {
        if let Some(terminal) = &self.terminal {
            if let Ok(result) = terminal.try_recv() {
                return Ok(result);
            }
        }
        self.ready.try_recv()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<PreviewResult, RecvTimeoutError> {
        if let Ok(result) = self.try_recv() {
            return Ok(result);
        }
        let Some(terminal) = &self.terminal else {
            return self.ready.recv_timeout(timeout);
        };
        crossbeam_channel::select! {
            recv(terminal) -> result => result.map_err(|_| RecvTimeoutError::Disconnected),
            recv(self.ready) -> result => result.map_err(|_| RecvTimeoutError::Disconnected),
            default(timeout) => Err(RecvTimeoutError::Timeout),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ready.is_empty() && self.terminal.as_ref().is_none_or(Receiver::is_empty)
    }
}

/// Allows callers to inject a deterministic inbox without starting workers.
impl From<Receiver<PreviewResult>> for PreviewInbox {
    fn from(ready: Receiver<PreviewResult>) -> Self {
        Self {
            ready,
            terminal: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_backlog_is_bounded_and_terminal_admission_never_blocks() {
        let (tx, rx) = channel();
        for id in 0..READY_CAP {
            tx.ready
                .try_send(PreviewResult {
                    id: id as u32,
                    outcome: PreviewOutcome::Ready(1, 1, vec![0; 4]),
                })
                .unwrap();
        }
        assert!(tx
            .ready
            .try_send(PreviewResult {
                id: 3,
                outcome: PreviewOutcome::Ready(1, 1, vec![0; 4]),
            })
            .unwrap_err()
            .is_full());
        tx.terminal(4, PreviewOutcome::Cancelled);
        assert_eq!(rx.try_recv().unwrap().id, 4);
        assert_eq!(rx.ready.len(), READY_CAP);
    }
}
