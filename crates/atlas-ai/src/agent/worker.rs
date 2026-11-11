//! Source-link worker; artifact publication stays with the strict history owner.
use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};

type Snapshot<T> = Arc<Mutex<Option<T>>>;
const IO_WORKERS: usize = 4;
const MAILBOX_CAPACITY: usize = 8;

#[derive(Clone, Default)]
pub(super) struct ReadyQueue(Arc<ReadyScheduler>);

#[derive(Default)]
struct ReadyScheduler {
    links: Mutex<ReadyLinks>,
    ordinary_ready: Condvar,
    foreground_ready: Condvar,
}

#[derive(Default)]
struct ReadyLinks {
    ordinary: VecDeque<Arc<LinkWorker>>,
    foreground: VecDeque<Arc<LinkWorker>>,
}

impl ReadyLinks {
    fn take(&mut self, reserved: bool) -> Option<Arc<LinkWorker>> {
        if reserved {
            self.foreground.pop_front()
        } else {
            self.ordinary
                .pop_front()
                .or_else(|| self.foreground.pop_front())
        }
    }
}

impl ReadyQueue {
    fn enqueue(&self, link: Arc<LinkWorker>, foreground: bool) {
        {
            let mut links = self.0.links.lock().expect("agent ready queue");
            if foreground {
                links.foreground.push_back(link);
            } else {
                links.ordinary.push_back(link);
            }
        }
        if foreground {
            self.0.foreground_ready.notify_one();
        }
        self.0.ordinary_ready.notify_one();
    }

    /// Move an existing ready token. If a worker has already taken it, its
    /// next turn will use the queued work's priority.
    fn promote(&self, link: &Arc<LinkWorker>) {
        {
            let mut links = self.0.links.lock().expect("agent ready queue");
            let Some(index) = links
                .ordinary
                .iter()
                .position(|queued| Arc::ptr_eq(queued, link))
            else {
                return;
            };
            let token = links.ordinary.remove(index).expect("ready token");
            links.foreground.push_back(token);
        }
        self.0.foreground_ready.notify_one();
        self.0.ordinary_ready.notify_one();
    }

    fn take(&self, reserved: bool) -> Arc<LinkWorker> {
        let signal = if reserved {
            &self.0.foreground_ready
        } else {
            &self.0.ordinary_ready
        };
        let mut links = self.0.links.lock().expect("agent ready queue");
        loop {
            if let Some(link) = links.take(reserved) {
                return link;
            }
            links = signal.wait(links).expect("agent ready queue");
        }
    }

    #[cfg(test)]
    pub(super) fn try_take(&self, reserved: bool) -> Option<Arc<LinkWorker>> {
        self.0.links.lock().unwrap().take(reserved)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        let links = self.0.links.lock().unwrap();
        links.ordinary.len() + links.foreground.len()
    }
}

pub(super) fn ready_queue() -> ReadyQueue {
    static READY: OnceLock<ReadyQueue> = OnceLock::new();
    READY
        .get_or_init(|| {
            let ready = ReadyQueue::default();
            for index in 0..IO_WORKERS {
                let pending = ready.clone();
                let reserved = index == IO_WORKERS - 1;
                std::thread::Builder::new()
                    .name(format!("atlas-agent-io-{index}"))
                    .spawn(move || loop {
                        // Release the ready lock before touching a mailbox.
                        pending.take(reserved).run_next();
                    })
                    .expect("start agent link I/O worker");
            }
            ready
        })
        .clone()
}

pub(super) struct LinkWorker {
    ready: Weak<ReadyScheduler>,
    mailbox: Mutex<Mailbox>,
}

struct PendingWork {
    work: LinkWork,
    foreground: bool,
}

struct Mailbox {
    pending: VecDeque<PendingWork>,
    scheduled: bool,
    io: Option<LinkIo>,
}

impl LinkWorker {
    pub(super) fn new(
        ready: ReadyQueue,
        result: Snapshot<AgentSession>,
        outputs_out: Snapshot<crate::outputs::LinkOutputs>,
        place_out: Snapshot<String>,
        history_out: Snapshot<Result<bool, String>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            ready: Arc::downgrade(&ready.0),
            mailbox: Mutex::new(Mailbox {
                pending: VecDeque::new(),
                scheduled: false,
                io: Some(LinkIo {
                    link: FileAgentLink::new(),
                    captured: None,
                    roots: None,
                    turns: crate::outputs::skeleton(None),
                    watch: crate::outputs::OutputWatch::default(),
                    artifact_request: false,
                    result,
                    outputs_out,
                    place_out,
                    history_out,
                }),
            }),
        })
    }

    pub(super) fn try_send(self: &Arc<Self>, work: LinkWork) -> Result<(), ()> {
        let foreground = matches!(&work, LinkWork::Request(..));
        self.enqueue(work, foreground)
    }

    pub(super) fn enqueue(self: &Arc<Self>, work: LinkWork, foreground: bool) -> Result<(), ()> {
        let ready = self.ready.upgrade().map(ReadyQueue).ok_or(())?;
        let mut mailbox = self.mailbox.lock().map_err(|_| ())?;
        if mailbox.pending.len() == MAILBOX_CAPACITY || (!mailbox.scheduled && mailbox.io.is_none())
        {
            return Err(());
        }
        let promote = foreground && !mailbox.pending.iter().any(|job| job.foreground);
        mailbox.pending.push_back(PendingWork { work, foreground });
        // Mailbox-to-ready is the only nested lock order. Publishing the token
        // under the mailbox lock keeps promotion from missing a queued link.
        if !mailbox.scheduled {
            mailbox.scheduled = true;
            ready.enqueue(self.clone(), foreground);
        } else if promote {
            ready.promote(self);
        }
        Ok(())
    }

    pub(super) fn prioritize_read(self: &Arc<Self>, path: &Path) {
        let Some(ready) = self.ready.upgrade().map(ReadyQueue) else {
            return;
        };
        let Ok(mut mailbox) = self.mailbox.lock() else {
            return;
        };
        if let Some(job) = mailbox
            .pending
            .iter_mut()
            .find(|job| matches!(&job.work, LinkWork::Read(queued) if queued.as_path() == path))
        {
            if !job.foreground {
                job.foreground = true;
                ready.promote(self);
            }
        }
    }

    pub(super) fn run_next(self: &Arc<Self>) {
        let (work, mut io) = {
            let mut mailbox = self.mailbox.lock().expect("agent link mailbox");
            let work = mailbox
                .pending
                .pop_front()
                .expect("scheduled link work")
                .work;
            let io = mailbox.io.take().expect("scheduled link state");
            (work, io)
        };
        // A failure in one source must not take a worker away from all others.
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| io.process(work))).is_err() {
            let mut mailbox = self.mailbox.lock().expect("agent link mailbox");
            mailbox.scheduled = false;
            let pending = std::mem::take(&mut mailbox.pending);
            drop(mailbox);
            drop(pending);
            return;
        }
        let mut mailbox = self.mailbox.lock().expect("agent link mailbox");
        mailbox.io = Some(io);
        mailbox.scheduled = !mailbox.pending.is_empty();
        if mailbox.scheduled {
            if let Some(ready) = self.ready.upgrade().map(ReadyQueue) {
                let foreground = mailbox.pending.iter().any(|job| job.foreground);
                ready.enqueue(self.clone(), foreground);
            }
        }
    }
}

struct LinkIo {
    link: FileAgentLink,
    // Snapshot files only when completion or the artifact list can have
    // changed, not on every streamed token.
    captured: Option<(PathBuf, bool, usize, usize)>,
    roots: Option<Vec<PathBuf>>,
    turns: AgentSession,
    watch: crate::outputs::OutputWatch,
    // A strict artifact run owns return.json and its revision capture. Generic
    // deliverable polling must never consume that manifest first.
    artifact_request: bool,
    result: Snapshot<AgentSession>,
    outputs_out: Snapshot<crate::outputs::LinkOutputs>,
    place_out: Snapshot<String>,
    history_out: Snapshot<Result<bool, String>>,
}

impl LinkIo {
    fn process(&mut self, work: LinkWork) {
        let Self {
            link,
            captured,
            roots,
            turns,
            watch,
            artifact_request,
            result,
            outputs_out,
            place_out,
            history_out,
        } = self;
        let reading = matches!(&work, LinkWork::Read(_));
        let update = match work {
            LinkWork::Roots(r) => {
                *roots = Some(r);
                None
            }
            LinkWork::Context(dir, ctx) => {
                let ctx = *ctx;
                link.tick_write_context_in(&dir, &ctx);
                None
            }
            LinkWork::Request(dir, req) => {
                let req = *req;
                *artifact_request = req.artifact.is_some();
                link.send_request_in(&dir, &req)
                    .err()
                    .map(|e| AgentSession {
                        activity: Vec::new(),
                        usage: None,
                        approval: None,
                        conversation: String::new(),
                        artifacts: Vec::new(),
                        request: req.id.clone(),
                        status: AgentStatus::Error(format!("Could not write agent request: {e}")),
                        provider: String::new(),
                        turns: vec![],
                        updated_at: req.at,
                        bundle: Default::default(),
                    })
            }
            LinkWork::Read(path) => {
                if let Some(raw) = path
                    .parent()
                    .filter(|_| !*artifact_request)
                    .and_then(read_place_request)
                {
                    if let Ok(mut value) = place_out.lock() {
                        *value = Some(raw);
                    }
                }
                let session = link.tick_read_session_file(&path);
                let mut copied = false;
                if let (Some(s), Some(dir)) =
                    (&session, path.parent().filter(|_| !*artifact_request))
                {
                    let key = (
                        dir.to_path_buf(),
                        s.status == AgentStatus::Thinking,
                        s.turns.len(),
                        s.artifacts.len(),
                    );
                    if captured.as_ref() != Some(&key) {
                        if let Ok(changed) = crate::versions::capture(dir, s) {
                            *captured = Some(key);
                            copied = changed;
                        }
                    }
                    if s.turns.len() != turns.turns.len()
                        || s.artifacts.len() != turns.artifacts.len()
                    {
                        *turns = crate::outputs::skeleton(Some(s));
                    }
                }
                if let (Some(roots), Some(dir)) =
                    (roots.as_ref(), path.parent().filter(|_| !*artifact_request))
                {
                    if let Some(found) = watch.tick(dir, turns, roots, copied) {
                        if let Ok(mut value) = outputs_out.lock() {
                            *value = Some(found);
                        }
                    }
                }
                session
            }
        };
        if update.is_some() {
            if let Ok(mut value) = result.lock() {
                *value = update;
            }
        }
        if reading {
            if let Ok(mut state) = history_out.lock() {
                *state = link.history_read.clone();
            }
        }
    }
}
