//! Existing read/work advancement and canonical completion adapter.
use super::*;
impl SlateApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance_presence_actions(
        &mut self,
        ctx: &egui::Context,
        board_visible: bool,
        active_tab: u64,
        pointer_press: bool,
        elapsed: Duration,
        now: Instant,
    ) {
        let mut read_budget = 256usize;
        for index in 0..self.presence.agents.len() {
            let mut a = self.presence.agents.remove(index);
            let mut completed_approval = None;
            let mut cancelled_approval = None;
            if board_visible && a.tab == active_tab && a.departure.is_none() {
                if !pointer_press && !a.paused {
                    if let Some(reply) = self.advance_presence_portal_call(&mut a, elapsed) {
                        let _ = self.presence.links.reply(&a.dir, reply);
                    }
                }
                if let Some(reply) =
                    if !pointer_press && (!a.paused || (a.chat_only && a.work.is_none())) {
                        self.advance_presence_read(&mut a, &mut read_budget, elapsed)
                    } else {
                        None
                    }
                {
                    let _ = self.presence.links.reply(&a.dir, reply);
                }
                let advance_work = !a.paused
                    || self.presence.approved_while_listening(&a)
                    || a.work
                        .as_ref()
                        .is_some_and(|w| w.relay_epoch_changed(a.control_epoch));
                if let Some(mut work) = advance_work.then(|| a.work.take()).flatten() {
                    let generation = self.doc().scene.scene_gen();
                    let authorization = self.validate_presence_work(&a, &work);
                    let applicable = authorization.is_ok()
                        && work.tab_id() == active_tab
                        && (work.checked_generation == Some(generation)
                            || work.expected_matches(&self.doc().scene).is_ok());
                    if applicable {
                        work.checked_generation = Some(generation);
                    }
                    let result = if !applicable {
                        Err(authorization.err().unwrap_or_else(|| {
                            "A human changed an action target; preview cancelled".to_owned()
                        }))
                    } else if pointer_press {
                        // Keep the displayed preview through late foreground/board routing.
                        Ok(false)
                    } else if let Some(motion) = self.presence.roster.session_mut(&a.motion) {
                        let start = if !work.started {
                            work.begin_step(motion)
                        } else {
                            Ok(())
                        };
                        start.and_then(|()| {
                            let sample = motion.advance(elapsed);
                            work.preview(sample)?;
                            if sample.phase == MotionPhase::ReadyToCommit {
                                let displayed = work.display_position(sample);
                                let settle = displayed != sample.position;
                                if settle {
                                    displayed.validate().map_err(|error| error.to_string())?;
                                    if motion.queued_len() != 0 {
                                        return Err(
                                            "Queued motion prevents crop contact settlement".into(),
                                        );
                                    }
                                }
                                let _ = motion.resolve_ready(|_| true);
                                if settle {
                                    motion
                                        .settle_idle_position(displayed)
                                        .map_err(|error| error.to_string())?;
                                }
                                work.complete_step();
                            }
                            Ok(work.index == work.total_steps())
                        })
                    } else {
                        Err("Session ended".into())
                    };
                    match result {
                        Ok(true) => {
                            if let witness::WitnessPurpose::ApprovedProposal(id) = &work.purpose {
                                completed_approval = Some(id.clone());
                                a.work = Some(work);
                            } else {
                                self.finish_presence_work(&mut a, work, now);
                            }
                        }
                        Ok(false) => {
                            a.work = Some(work);
                            ctx.request_repaint();
                        }
                        Err(error) => {
                            if let witness::WitnessPurpose::ApprovedProposal(id) = &work.purpose {
                                cancelled_approval = Some(id.clone());
                            }
                            a.note_helper_outcome("blocked", &error);
                            let _ = self
                                .presence
                                .links
                                .reply(&a.dir, BoardReply::error(&work.call, &error));
                            if let Some(m) = self.presence.roster.session_mut(&a.motion) {
                                if let Err(settlement) = work.cancel_motion(m) {
                                    a.status = format!("{error}; {settlement}");
                                } else {
                                    a.status = error;
                                }
                            } else {
                                a.status = error;
                            }
                            a.activity = "blocked".into();
                        }
                    }
                }
            }
            self.presence.agents.insert(index, a);
            if let Some(id) = cancelled_approval {
                self.presence_end_proposal_motion(&id, false);
            }
            if let Some(id) = completed_approval {
                self.complete_presence_proposal(&id);
            }
        }
    }
}
