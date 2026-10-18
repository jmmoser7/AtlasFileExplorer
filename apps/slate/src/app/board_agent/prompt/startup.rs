//! Cursor startup authority and process ownership survive asynchronous boots.
use std::process::Child;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

#[derive(Clone, Default)]
pub(in crate::app::board_agent) struct SidecarStartup {
    generation: Arc<AtomicU64>,
    gate: Arc<Mutex<Option<(u64, Child)>>>,
    retired: Arc<Mutex<Vec<Child>>>,
}

impl SidecarStartup {
    pub(in crate::app::board_agent) fn advance(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(in crate::app::board_agent) fn current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
    }

    /// One session's stop, request delivery and spawn execute in order. A
    /// superseded worker never queues a request or leaves an unowned child.
    pub(in crate::app::board_agent) fn run(
        &self,
        generation: u64,
        work: impl FnOnce() -> Result<Child, String>,
    ) -> Option<Result<(), String>> {
        let mut pending = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        if !self.current(generation) {
            return None;
        }
        if let Some((_, child)) = pending.take() {
            stop(child);
        }
        self.drain_retired();
        if !self.current(generation) {
            return None;
        }
        let result = work();
        if !self.current(generation) {
            if let Ok(child) = result {
                stop(child);
            }
            return None;
        }
        Some(result.map(|child| *pending = Some((generation, child))))
    }

    pub(in crate::app::board_agent) fn claim(&self, generation: u64) -> Option<Child> {
        let mut pending = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        if self.current(generation)
            && pending
                .as_ref()
                .is_some_and(|(owner, _)| *owner == generation)
        {
            pending.take().map(|(_, child)| child)
        } else {
            None
        }
    }

    /// Invalidating is immediate; process waits stay off the frame loop. New
    /// startup drains these handles before exposing its next request.
    pub(in crate::app::board_agent) fn retire(&self, child: Option<Child>) {
        let generation = self.advance();
        if let Some(child) = child {
            self.retired
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(child);
        }
        let startup = self.clone();
        std::thread::spawn(move || {
            let mut pending = startup.gate.lock().unwrap_or_else(|e| e.into_inner());
            startup.drain_retired();
            if pending
                .as_ref()
                .is_some_and(|(owner, _)| *owner < generation)
            {
                if let Some((_, child)) = pending.take() {
                    stop(child);
                }
            }
        });
    }

    fn drain_retired(&self) {
        let children = std::mem::take(&mut *self.retired.lock().unwrap_or_else(|e| e.into_inner()));
        for child in children {
            stop(child);
        }
    }
}

fn stop(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replacement_boot_has_distinct_identity_even_with_the_same_policy() {
        let startup = SidecarStartup::default();
        let old = startup.advance();
        let replacement = startup.advance();
        assert_ne!(old, replacement);
        assert!(startup
            .run(old, || panic!("a stale boot must not touch the link"))
            .is_none());
        assert!(startup.current(replacement));
    }

    #[test]
    fn a_replacement_waits_for_the_superseded_startup_to_relinquish_the_gate() {
        let startup = SidecarStartup::default();
        let old = startup.advance();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first = startup.clone();
        let first = std::thread::spawn(move || {
            first.run(old, || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Err("old startup".into())
            })
        });
        entered_rx.recv().unwrap();
        let replacement = startup.advance();
        let second = startup.clone();
        let (replacement_tx, replacement_rx) = std::sync::mpsc::channel();
        let second = std::thread::spawn(move || {
            second.run(replacement, || {
                replacement_tx.send(()).unwrap();
                Err("replacement startup".into())
            })
        });
        assert!(replacement_rx.try_recv().is_err());
        release_tx.send(()).unwrap();
        assert!(first.join().unwrap().is_none());
        assert_eq!(
            second.join().unwrap(),
            Some(Err("replacement startup".into()))
        );
        replacement_rx.recv().unwrap();
    }
}
