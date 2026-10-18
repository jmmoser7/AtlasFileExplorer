//! Bounded worker and cache for wired image input preparation.

use super::*;
use std::collections::VecDeque;
use std::path::Path;

impl SlateApp {
    /// A wire captures paths, not the host's current album selection. Resolve
    /// each captured path to its result stack while retaining host-level styles.
    pub(super) fn image_for_captured_input(
        &self,
        item: &atlas_agent::ContextItem,
        source: &Path,
    ) -> Result<Option<slate_doc::scene::ImageNode>, String> {
        use slate_doc::image_paint::ImageResultId;
        let id = NodeId(item.node);
        let Some(NodeKind::Image(image)) = self.doc().scene.node(id).map(|node| &node.kind) else {
            return Ok(None);
        };
        let result = if image.agent.is_some() {
            let output = item
                .outputs
                .iter()
                .find(|(_, path)| Path::new(path) == source)
                .map(|(id, _)| id.clone())
                .or_else(|| {
                    self.agent_images(id)
                        .iter()
                        .find(|output| {
                            resolve_source(self.tab().path.as_deref(), &output.source) == source
                        })
                        .map(|output| output.id.clone())
                });
            ImageResultId::Output(output.ok_or("A captured image output is unavailable.")?)
        } else {
            ImageResultId::Item(image.item)
        };
        let Some(mut resolved) = self.image_for_result_id(id, &result) else {
            return Ok(None);
        };
        // Pending copied-layer readiness is keyed to the captured file's item,
        // never to a different album result that the host happens to display.
        if self.item_path(resolved.item).as_deref() != Some(source) {
            resolved.item = self
                .doc()
                .items
                .iter()
                .find(|item| self.item_path(item.id).as_deref() == Some(source))
                .map_or(slate_doc::ItemId::NONE, |item| item.id);
        }
        Ok(Some(resolved))
    }
}

#[derive(Clone)]
pub(crate) struct InputPreparation {
    pub(crate) inputs: atlas_agent::InputSnapshot,
    pub(super) replacements: Vec<(NodeId, u64, PathBuf)>,
    pub(super) bake_dependencies: Vec<crate::app::board_image_layers::LayerBakeReadiness>,
}

struct Work {
    key: u64,
    bake: Option<crate::app::board_image_layers::LayerBakeReadiness>,
    run: Box<dyn FnOnce() -> Option<PathBuf> + Send>,
}

type Made = (u64, Option<PathBuf>);

#[derive(Default)]
pub(super) struct ImageInputPrep {
    ready: HashMap<u64, Option<PathBuf>>,
    pending: HashSet<u64>,
    queue: VecDeque<Work>,
    active: usize,
    done: Option<(Sender<Made>, Receiver<Made>)>,
}

impl ImageInputPrep {
    const KEEP: usize = 256;
    const WORKERS: usize = 2;
    const MAX_PENDING: usize = 32;

    pub(super) fn receive(&mut self) {
        let completed: Vec<_> = self
            .done
            .as_ref()
            .map(|(_, rx)| rx.try_iter().collect())
            .unwrap_or_default();
        for (key, result) in completed {
            self.active = self.active.saturating_sub(1);
            self.record_result(key, result);
        }
        self.start_queued();
    }

    fn record_result(&mut self, key: u64, result: Option<PathBuf>) {
        self.pending.remove(&key);
        if self.ready.len() >= Self::KEEP {
            self.ready.clear();
        }
        self.ready.insert(key, result);
    }

    pub(super) fn key(
        &mut self,
        node: &Node,
        image: &slate_doc::scene::ImageNode,
        source: &Path,
        revision: (u64, u64),
    ) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        node.id.hash(&mut hash);
        revision.hash(&mut hash);
        source.hash(&mut hash);
        serde_json::to_string(image)
            .unwrap_or_default()
            .hash(&mut hash);
        hash.finish()
    }

    pub(super) fn result(&self, key: u64) -> Option<Option<&PathBuf>> {
        self.ready.get(&key).map(Option::as_ref)
    }

    pub(super) fn pending(&self, key: u64) -> bool {
        self.pending.contains(&key)
    }

    pub(super) fn schedule(
        &mut self,
        key: u64,
        bake: Option<crate::app::board_image_layers::LayerBakeReadiness>,
        run: Box<dyn FnOnce() -> Option<PathBuf> + Send>,
    ) -> Result<(), ()> {
        if self.ready.contains_key(&key) || self.pending.contains(&key) {
            return Ok(());
        }
        if self.pending.len() >= Self::MAX_PENDING {
            return Err(());
        }
        self.pending.insert(key);
        self.queue.push_back(Work { key, bake, run });
        self.start_queued();
        Ok(())
    }

    fn start_queued(&mut self) {
        while self.active < Self::WORKERS {
            let queued = self.queue.len();
            let mut selected = None;
            for _ in 0..queued {
                let Some(work) = self.queue.pop_front() else {
                    break;
                };
                match work.bake.as_ref().and_then(|bake| bake.result()) {
                    None if work.bake.is_some() => self.queue.push_back(work),
                    Some(Err(_)) => {
                        self.record_result(work.key, None);
                    }
                    Some(Ok(())) | None => {
                        selected = Some(work);
                        break;
                    }
                }
            }
            let Some(work) = selected else {
                break;
            };
            self.active += 1;
            let tx = self.done.get_or_insert_with(unbounded).0.clone();
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work.run))
                    .ok()
                    .flatten();
                let _ = tx.send((work.key, result));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Arc, Barrier};

    #[test]
    fn input_work_runs_off_the_request_thread_on_at_most_two_workers() {
        let request_thread = std::thread::current().id();
        let gate = Arc::new(Barrier::new(3));
        let (thread_tx, thread_rx) = mpsc::channel();
        let mut prep = ImageInputPrep::default();
        for key in 1..=2 {
            let gate = gate.clone();
            let thread_tx = thread_tx.clone();
            prep.schedule(
                key,
                None,
                Box::new(move || {
                    gate.wait();
                    let _ = thread_tx.send(std::thread::current().id());
                    Some(PathBuf::from(format!("clip-{key}.png")))
                }),
            )
            .unwrap();
        }
        assert_eq!(prep.active, 2, "the worker pool has a fixed size of two");
        assert!(prep.queue.is_empty());
        gate.wait();
        for _ in 0..2 {
            let worker = thread_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap();
            assert_ne!(
                worker, request_thread,
                "image work never runs on the UI thread"
            );
        }
        for _ in 0..100 {
            prep.receive();
            if prep.active == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(prep.active, 0);
        assert!(prep.result(1).unwrap().is_some());
        assert!(prep.result(2).unwrap().is_some());
    }

    #[test]
    fn a_pending_bake_does_not_occupy_a_prep_worker() {
        let token = crate::app::board_image_layers::LayerBakeReadiness::new();
        let (done_tx, done_rx) = mpsc::channel();
        let mut prep = ImageInputPrep::default();
        prep.schedule(
            1,
            Some(token.clone()),
            Box::new(|| Some(PathBuf::from("wait.png"))),
        )
        .unwrap();
        prep.schedule(
            2,
            None,
            Box::new(move || {
                let _ = done_tx.send(());
                Some(PathBuf::from("ready.png"))
            }),
        )
        .unwrap();
        done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        for _ in 0..100 {
            prep.receive();
            if prep.result(2).is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(prep.result(2).unwrap().is_some());
        assert_eq!(prep.active, 0);
        assert!(prep.pending(1));
        token.resolve(&Ok((1, 1)));
        prep.receive();
        for _ in 0..100 {
            prep.receive();
            if !prep.pending(1) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!prep.pending(1));
    }

    #[test]
    fn a_failed_bake_does_not_free_a_worker_that_is_still_running() {
        let gate = Arc::new(Barrier::new(3));
        let mut prep = ImageInputPrep::default();
        let first_gate = gate.clone();
        prep.schedule(
            1,
            None,
            Box::new(move || {
                first_gate.wait();
                Some(PathBuf::from("held-1.png"))
            }),
        )
        .unwrap();
        let failed = crate::app::board_image_layers::LayerBakeReadiness::new();
        failed.resolve(&Err("bake failed".into()));
        prep.schedule(
            2,
            Some(failed),
            Box::new(|| panic!("a failed bake must not run its preparation")),
        )
        .unwrap();
        let second_gate = gate.clone();
        prep.schedule(
            3,
            None,
            Box::new(move || {
                second_gate.wait();
                Some(PathBuf::from("held-3.png"))
            }),
        )
        .unwrap();
        let (ran_tx, ran_rx) = mpsc::channel();
        prep.schedule(
            4,
            None,
            Box::new(move || {
                let _ = ran_tx.send(());
                Some(PathBuf::from("queued.png"))
            }),
        )
        .unwrap();

        prep.receive();
        assert_eq!(prep.active, 2);
        assert_eq!(prep.queue.len(), 1);
        assert!(ran_rx.try_recv().is_err());
        gate.wait();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            prep.receive();
            if ran_rx.try_recv().is_ok() {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        for _ in 0..100 {
            prep.receive();
            if prep.active == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(prep.active, 0);
        assert!(prep.result(2).is_some_and(|result| result.is_none()));
        assert!(prep.result(4).unwrap().is_some());
    }
}
