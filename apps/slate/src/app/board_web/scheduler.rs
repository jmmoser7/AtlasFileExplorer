//! Browser acquisition and GPU-copy retirement have separate clocks. A ready
//! focused copy should paint on the next board frame, even between captures.

use super::*;

impl SlateApp {
    pub(super) fn pump_web_pixels(&mut self, ctx: &egui::Context, admitted: Vec<NodeId>) {
        let mut pending = std::mem::take(&mut self.web.backlog);
        for id in admitted {
            if !pending.contains(&id) {
                pending.push(id);
            }
        }
        pending.retain(|id| {
            self.web.views.get(id).is_some_and(|v| {
                v.live && (v.height_px >= LOD_STRIP_PX || self.web.focused == Some(*id))
            })
        });
        if let Some(focused) = self.web.focused {
            if let Some(index) = pending.iter().position(|id| *id == focused) {
                pending[..=index].rotate_right(1);
            }
        }

        let mut polls = 0;
        let mut readbacks = 0;
        let mut requests = 0;
        let mut serviced = [None; READS_PER_FRAME * 2];
        let mut serviced_count = 0;
        let mut next_due: Option<Duration> = None;
        let mut wake_after = |delay: Duration| {
            next_due = Some(next_due.map_or(delay, |old| old.min(delay)));
        };

        // At most READS_PER_FRAME completed copies become CPU images, and at
        // most that many portals are probed. Failed nonblocking maps do not
        // stop the acquisition pass from using another free staging slot.
        for &id in &pending {
            if polls >= READS_PER_FRAME || self.web.uploads_this_frame >= UPLOADS_PER_FRAME {
                break;
            }
            if !self.web.host.has_pending_frame(id) {
                continue;
            }
            polls += 1;
            serviced[serviced_count] = Some(id);
            serviced_count += 1;
            let started = Instant::now();
            let frame = self.web.host.poll_frame(id);
            self.web.pixel_ms += started.elapsed().as_secs_f32() * 1000.0;
            if let Some(image) = frame {
                readbacks += 1;
                self.upload_poster(ctx, id, image);
                self.web.uploads_this_frame += 1;
            }
        }

        let now = Instant::now();
        let asynchronous = self.web.host.frame_requests_are_async();
        for &id in &pending {
            let interval = self.web.focused_frame_interval(id);
            let remaining = self.web.views[&id]
                .last_frame_probe
                .map_or(Duration::ZERO, |last| {
                    interval.saturating_sub(now.saturating_duration_since(last))
                });
            if remaining.is_zero() {
                if requests < READS_PER_FRAME
                    && (asynchronous
                        || (readbacks < READS_PER_FRAME
                            && self.web.uploads_this_frame < UPLOADS_PER_FRAME))
                {
                    requests += 1;
                    serviced[serviced_count] = Some(id);
                    serviced_count += 1;
                    self.web.views.get_mut(&id).unwrap().last_frame_probe = Some(now);
                    let started = Instant::now();
                    let frame = self.web.host.request_frame(id);
                    debug_assert!(
                        !asynchronous || frame.is_none(),
                        "async requests only queue copies"
                    );
                    self.web.pixel_ms += started.elapsed().as_secs_f32() * 1000.0;
                    // Immediate/fake hosts retain the original take_frame
                    // seam. Their returned pixels spend the same budgets.
                    if let Some(image) = frame {
                        readbacks += 1;
                        self.upload_poster(ctx, id, image);
                        self.web.uploads_this_frame += 1;
                    }
                    wake_after(interval);
                } else {
                    wake_after(Duration::ZERO);
                }
            } else {
                wake_after(remaining);
            }
            if self.web.host.has_pending_frame(id) {
                let fps = if self.web.focused == Some(id) {
                    INTERACTIVE_FPS
                } else {
                    IDLE_FPS
                };
                wake_after(Duration::from_secs_f32(1.0 / fps));
            }
        }

        // Retain focus priority, but rotate other serviced views so a six-view
        // board shares the two-copy budget even while some GPU maps are busy.
        for id in serviced.into_iter().flatten() {
            if self.web.focused != Some(id) {
                if let Some(index) = pending.iter().position(|queued| *queued == id) {
                    pending.remove(index);
                    pending.push(id);
                }
            }
        }
        let cap = atlas_core::display::WEB_BACKLOG_CAP;
        if pending.len() > cap {
            pending.drain(0..pending.len() - cap);
        }
        self.web.backlog = pending;
        if let Some(after) = next_due {
            ctx.request_repaint_after(after);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct Log {
        pending: HashSet<NodeId>,
        ready: HashSet<NodeId>,
        unavailable: HashSet<NodeId>,
        polls: Vec<NodeId>,
        requests: Vec<NodeId>,
    }

    struct SplitHost(Rc<RefCell<Log>>);

    impl WebHost for SplitHost {
        fn available(&self) -> bool {
            true
        }
        fn admit(&mut self, _: NodeId, _: &WebRequest) {}
        fn evict(&mut self, _: NodeId) {}
        fn take_frame(&mut self, _: NodeId) -> Option<WebFrame> {
            panic!("split hosts should not use the combined acquisition/readback seam")
        }
        fn has_pending_frame(&self, id: NodeId) -> bool {
            self.0.borrow().pending.contains(&id)
        }
        fn poll_frame(&mut self, id: NodeId) -> Option<WebFrame> {
            let mut log = self.0.borrow_mut();
            log.polls.push(id);
            log.ready
                .contains(&id)
                .then(|| egui::ColorImage::new([4, 4], Color32::BLUE).into())
        }
        fn request_frame(&mut self, id: NodeId) -> Option<WebFrame> {
            let mut log = self.0.borrow_mut();
            log.requests.push(id);
            if !log.unavailable.contains(&id) {
                log.pending.insert(id);
            }
            None
        }
        fn frame_requests_are_async(&self) -> bool {
            true
        }
        fn capture_poster(&mut self, _: NodeId) -> Option<WebFrame> {
            None
        }
        fn send_input(&mut self, _: NodeId, _: WebInput) {}
        fn cursor(&self, _: NodeId) -> Option<egui::CursorIcon> {
            None
        }
        fn load_error(&self, _: NodeId) -> Option<String> {
            None
        }
    }

    fn fixture(count: usize) -> (egui::Context, SlateApp, Rc<RefCell<Log>>, Vec<NodeId>) {
        let ctx = egui::Context::default();
        let mut app = SlateApp::with_ctx(&ctx, None);
        let log = Rc::new(RefCell::new(Log::default()));
        app.web.host = Box::new(SplitHost(Rc::clone(&log)));
        let ids: Vec<_> = (1..=count).map(|id| NodeId(id as u64)).collect();
        for &id in &ids {
            let mut view = WebView::new("test".into());
            view.live = true;
            view.height_px = 400.0;
            app.web.views.insert(id, view);
        }
        (ctx, app, log, ids)
    }

    #[test]
    fn focused_completed_copy_paints_before_another_capture_is_due() {
        let (ctx, mut app, log, ids) = fixture(LIVE_POOL);
        let focused = *ids.last().unwrap();
        app.web.focused = Some(focused);
        for view in app.web.views.values_mut() {
            view.last_frame_probe = Some(Instant::now() + Duration::from_secs(1));
        }
        log.borrow_mut().pending.extend(ids.iter().copied());
        log.borrow_mut().ready.extend(ids.iter().copied());
        app.pump_web_pixels(&ctx, ids);
        assert_eq!(
            log.borrow().polls[0],
            focused,
            "the active dashboard retires first"
        );
        assert_eq!(log.borrow().polls.len(), READS_PER_FRAME);
        assert!(
            log.borrow().requests.is_empty(),
            "retirement does not restart acquisition cadence"
        );
        assert_eq!(app.web.uploads_this_frame, UPLOADS_PER_FRAME);
        assert!(app.web.views[&focused].poster_fresh);
    }

    #[test]
    fn busy_completed_copy_does_not_prevent_new_capture_work() {
        let (ctx, mut app, log, ids) = fixture(1);
        let id = ids[0];
        app.web.focused = Some(id);
        log.borrow_mut().pending.insert(id);
        app.upload_poster(&ctx, id, egui::ColorImage::new([4, 4], Color32::RED));
        let texture = app.web.views[&id].poster.as_ref().unwrap().id();
        let captured = app.web.views[&id].poster_at;
        app.pump_web_pixels(&ctx, ids);
        assert_eq!(log.borrow().polls, vec![id]);
        assert_eq!(log.borrow().requests, vec![id]);
        assert_eq!(app.web.uploads_this_frame, 0);
        assert_eq!(app.web.views[&id].poster_at, captured);
        assert_eq!(app.web.views[&id].poster.as_ref().unwrap().id(), texture);
    }

    #[test]
    fn retiring_two_ready_copies_still_refills_native_capture_in_the_same_pump() {
        let (ctx, mut app, log, ids) = fixture(READS_PER_FRAME);
        let focused = *ids.last().unwrap();
        app.web.focused = Some(focused);
        log.borrow_mut().pending.extend(ids.iter().copied());
        log.borrow_mut().ready.extend(ids.iter().copied());
        app.pump_web_pixels(&ctx, ids);
        let state = log.borrow();
        assert_eq!(state.polls.len(), READS_PER_FRAME);
        assert_eq!(app.web.uploads_this_frame, UPLOADS_PER_FRAME);
        assert_eq!(state.requests.len(), READS_PER_FRAME);
        assert_eq!(
            state.requests[0], focused,
            "the active dashboard immediately queues its next copy"
        );
    }

    #[test]
    fn six_ready_views_share_two_readbacks_while_focus_keeps_priority() {
        let (ctx, mut app, log, ids) = fixture(LIVE_POOL);
        let focused = *ids.last().unwrap();
        app.web.focused = Some(focused);
        for view in app.web.views.values_mut() {
            view.last_frame_probe = Some(Instant::now() + Duration::from_secs(1));
        }
        log.borrow_mut().pending.extend(ids.iter().copied());
        log.borrow_mut().ready.extend(ids.iter().copied());
        let mut served = HashSet::new();
        for _ in 0..LIVE_POOL {
            app.web.uploads_this_frame = 0;
            log.borrow_mut().polls.clear();
            app.pump_web_pixels(&ctx, ids.clone());
            let state = log.borrow();
            assert_eq!(state.polls[0], focused);
            assert!(state.polls.len() <= READS_PER_FRAME);
            assert!(app.web.uploads_this_frame <= UPLOADS_PER_FRAME);
            served.extend(state.polls.iter().copied());
        }
        assert_eq!(
            served.len(),
            LIVE_POOL,
            "background dashboards do not starve"
        );
    }

    #[test]
    fn new_capture_requests_remain_bounded_when_every_copy_is_busy() {
        let (ctx, mut app, log, ids) = fixture(LIVE_POOL);
        log.borrow_mut().pending.extend(ids.iter().copied());
        app.pump_web_pixels(&ctx, ids);
        assert_eq!(log.borrow().polls.len(), READS_PER_FRAME);
        assert_eq!(log.borrow().requests.len(), READS_PER_FRAME);
        assert_eq!(app.web.uploads_this_frame, 0);
    }

    #[test]
    fn hosts_that_never_paint_do_not_starve_other_capture_requests() {
        let (ctx, mut app, log, ids) = fixture(LIVE_POOL);
        log.borrow_mut()
            .unavailable
            .extend(ids.iter().take(READS_PER_FRAME).copied());
        let mut requested = HashSet::new();
        for _ in 0..LIVE_POOL {
            // Keep every host due, including the two that cannot queue any
            // pixels yet. Ordering alone must distribute the finite budget.
            for view in app.web.views.values_mut() {
                view.last_frame_probe = None;
            }
            log.borrow_mut().requests.clear();
            log.borrow_mut().polls.clear();
            app.pump_web_pixels(&ctx, ids.clone());
            let state = log.borrow();
            assert!(state.requests.len() <= READS_PER_FRAME);
            assert!(state.polls.len() <= READS_PER_FRAME);
            requested.extend(state.requests.iter().copied());
        }
        assert_eq!(
            requested.len(),
            LIVE_POOL,
            "a loading dashboard cannot monopolize new captures"
        );
    }

    #[test]
    fn immediate_hosts_keep_the_legacy_seam_and_upload_budget() {
        let (ctx, mut app, _, ids) = fixture(LIVE_POOL);
        let host = crate::app::tests::FakeWebHost::default();
        host.0.borrow_mut().admitted.extend(ids.iter().copied());
        app.web.host = Box::new(host);
        app.pump_web_pixels(&ctx, ids.clone());
        assert_eq!(app.web.uploads_this_frame, UPLOADS_PER_FRAME);
        assert_eq!(
            app.web
                .views
                .values()
                .filter(|view| view.poster_fresh)
                .count(),
            UPLOADS_PER_FRAME
        );
        for _ in 0..LIVE_POOL {
            app.web.uploads_this_frame = 0;
            app.pump_web_pixels(&ctx, ids.clone());
            assert!(app.web.uploads_this_frame <= UPLOADS_PER_FRAME);
        }
        assert!(app.web.views.values().all(|view| view.poster_fresh));
    }
}
