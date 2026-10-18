//! Typed command adapters for media controls and the tag visibility palette.
use super::*;

#[derive(serde::Deserialize)]
struct NodeRequest {
    node: slate_doc::NodeId,
}

#[derive(serde::Deserialize)]
struct CountRequest {
    node: slate_doc::NodeId,
    count: u8,
}

#[derive(serde::Deserialize)]
struct ArtifactRequest {
    node: slate_doc::NodeId,
    #[serde(default)]
    revision: Option<String>,
}

#[derive(serde::Deserialize)]
struct TagVisibility {
    tags: Vec<slate_doc::TagId>,
    visible: bool,
}

#[derive(serde::Deserialize)]
struct AddTag {
    name: String,
    color: [u8; 3],
    #[serde(default)]
    group: Option<slate_doc::GroupId>,
}

impl SlateApp {
    pub(super) fn dispatch_curve_fade(&mut self, detail: Option<&str>) -> bool {
        let enabled = detail
            .and_then(|detail| serde_json::from_str::<serde_json::Value>(detail).ok())
            .and_then(|value| value.get("enabled").and_then(|value| value.as_bool()));
        self.set_curve_fade_armed(enabled);
        true
    }

    fn media_command_request<T: serde::de::DeserializeOwned>(
        &mut self,
        detail: Option<&str>,
    ) -> Option<T> {
        let request = self.object_command_request(detail)?;
        if self.tab().read_only {
            self.toast("Workbook is read-only.");
            return None;
        }
        Some(request)
    }

    fn object_command_request<T: serde::de::DeserializeOwned>(
        &mut self,
        detail: Option<&str>,
    ) -> Option<T> {
        if self.at_home || self.tabs.is_empty() {
            self.toast("Open a workbook before using this command.");
            return None;
        }
        match detail.and_then(|detail| serde_json::from_str(detail).ok()) {
            Some(request) => Some(request),
            None => {
                self.toast("This command needs valid object parameters.");
                None
            }
        }
    }

    pub(super) fn dispatch_image_generate(&mut self, detail: Option<&str>) -> bool {
        if detail
            .and_then(|detail| serde_json::from_str::<serde_json::Value>(detail).ok())
            .is_some_and(|value| value.get("targets").is_some())
        {
            return self.dispatch_targeted_human("portal.agent.run", detail);
        }
        let Some(NodeRequest { node }) = self.media_command_request(detail) else {
            return false;
        };
        if self
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::scene::artifact)
            .is_some()
        {
            return self.queue_artifact_generation(node);
        }
        if !slate_doc::agent_inputs::is_image_generator(&self.doc().scene, node) {
            return false;
        }
        self.run_agent_media(node);
        true
    }

    pub(super) fn dispatch_artifact(&mut self, command: &str, detail: Option<&str>) -> bool {
        let request = if matches!(
            command,
            "portal.artifact.source_open" | "portal.artifact.memory_inspect"
        ) {
            self.object_command_request(detail)
        } else {
            self.media_command_request(detail)
        };
        let Some(ArtifactRequest { node, revision }) = request else {
            return false;
        };
        if self
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::scene::artifact)
            .is_none()
        {
            return false;
        }
        match command {
            "portal.artifact.accept" => self.accept_artifact_output(node),
            "portal.artifact.reject" => self.reject_artifact_output(node),
            "portal.artifact.fork" => self.artifact_fork(node),
            "portal.artifact.memory_disconnect" => self.artifact_disconnect_memory(node),
            "portal.artifact.memory_inspect" => self.artifact_toggle_memory_inspect(node),
            "portal.artifact.revision" => {
                revision.is_some_and(|revision| self.artifact_pick_revision(node, &revision))
            }
            "portal.artifact.source_open" => self.artifact_open_source(node),
            _ => false,
        }
    }

    pub(super) fn switch_selected_agent_chat(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        self.open_agent_chat_picker(id);
        true
    }

    pub(super) fn dispatch_image_count(&mut self, detail: Option<&str>) -> bool {
        let Some(CountRequest { node, count }) = self.media_command_request(detail) else {
            return false;
        };
        self.set_image_agent_count(node, count)
    }

    pub(super) fn dispatch_tag_visibility(&mut self, detail: Option<&str>) -> bool {
        let Some(TagVisibility { tags, visible }) = self.media_command_request(detail) else {
            return false;
        };
        self.set_tag_visibility(&tags, visible) > 0
    }

    pub(super) fn dispatch_tag_add(&mut self, detail: Option<&str>) -> bool {
        let Some(AddTag { name, color, group }) = self.media_command_request(detail) else {
            return false;
        };
        let Some(command) = slate_doc::TagCommand::prepare_add(
            self.doc(),
            &name,
            color,
            group,
            slate_doc::scene::CmdAuthor::Human,
        ) else {
            self.toast("Enter a tag name and choose an existing facet when provided.");
            return false;
        };
        if !command.apply(self.doc_mut()) {
            self.toast("Couldn't add that tag.");
            return false;
        }
        let tab = self.tab_mut();
        tab.dirty = true;
        tab.edits.push(board::BoardMark::Tags(command));
        tab.edit_redo.clear();
        tab.journal.discard_redo();
        self.last_board_edit = None;
        self.publish_session_tags();
        true
    }
}

impl SlateApp {
    pub(super) fn dispatch_media_intake(
        &mut self,
        ctx: &egui::Context,
        command: &str,
        detail: Option<&str>,
    ) -> bool {
        match command {
            "board.media.image" | "board.media.model" | "board.media.video"
            | "board.media.text" => {
                let group = match command {
                    "board.media.model" => slate_doc::media::MediaGroup::Model,
                    "board.media.video" => slate_doc::media::MediaGroup::Video,
                    "board.media.text" => slate_doc::media::MediaGroup::Text,
                    _ => slate_doc::media::MediaGroup::Image,
                };
                self.add_media_dialog(group);
                true
            }
            "board.media.text_new" => {
                let screen = ctx
                    .input(|i| i.pointer.hover_pos())
                    .filter(|p| self.canvas_rect.contains(*p))
                    .unwrap_or_else(|| self.canvas_rect.center());
                let world = self.screen_to_world(screen);
                self.place_text_document_at(world)
            }
            "board.media.page" => {
                if let Some((item, page)) = detail
                    .and_then(|s| s.split_once(':'))
                    .and_then(|(a, b)| Some((a.parse::<u64>().ok()?, b.parse::<u16>().ok()?)))
                {
                    self.set_pdf_poster_page(slate_doc::ItemId(item), page);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
