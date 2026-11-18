//! Monitor requested definitions and their linked images without rendering an idle board.
use super::*;
use links::image;
use std::collections::HashSet;

type Changes = (Vec<SourceRevisionUpdate>, image::Revisions);

pub(super) struct Watch {
    tab: u64,
    images: HashSet<PathBuf>,
    results: Receiver<Changes>,
    // A new watch or a closed document interrupts the old scan and its timed wait.
    _lifetime: Sender<()>,
}

impl Viewers {
    pub(super) fn requested_image_paths(&self, tab: u64) -> HashSet<PathBuf> {
        self.graph.requested_images(
            self.views
                .keys()
                .filter_map(|&(doc, id)| (doc == tab).then_some(id)),
        )
    }

    pub(super) fn refresh_source_watch(&mut self, tab: u64) {
        if self.source_poll.as_ref().is_some_and(|watch| {
            watch.tab != tab || watch.images != self.requested_image_paths(tab)
        }) {
            self.source_poll = None;
        }
    }

    pub(super) fn poll_sources(&mut self, ctx: &egui::Context, tab: u64) -> bool {
        let mut changed = false;
        if let Some(watch) = &self.source_poll {
            match watch.results.try_recv() {
                Ok((revisions, images)) => {
                    changed |=
                        image::accept_revisions(&mut self.image_revisions, images, &watch.images);
                    for revision in revisions {
                        let Some(view) = self.views.get_mut(&revision.key) else {
                            continue;
                        };
                        if view.instance == revision.instance
                            && view.source.key == revision.source_key
                            && view.source_revision == revision.before
                        {
                            view.source_revision = revision.current;
                            // A first observation establishes the baseline for a
                            // pending or running solve. The worker independently
                            // checks for source changes before returning its result.
                            let solving = self.background.as_ref().and_then(|bg| bg.busy)
                                == Some(revision.key);
                            if revision.before.is_some() || (view.due.is_none() && !solving) {
                                view.invalidate(Duration::ZERO);
                                changed = true;
                            }
                        }
                    }
                }
                Err(crossbeam_channel::TryRecvError::Empty) => return false,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {}
            }
            self.source_poll = None;
        }
        let active: Vec<_> = self
            .views
            .iter()
            .filter(|(key, _)| key.0 == tab)
            .map(|(&key, view)| {
                (
                    key,
                    view.instance,
                    view.source_revision.clone(),
                    view.source.clone(),
                )
            })
            .collect();
        if active.is_empty() {
            return changed;
        }
        let images = self.requested_image_paths(tab);
        self.image_revisions.retain(|path, _| images.contains(path));
        let paths = images.clone();
        let mut previous = self.image_revisions.clone();
        let (send, results) = crossbeam_channel::bounded(1);
        let (lifetime, stop) = crossbeam_channel::bounded(0);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let cancelled =
                || !matches!(stop.try_recv(), Err(crossbeam_channel::TryRecvError::Empty));
            loop {
                if cancelled() {
                    return;
                }
                let revisions: Vec<_> = active
                    .iter()
                    .filter_map(|(key, instance, before, source)| {
                        let current = source_revision(source).ok();
                        (*before != current).then(|| SourceRevisionUpdate {
                            key: *key,
                            instance: *instance,
                            source_key: source.key.clone(),
                            before: before.clone(),
                            current,
                        })
                    })
                    .collect();
                let fresh = image::inspect_revisions(&paths, &previous, &cancelled);
                if cancelled() {
                    return;
                }
                if !revisions.is_empty() || image::changed(&previous, &fresh) {
                    if send.send((revisions, fresh)).is_ok() {
                        ctx.request_repaint();
                    }
                    return;
                }
                previous = fresh;
                if stop.recv_timeout(Duration::from_millis(750))
                    != Err(crossbeam_channel::RecvTimeoutError::Timeout)
                {
                    return;
                }
            }
        });
        self.source_poll = Some(Watch {
            tab,
            images,
            results,
            _lifetime: lifetime,
        });
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;

    #[test]
    fn first_source_observation_preserves_an_inflight_solve_and_later_edits_requeue() {
        let mut h = Harness::new("grasshopper-inflight-first-observation");
        let file = h.base.join("facade.ghx");
        std::fs::write(&file, "definition").unwrap();
        let item = h.app.add_paths(&[file])[0];
        h.app.place_items_on_board(&[item], egui::Pos2::ZERO);
        let model = *h.app.board_sel.iter().next().unwrap();
        h.app.request_grasshopper(model, &h.ctx);
        let tab = h.app.tab().id;
        let key = (tab, model);
        let viewers = &mut h.app.grasshopper;
        let view = viewers.views.get_mut(&key).unwrap();
        let initial_generation = view.generation;
        view.due = None;
        let mut background = Background::start(&h.ctx);
        background.busy = Some(key);
        viewers.background = Some(background);

        // Deliver observations deterministically at the point between
        // scheduling the native job and receiving its first response.
        for (before, current, changed) in [(None, "first", false), (Some("first"), "edited", true)]
        {
            let view = &viewers.views[&key];
            let (send, results) = crossbeam_channel::bounded(1);
            send.send((
                vec![SourceRevisionUpdate {
                    key,
                    instance: view.instance,
                    source_key: view.source.key.clone(),
                    before: before.map(str::to_owned),
                    current: Some(current.into()),
                }],
                Default::default(),
            ))
            .unwrap();
            viewers.source_poll = Some(Watch {
                tab,
                images: HashSet::new(),
                results,
                _lifetime: crossbeam_channel::bounded(1).0,
            });
            assert_eq!(viewers.poll_sources(&h.ctx, tab), changed);
            let view = &viewers.views[&key];
            assert_eq!(view.source_revision.as_deref(), Some(current));
            assert_eq!(view.generation, initial_generation + u64::from(changed));
            assert_eq!(view.due.is_some(), changed);
        }
    }

    #[test]
    fn unchanged_definitions_stay_quiet_until_an_external_edit() {
        let mut h = Harness::new("grasshopper-idle-source-watch");
        let definition = h.base.join("facade.ghx");
        std::fs::write(&definition, "first").unwrap();
        let item = h.app.add_paths(std::slice::from_ref(&definition))[0];
        h.app.place_items_on_board(&[item], egui::Pos2::ZERO);
        let model = *h.app.board_sel.iter().next().unwrap();
        h.app.request_grasshopper(model, &h.ctx);
        let tab = h.app.tab().id;
        let key = (tab, model);
        h.app.grasshopper.poll_sources(&h.ctx, tab);
        let wait_for_change = |app: &SlateApp| {
            let end = Instant::now() + Duration::from_secs(10);
            while app
                .grasshopper
                .source_poll
                .as_ref()
                .unwrap()
                .results
                .is_empty()
            {
                assert!(Instant::now() < end, "file change did not wake the board");
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        wait_for_change(&h.app);
        h.app.grasshopper.poll_sources(&h.ctx, tab);
        let original = h.app.grasshopper.views[&key]
            .source_revision
            .clone()
            .unwrap();
        std::thread::sleep(Duration::from_millis(1600));
        assert!(h
            .app
            .grasshopper
            .source_poll
            .as_ref()
            .unwrap()
            .results
            .is_empty());
        std::fs::write(definition, "second definition").unwrap();
        wait_for_change(&h.app);
        assert!(h.app.grasshopper.poll_sources(&h.ctx, tab));
        assert_ne!(
            h.app.grasshopper.views[&key].source_revision.as_ref(),
            Some(&original)
        );
    }
}
