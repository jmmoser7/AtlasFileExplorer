//! One cancellation/deadline policy for host calls, pipe IO and artifact phases.
//! Polling pause is worker-only; no IO can silently reset an operation's budget.

use crate::transport::Cancellation;
#[cfg(windows)]
use std::thread;
use std::{
    io,
    time::{Duration, Instant},
};

pub(crate) struct Deadline<'a> {
    until: Instant,
    cancel: &'a Cancellation,
}

impl<'a> Deadline<'a> {
    pub fn new(timeout: Duration, cancel: &'a Cancellation) -> io::Result<Self> {
        let until = Instant::now().checked_add(timeout).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "connector timeout too large")
        })?;
        Ok(Self { until, cancel })
    }

    pub fn remaining(&self) -> io::Result<Duration> {
        // Read::read_exact retries Interrupted forever. Cancellation must be a
        // terminal error, including when it interrupts a partial frame.
        if self.cancel.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "connector operation cancelled",
            ));
        }
        let remaining = self.until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "connector operation timed out",
            ));
        }
        Ok(remaining)
    }

    #[cfg(windows)]
    pub fn check(&self) -> io::Result<()> {
        self.remaining().map(|_| ())
    }

    #[cfg(windows)]
    pub fn pause(&self) -> io::Result<()> {
        thread::sleep(self.remaining()?.min(Duration::from_millis(5)));
        self.check()
    }
}
