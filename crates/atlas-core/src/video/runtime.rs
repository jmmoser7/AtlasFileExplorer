use super::events::{Enqueue, Events};
use super::process::Decoder;
use super::{Job, VideoEvent, VideoStatus};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const STALL_LIMIT: Duration = Duration::from_secs(10);

pub(super) struct Request {
    pub job: Job,
    pub generation: u64,
    pub epoch: u64,
}
pub(super) struct StripRequest {
    pub job: Job,
    pub epoch: u64,
}
pub(super) struct Control {
    pub desired: AtomicU64,
    pub closed: AtomicBool,
    pub epoch: AtomicU64,
    serial: AtomicU64,
    latest: Mutex<Option<Request>>,
}
impl Control {
    pub fn new() -> Self {
        Self {
            desired: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            serial: AtomicU64::new(0),
            latest: Mutex::new(None),
        }
    }
    pub fn play(&self, job: Job) {
        let mut latest = self
            .latest
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let generation = self
            .serial
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1)
            .max(1);
        self.desired.store(generation, Ordering::SeqCst);
        *latest = Some(Request {
            job,
            generation,
            epoch: self.epoch.load(Ordering::SeqCst),
        });
    }
    pub fn stop(&self) {
        self.desired.store(0, Ordering::SeqCst);
        self.latest
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
    }
    pub fn cancel_all(&self) -> u64 {
        let epoch = self.epoch.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        self.stop();
        epoch
    }
    fn take(&self) -> Option<Request> {
        self.latest
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
    }
    fn finished(&self, generation: u64) {
        if generation != 0 {
            let _ =
                self.desired
                    .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst);
        }
    }
}
struct Active {
    request: Request,
    decoder: Decoder,
    progress: Instant,
    pending_event: Option<VideoEvent>,
}

fn pending_strip(
    pending: &mut VecDeque<StripRequest>,
    request: StripRequest,
    events: &Events,
    front: bool,
) {
    let id = request.job.key().0;
    pending.retain(|old| old.job.key().0 != id);
    if pending.len() == 64 {
        if let Some(old) = pending.pop_front() {
            let (id, token) = old.job.key();
            events.failed_in_context(id, token, old.epoch);
        }
    }
    if front {
        pending.push_front(request);
    } else {
        pending.push_back(request);
    }
}

fn begin(request: Request, events: &Events, fault: Option<&str>) -> Result<Active, Request> {
    match Decoder::launch(request.job.clone(), fault) {
        Ok(decoder) => {
            events.worker_pid.store(decoder.id(), Ordering::Relaxed);
            events.status(VideoStatus::Decoding);
            Ok(Active {
                request,
                decoder,
                progress: Instant::now(),
                pending_event: None,
            })
        }
        Err(_) => Err(request),
    }
}

pub(super) fn supervise(
    jobs: crossbeam_channel::Receiver<StripRequest>,
    events: Arc<Events>,
    control: Arc<Control>,
    mut fault: Option<&'static str>,
) {
    let mut active: Option<Active> = None;
    let mut pending = VecDeque::new();
    loop {
        if control.closed.load(Ordering::SeqCst) {
            break;
        }
        // Latest Play is a separate mailbox: strip spam cannot reject it.
        if let Some(request) = control.take() {
            if let Some(old) = active.take() {
                if matches!(old.request.job, Job::Strip { .. }) {
                    pending_strip(
                        &mut pending,
                        StripRequest {
                            job: old.request.job.clone(),
                            epoch: old.request.epoch,
                        },
                        &events,
                        true,
                    );
                }
                drop(old);
                events.worker_pid.store(0, Ordering::Relaxed); // Kill/reap on supervisor thread, never the UI.
            }
            if request.generation == control.desired.load(Ordering::SeqCst) {
                match begin(request, &events, fault.take()) {
                    Ok(next) => active = Some(next),
                    Err(request) => {
                        let (id, token) = request.job.key();
                        control.finished(request.generation);
                        events.failed_in_context(id, token, request.epoch);
                    }
                }
            }
        }
        // The generation is independent of user tokens (which different
        // document nodes may reuse). Cancel before reading any old failures.
        let desired = control.desired.load(Ordering::SeqCst);
        let epoch = control.epoch.load(Ordering::SeqCst);
        pending.retain(|request| request.epoch == epoch);
        if active.as_ref().is_some_and(|run| {
            run.request.epoch != epoch
                || if run.request.generation == 0 {
                    desired != 0
                } else {
                    run.request.generation != desired
                }
        }) {
            let old = active.take().unwrap();
            if old.request.epoch == epoch && matches!(old.request.job, Job::Strip { .. }) {
                pending_strip(
                    &mut pending,
                    StripRequest {
                        job: old.request.job.clone(),
                        epoch: old.request.epoch,
                    },
                    &events,
                    true,
                );
            }
            drop(old);
            events.worker_pid.store(0, Ordering::Relaxed);
            events.status(VideoStatus::Idle);
        }
        for _ in 0..64 {
            match jobs.try_recv() {
                Ok(request) if request.epoch == epoch => {
                    pending_strip(&mut pending, request, &events, false)
                }
                Ok(_) => {}
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => return,
            }
        }
        if active.is_none() && control.desired.load(Ordering::SeqCst) == 0 {
            if let Some(strip) = pending.pop_front() {
                let request = Request {
                    job: strip.job,
                    generation: 0,
                    epoch: strip.epoch,
                };
                match begin(request, &events, fault.take()) {
                    Ok(next) => active = Some(next),
                    Err(request) => {
                        let (id, token) = request.job.key();
                        events.failed_in_context(id, token, request.epoch);
                    }
                }
            }
        }
        let mut completion = None;
        if let Some(run) = active.as_mut() {
            for _ in 0..4 {
                let received = if let Some(event) = run.pending_event.take() {
                    // This delay belongs to the consumer, not the codec. Keep
                    // polling Play/stop/context at the top of every iteration.
                    run.progress = Instant::now();
                    Ok(Ok(event))
                } else {
                    run.decoder.receive()
                };
                match received {
                    Ok(Ok(event))
                        if event.key() == run.request.job.key()
                            && compatible(&run.request.job, &event) =>
                    {
                        run.progress = Instant::now();
                        let terminal = event.terminal();
                        let failed = matches!(event, VideoEvent::Failed { .. });
                        match events.enqueue_in_context(event, run.request.epoch) {
                            Enqueue::Accepted if terminal => {
                                completion = Some((failed, false));
                                break;
                            }
                            Enqueue::Accepted => {}
                            Enqueue::Stale => {
                                completion = Some((false, false));
                                break;
                            }
                            Enqueue::Full(event) => {
                                run.pending_event = Some(event);
                                break;
                            }
                        }
                    }
                    Ok(_) | Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        completion = Some((true, true));
                        break;
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                }
            }
            if completion.is_none()
                && run.pending_event.is_none()
                && run.progress.elapsed() > STALL_LIMIT
            {
                completion = Some((true, true));
            }
        }
        if let Some((failed, abnormal)) = completion {
            let old = active.take().unwrap();
            let (id, token) = old.request.job.key();
            control.finished(old.request.generation);
            let epoch = old.request.epoch;
            drop(old);
            events.worker_pid.store(0, Ordering::Relaxed);
            if abnormal {
                events.restarts.fetch_add(1, Ordering::Relaxed);
                events.failed_in_context(id, token, epoch);
            } else if !failed {
                events.status(VideoStatus::Idle);
            }
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    drop(active);
    events.worker_pid.store(0, Ordering::Relaxed);
}
fn compatible(job: &Job, event: &VideoEvent) -> bool {
    matches!(
        (job, event),
        (_, VideoEvent::Probed { .. } | VideoEvent::Failed { .. })
            | (
                Job::Strip { .. },
                VideoEvent::Strip { .. } | VideoEvent::StripDone { .. }
            )
            | (
                Job::Play { .. },
                VideoEvent::Play { .. } | VideoEvent::Ended { .. }
            )
    )
}
impl Job {
    pub(super) fn key(&self) -> (u64, u64) {
        match self {
            Self::Strip { id, token, .. } | Self::Play { id, token, .. } => (*id, *token),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn play(token: u64) -> Job {
        Job::Play {
            id: 1,
            token,
            path: "a.mp4".into(),
            start: 0.0,
            end: None,
            from: 0.0,
            looped: false,
        }
    }
    #[test]
    fn latest_play_replaces_obsolete_requests_even_when_tokens_repeat() {
        let control = Control::new();
        control.play(play(7));
        let first = control.take().unwrap();
        control.play(play(7));
        control.play(play(7));
        let latest = control.take().unwrap();
        assert!(latest.generation > first.generation);
        control.finished(first.generation);
        assert_eq!(control.desired.load(Ordering::SeqCst), latest.generation);
        control.stop();
        assert_eq!(control.desired.load(Ordering::SeqCst), 0);
        assert!(control.take().is_none());
    }
    #[test]
    fn interrupted_strip_resumes_before_background_jobs() {
        let events = Events::new();
        let mut pending = VecDeque::new();
        for id in [2u64, 3, 4] {
            pending_strip(
                &mut pending,
                StripRequest {
                    job: Job::Strip {
                        id,
                        token: id,
                        path: "clip.mp4".into(),
                        start: 0.0,
                        end: None,
                    },
                    epoch: 0,
                },
                &events,
                false,
            );
        }
        pending_strip(
            &mut pending,
            StripRequest {
                job: Job::Strip {
                    id: 1,
                    token: 10,
                    path: "clip.mp4".into(),
                    start: 0.0,
                    end: None,
                },
                epoch: 0,
            },
            &events,
            true,
        );
        assert_eq!(
            pending
                .iter()
                .map(|strip| strip.job.key())
                .collect::<Vec<_>>(),
            [(1, 10), (2, 2), (3, 3), (4, 4)]
        );
    }
}
