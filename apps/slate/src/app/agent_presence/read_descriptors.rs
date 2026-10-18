//! Bounded descriptors of already-authorized scene objects. No filesystem I/O.
use super::*;
use serde_json::{json, Value};
use slate_doc::scene::{Node, NodeKind};

fn excerpt(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
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
            "rect":node.rect,"rotation_deg":node.rotation_deg,"locked":node.locked,"hidden":node.hidden,"text":text,
        });
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
