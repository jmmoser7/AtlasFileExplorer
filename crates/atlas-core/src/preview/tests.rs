use super::*;

fn temp_raster(name: &str, w: u32, h: u32) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nfa_preview_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
    });
    img.save(&path).unwrap();
    path
}

#[test]
fn tier_ladder_quantizes_and_clamps() {
    assert_eq!(tier_for(100.0, 2048), 256);
    assert_eq!(tier_for(256.0, 2048), 256);
    assert_eq!(tier_for(300.0, 2048), 512);
    assert_eq!(tier_for(513.0, 2048), 1024);
    assert_eq!(tier_for(1500.0, 2048), 2048);
    // The user cap wins over the ladder…
    assert_eq!(tier_for(9000.0, 2048), 2048);
    assert_eq!(tier_for(9000.0, 1500), 1500);
    // …and a degenerate cap never drops below the smallest rung.
    assert_eq!(tier_for(9000.0, 0), TIER_MIN);
}

#[test]
fn decode_downscales_but_never_upscales() {
    let big = temp_raster("big.png", 1000, 500);
    let (w, h, rgba) = decode_preview(&big, 512, None).expect("large png decodes");
    assert_eq!((w, h), (512, 256));
    assert_eq!(rgba.len(), (w * h * 4) as usize);

    let small = temp_raster("small.png", 100, 80);
    let (w, h, _) = decode_preview(&small, 512, None).expect("small png decodes");
    assert_eq!((w, h), (100, 80), "sources are never upscaled");
}

#[test]
fn webp_uses_bounded_raster_downscale_without_upscaling() {
    let big = temp_raster("bounded.webp", 1000, 500);
    assert_eq!(
        image::ImageReader::open(&big).unwrap().format(),
        Some(image::ImageFormat::WebP)
    );
    let (w, h, rgba) = decode_preview(&big, 256, None).expect("WebP shares the raster decoder");
    assert_eq!((w, h), (256, 128));
    assert_eq!(rgba.len(), (w * h * 4) as usize);
    let small = temp_raster("small.webp", 40, 20);
    let (w, h, _) = decode_preview(&small, 512, None).expect("small WebP decodes");
    assert_eq!(
        (w, h),
        (40, 20),
        "a requested tier never upscales the source"
    );
    assert!(
        decode_preview(&big, 0, None).is_none(),
        "invalid target is refused before decode"
    );
}

#[test]
fn pool_round_trips_and_filters_sub_thumbnail_results() {
    let pool = PreviewPool::new();
    let big = temp_raster("pool_big.png", 640, 640);
    let tiny = temp_raster("pool_tiny.png", 64, 64);
    pool.request(PreviewRequest {
        min_edge: crate::thumbs::THUMB_PX as u32,
        local_only: false,
        id: 1,
        path: big,
        key: "big".into(),
        target_px: 512,
        pdf_page: None,
    });
    pool.request(PreviewRequest {
        min_edge: crate::thumbs::THUMB_PX as u32,
        local_only: false,
        id: 2,
        path: tiny,
        key: "tiny".into(),
        target_px: 512,
        pdf_page: None,
    });
    let mut got_big = false;
    let mut got_tiny = false;
    for _ in 0..2 {
        let res = pool
            .rx
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("preview result");
        match res.id {
            1 => {
                let PreviewOutcome::Ready(w, h, _) = res.outcome else {
                    panic!("big source produces a preview");
                };
                assert_eq!((w, h), (512, 512));
                got_big = true;
            }
            2 => {
                assert!(matches!(res.outcome, PreviewOutcome::Unavailable));
                got_tiny = true;
            }
            other => panic!("unexpected id {other}"),
        }
    }
    assert!(got_big && got_tiny);
}

fn unstarted_pool() -> PreviewPool {
    let shared = Arc::new(Shared {
        queue: Mutex::new(Vec::new()),
        cv: Condvar::new(),
    });
    let (tx, rx) = inbox::channel();
    PreviewPool { shared, tx, rx }
}

fn request(id: u32, key: &str, target_px: u32) -> PreviewRequest {
    PreviewRequest {
        min_edge: crate::thumbs::THUMB_PX as u32,
        local_only: false,
        id,
        path: PathBuf::new(),
        key: key.into(),
        target_px,
        pdf_page: None,
    }
}

#[test]
fn a_newer_tier_cancels_the_queued_request_and_keeps_lifo_order() {
    // Admission is tested without workers, decoding, filesystem I/O or waits.
    let pool = unstarted_pool();
    pool.request(request(1, "same", 256));
    pool.request(request(2, "other", 256));
    pool.request(request(3, "same", 512));
    let cancelled = pool.rx.try_recv().expect("superseded terminal result");
    assert_eq!(cancelled.id, 1);
    assert!(matches!(cancelled.outcome, PreviewOutcome::Cancelled));
    assert!(
        pool.rx.try_recv().is_err(),
        "each drop reports exactly once"
    );
    let mut queued = pool.shared.queue.lock().unwrap();
    assert_eq!(queued.len(), 2);
    assert_eq!(queued.pop().unwrap().id(), 3);
    assert_eq!(queued.pop().unwrap().id(), 2);
}

#[test]
fn queue_pressure_cancels_oldest_requests_and_keeps_the_newest_64() {
    let pool = unstarted_pool();
    for id in 0..80 {
        pool.request(request(id, &format!("key-{id}"), 512));
        assert!(pool.shared.queue.lock().unwrap().len() <= PREVIEW_QUEUE_CAP);
    }
    for id in 0..16 {
        let cancelled = pool.rx.try_recv().expect("evicted terminal result");
        assert_eq!(cancelled.id, id);
        assert!(matches!(cancelled.outcome, PreviewOutcome::Cancelled));
    }
    assert!(pool.rx.try_recv().is_err(), "survivors are still queued");
    let mut queued = pool.shared.queue.lock().unwrap();
    assert_eq!(queued.len(), PREVIEW_QUEUE_CAP);
    for id in (16..80).rev() {
        assert_eq!(
            queued.pop().unwrap().id(),
            id,
            "newest request decodes first"
        );
    }
}

#[test]
fn a_request_already_taken_by_a_worker_is_not_cancelled() {
    let pool = unstarted_pool();
    pool.request(request(1, "same", 256));
    let inflight = pool.shared.queue.lock().unwrap().pop().unwrap();
    pool.request(request(2, "same", 512));
    assert_eq!(inflight.id(), 1);
    assert!(
        pool.rx.try_recv().is_err(),
        "in-flight work finishes normally"
    );
    assert_eq!(pool.shared.queue.lock().unwrap().pop().unwrap().id(), 2);
}

#[test]
fn encoded_frames_share_keyed_queue_and_have_an_aggregate_bound() {
    let pool = unstarted_pool();
    let image = Arc::new(vec![0; MAX_LIVE_ENCODED_BYTES]);
    for id in 0..6 {
        pool.request_bytes(PreviewBytesRequest {
            id,
            key: format!("live-{id}"),
            image: image.clone(),
            target_px: 1024,
        });
    }
    let queue = pool.shared.queue.lock().unwrap();
    assert_eq!(queue.len(), 4);
    assert_eq!(
        queue.iter().map(Job::encoded_len).sum::<usize>(),
        4 * MAX_LIVE_ENCODED_BYTES
    );
    drop(queue);
    assert_eq!(pool.rx.try_recv().unwrap().id, 0);
    assert_eq!(pool.rx.try_recv().unwrap().id, 1);
    pool.request_bytes(PreviewBytesRequest {
        id: 7,
        key: "live-5".into(),
        image,
        target_px: 1024,
    });
    let old = pool.rx.try_recv().unwrap();
    assert_eq!(old.id, 5);
    assert!(matches!(old.outcome, PreviewOutcome::Cancelled));
}

#[test]
fn oversized_encoded_request_retires_without_queueing() {
    let pool = unstarted_pool();
    pool.request_bytes(PreviewBytesRequest {
        id: 9,
        key: "large".into(),
        image: Arc::new(vec![0; MAX_LIVE_ENCODED_BYTES + 1]),
        target_px: 1024,
    });
    assert!(pool.shared.queue.lock().unwrap().is_empty());
    let result = pool.rx.try_recv().unwrap();
    assert_eq!(result.id, 9);
    assert!(matches!(result.outcome, PreviewOutcome::Unavailable));
}
