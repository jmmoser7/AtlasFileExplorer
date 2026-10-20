//! Existing read/work advancement and canonical completion adapter.
use super::*;
impl SlateApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance_presence_actions(
        &mut self,
        ctx: &egui::Context,
        board_visible: bool,
        maximized_portal: Option<slate_doc::NodeId>,
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
            let presence_visible = board_visible || maximized_portal.is_some();
            if a.tab == active_tab && a.departure.is_none() {
                if presence_visible && !pointer_press && !a.paused {
                    if let Some(reply) = self.advance_presence_portal_call(ctx, &mut a, elapsed) {
                        let _ = self.presence.links.reply(&a.dir, reply);
                    }
                }
                if let Some(reply) = if presence_visible
                    && !pointer_press
                    && (!a.paused || (a.chat_only && a.work.is_none()))
                {
                    self.advance_presence_read_with_context(
                        Some(ctx),
                        &mut a,
                        &mut read_budget,
                        elapsed,
                    )
                } else {
                    None
                } {
                    let _ = self.presence.links.reply(&a.dir, reply);
                }
                let advance_work = presence_visible
                    && (!a.paused
                        || self.presence.approved_while_listening(&a)
                        || a.work
                            .as_ref()
                            .is_some_and(|w| w.relay_epoch_changed(a.control_epoch)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{only_portal, web_board, with_fake_host};
    use atlas_agent::presence::WorldPoint;
    use serde_json::json;

    fn join(h: &mut crate::app::tests::Harness) -> String {
        h.app.ai.config.workspace_dir = Some(h.base.clone());
        let profile = StoredAgent {
            id: atlas_agent::request_id(),
            name: "Max portal agent".into(),
            hat: "none".into(),
            provider: "local".into(),
            retired: false,
        };
        let id = profile.id.clone();
        h.app.join_presence(profile, None).unwrap();
        id
    }

    fn begin_request(a: &mut Collaborator) {
        a.request = "max-portal-test".into();
        a.sequences.clear();
        a.calls = 0;
        a.grant.as_mut().unwrap().max_actions = 20;
    }

    fn call(a: &Collaborator, sequence: u64, operation: BoardOperation) -> BoardCall {
        BoardCall {
            id: atlas_agent::request_id(),
            session: a.binding.clone(),
            request_id: a.request.clone(),
            sequence,
            operation,
        }
    }

    fn advance(h: &mut crate::app::tests::Harness, max_portal: slate_doc::NodeId) {
        let tab = h.app.tab().id;
        for _ in 0..80 {
            h.app.advance_presence_actions(
                &h.ctx,
                false,
                Some(max_portal),
                tab,
                false,
                Duration::from_millis(16),
                Instant::now(),
            );
            if h.app.presence.agents[0].read.is_none() && h.app.presence.agents[0].portal.is_none()
            {
                break;
            }
        }
    }

    fn has_avatar_head_at(shape: &egui::Shape, center: egui::Pos2) -> bool {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().any(|s| has_avatar_head_at(s, center)),
            egui::Shape::Mesh(mesh) => {
                let bounds = mesh.vertices.iter().filter(|v| v.color.a() == 255).fold(
                    egui::Rect::NOTHING,
                    |mut rect, vertex| {
                        rect.extend_with(vertex.pos);
                        rect
                    },
                );
                bounds.is_positive()
                    && bounds.center().distance(center) < 0.05
                    && bounds.width() > 30.0
            }
            _ => false,
        }
    }

    #[test]
    fn pending_board_read_advances_while_file_atlas_portal_is_maximized() {
        let mut h = web_board("maximized_atlas_read");
        let _web_host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/", egui::Pos2::ZERO);
        let atlas_node = h.app.doc().scene.node_preparation().build_node(
            slate_doc::scene::WorldRect::new(400.0, 100.0, 280.0, 180.0),
            slate_doc::scene::NodeKind::Portal(slate_doc::PortalNode::unbound_file_atlas("Folder")),
        );
        let atlas_id = atlas_node.id;
        h.app.add_nodes(vec![atlas_node]);
        h.frame();
        assert!(h.app.portal_maximize(atlas_id));

        let who = join(&mut h);
        let mut a = h.app.presence.agents.remove(0);
        assert_eq!(a.profile.id, who);
        begin_request(&mut a);
        h.app
            .presence
            .roster
            .session_mut(&a.motion)
            .unwrap()
            .begin_request(id(&a.request).unwrap())
            .unwrap();
        let read = call(
            &a,
            0,
            BoardOperation::Read {
                node_ids: vec![atlas_id.0.to_string()],
                offset: 0,
                limit: 1,
            },
        );
        h.app.begin_presence_read(&mut a, &read).unwrap();
        h.app.presence.agents.insert(0, a);

        advance(&mut h, atlas_id);
        assert!(h.app.presence.agents[0].read.is_none());
    }

    #[test]
    fn focused_web_portal_call_completes_while_its_portal_is_maximized() {
        let mut h = web_board("maximized_web_call");
        let _host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/", egui::Pos2::ZERO);
        let (portal_id, _) = only_portal(&h);
        h.app.web_allow_origin(portal_id);
        h.frame();
        assert!(h.app.portal_maximize(portal_id));
        // Let the ordinary Web admission pass see the maximized host rect.
        // The focused advance loop below does not run app frames or pump the
        // fake host, so it needs a live page before the call starts.
        h.frame();
        assert!(
            h.app.web.is_live(portal_id),
            "maximized Web portal must be live before advancing its agent call"
        );
        h.app.web_blur();

        let who = join(&mut h);
        let mut a = h.app.presence.agents.remove(0);
        assert_eq!(a.profile.id, who);
        begin_request(&mut a);
        h.app
            .presence
            .roster
            .session_mut(&a.motion)
            .unwrap()
            .begin_request(id(&a.request).unwrap())
            .unwrap();
        let focus = call(
            &a,
            0,
            BoardOperation::Invoke {
                command: "portal.web.focus".into(),
                targets: vec![portal_id.0.to_string()],
                arguments: json!({}),
            },
        );
        assert!(h
            .app
            .begin_presence_portal_call(Some(&h.ctx), &mut a, &focus)
            .unwrap()
            .is_none());
        h.app.presence.agents.insert(0, a);

        advance(&mut h, portal_id);
        assert!(h.app.presence.agents[0].portal.is_none());
        assert_eq!(h.app.web.focused, Some(portal_id));
    }

    #[test]
    fn web_click_sends_primary_button_to_fake_host() {
        let mut h = web_board("agent_web_click_primary");
        let host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/", egui::Pos2::ZERO);
        let (portal_id, _) = only_portal(&h);
        h.app.web_allow_origin(portal_id);
        h.frame();
        assert!(h.app.portal_maximize(portal_id));
        h.frame();
        assert!(h.app.web.is_live(portal_id));
        h.app.web_blur();

        join(&mut h);
        let mut a = h.app.presence.agents.remove(0);
        begin_request(&mut a);
        h.app
            .presence
            .roster
            .session_mut(&a.motion)
            .unwrap()
            .begin_request(id(&a.request).unwrap())
            .unwrap();
        let click = call(
            &a,
            0,
            BoardOperation::Invoke {
                command: "portal.web.click".into(),
                targets: vec![portal_id.0.to_string()],
                arguments: json!({"x":32.0,"y":24.0}),
            },
        );
        assert!(h
            .app
            .begin_presence_portal_call(Some(&h.ctx), &mut a, &click)
            .unwrap()
            .is_none());
        h.app.presence.agents.insert(0, a);

        advance(&mut h, portal_id);
        let buttons: Vec<(bool, u8)> = host.sent(|input| match input {
            crate::app::board_web::WebInput::Down { button, .. } => Some((true, *button)),
            crate::app::board_web::WebInput::Up { button, .. } => Some((false, *button)),
            _ => None,
        });
        assert_eq!(buttons, vec![(true, 0), (false, 0)]);
    }

    #[test]
    fn maximized_web_projects_agent_contact_and_paints_the_in_view_witness() {
        let mut h = web_board("maximized_web_presence_projection");
        let _host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://example.com/", egui::Pos2::ZERO);
        let (portal_id, portal) = only_portal(&h);
        h.frame();
        assert!(h.app.portal_maximize(portal_id));
        let who = join(&mut h);
        let motion = h.app.presence.agents[0].motion.clone();
        let node = h.app.doc().scene.node(portal_id).unwrap().clone();
        h.app
            .presence
            .roster
            .session_mut(&motion)
            .unwrap()
            .settle_idle_position(WorldPoint {
                x: f64::from(node.rect.x + node.rect.w * 0.5),
                y: f64::from(node.rect.y + node.rect.h * 0.5),
            })
            .unwrap();
        assert_eq!(h.app.presence.agents[0].profile.id, who);

        let screen = h.ctx.screen_rect();
        let layout = crate::app::board_portal_chrome::layout_for_portal(
            portal.kind,
            screen,
            false,
            true,
            h.app.node_resolved_corner(&node),
            1.0,
        );
        let xf = crate::app::board_portal_chrome::fit_xf(node.rect, layout.body);
        let center = xf.w2s(egui::pos2(
            node.rect.x + node.rect.w * 0.5,
            node.rect.y + node.rect.h * 0.5,
        ));
        let viewport = portal.web_ref().viewport;
        let (width, height) = if viewport.zoom == slate_doc::scene::WebZoom::Auto {
            (layout.body.width(), layout.body.height())
        } else {
            let width = viewport.width_css as f32;
            (
                width,
                width * layout.body.height() / layout.body.width().max(1.0),
            )
        };
        let contact = h
            .app
            .agent_web_contact(&h.ctx, portal_id, width * 0.5, height * 0.5)
            .unwrap();
        assert!((contact.0 - f64::from(node.rect.x + node.rect.w * 0.5)).abs() < 0.01);
        assert!((contact.1 - f64::from(node.rect.y + node.rect.h * 0.5)).abs() < 0.01);

        let output = h.frame_output(|_| {});
        assert!(output
            .shapes
            .iter()
            .any(|shape| has_avatar_head_at(&shape.shape, center)));
    }
}
