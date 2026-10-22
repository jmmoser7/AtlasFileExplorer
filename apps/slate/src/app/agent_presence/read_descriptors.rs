//! Bounded descriptors of already-authorized scene objects. No filesystem I/O.
use super::*;
use serde_json::{json, Value};
use slate_doc::scene::{Node, NodeKind};
use slate_doc::{GroupId, TagId};

fn excerpt(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}
fn assignment_rows(assignments: &std::collections::BTreeMap<GroupId, TagId>) -> Vec<Value> {
    assignments
        .iter()
        .take(64)
        .map(|(group, tag)| {
            json!({
            "group_id":group.0,
            "tag_id":tag.0,
            })
        })
        .collect()
}
impl SlateApp {
    pub(super) fn presence_object_descriptor(
        &self,
        node: &Node,
        grant: &CollaboratorGrant,
    ) -> Value {
        let text = slate_doc::authored_text::text(node)
            .map(|text| excerpt(text, 384))
            .or_else(|| match &node.kind {
                NodeKind::Frame(frame) => Some(excerpt(&frame.title, 128)),
                _ => None,
            });
        let mut value = json!({
            "id":node.id.0.to_string(), "kind":node.kind.kind_name(),
            "rect":node.rect,"rotation_deg":node.rotation_deg,"opacity":node.opacity,
            "locked":node.locked,"hidden":node.hidden,"text":text,
        });
        value["z_index"] = json!(self.doc().scene.index_of(node.id));
        match &node.kind {
            NodeKind::Frame(frame) => {
                value["frame"] = json!({
                    "title":excerpt(&frame.title, 128),
                    "order":frame.order,
                    "fill":frame.fill,
                    "stroke":frame.stroke,
                    "corner":frame.corner,
                    "assignment_count":frame.assignments.len(),
                    "assignments":assignment_rows(&frame.assignments),
                    "assignments_truncated":frame.assignments.len() > 64,
                });
            }
            NodeKind::Shape(shape) => {
                value["style"] = json!({
                    "fill":shape.fill,
                    "stroke":shape.stroke,
                    "corner":shape.corner,
                });
                if let Some(text) = &shape.text {
                    value["text_style"] = json!({
                        "font":text.family.label(),
                        "size":text.size,
                        "color":text.color,
                        "alignment":text.align,
                        "bold":text.style.bold,
                        "italic":text.style.italic,
                        "underline":text.style.underline,
                    });
                }
            }
            NodeKind::Text(text) => {
                value["style"] = json!({
                    "font":text.family.label(),
                    "size":text.size,
                    "color":text.color,
                    "alignment":text.align,
                    "fill":text.fill,
                    "stroke":text.stroke,
                    "bold":text.style.bold,
                    "italic":text.style.italic,
                    "underline":text.style.underline,
                });
            }
            _ => {}
        }
        if let NodeKind::Image(image) = &node.kind {
            value["crop"] = json!(image.crop);
            value["mirror"] = json!({"x":image.flip_x,"y":image.flip_y});
            value["croppable"] = json!(self.croppable_image(node.id));
            value["mask"] = json!({"present":node.clip.is_some(),
                "closed":node.clip.as_ref().map(|clip|clip.closed),
                "segments":node.clip.as_ref().map(|clip|clip.segs.len()),
                "extra_contours":node.clip.as_ref().map(|clip|clip.extra.len()),
                "fill_rule":node.clip.as_ref().map(|clip|clip.fill_rule)});
            value["item"] = json!({"id":image.item.0.to_string()});
            if let Some(item) = self.doc().item(image.item) {
                // Names are board metadata, not permission to open source bytes.
                value["item"]["name"] = json!(excerpt(&item.file_name, 256));
                value["item"]["size"] = json!(item.size);
                value["item"]["modified"] = json!(item.mtime);
                value["item"]["pdf_page"] = json!(item.pdf_page);
                value["item"]["tag_assignments"] = json!({
                    "assignment_count":item.assignments.len(),
                    "assignments":assignment_rows(&item.assignments),
                    "assignments_truncated":item.assignments.len() > 64,
                });
            } else if let Some(path) = self.image_item_path(node.id) {
                value["item"]["name"] = json!(path
                    .file_name()
                    .map(|name| excerpt(&name.to_string_lossy(), 256)));
            }
            value["item"]["source_inputs_granted"] = json!(grant
                .readable_sources
                .iter()
                .any(|source| source == "board-file-inputs"));
        }
        if let NodeKind::Portal(portal) = &node.kind {
            value["title"] = json!(excerpt(&portal.title, 128));
            value["portal_kind"] = json!(portal.kind);
        }
        if let Some(program) = slate_doc::agent_chat::agent(node) {
            value["program"] = json!({
                "provider":excerpt(&program.provider, 128),
                "model":program.model.as_deref().map(|model|excerpt(model,128)),
                "view":program.view,"instruction":excerpt(&program.instruction,512),
                "image_settings":program.image,
            });
            if slate_doc::agent_inputs::is_image_generator(&self.doc().scene, node.id) {
                let session = self.agents.session(node.id);
                let request = self
                    .agents
                    .request_of(node.id)
                    .filter(|request| !request.is_empty())
                    .or_else(|| {
                        session
                            .map(|session| session.request.as_str())
                            .filter(|request| !request.is_empty())
                    })
                    .or_else(|| {
                        session.and_then(|session| {
                            session
                                .bundle
                                .images
                                .last()
                                .map(|image| image.request.as_str())
                        })
                    })
                    .filter(|request| !request.is_empty());
                let output_count = session.map_or(0, |session| {
                    session
                        .bundle
                        .images
                        .iter()
                        .filter(|image| request.is_some_and(|request| image.request == request))
                        .count()
                });
                let output_dimensions: Vec<_> = session
                    .into_iter()
                    .flat_map(|session| session.bundle.images.iter())
                    .filter(|image| request.is_some_and(|request| image.request == request))
                    .filter_map(|image| {
                        if let Some([width, height]) =
                            self.known_agent_image_dimensions(node.id, &image.id, &image.request)
                        {
                            return Some(json!({"id":image.id,"width":width,"height":height}));
                        }
                        let path = slate_doc::scene::resolve_source(
                            self.tab().path.as_deref(),
                            &image.source,
                        );
                        let key = super::super::preview::linked_image_key(&path, "shown");
                        self.preview_cache
                            .get(&key)
                            .filter(|entry| entry.px == super::super::preview::PX_EXACT)
                            .map(|entry| {
                                json!({
                                    "id":image.id,
                                    "width":entry.pixels.size[0],
                                    "height":entry.pixels.size[1],
                                })
                            })
                    })
                    .collect();
                let dimensions_status = if output_count == 0 {
                    "no_outputs"
                } else if output_dimensions.len() == output_count {
                    "exact"
                } else if output_dimensions.is_empty() {
                    "unknown"
                } else {
                    "partial"
                };
                let queued = self.generations_waiting(node.id);
                let status = if queued > 0 {
                    "queued"
                } else if self.agent_is_running(node.id) {
                    "running"
                } else if self.agent_failure_reason(node.id).is_some()
                    || session.is_some_and(|session| {
                        matches!(&session.status, atlas_agent::AgentStatus::Error(_))
                    })
                {
                    "failed"
                } else if output_count > 0 {
                    "complete"
                } else {
                    "idle"
                };
                let registered_output = self
                    .registered_agent_image_item(node.id)
                    .and_then(|item| self.doc().item(item))
                    .map(|item| json!({"item_id":item.id.0,"bytes":item.size}));
                value["program"]["job"] = json!({
                    "request":request,
                    "status":status,
                    "queued":queued,
                    "output_count":output_count,
                    "output_dimensions":output_dimensions,
                    "dimensions_status":dimensions_status,
                    "display_dimensions":{"width":node.rect.w,"height":node.rect.h},
                    "registered_output":registered_output,
                });
            }
            // A visible program does not implicitly disclose its conversation.
            if grant.conversation_ids.contains(&program.session) {
                value["program"]["conversation"] = json!(program.session);
                value["program"]["channel"] = json!(program
                    .channel
                    .as_deref()
                    .map(|channel| excerpt(channel, 256)));
                if let Some(snapshot) = self.agents.session(node.id) {
                    value["program"]["status"] = json!(snapshot.status);
                    value["program"]["reply"] = json!(snapshot
                        .turns
                        .iter()
                        .rev()
                        .find(|turn| turn.role == "assistant")
                        .map(|turn| excerpt(&turn.text, 512)));
                }
            }
        }
        value
    }
}
