//! Thumbnail worker loop; priority, extraction and cache policy stay with thumbs.
use super::*;

pub(super) fn run(shared: Arc<Shared>, tx: Sender<ThumbResult>, cache_dir: PathBuf) {
    #[cfg(windows)]
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    loop {
        // Tier 0 = on-demand, 1 = warm, 2 = pinned pre-warm.
        let (req, tier) = {
            let mut q = shared.queue.lock().unwrap();
            loop {
                if shared.closing.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(r) = pop_preferred_hot(&mut q.hot) {
                    break (r, 0u8);
                }
                if !q.warm.is_empty()
                    && shared.warm_active.load(Ordering::Relaxed) < WARM_CONCURRENCY
                {
                    shared.warm_active.fetch_add(1, Ordering::Relaxed);
                    break (pop_preferred_warm(&mut q.warm), 1);
                }
                if shared.slow_active.load(Ordering::Relaxed)
                    < shared.slow_limit.load(Ordering::Relaxed)
                {
                    if let Some(r) = q.slow.pop_front() {
                        shared.slow_active.fetch_add(1, Ordering::Relaxed);
                        break (r, 2);
                    }
                    if let Some(r) = q.slow_deferred.pop_front() {
                        shared.slow_active.fetch_add(1, Ordering::Relaxed);
                        break (r, 2);
                    }
                }
                q = shared.cv.wait(q).unwrap();
            }
        };
        let done_tier = || match tier {
            1 => {
                shared.warm_active.fetch_sub(1, Ordering::Relaxed);
                shared.cv.notify_one();
            }
            2 => {
                shared.slow_active.fetch_sub(1, Ordering::Relaxed);
                shared.cv.notify_one();
            }
            _ => {}
        };
        let stale = req.generation != PINNED_GENERATION
            && req.generation != shared.active_generation.load(Ordering::Relaxed);
        if stale {
            done_tier();
            continue;
        }

        let cache_file = cache_dir.join(format!("{}.jpg", req.key));
        let icon_file = cache_dir.join(format!("{}.icon.jpg", req.key));
        let shared_file = req
            .shared_dir
            .as_ref()
            .map(|d| d.join(format!("{}.jpg", req.key)));
        let image = load_cached(&cache_file)
            .inspect(|_img| {
                // Any tier: if we have a local JPEG and the shared tier is
                // missing it, publish now. Hot on-demand views are the main
                // way thumbnails first land in the project cache.
                if let Some(sf) = &shared_file {
                    publish_shared(&cache_file, sf);
                }
            })
            .or_else(|| {
                // Shared project tier: pull the ready-made JPEG onto local
                // disk (one small copy) and decode from there.
                let sf = shared_file.as_ref()?;
                std::fs::copy(sf, &cache_file).ok()?;
                load_cached(&cache_file)
            })
            .or_else(|| {
                // A file we could previously only get an icon for: serve that
                // icon unless this session still owes it a retry.
                if icon_file.exists() && !shared.should_retry_icon(&req.key) {
                    return load_cached(&icon_file);
                }
                let got = extract_thumbnail(&req.path, req.pdf_page)?;
                if got.cacheable {
                    save_cached(&cache_file, got.w, got.h, &got.rgba);
                    if let Some(sf) = &shared_file {
                        publish_shared(&cache_file, sf);
                    }
                    // The real preview finally arrived; the icon is now noise.
                    let _ = std::fs::remove_file(&icon_file);
                } else {
                    // Icons live in their own tier so they are never mistaken
                    // for a preview, never published to the shared project
                    // cache, and never permanent.
                    save_cached(&icon_file, got.w, got.h, &got.rgba);
                }
                Some((got.w, got.h, got.rgba))
            });
        done_tier();

        if req.generation != PINNED_GENERATION
            && req.generation != shared.active_generation.load(Ordering::Relaxed)
        {
            continue;
        }
        let warm = tier > 0;
        let avg = image.as_ref().map(|(_, _, rgba)| avg_of(rgba));
        let _ = tx.send(ThumbResult {
            id: req.id,
            generation: req.generation,
            color_only: req.color_only,
            warm,
            dropped: false,
            src_bytes: req.src_bytes,
            avg,
            // Warm and color-only jobs exist to fill the disk cache and
            // harvest the average color; shipping pixels back would balloon
            // UI memory (the result channel holds them until drained).
            image: if warm || req.color_only { None } else { image },
        });
    }
}
