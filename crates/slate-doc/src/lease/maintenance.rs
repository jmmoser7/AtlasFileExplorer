//! One worker for lease refresh and release, independent of app frame delivery.

use super::*;
use std::sync::{OnceLock, Weak};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) enum Task {
    Register(Weak<LeaseInner>),
    Release(Arc<LeaseInner>),
    Barrier(mpsc::Sender<()>),
}

static WORKER: OnceLock<Result<mpsc::Sender<Task>, String>> = OnceLock::new();

pub(super) fn worker() -> io::Result<mpsc::Sender<Task>> {
    match WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("slate-lease-maintenance".into())
            .spawn(move || run(rx))
            .map(|_| tx)
            .map_err(|error| error.to_string())
    }) {
        Ok(tx) => Ok(tx.clone()),
        Err(error) => Err(io::Error::other(error.clone())),
    }
}

pub(super) fn flush(timeout: Duration) -> io::Result<()> {
    let Some(Ok(worker)) = WORKER.get() else {
        return Ok(());
    };
    flush_worker(worker, timeout)
}

fn flush_worker(worker: &mpsc::Sender<Task>, timeout: Duration) -> io::Result<()> {
    let (done, received) = mpsc::channel();
    worker.send(Task::Barrier(done)).map_err(io::Error::other)?;
    received.recv_timeout(timeout).map_err(|error| match error {
        mpsc::RecvTimeoutError::Timeout => {
            io::Error::new(io::ErrorKind::TimedOut, "lease release flush timed out")
        }
        mpsc::RecvTimeoutError::Disconnected => io::Error::new(
            io::ErrorKind::BrokenPipe,
            "lease maintenance worker stopped",
        ),
    })
}

fn run(rx: mpsc::Receiver<Task>) {
    let mut leases = Vec::new();
    let mut tick = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Task::Register(lease)) => leases.push(lease),
            Ok(Task::Release(lease)) => lease.remove_if_ours(),
            Ok(Task::Barrier(done)) => {
                let _ = done.send(());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        if tick.elapsed() >= Duration::from_secs(1) {
            refresh_all(&mut leases, unix_now());
            tick = Instant::now();
        }
    }
}

fn refresh_all(leases: &mut Vec<Weak<LeaseInner>>, now: i64) {
    leases.retain(|weak| {
        let Some(lease) = weak.upgrade() else {
            return false;
        };
        if !lease.active.load(Ordering::Acquire) {
            return false;
        }
        if lease.failure.lock().is_ok_and(|failure| failure.is_some()) {
            // A failed lease stays fail-closed until the user saves a copy or
            // closes it; do not hammer an unavailable share every second.
            return true;
        }
        // An explicit save can hold the record. Never wait behind it before
        // refreshing other open workbooks.
        if let Ok(mut record) = lease.record.try_lock() {
            let result = lease.refresh(&mut record, now);
            lease.remember_failure(&result);
        }
        true
    });
}

#[cfg(test)]
pub(super) fn settle() {
    flush(Duration::from_secs(5)).unwrap();
}

#[cfg(test)]
pub(super) fn refresh_for_test(leases: &[&Lease], now: i64) {
    let mut weak = leases
        .iter()
        .map(|lease| Arc::downgrade(lease.inner.as_ref().unwrap()))
        .collect();
    refresh_all(&mut weak, now);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_flush_times_out_if_maintenance_is_stalled() {
        let (worker, queued) = mpsc::channel();
        let error = flush_worker(&worker, Duration::ZERO).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(matches!(queued.try_recv(), Ok(Task::Barrier(_))));
    }
}
