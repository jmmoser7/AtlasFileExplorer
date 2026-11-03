//! Text-answer selection and hit targets over the existing authored galley.
use super::*;
use slate_doc::text_answers::{self, TextAnswer, TextAnswerRef};
use std::{ops::Range, sync::Arc};

#[derive(Clone, PartialEq)]
struct SelectedPassage {
    source: String,
    range: Range<usize>,
}

#[derive(Clone)]
struct LinkHit {
    answer: TextAnswer,
    rects: Vec<Rect>,
}

#[derive(Clone)]
struct HitCache {
    galley: Arc<egui::Galley>,
    answers: Arc<Vec<TextAnswer>>,
    revision: Option<String>,
    links: Arc<Vec<LinkHit>>,
    selection: Option<Arc<SelectedPassage>>,
    selected_rects: Arc<Vec<Rect>>,
}

fn passage_id(id: NodeId) -> Id {
    Id::new(("text-answer-passage", id.0))
}

fn link_rects(galley: &egui::Galley, range: Range<usize>) -> Vec<Rect> {
    let mut offset = 0;
    let mut rects = Vec::new();
    for row in &galley.rows {
        let end = offset + row.char_count_excluding_newline();
        let start = range.start.max(offset);
        let stop = range.end.min(end);
        if start < stop {
            rects.push(Rect::from_min_max(
                Pos2::new(row.x_offset(start - offset), row.min_y()),
                Pos2::new(row.x_offset(stop - offset), row.max_y()),
            ));
        }
        offset += row.char_count_including_newline();
        if offset >= range.end {
            break;
        }
    }
    rects
}

impl SlateApp {
    pub(crate) fn text_answer_owns_request(&self, id: NodeId) -> bool {
        let Some(session) = self.agents.session(id) else {
            return false;
        };
        let request = self.agents.request_of(id).unwrap_or(&session.request);
        if self
            .agents
            .flow
            .text_query_runs
            .get(&(self.tab().id, id))
            .is_some_and(|query| query == request)
            || self.agents.flow.text_answer_requests.contains_key(&(
                self.tab().id,
                id,
                request.to_owned(),
            ))
        {
            return true;
        }
        let Some(node) = self.doc().scene.node(id) else {
            return false;
        };
        let Some(agent) = slate_doc::agent_chat::agent(node) else {
            return false;
        };
        let answers = match &node.kind {
            NodeKind::Text(text) => &text.answers,
            NodeKind::Image(image) => &image.answers,
            _ => return false,
        };
        let through = session
            .turns
            .iter()
            .enumerate()
            .rev()
            .find(|(_, turn)| turn.role == "assistant")
            .map(|(index, _)| index + 1);
        session.request == request
            && answers.iter().any(|answer| {
                answer.response.session == agent.session && Some(answer.response.through) == through
            })
    }
    /// Current cached words/revision; this never reads or writes the linked file.
    pub(crate) fn text_answer_source(&self, id: NodeId) -> Option<(String, Option<String>)> {
        match &self.doc().scene.node(id)?.kind {
            NodeKind::Text(text) => Some((text.text.clone(), None)),
            NodeKind::Image(image) => image
                .text_override
                .clone()
                .map(|text| (text, None))
                .or_else(|| {
                    Some((
                        self.cached_snippet(image.item)?.to_owned(),
                        Some(self.doc().item(image.item)?.cache_key.clone()),
                    ))
                }),
            _ => None,
        }
    }
    /// Capture a widget selection without consuming it when focus moves to chat.
    pub(crate) fn capture_text_answer_selection(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        text: &str,
        chars: Range<usize>,
    ) {
        let Some(range) = text_answers::byte_range(text, chars) else {
            return;
        };
        if text[range.clone()].trim().is_empty() {
            return;
        }
        self.agents
            .flow
            .text_selections
            .insert((self.tab().id, id), (text.into(), range.clone()));
        ctx.data_mut(|data| {
            data.insert_temp(
                passage_id(id),
                Arc::new(SelectedPassage {
                    source: text.into(),
                    range,
                }),
            )
        });
    }

    /// The request adapter snapshots this alongside the prompt; it is not memory.
    pub(crate) fn text_answer_selection(
        &self,
        ctx: &egui::Context,
        id: NodeId,
    ) -> Option<(String, Range<usize>)> {
        let selected = ctx.data(|data| data.get_temp::<Arc<SelectedPassage>>(passage_id(id)))?;
        let node = self.doc().scene.node(id)?;
        let (text, _) = self.text_answer_source(node.id)?;
        (text == selected.source).then(|| (selected.source.clone(), selected.range.clone()))
    }

    /// Record only against the exact source captured when the question was sent.
    pub(crate) fn record_text_answer(
        &mut self,
        id: NodeId,
        source: &str,
        range: Range<usize>,
        response: TextAnswerRef,
        question: String,
        answer: String,
    ) -> bool {
        let Some((text, revision)) = self.text_answer_source(id) else {
            return false;
        };
        if text != source || self.refuse_read_only_edit() {
            return false;
        }
        let Some(mut link) = TextAnswer::new(source, range, response, question, answer) else {
            return false;
        };
        link.source_revision = revision;
        link.origin = self.text_output_source(id);
        let author = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|agent| slate_doc::scene::CmdAuthor::Agent(agent.provider.clone()))
            .unwrap_or(slate_doc::scene::CmdAuthor::Human);
        self.patch_nodes_as(&[id], author, |node| match &mut node.kind {
            NodeKind::Text(text) => {
                text_answers::upsert(&mut text.answers, &text.text, link.clone());
            }
            NodeKind::Image(image) => {
                text_answers::upsert(&mut image.answers, source, link.clone());
            }
            _ => {}
        });
        true
    }

    /// The generator's navigation adapter consumes this; no session is resumed.
    pub(crate) fn take_text_answer_navigation(
        &self,
        ctx: &egui::Context,
    ) -> Option<(NodeId, slate_doc::text_answers::TextOutputRef)> {
        ctx.data_mut(|data| {
            let id = Id::new("text-answer-navigation");
            let target = data.get_temp::<(NodeId, slate_doc::text_answers::TextOutputRef)>(id);
            data.remove::<(NodeId, slate_doc::text_answers::TextOutputRef)>(id);
            target
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint_text_answer_links(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        node: &slate_doc::scene::Node,
        origin: Pos2,
        center: Pos2,
        galley: &Arc<egui::Galley>,
        shown: &str,
        zoom: f32,
        interactive: bool,
    ) {
        let answers = match &node.kind {
            NodeKind::Text(text) => &text.answers,
            NodeKind::Image(image) => &image.answers,
            _ => return,
        };
        let revision = match &node.kind {
            NodeKind::Image(image) if image.text_override.is_none() => self
                .doc()
                .item(image.item)
                .map(|item| item.cache_key.as_str()),
            _ => None,
        };
        let selection = ui
            .ctx()
            .data(|data| data.get_temp::<Arc<SelectedPassage>>(passage_id(node.id)))
            .filter(|selection| selection.source == shown);
        if answers.is_empty() && selection.is_none() {
            return;
        }
        let cache_id = Id::new(("text-answer-hits", node.id.0));
        let prior = ui.ctx().data(|data| data.get_temp::<HitCache>(cache_id));
        let cache = prior
            .filter(|cache| {
                Arc::ptr_eq(&cache.galley, galley)
                    && cache.answers.as_slice() == answers.as_slice()
                    && cache.selection == selection
                    && cache.revision.as_deref() == revision
            })
            .unwrap_or_else(|| HitCache {
                galley: galley.clone(),
                answers: Arc::new(answers.clone()),
                revision: revision.map(str::to_owned),
                links: Arc::new(
                    text_answers::valid_answers(shown, answers)
                        .into_iter()
                        .filter(|answer| answer.is_valid_for_revision(shown, revision))
                        .filter_map(|answer| {
                            Some(LinkHit {
                                rects: link_rects(galley, answer.char_range(shown)?),
                                answer: answer.clone(),
                            })
                        })
                        .collect(),
                ),
                selected_rects: Arc::new(selection.as_ref().map_or_else(Vec::new, |selection| {
                    let range = shown[..selection.range.start].chars().count()
                        ..shown[..selection.range.end].chars().count();
                    link_rects(galley, range)
                })),
                selection,
            });
        ui.ctx()
            .data_mut(|data| data.insert_temp(cache_id, cache.clone()));
        let rotation = egui::emath::Rot2::from_angle(node.rotation_deg.to_radians());
        let to_screen = |p: Pos2| center + rotation * (origin + p.to_vec2() - center);
        let pointer = ui.ctx().pointer_hover_pos();
        let local_pointer = pointer.map(|p| {
            let local = center + rotation.inverse() * (p - center);
            local - origin.to_vec2()
        });
        let color = self
            .palette()
            .link
            .gamma_multiply(node.opacity.clamp(0.0, 1.0));
        for rect in cache.selected_rects.iter() {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    to_screen(rect.left_top()),
                    to_screen(rect.right_top()),
                    to_screen(rect.right_bottom()),
                    to_screen(rect.left_bottom()),
                ],
                color.gamma_multiply(0.13),
                egui::Stroke::NONE,
            ));
        }
        for (index, link) in cache.links.iter().enumerate() {
            for (part, rect) in link.rects.iter().enumerate() {
                let a = to_screen(rect.left_bottom());
                let b = to_screen(rect.right_bottom());
                painter.line_segment([a, b], egui::Stroke::new(0.7 * zoom, color));
                if !interactive {
                    continue;
                }
                let bounds = Rect::from_points(&[
                    to_screen(rect.left_top()),
                    to_screen(rect.right_top()),
                    a,
                    b,
                ])
                .intersect(painter.clip_rect());
                if !bounds.is_positive() {
                    continue;
                }
                let inside = local_pointer.is_some_and(|p| rect.contains(p))
                    && pointer.is_some_and(|p| painter.clip_rect().contains(p));
                let response = ui.interact(
                    bounds,
                    Id::new(("text-answer", node.id.0, index, part)),
                    if self.board_sel.contains(&node.id) {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    },
                );
                let keyboard = response.has_focus()
                    && ui.input(|input| {
                        input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
                    });
                if response.clicked() && (inside || keyboard) {
                    let source = link.answer.origin.clone().or_else(|| {
                        self.doc()
                            .scene
                            .node(node.id)
                            .and_then(slate_doc::agent_chat::agent)
                            .filter(|agent| agent.session == link.answer.response.session)
                            .and_then(|_| self.text_output_source(node.id))
                    });
                    if let Some(source) = source {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                Id::new("text-answer-navigation"),
                                (
                                    node.id,
                                    slate_doc::text_answers::TextOutputRef {
                                        review_id: link.answer.review_id.clone(),
                                        source,
                                        response: link.answer.response.clone(),
                                    },
                                ),
                            )
                        });
                    } else {
                        self.toast("The recorded response source is unavailable. Its saved answer remains here.");
                    }
                }
                if response.has_focus() && !inside {
                    egui::show_tooltip_for(ui.ctx(), ui.layer_id(), response.id, &bounds, |ui| {
                        ui.set_max_width(320.0);
                        ui.weak(&link.answer.question);
                        ui.separator();
                        ui.label(&link.answer.answer);
                    });
                } else if inside {
                    response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_ui(|ui| {
                            ui.set_max_width(320.0);
                            ui.weak(&link.answer.question);
                            ui.separator();
                            ui.label(&link.answer.answer);
                        });
                }
            }
        }
    }
}
