//! Last-handle shutdown also releases workers blocked on a full pixel inbox.
use super::*;
use crossbeam_channel::{unbounded, Receiver, Sender};

pub(super) struct PoolLifetime {
    shared: Arc<Shared>,
    _shutdown: Sender<()>,
}

impl PoolLifetime {
    pub(super) fn new(shared: Arc<Shared>) -> (Arc<Self>, Receiver<()>) {
        let (shutdown, stopped) = unbounded();
        (
            Arc::new(Self {
                shared,
                _shutdown: shutdown,
            }),
            stopped,
        )
    }
}

impl Drop for PoolLifetime {
    fn drop(&mut self) {
        let _queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        self.shared.closing.store(true, Ordering::Relaxed);
        self.shared.cv.notify_all();
        // Dropping the only sender broadcasts disconnection to every worker's
        // cancellable pixel send, even if a caller retained the public inbox.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn released(shared: std::sync::Weak<Shared>) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while shared.upgrade().is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            shared.upgrade().is_none(),
            "preview workers retained a dropped pool"
        );
    }

    #[test]
    fn last_handle_releases_idle_workers_across_repeated_pools() {
        for _ in 0..24 {
            let pool = PreviewPool::new();
            let shared = Arc::downgrade(&pool.shared);
            let retained = pool.clone();
            drop(pool);
            assert!(!retained.shared.closing.load(Ordering::Relaxed));
            drop(retained);
            released(shared);
        }
    }

    #[test]
    fn last_handle_releases_full_ready_inbox_with_retained_receiver() {
        let pool = PreviewPool::new();
        let shared = Arc::downgrade(&pool.shared);
        let retained_inbox = pool.rx.clone();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let image = Arc::new(bytes.into_inner());
        for id in 0..16 {
            pool.request_bytes(PreviewBytesRequest {
                id,
                key: id.to_string(),
                image: image.clone(),
                target_px: 2,
            });
        }
        // The bounded inbox fills while its receiver remains deliberately idle.
        let deadline = Instant::now() + Duration::from_secs(5);
        while pool.shared.queue.lock().unwrap().len() > 12 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!retained_inbox.is_empty());
        assert_eq!(
            pool.shared.queue.lock().unwrap().len(),
            12,
            "two ready images plus one pending send per worker fill the pipeline"
        );
        drop(pool);
        released(shared);
        drop(retained_inbox);
    }
}
