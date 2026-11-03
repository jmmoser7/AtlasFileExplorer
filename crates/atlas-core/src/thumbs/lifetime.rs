//! Pool handles, rather than worker-held Arcs, own the worker lifetime.
use super::*;

pub(super) struct PoolLifetime {
    shared: Arc<Shared>,
}

impl PoolLifetime {
    pub(super) fn new(shared: Arc<Shared>) -> Self {
        Self { shared }
    }
}

impl Drop for PoolLifetime {
    fn drop(&mut self) {
        // Serialize with the worker's empty-queue check so a final drop cannot
        // lose its wakeup between that check and Condvar::wait.
        let _queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        self.shared.closing.store(true, Ordering::Relaxed);
        self.shared.cv.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn last_handle_releases_idle_workers_across_repeated_pools() {
        for _ in 0..24 {
            let pool = ThumbPool::new();
            let shared = Arc::downgrade(&pool.shared);
            let retained = pool.clone();
            drop(pool);
            assert!(!retained.shared.closing.load(Ordering::Relaxed));
            drop(retained);
            let deadline = Instant::now() + Duration::from_secs(5);
            while shared.upgrade().is_some() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(
                shared.upgrade().is_none(),
                "thumbnail workers retained a dropped pool"
            );
        }
    }
}
