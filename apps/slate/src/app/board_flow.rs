//! Dataflow on image generators and text blocks, after Flora and xFigura:
//! typed left-edge inputs and an output whose click or dropped wire offers
//! Text, Image or Agent. Text block cards run a local model once per press.
//!
//! `WireHost::ports` reads `slate_doc::agent_inputs`; runs use the generator queue. See
//! `docs/keymap/contracts/portal-agent-link.md` (Flow ports, 23 September 2026).

mod ports;
mod text_note;
pub(crate) use ports::PORT_RADIUS;

use std::collections::HashMap;

use atlas_ai::agent::{AgentRequest, PortalView};
use atlas_shell::{canvas_scale, canvas_text};
use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect};
use slate_doc::agent_inputs::{self, InputKind, OUTPUT_T};
use slate_doc::scene::{ConnectorEnd, NodeId, NodeKind, PortalNode, SceneCmd, Side};
use slate_doc::wire::connector_route_in_scene;

use super::board::{BoardDrag, BoardXf};
use super::board_agent::{bind_program, paint_overlay_pill, program_card_size, InputRole};
use super::SlateApp;

mod image_editor;
mod result_actions;
mod state;
pub(crate) use state::FlowUi;
use state::{AgentDraft, Engines, FlowMenu};
mod image_workflow;
mod model_defaults;
mod spawn_flow;

/// Designed width of a picture an agent generates.
const AGENT_PICTURE_W: f32 = 480.0;
/// Designed size of port labels and the text block's type.
const PORT_LABEL_PX: f32 = 11.0;
const BLOCK_TEXT_PX: f32 = 14.0;
/// Ports show while the pointer is this near a flow node (designed px).
const PORT_REVEAL: f32 = 56.0;
/// Pointer travel that still counts as a click on the output port (screen px).
const CLICK_SLOP: f32 = 4.0;
/// Room the docked prompt takes under a note's words, designed px.
const NOTE_DOCK: f32 = 56.0;
/// A note an agent writes grows to fit its reply up to this height.
const NOTE_MAX_H: f32 = 1200.0;
/// Providers a text block may run on: local, ChatGPT sign-in, OpenAI API.
const TEXT_ENGINES: [&str; 3] = ["ollama", "codex-text", "openai-text"];

mod editor_focus;
mod editor_helpers;
use editor_helpers::{absorb_key, fresh_seed, model_index};

/// The instruction a text block spawned from a picture starts with.
const DESCRIBE_STYLE: &str = "Describe the visual style of this image as a comma-separated image prompt of 15 to 30 words: medium, palette, lighting, texture and mood. Leave out the subject.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpawnKind {
    Text,
    Image,
    Artifact,
}

/// One choice of the output-port and wire-drop menu ([`SPAWN_CHOICES`]).
/// The editor mode switch reads Text and Image as [`MODALITIES`]; 3D and video
/// join when supported. Agent opens a chat train without making media.
pub(crate) struct Modality {
    /// `None` for Agent.
    pub kind: Option<SpawnKind>,
    pub id: &'static str,
    pub label: &'static str,
    pub menu: &'static str,
    pub icon: atlas_shell::menu::MenuIcon,
    /// A catalog glyph painted instead of `icon`.
    pub glyph: Option<atlas_shell::icons::Icon>,
}

const TEXT_CHOICE: Modality = Modality {
    kind: Some(SpawnKind::Text),
    id: "text",
    label: "Text",
    menu: "Text · language model",
    icon: atlas_shell::menu::MenuIcon::Chat,
    glyph: None,
};

const IMAGE_CHOICE: Modality = Modality {
    kind: Some(SpawnKind::Image),
    id: "image",
    label: "Image",
    menu: "Image · generator",
    icon: atlas_shell::menu::MenuIcon::Image,
    glyph: None,
};

/// A chat-train portal whose first input is the source (user, 28 September 2026).
pub(crate) const AGENT_CHOICE: Modality = Modality {
    kind: None,
    id: "agent",
    label: "Agent",
    menu: "Agent · chat train",
    icon: atlas_shell::menu::MenuIcon::None,
    glyph: Some(atlas_shell::icons::Icon::Agent),
};

pub(crate) const MODALITIES: &[Modality] = &[TEXT_CHOICE, IMAGE_CHOICE];

const ARTIFACT_CHOICE: Modality = Modality {
    kind: Some(SpawnKind::Artifact),
    id: "artifact",
    label: "Artifact",
    menu: "Artifact · interactive dashboard",
    icon: atlas_shell::menu::MenuIcon::None,
    glyph: Some(atlas_shell::icons::Icon::WebPortal),
};
pub(crate) const SPAWN_CHOICES: &[Modality] =
    &[TEXT_CHOICE, IMAGE_CHOICE, AGENT_CHOICE, ARTIFACT_CHOICE];

impl SpawnKind {
    fn modality(self) -> &'static Modality {
        SPAWN_CHOICES.iter().find(|m| m.kind == Some(self)).unwrap()
    }

    fn from_id(id: &str) -> Option<Self> {
        SPAWN_CHOICES
            .iter()
            .find(|m| m.id == id)
            .and_then(|m| m.kind)
    }

    fn index(self) -> usize {
        MODALITIES
            .iter()
            .position(|m| m.kind == Some(self))
            .unwrap()
    }
}

impl SlateApp {
    fn is_flow_output(&self, grip: (NodeId, Side, f32)) -> bool {
        grip.1 == Side::Right
            && (grip.2 - OUTPUT_T).abs() < 0.001
            && agent_inputs::is_flow_node(&self.doc().scene, grip.0)
    }

    fn flow_output_at(&self, screen: Pos2, xf: &BoardXf) -> Option<NodeId> {
        self.wire_grip_at(screen, xf)
            .filter(|grip| self.is_flow_output(*grip))
            .map(|grip| grip.0)
    }

    fn open_flow_menu(
        &mut self,
        source: NodeId,
        at: Pos2,
        dropped: bool,
        grip: Option<(Side, f32)>,
    ) {
        self.agents.flow.menu = Some(FlowMenu {
            source,
            at,
            dropped,
            grip,
            armed: false,
        });
    }

    /// The output-port menu and output-port clicks. Runs before canvas
    /// gestures; true while the pointer is over the menu.
    pub(crate) fn flow_input(&mut self, ui: &egui::Ui, xf: &BoardXf) -> bool {
        let pointer = ui.ctx().pointer_latest_pos();
        let mut captured = false;
        if let Some(mut menu) = self.agents.flow.menu.take() {
            let dark = self.palette().dark_mode;
            let shown = atlas_shell::menu::anchored(
                ui.ctx(),
                Id::new(("flow-spawn", menu.source.0)),
                ui.layer_id(),
                xf.w2s(menu.at) + egui::vec2(10.0, -12.0),
                dark,
                240.0,
                Some(&mut menu.armed),
                |ui| {
                    let upstream = self.flow_is_upstream(menu.source, menu.grip);
                    atlas_shell::menu::heading(
                        ui,
                        if upstream {
                            "Add an input"
                        } else {
                            "Make with an agent"
                        },
                        dark,
                    );
                    let mut chosen = None;
                    let choices = if upstream { MODALITIES } else { SPAWN_CHOICES };
                    for m in choices {
                        let label = upstream.then(|| self.flow_input_label(menu.source, m.label));
                        let row = atlas_shell::menu::Row {
                            glyph: m.glyph,
                            ..atlas_shell::menu::Row::new(
                                m.icon,
                                label.as_deref().unwrap_or(m.menu),
                            )
                        };
                        if atlas_shell::menu::row(ui, row, dark).clicked() {
                            chosen = Some(m.id);
                        }
                    }
                    chosen
                },
            );
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if let Some(kind) = shown.inner {
                let detail = serde_json::json!({
                    "source": menu.source.0,
                    "kind": kind,
                    "at": menu.dropped.then_some([menu.at.x, menu.at.y]),
                    "from": menu.grip,
                });
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.agent.spawn"),
                    Some(detail.to_string()),
                );
                captured = true;
            } else if !(shown.dismissed || escape) {
                captured = pointer.is_some_and(|p| shown.rect.contains(p));
                self.agents.flow.menu = Some(menu);
            }
        }
        if self.tab().read_only || self.board_tool != super::board::BoardTool::Select {
            return captured;
        }
        let Some(p) = pointer else {
            return captured;
        };
        if ui.input(|i| i.pointer.primary_pressed()) {
            self.agents.flow.press = self.flow_output_at(p, xf).map(|id| (id, p));
        }
        if ui.input(|i| i.pointer.primary_released()) {
            if let Some((id, origin)) = self.agents.flow.press.take() {
                let wiring = matches!(self.board_drag, Some(BoardDrag::Wire(_)));
                if p.distance(origin) < CLICK_SLOP && !wiring {
                    if let Some(node) = self.doc().scene.node(id) {
                        let at = self.wire_host(node).anchor(Side::Right, OUTPUT_T);
                        self.open_flow_menu(id, Pos2::new(at[0], at[1]), false, None);
                    }
                }
            }
        }
        captured
    }

    /// A media wire released on empty board opens the modality menu there.
    /// Generators and notes offer it from their output; left edges hold inputs.
    /// False when `from` offers no agent.
    pub(crate) fn flow_wire_released(&mut self, from: (NodeId, Side, f32), cursor: Pos2) -> bool {
        let upstream = self.flow_is_upstream(from.0, Some((from.1, from.2)));
        let offers = if upstream {
            true
        } else if agent_inputs::is_flow_node(&self.doc().scene, from.0) {
            self.is_flow_output(from)
        } else {
            self.is_agent_media(from.0)
        };
        if !offers {
            return false;
        }
        let on_source = self
            .doc()
            .scene
            .node(from.0)
            .is_some_and(|n| n.rect.contains(cursor.x, cursor.y));
        if !on_source {
            self.open_flow_menu(from.0, cursor, true, Some((from.1, from.2)));
        }
        true
    }

    /// Media an agent can start from: a placed picture, video, 3D model or text
    /// document, authored words, or a generator's or text block's output.
    /// Chat cards and wires are not media.
    pub(crate) fn is_agent_media(&self, id: NodeId) -> bool {
        let scene = &self.doc().scene;
        if agent_inputs::is_flow_node(scene, id) {
            return true;
        }
        let Some(node) = scene.node(id) else {
            return false;
        };
        let media = match &node.kind {
            NodeKind::Text(_) => true,
            // A web page offers its shown picture to both agents.
            NodeKind::Portal(_) => agent_inputs::is_web_page(scene, id),
            // Pictures, video, 3D and text documents; not PDFs or design files.
            NodeKind::Image(image) => self.doc().item(image.item).is_some_and(|item| {
                use slate_doc::MediaKind;
                matches!(
                    slate_doc::media_kind(&item.path),
                    MediaKind::Image | MediaKind::Video | MediaKind::Model | MediaKind::Text
                )
            }),
            _ => false,
        };
        media
            && agent_inputs::wire_kind(scene, id, &|item| {
                self.doc().item(item).map(|i| i.path.as_path())
            })
            .is_some()
    }

    /// The menu's Agent: a local chat train bound to the workbook's `assets/agent`
    /// folder, with a new chat and `source` context wire in one journal step.
    pub(crate) fn prepare_agent_train_node(
        scene: &mut slate_doc::scene::Scene,
        rect: slate_doc::WorldRect,
        program: &atlas_ai::agent::AgentProvider,
        binding: &super::board_agent::ProgramBinding,
    ) -> slate_doc::scene::Node {
        let mut node = Self::prepare_agent_portal_node(scene, rect);
        bind_program(&mut node, program, binding);
        node.rect = rect;
        if let Some(agent) = slate_doc::agent_chat::agent_mut(&mut node) {
            agent.chat.train = true;
            agent.chat.detail = slate_doc::agent_chat::Detail::Summary;
            agent.channel = None;
        }
        node
    }

    fn spawn_agent_train(
        &mut self,
        source: NodeId,
        at: Option<[f32; 2]>,
        from: Option<(Side, f32)>,
    ) -> bool {
        const CONTEXT_T: f32 = 0.5;
        let (w, h) = self.agent_portal_size();
        let size = egui::vec2(w, h);
        let drop = at.map(|[x, y]| Pos2::new(x, y - CONTEXT_T * size.y));
        let zoom = self.tab().cam.z;
        let Some(rect) = self.agent_spawn_rect(source, drop, size, zoom) else {
            return false;
        };
        let (dir, fallback) = self.agent_build_folder();
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.toast(format!("Could not create the agent folder: {err}"));
            return false;
        }
        let program = atlas_ai::agent::provider_by_id("local");
        let mut binding = self.program_binding(None);
        binding.set_locator(super::board_portal::source_locator(
            self.tab().path.as_deref(),
            &dir,
        ));
        let node =
            Self::prepare_agent_train_node(&mut self.doc_mut().scene, rect, &program, &binding);
        let id = node.id;
        let (side, t) = from.unwrap_or((Side::Right, OUTPUT_T));
        let wire = self.build_connector_with(
            ConnectorEnd::Anchored {
                node: source,
                side,
                t,
            },
            ConnectorEnd::Anchored {
                node: id,
                side: Side::Left,
                t: CONTEXT_T,
            },
            std::slice::from_ref(&node),
        );
        if !matches!(&wire.kind, NodeKind::Connector(c) if c.binding.is_some()) {
            self.toast("This node has no input for that output.");
            return false;
        }
        if self.add_nodes(vec![node, wire]).is_empty() {
            return false;
        }
        self.board_sel = std::iter::once(id).collect();
        if fallback {
            self.toast(format!(
                "This workbook isn't saved yet, so the agent is using {}.",
                dir.display()
            ));
        }
        true
    }

    /// Link a fresh `response.txt` under slate-outputs for a text-language-model window.
    fn seed_agent_text_output_on_node(&mut self, node: &mut slate_doc::scene::Node) -> bool {
        let id = node.id;
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            return false;
        };
        let Some(agent) = slate_doc::agent_chat::agent(node) else {
            return false;
        };
        let session = agent.session.clone();
        let Some(output_dir) = self.agent_output_dir(id, &ws, &session) else {
            return false;
        };
        let path = std::path::PathBuf::from(&output_dir).join("response.txt");
        let item = self
            .doc_mut()
            .add_item(path.clone(), "response.txt", 0, 0, "txt");
        if let NodeKind::Image(i) = &mut node.kind {
            i.item = item;
        }
        self.cache_snippet(item, Some(String::new()));
        self.agents
            .text_output
            .write_now(id, item, path, String::new());
        true
    }

    /// Placeholder the text block's composer shows while it is empty.
    pub(crate) fn composer_hint(&self, id: NodeId) -> &'static str {
        if self.is_text_block(id) {
            "Tell the model what to write"
        } else {
            "Message"
        }
    }

    pub(crate) fn is_text_block(&self, id: NodeId) -> bool {
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.view == PortalView::Text)
    }

    /// A text block's prompt: its wired words (a note, a document, a page's
    /// text), then its instruction.
    pub(crate) fn text_block_prompt(
        &mut self,
        id: NodeId,
        inputs: &atlas_agent::InputSnapshot,
    ) -> String {
        let typed = self.agents.prompt_mut(id).trim().to_string();
        inputs
            .on(atlas_agent::InputSlot::Prompt)
            .map(|item| item.text.trim())
            .chain(std::iter::once(typed.as_str()))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Run a text block once with wired prompts, its instruction, and Image inputs.
    pub(crate) fn queue_text_block(&mut self, id: NodeId) {
        self.last_board_edit = None;
        let journaled = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.instruction.clone())
            .unwrap_or_default();
        self.sync_instruction(id, &journaled);
        let inputs = match self.agent_input_snapshot(id) {
            Ok(inputs) => inputs,
            Err(error) => {
                self.fail_agent_await(id, error);
                return;
            }
        };
        let prompt = self.text_block_prompt(id, &inputs);
        if prompt.is_empty() {
            self.fail_agent_await(
                id,
                "Type an instruction, or wire a note to Prompt.".to_string(),
            );
            return;
        }
        let model = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|a| a.model.clone());
        let ws = self
            .ai
            .config
            .valid_workspace()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let session = self
            .agent_session_for(id)
            .map(|(s, _)| s)
            .unwrap_or_default();
        let request = AgentRequest {
            id: atlas_ai::agent::request_id(),
            prompt,
            model,
            model_parameters: self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .map(|a| a.model_parameters.clone())
                .unwrap_or_default(),
            at: atlas_ai::context::now_secs(),
            inputs,
            history: vec![],
            image: None,
            output_dir: self.agent_output_dir(id, &ws, &session),
            oneshot: true,
            ..Default::default()
        };
        self.enqueue_generation(id, request);
        self.last_board_edit = None;
    }

    pub(crate) fn agent_text_window_owned(&self, id: NodeId) -> bool {
        let Some(NodeKind::Image(_)) = self.doc().scene.node(id).map(|n| &n.kind) else {
            return matches!(
                self.doc().scene.node(id).map(|n| &n.kind),
                Some(NodeKind::Text(t)) if !t.text.trim().is_empty()
            );
        };
        self.agents
            .text_output
            .file_has_content(id, |item| self.cached_snippet(item))
    }

    pub(crate) fn clear_agent_text_output_file(&mut self, id: NodeId) {
        let Some(NodeKind::Image(i)) = self.doc().scene.node(id).map(|n| n.kind.clone()) else {
            return;
        };
        let Some(path) = self.doc().item(i.item).map(|it| it.path.clone()) else {
            return;
        };
        let path =
            slate_doc::scene::resolve_source(self.tab().path.as_deref(), &path.to_string_lossy());
        self.invalidate_document_card(i.item);
        self.agents.text_output.begin_run(id, i.item, path);
    }

    /// Derived streaming sync: assistant text mirrors into the linked file.
    pub(crate) fn sync_agent_text_output(&mut self, id: NodeId) {
        if !self.is_text_block(id) {
            return;
        }
        let Some((item, path)) = (|| {
            let node = self.doc().scene.node(id)?;
            let NodeKind::Image(i) = &node.kind else {
                return None;
            };
            let path = self.item_path(i.item)?;
            Some((i.item, path))
        })() else {
            return;
        };
        let Some(text) = self.agent_reply(id) else {
            return;
        };
        let streaming = self.agent_is_running(id) || self.generations_waiting(id) > 0;
        self.agents
            .text_output
            .sync(id, item, &path, &text, streaming);
    }

    pub(crate) fn pump_agent_text_outputs(&mut self) {
        for (item, path) in self.agents.text_output.pump() {
            self.invalidate_document_source(item, &path);
        }
    }

    /// Text-language-model window: linked text document card plus agent chrome.
    pub(crate) fn paint_agent_text_window(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &slate_doc::scene::Node,
        body: Rect,
    ) {
        self.paint_agent_note(ui, painter, xf, node, body);
    }

    /// The newest reply of an agent's session, once it has finished.
    pub(crate) fn agent_reply(&self, id: NodeId) -> Option<String> {
        self.agents
            .session(id)
            .and_then(|s| s.turns.iter().rev().find(|t| t.role == "assistant"))
            .map(|t| t.text.trim().to_string())
            .filter(|t| !t.is_empty())
    }

    /// Every note's shown reply, for the artifact writer.
    pub(crate) fn export_agent_replies(&self) -> std::collections::BTreeMap<NodeId, String> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| Some((n.id, self.agent_note_reply(n.id)?)))
            .collect()
    }

    /// What a note an agent writes shows while the person has not made the
    /// words their own: the agent's reply.
    pub(crate) fn agent_note_reply(&self, id: NodeId) -> Option<String> {
        if !self.is_text_block(id) {
            return None;
        }
        match self.doc().scene.node(id).map(|n| &n.kind) {
            Some(NodeKind::Text(t)) if t.text.trim().is_empty() => self.agent_reply(id),
            Some(NodeKind::Image(i))
                if i.agent.is_some()
                    && self
                        .doc()
                        .item(i.item)
                        .and_then(|it| std::fs::read_to_string(&it.path).ok())
                        .is_none_or(|text| text.trim().is_empty()) =>
            {
                self.agent_reply(id)
            }
            _ => None,
        }
    }

    fn engines(&mut self) -> Engines {
        *self.agents.flow.engines.get_or_insert_with(Engines::probe)
    }

    /// (provider, model, label) choices for image or text agents, across every
    /// engine this machine reaches. The first is local, so no agent needs an
    /// account (Art. I.4). Catalogs are discovered off-thread as they are needed.
    pub(crate) fn agent_models(&mut self, image: bool) -> Vec<(String, String, String)> {
        let engines = self.engines();
        let openai = self.ai.packs.ok(atlas_ai::packs::OPENAI);
        if image {
            self.agents.want_catalog("comfy");
            if openai {
                self.agents.want_catalog("openai-image");
            }
            return atlas_ai::runtime::image_models(
                self.agents.catalog("comfy"),
                engines.codex,
                openai,
                self.agents.catalog("openai-image"),
            );
        }
        self.agents.want_catalog("ollama");
        if engines.codex {
            self.agents.want_catalog("codex");
        }
        if openai {
            self.agents.want_catalog("openai-text");
        }
        atlas_ai::runtime::text_models(
            self.agents.catalog("ollama"),
            engines.codex.then(|| self.agents.catalog("codex")),
            openai,
            self.agents.catalog("openai-text"),
        )
    }

    /// Opens the model list of `source`'s Agent editor, as a click would.
    #[cfg(test)]
    pub(crate) fn open_agent_model_menu_for_test(&mut self, source: NodeId) {
        let mut draft = AgentDraft::new();
        draft.model_open = true;
        draft.take_focus = false;
        self.agents.flow.drafts.insert(source, draft);
    }

    fn key_saved(&mut self) {
        self.agents.refresh_catalog("openai-image");
        self.agents.refresh_catalog("openai-text");
    }

    /// The Agent squircle's editor for `source`. Returns open popup rects.
    pub(crate) fn agent_editor_body(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        source: NodeId,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) -> Vec<Rect> {
        if self.is_artifact_node(source) {
            return self.artifact_editor_body(ui, rect, source, z, theme);
        }
        if agent_inputs::is_flow_node(&self.doc().scene, source) {
            return self.attached_editor_body(ui, rect, source, z, theme);
        }
        let mut draft = self
            .agents
            .flow
            .drafts
            .remove(&source)
            .unwrap_or_else(AgentDraft::new);
        let models = self.agent_models(draft.kind == SpawnKind::Image);
        let modes: Vec<&str> = MODALITIES.iter().map(|m| m.label).collect();
        let mode = if draft.kind == SpawnKind::Image {
            SpawnKind::Image
        } else {
            SpawnKind::Text
        }
        .index();
        let current = draft
            .choice
            .clone()
            .or_else(|| self.default_model(draft.kind == SpawnKind::Image));
        let index = model_index(&models, current.as_ref());
        let labels: Vec<&str> = models.iter().map(|m| m.2.as_str()).collect();
        let aspects: Vec<&str> = atlas_ai::agent::Aspect::ALL
            .iter()
            .map(|a| a.label())
            .collect();
        let pasting = draft.key.is_some();
        let hint = if pasting {
            "Paste your OpenAI API key, then Submit. It stays in Windows Credential Manager."
        } else if draft.kind == SpawnKind::Image {
            "Describe the picture to make, or the look to give this one"
        } else {
            "Ask for a description, a caption, or a rewrite"
        };
        let mut hidden = String::new();
        let key_masked = draft.key.as_ref().map(|k| "•".repeat(k.chars().count()));
        let prompt_text = match &key_masked {
            Some(masked) => {
                hidden.clone_from(masked);
                &mut hidden
            }
            None => &mut draft.prompt,
        };
        let out = self.paint_image_agent_editor(
            ui,
            rect,
            source,
            atlas_shell::selection_tools::AgentEdit {
                modes: &modes,
                mode,
                image: draft.kind == SpawnKind::Image,
                prompt: prompt_text,
                hint,
                models: &labels,
                model: index,
                model_open: &mut draft.model_open,
                count: draft.count,
                aspects: &aspects,
                aspect: draft.aspect,
                aspect_open: &mut draft.aspect_open,
                seed_locked: draft.seed.is_some(),
                live: draft.live,
                take_focus: std::mem::take(&mut draft.take_focus),
            },
            z,
            theme,
        );
        if out.prompt.focused {
            self.web_release_keyboard();
        }
        if pasting && out.prompt.changed {
            absorb_key(&mut draft, &hidden);
        }
        if let Some(mode) = out.mode {
            draft.kind = MODALITIES
                .get(mode)
                .and_then(|m| m.kind)
                .unwrap_or(SpawnKind::Image);
            draft.choice = None;
            draft.key = None;
        }
        if let Some((provider, model, _)) = out.model.and_then(|i| models.get(i)) {
            draft.choice = Some((provider.clone(), model.clone()));
            self.remember_model(draft.kind == SpawnKind::Image, provider, model);
        }
        if let Some(count) = out.count {
            draft.count = count.clamp(1, 8);
        }
        if let Some(aspect) = out.aspect {
            draft.aspect = aspect;
        }
        if let Some(lock) = out.seed_locked {
            draft.seed = lock.then(fresh_seed);
        }
        if let Some(live) = out.live {
            draft.live = live;
        }
        if out.submit {
            let chosen = draft
                .choice
                .clone()
                .or_else(|| models.get(index).map(|m| (m.0.clone(), m.1.clone())));
            self.submit_agent_draft(ui.ctx(), source, &mut draft, chosen);
        }
        self.agents.flow.drafts.insert(source, draft);
        out.popups
    }

    fn submit_agent_draft(
        &mut self,
        ctx: &egui::Context,
        source: NodeId,
        draft: &mut AgentDraft,
        chosen: Option<(String, String)>,
    ) {
        if self.save_pasted_key(draft) {
            return;
        }
        let Some((provider, model)) = chosen else {
            return;
        };
        if provider.starts_with("openai") && !self.ai.packs.ok(atlas_ai::packs::OPENAI) {
            draft.key = Some(String::new());
            draft.take_focus = true;
            return;
        }
        self.remember_model(draft.kind == SpawnKind::Image, &provider, &model);
        if draft.prompt.trim().is_empty() {
            self.toast("Write a prompt for the agent first.");
            draft.take_focus = true;
            return;
        }
        let aspect = atlas_ai::agent::Aspect::ALL
            .get(draft.aspect)
            .copied()
            .unwrap_or_default();
        if draft.kind == SpawnKind::Image
            && self.submit_image_in_place(source, draft, &provider, &model, aspect)
        {
            return;
        }
        let detail = serde_json::json!({
            "source": source.0,
            "kind": if draft.kind == SpawnKind::Image { "image" } else { "text" },
            "prompt": draft.prompt.trim(),
            "provider": provider,
            "model": (!model.is_empty()).then_some(model),
            "image": {"count": draft.count, "aspect": aspect, "seed": draft.seed},
            "live": draft.live && draft.kind == SpawnKind::Image,
        });
        self.dispatch(
            ctx,
            atlas_commands::CommandId("portal.agent.spawn"),
            Some(detail.to_string()),
        );
        // The source stays selected, so the next variation is one Submit away.
        self.board_sel = std::iter::once(source).collect();
    }

    /// Consume a pasted key through PackSession; true even if saving fails.
    fn save_pasted_key(&mut self, draft: &mut AgentDraft) -> bool {
        let Some(key) = draft.key.take() else {
            return false;
        };
        self.ai.packs.key_draft = key;
        match self
            .ai
            .packs
            .save_openai_key(self.ai.config.workspace_dir.clone())
        {
            Ok(()) => {
                self.key_saved();
                self.toast("OpenAI API key saved for this Windows user.");
            }
            Err(error) => {
                self.toast(error);
                draft.key = Some(String::new());
            }
        }
        true
    }

    /// The squircle on a picture or note an agent makes edits that agent:
    /// model, count, aspect, seed and Live apply at once (journaled), and
    /// Submit runs it again with the prompt shown here and docked on the card.
    fn attached_editor_body(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: NodeId,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) -> Vec<Rect> {
        let Some(agent) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .cloned()
        else {
            return Vec::new();
        };
        let image = agent.view == PortalView::Images;
        let kind = if image {
            SpawnKind::Image
        } else {
            SpawnKind::Text
        };
        self.sync_instruction(id, &agent.instruction);
        let mut draft = self
            .agents
            .flow
            .drafts
            .remove(&id)
            .unwrap_or_else(AgentDraft::new);
        if image {
            let presentation = self.generator_presentation(id);
            draft.model_open = presentation.model_open;
            draft.aspect_open = presentation.aspect_open;
        }
        let models = self.agent_models(image);
        let current = (
            agent.provider.clone(),
            agent.model.clone().unwrap_or_default(),
        );
        let index = model_index(&models, Some(&current));
        let labels: Vec<&str> = models.iter().map(|m| m.2.as_str()).collect();
        let aspects: Vec<&str> = atlas_ai::agent::Aspect::ALL
            .iter()
            .map(|a| a.label())
            .collect();
        let aspect = atlas_ai::agent::Aspect::ALL
            .iter()
            .position(|a| *a == agent.image.aspect)
            .unwrap_or(0);
        let pasting = draft.key.is_some();
        let hint = if pasting {
            "Paste your OpenAI API key, then Submit. It stays in Windows Credential Manager."
        } else if image {
            "Describe the picture. Submit makes it again."
        } else {
            "Tell the model what to write. Submit writes it again."
        };
        let mut text = match &draft.key {
            Some(key) => "•".repeat(key.chars().count()),
            None => self.agents.prompt_mut(id).clone(),
        };
        let modes = [kind.modality().label];
        let out = self.paint_image_agent_editor(
            ui,
            rect,
            id,
            atlas_shell::selection_tools::AgentEdit {
                modes: &modes,
                mode: 0,
                image,
                prompt: &mut text,
                hint,
                models: &labels,
                model: index,
                model_open: &mut draft.model_open,
                count: agent.image.count.max(1),
                aspects: &aspects,
                aspect,
                aspect_open: &mut draft.aspect_open,
                seed_locked: agent.image.seed.is_some(),
                live: agent.live,
                take_focus: std::mem::take(&mut draft.take_focus),
            },
            z,
            theme,
        );
        if out.prompt.focused {
            self.web_release_keyboard();
        }
        if out.prompt.changed {
            if pasting {
                absorb_key(&mut draft, &text);
            } else {
                *self.agents.prompt_mut(id) = text;
            }
        }
        if let Some((provider, model, _)) = out.model.and_then(|i| models.get(i)) {
            if provider.starts_with("openai") && !self.ai.packs.ok(atlas_ai::packs::OPENAI) {
                draft.key = Some(String::new());
                draft.take_focus = true;
            } else {
                self.set_flow_model(id, image, provider, model);
            }
        }
        let settings = {
            let mut s = agent.image;
            if let Some(count) = out.count {
                s.count = count.clamp(1, 8);
            }
            if let Some(i) = out.aspect {
                s.aspect = atlas_ai::agent::Aspect::ALL
                    .get(i)
                    .copied()
                    .unwrap_or_default();
            }
            if let Some(lock) = out.seed_locked {
                s.seed = lock.then(fresh_seed);
            }
            s
        };
        if settings != agent.image && !self.tab().read_only {
            self.patch_nodes(&[id], |n| {
                if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                    a.image = settings;
                }
            });
        }
        if let Some(live) = out.live {
            self.set_generator_live(id, live);
        }
        if out.submit && !self.save_pasted_key(&mut draft) {
            self.commit_instruction(id);
            self.board_sel = std::iter::once(id).collect();
            if !agent.live {
                self.run_agent_media(id);
            }
        }
        self.agents.flow.drafts.insert(id, draft);
        out.popups
    }

    /// The prompt capsule is expanded, or the frame's prompt field has the caret.
    pub(crate) fn flow_capsule_open(&self, id: NodeId) -> bool {
        self.agents.flow.capsule == Some(id) || self.agents.flow.typing == Some(id)
    }

    /// What a running generator or text block is doing, how far it is and how
    /// long it has taken. Engines without step previews also get a moving bar,
    /// so a remote run never looks stalled.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint_run_progress(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        id: NodeId,
        body: Rect,
        z: f32,
        working: &str,
        previews: bool,
    ) {
        let running = self.agent_is_running(id);
        let waiting = self.generations_waiting(id);
        if !running && waiting == 0 {
            self.agents.flow.started.remove(&id);
            return;
        }
        let request = self.agents.request_of(id).unwrap_or_default().to_string();
        let started = match self.agents.flow.started.get(&id) {
            Some((shown, at)) if *shown == request => *at,
            _ => {
                let now = std::time::Instant::now();
                self.agents.flow.started.insert(id, (request.clone(), now));
                now
            }
        };
        let agent = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .cloned();
        let total = match &agent {
            Some(a) if a.view == PortalView::Images && !a.live => {
                a.image.count.clamp(1, 8) as usize
            }
            _ => 1,
        };
        let done = self
            .agents
            .session(id)
            .map(|s| {
                s.bundle
                    .images
                    .iter()
                    .filter(|i| i.request == request)
                    .count()
            })
            .unwrap_or(0);
        let secs = started.elapsed().as_secs();
        let mut text = if !running {
            "Waiting".to_string()
        } else if total > 1 {
            format!("{working} image {} of {total}", (done + 1).min(total))
        } else {
            working.to_string()
        };
        if running {
            text.push_str(&format!(" · {}:{:02}", secs / 60, secs % 60));
        }
        if waiting > 0 {
            text.push_str(&format!(" · {waiting} waiting"));
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
        let font = canvas_scale::font(12.0, z);
        if !canvas_text::legible(font.size) {
            return;
        }
        let ink = self.palette().overlay();
        let laid = canvas_text::layout_no_wrap(painter, text, font, ink.text);
        let spin = canvas_scale::px(6.0, z);
        let pad = canvas_scale::px(10.0, z);
        let size = laid.size() + egui::vec2(pad * 3.0 + spin * 2.0, pad);
        let pill = Rect::from_center_size(
            Pos2::new(
                body.center().x,
                body.top() + canvas_scale::px(16.0, z) + size.y * 0.5,
            ),
            size,
        );
        paint_overlay_pill(painter, pill, size.y * 0.5, ink.progress, &ink, z);
        let time = ui.input(|i| i.time) as f32;
        super::board_agent::paint_agent_spinner(
            painter,
            Pos2::new(pill.left() + pad + spin, pill.center().y),
            spin,
            time,
            ink.text,
        );
        laid.paint_anchored(
            painter,
            Pos2::new(pill.left() + pad * 2.0 + spin * 2.0, pill.center().y),
            Align2::LEFT_CENTER,
            ink.text,
        );
        if running && !previews {
            atlas_shell::selection_tools::generator::progress::paint_sweep(
                painter,
                body,
                z,
                time as f64,
                self.palette(),
            );
        }
    }

    /// A generator's own prompt, typed in the frame. Wired prompts are read
    /// before it; Enter journals it and runs.
    pub(crate) fn paint_generator_prompt(
        &mut self,
        ui: &egui::Ui,
        id: NodeId,
        field: Rect,
        z: f32,
        hint: &str,
    ) {
        let journaled = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.instruction.clone())
            .unwrap_or_default();
        self.sync_instruction(id, &journaled);
        let mut theme = self.palette();
        // The field sits on the overlay scrim, not on the canvas.
        let ink = theme.overlay();
        theme.ink = ink.text;
        theme.sub = ink.muted;
        let mut text = self.agents.prompt_mut(id).clone();
        let take_focus = self.take_flow_prompt_focus(id);
        let out = atlas_shell::selection_tools::prompt_field(
            ui,
            Id::new(("generator-prompt", id.0)),
            field,
            z,
            &mut text,
            hint,
            true,
            take_focus,
            theme,
        );
        self.track_flow_prompt_focus(id, out.focused);
        if out.changed {
            *self.agents.prompt_mut(id) = text;
        }
        if out.submit {
            self.agents.flow.typing = None;
            ui.memory_mut(|m| m.surrender_focus(Id::new(("generator-prompt", id.0))));
            self.commit_instruction(id);
            self.board_sel = std::iter::once(id).collect();
            self.run_agent_media(id);
        }
    }

    /// Journal the typed prompt now instead of on blur.
    fn commit_instruction(&mut self, id: NodeId) {
        let draft = self.agents.prompt_mut(id).trim().to_string();
        let journaled = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.instruction.clone())
            .unwrap_or_default();
        if draft != journaled && !self.tab().read_only {
            self.patch_nodes(&[id], |n| {
                if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                    a.instruction = draft.clone();
                }
            });
        }
        *self.agents.prompt_mut(id) = draft.clone();
        self.agents.flow.synced.insert(id, draft);
    }

    /// The prompt as a capsule in the bottom bar. Hovering expands it into an
    /// editable field above the bar; Enter runs the frame again.
    pub(crate) fn paint_prompt_capsule(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        id: NodeId,
        capsule: Rect,
        body: Rect,
        z: f32,
    ) {
        if capsule.width() < canvas_scale::px(40.0, z) {
            return;
        }
        let journaled = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.instruction.clone())
            .unwrap_or_default();
        self.sync_instruction(id, &journaled);
        let typed = self.agents.prompt_mut(id).trim().to_string();
        let wired = self.generator_view(id).prompt.clone();
        let label = [wired.as_str(), typed.as_str()]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        let px = |v: f32| canvas_scale::px(v, z);
        let expanded = Rect::from_min_max(
            Pos2::new(body.left() + px(12.0), capsule.top() - px(6.0) - px(84.0)),
            Pos2::new(body.right() - px(12.0), capsule.top() - px(6.0)),
        );
        let pointer = ui.ctx().pointer_latest_pos();
        let was_open = self.agents.flow.capsule == Some(id);
        let over = pointer.is_some_and(|p| {
            capsule.contains(p)
                || (was_open && (expanded.contains(p) || expanded.union(capsule).contains(p)))
        });
        let open = over || self.agents.flow.typing == Some(id);
        if open {
            self.agents.flow.capsule = Some(id);
        } else if was_open {
            self.agents.flow.capsule = None;
        }
        let ink = self.palette().overlay();
        paint_overlay_pill(painter, capsule, capsule.height() * 0.5, ink.fill, &ink, z);
        let font = canvas_scale::font(super::board_agent::GENERATOR_CHIP_PX, z);
        let shown = if label.is_empty() {
            "Add a prompt".to_string()
        } else {
            format!("“{label}”")
        };
        let laid = canvas_text::layout_rows(
            painter,
            shown,
            font,
            if label.is_empty() {
                ink.muted
            } else {
                ink.text
            },
            capsule.width() - px(24.0),
            1,
        );
        laid.paint_anchored(
            &painter.with_clip_rect(capsule),
            Pos2::new(capsule.left() + px(12.0), capsule.center().y),
            Align2::LEFT_CENTER,
            ink.text,
        );
        if !open {
            return;
        }
        paint_overlay_pill(painter, expanded, px(10.0), ink.raised, &ink, z);
        let hint = if wired.is_empty() {
            "Describe the picture. Enter runs it again."
        } else {
            "Add to the wired prompt. Enter runs it again."
        };
        self.paint_generator_prompt(ui, id, expanded.shrink(px(10.0)), z, hint);
    }

    /// Grow the note to its newest reply once, so reading never needs a scroll
    /// and the docked prompt never covers the words.
    fn fit_agent_note(&mut self, id: NodeId, text: &str, reply_h: f32) {
        if self.agents.flow.fitted.get(&id).map(String::as_str) == Some(text)
            || self.tab().read_only
            || self.board_drag.is_some()
        {
            return;
        }
        self.agents.flow.fitted.insert(id, text.to_string());
        let height = (reply_h + NOTE_DOCK).clamp(program_card_size(PortalView::Text).y, NOTE_MAX_H);
        if self
            .doc()
            .scene
            .node(id)
            .is_some_and(|n| n.rect.h + 1.0 < height)
        {
            self.patch_nodes(&[id], |n| n.rect.h = height);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    mod image_blister;
    mod media_chrome;
    use super::*;
    use slate_doc::agent_inputs::InputKind;

    #[test]
    fn media_overlays_follow_the_app_theme() {
        let mut h = super::super::tests::Harness::new("overlay_theme");
        h.app.dark_mode = false;
        let light = h.app.palette().overlay();
        h.app.dark_mode = true;
        let dark = h.app.palette().overlay();
        assert_ne!(light.fill, dark.fill);
        assert_ne!(light.text, dark.text);
        assert_eq!(dark.fill, Color32::from_black_alpha(175));
        assert_eq!(dark.text, Color32::WHITE);
    }

    /// A ComfyUI generator with one finished picture in its album.
    pub(crate) fn generator_with_output(tag: &str) -> (super::super::tests::Harness, NodeId) {
        let mut h = super::super::tests::Harness::new(tag);
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.ai.config.workspace_dir = Some(h.base.clone());
        h.app.place_agent_portal_at(Pos2::new(0.0, 0.0));
        let generator = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(generator, "comfy");
        let picture = h.base.join("hall.png");
        image::RgbImage::new(8, 8).save(&picture).unwrap();
        let source = picture.to_string_lossy().into_owned();
        h.app.patch_nodes(&[generator], |n| {
            slate_doc::agent_chat::agent_mut(n).unwrap().seed =
                Some(atlas_ai::agent::ImageOutput {
                    id: "img-1".into(),
                    source: source.clone(),
                    request: "r".into(),
                    prompt: "a hall".into(),
                    model: String::new(),
                    task: "generate".into(),
                    live: String::new(),
                });
        });
        (h, generator)
    }

    fn wire_into(h: &super::super::tests::Harness, target: NodeId) -> Option<(NodeId, String)> {
        h.app.doc().scene.nodes.iter().find_map(|n| match &n.kind {
            NodeKind::Connector(c) => {
                let binding = c.binding.as_ref()?;
                let end = binding.target_end(c);
                (agent_inputs::endpoint_node(end) == Some(target))
                    .then(|| (n.id, binding.slot.clone().unwrap_or_default()))
            }
            _ => None,
        })
    }

    #[test]
    fn a_dropped_output_wire_offers_text_and_the_text_block_describes_the_picture() {
        let (mut h, generator) = generator_with_output("flow_text");
        let out = (generator, Side::Right, OUTPUT_T);
        assert!(h.app.flow_wire_released(out, Pos2::new(1400.0, 200.0)));
        assert!(h.app.agents.flow.menu.as_ref().is_some_and(|m| m.dropped));
        // Releasing back on the generator is not a drop on the board.
        h.app.agents.flow.menu = None;
        assert!(h.app.flow_wire_released(out, Pos2::new(10.0, 10.0)));
        assert!(h.app.agents.flow.menu.is_none());
        // A left-edge release keeps an upstream-input menu alive.
        assert!(h
            .app
            .flow_wire_released((generator, Side::Left, 0.5), Pos2::new(1400.0, 0.0)));
        assert!(h.app.agents.flow.menu.is_some());
        h.app.agents.flow.menu = None;

        let before = h.app.doc().scene.nodes.len();
        let depth = h.app.tab().journal.undo_depth();
        let detail =
            serde_json::json!({"source": generator.0, "kind": "text", "at": [1400.0, 200.0]});
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            before + 2,
            "a block and its wire"
        );
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
        let block = *h.app.board_sel.iter().next().unwrap();
        assert!(h.app.is_text_block(block));
        let (_, slot) = wire_into(&h, block).expect("wired from the generator");
        assert_eq!(slot, "media", "the picture lands on the Image port");
        let agent = h
            .app
            .doc()
            .scene
            .node(block)
            .and_then(slate_doc::agent_chat::agent)
            .unwrap()
            .clone();
        assert_eq!(agent.provider, "ollama");
        assert_eq!(agent.instruction, DESCRIBE_STYLE);

        // The first paint seeds the draft from the journaled instruction.
        h.app.sync_instruction(block, &agent.instruction);
        h.app.queue_text_block(block);
        let (id, request) = h.app.agents.dispatched.last().unwrap().clone();
        assert_eq!(id, block);
        assert!(request.oneshot);
        assert!(request.history.is_empty());
        assert_eq!(request.prompt, DESCRIBE_STYLE);
        let pictures: Vec<_> = request
            .inputs
            .on(atlas_agent::InputSlot::Media)
            .flat_map(|i| i.images.clone())
            .collect();
        assert_eq!(pictures.len(), 1);
        assert!(pictures[0].ends_with("hall.png"));

        let tab = h.app.tab_mut();
        assert!(tab.journal.undo(&mut tab.doc.scene));
        assert_eq!(h.app.doc().scene.nodes.len(), before);
    }

    #[test]
    fn a_text_block_feeds_a_new_generator_through_its_prompt_port() {
        let (mut h, generator) = generator_with_output("flow_image");
        let detail = serde_json::json!({"source": generator.0, "kind": "text"});
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let block = *h.app.board_sel.iter().next().unwrap();
        let beside = h.app.doc().scene.node(block).unwrap().rect;
        let origin = h.app.doc().scene.node(generator).unwrap().rect;
        assert!(
            beside.x > origin.x + origin.w,
            "a click places it beside the source"
        );
        assert_eq!(
            agent_inputs::wire_kind(&h.app.doc().scene, block, &|id| {
                h.app.doc().item(id).map(|item| item.path.as_path())
            }),
            Some(InputKind::Text)
        );
        let detail = serde_json::json!({"source": block.0, "kind": "image"});
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let next = *h.app.board_sel.iter().next().unwrap();
        assert!(agent_inputs::is_image_generator(&h.app.doc().scene, next));
        let (_, slot) = wire_into(&h, next).unwrap();
        assert_eq!(slot, "prompt");
        // A generator from a generator varies its picture.
        let detail = serde_json::json!({"source": generator.0, "kind": "image"});
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let vary = *h.app.board_sel.iter().next().unwrap();
        assert_eq!(wire_into(&h, vary).unwrap().1, "media");
        assert_eq!(h.app.generator_view(vary).task().action(), "Transform");
    }

    /// Text shapes one real frame paints with the pointer at `pointer`.
    fn painted(h: &mut super::super::tests::Harness, pointer: Pos2) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
            ..Default::default()
        };
        input.events.push(egui::Event::PointerMoved(pointer));
        let ctx = h.ctx.clone();
        let app = &mut h.app;
        let output = ctx.run(input, |c| app.update_app(c));
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut texts);
        }
        texts
    }

    /// Media is the interface: the Image choice is an ordinary picture that
    /// shows its newest result until one is picked, and a pick is its file.
    #[test]
    fn the_image_choice_is_a_picture_whose_pick_is_its_file() {
        let (mut h, generator) = generator_with_output("media_pick");
        let NodeKind::Image(img) = &h.app.doc().scene.node(generator).unwrap().kind else {
            panic!("an image engine makes a picture, not a portal");
        };
        assert!(img.item.is_none() && img.agent.is_some());
        let seed = h.base.join("hall.png");
        assert_eq!(
            h.app.agent_shown_path(generator).as_deref(),
            Some(seed.as_path())
        );

        h.app.pick_agent_result(generator, 0);
        let NodeKind::Image(img) = &h.app.doc().scene.node(generator).unwrap().kind else {
            panic!("picture")
        };
        assert_eq!(
            h.app.doc().item(img.item).map(|i| i.path.clone()),
            Some(seed.clone()),
            "the picked result is the picture's linked file"
        );
        assert_eq!(h.app.agent_shown_index(generator), Some(0));
        assert_eq!(
            h.app
                .export_agent_images()
                .get(&generator)
                .and_then(|v| v.first())
                .cloned(),
            Some(seed)
        );
        // Live shows every frame, so turning it on lets the pick go.
        h.app.set_generator_live(generator, true);
        assert!(matches!(
            &h.app.doc().scene.node(generator).unwrap().kind,
            NodeKind::Image(i) if i.item.is_none()
        ));
    }

    /// An agent's picture is a raster like any other: a Brush on the selected
    /// picture draws into a paint layer while the idle image stays naked.
    #[test]
    fn a_brush_on_an_agent_picture_paints_a_layer_without_generator_overlays() {
        for picked in [false, true] {
            let (mut h, generator) = generator_with_output("agent_picture_paint");
            if picked {
                h.app.pick_agent_result(generator, 0);
            }
            h.app.board_sel = std::iter::once(generator).collect();
            h.frame();
            let rect = h.app.doc().scene.node(generator).unwrap().rect;
            let at = h.app.board_xf().rect_w2s(rect).center();
            let resting = painted(&mut h, at);
            let shows_generator_overlay = |texts: &[String]| {
                texts.iter().any(|text| {
                    text.starts_with("Prompt ·")
                        || text.starts_with("Geometry ·")
                        || matches!(
                            text.as_str(),
                            "Render" | "Generate" | "Keep" | "Stop" | "ComfyUI · Auto"
                        )
                })
            };
            assert!(
                !shows_generator_overlay(&resting),
                "picked {picked}: idle image keeps generator controls in its Agent blister: {resting:?}"
            );
            h.app.set_board_tool(super::super::board::BoardTool::Brush);
            h.app.sync_image_paint_for_tool();
            assert!(
                h.app
                    .image_paint_session()
                    .is_some_and(|s| s.image == generator),
                "picked {picked}: an agent picture hosts paint layers"
            );
            assert!(h.app.agent_picture_draw_mode(generator));
            let drawing = painted(&mut h, at);
            assert!(
                !shows_generator_overlay(&drawing),
                "picked {picked}: drawing keeps generator controls hidden: {drawing:?}"
            );
            let nodes = h.app.doc().scene.nodes.len();
            let (cx, cy) = rect.center();
            h.app.finish_freehand_brush(vec![
                Pos2::new(cx - rect.w * 0.25, cy),
                Pos2::new(cx + rect.w * 0.25, cy),
            ]);
            assert_eq!(
                h.app.doc().scene.nodes.len(),
                nodes,
                "the stroke is not a node"
            );
            let NodeKind::Image(img) = &h.app.doc().scene.node(generator).unwrap().kind else {
                panic!("picture");
            };
            let result = h.app.image_result_id(generator).unwrap();
            let layers = slate_doc::image_paint::layers_for_result(img, &result);
            assert_eq!(layers.len(), 1, "picked {picked}");
            assert_eq!(layers[0].nodes.len(), 1, "picked {picked}");
        }
    }

    /// Cover flow belongs to a selected picture with several results; hover
    /// never flows. Copying it copies only the picture it shows, as a plain
    /// placed image.
    #[test]
    fn a_selected_picture_flows_and_copies_as_its_shown_image() {
        let (mut h, generator) = generator_with_output("media_flow");
        let second = h.base.join("hall-2.png");
        image::RgbImage::new(8, 8).save(&second).unwrap();
        let session = atlas_ai::agent::AgentSession {
            usage: None,
            approval: None,
            conversation: String::new(),
            artifacts: vec![],
            status: atlas_ai::agent::AgentStatus::Idle,
            provider: "comfy".into(),
            turns: vec![],
            activity: vec![],
            updated_at: 0,
            bundle: atlas_ai::agent::ImageBundle {
                images: vec![atlas_ai::agent::ImageOutput {
                    id: "img-2".into(),
                    source: second.to_string_lossy().into_owned(),
                    request: "r2".into(),
                    ..Default::default()
                }],
            },
            request: String::new(),
        };
        let session = std::sync::Arc::new(session);
        h.app.agents.sessions.insert(generator, session.clone());
        h.app.patch_nodes(&[generator], |n| n.rect.x += 1.0);
        assert_eq!(h.app.agent_images(generator).len(), 2);
        assert_eq!(
            h.app.agent_shown_path(generator).as_deref(),
            Some(second.as_path())
        );

        h.app.board_sel.clear();
        assert_eq!(
            h.app.image_wheel(generator),
            super::super::board_agent::ImageWheel::Board
        );
        h.app.board_sel.insert(generator);
        assert_eq!(
            h.app.image_wheel(generator),
            super::super::board_agent::ImageWheel::Album
        );
        // Flowing hides the labels until the picture is clicked again.
        h.app.agents.flow.set_hud_hidden(generator, true);
        assert!(h.app.agents.flow.hud_hidden(generator));
        h.app.board_sel.clear();
        h.frame();
        assert!(
            !h.app.agents.flow.hud_hidden(generator),
            "deselecting shows it again"
        );

        // The frame's link poll replaces the stand-in session; restore it.
        h.app.agents.sessions.insert(generator, session);
        h.app.patch_nodes(&[generator], |n| n.rect.x += 1.0);
        h.app.board_sel = std::iter::once(generator).collect();
        assert_eq!(h.app.board_copy(&h.ctx), 1);
        let NodeKind::Image(copied) = &h.app.board_clipboard[0].kind else {
            panic!("a picture");
        };
        assert!(copied.agent.is_none(), "a plain picture");
        assert_eq!(
            h.app.doc().item(copied.item).map(|i| i.path.clone()),
            Some(second),
            "the shown (newest) result only"
        );
        assert!(
            matches!(
                &h.app.doc().scene.node(generator).unwrap().kind,
                NodeKind::Image(i) if i.agent.is_some()
            ),
            "the source keeps its agent"
        );
    }

    /// Text language model spawns a linked text document window wired as output.
    #[test]
    fn the_text_choice_is_a_note_that_shows_its_reply_until_edited() {
        let (mut h, generator) = generator_with_output("media_note");
        h.app.ai.config.workspace_dir = Some(h.base.join("ai-ws"));
        std::fs::create_dir_all(h.app.ai.config.workspace_dir.as_ref().unwrap()).unwrap();
        let detail = serde_json::json!({"source": generator.0, "kind": "text"});
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        for _ in 0..20 {
            h.app.pump_agent_text_outputs();
        }
        let note = *h.app.board_sel.iter().next().unwrap();
        assert!(matches!(
            &h.app.doc().scene.node(note).unwrap().kind,
            NodeKind::Image(i) if i.agent.is_some()
        ));
        let path = h
            .app
            .doc()
            .item(match &h.app.doc().scene.node(note).unwrap().kind {
                NodeKind::Image(i) => i.item,
                _ => panic!("text window"),
            })
            .unwrap()
            .path
            .clone();
        assert_eq!(slate_doc::media_kind(&path), slate_doc::MediaKind::Text);
        let session = atlas_ai::agent::AgentSession {
            usage: None,
            approval: None,
            conversation: String::new(),
            artifacts: vec![],
            status: atlas_ai::agent::AgentStatus::Idle,
            provider: "ollama".into(),
            activity: vec![],
            turns: vec![atlas_ai::agent::AgentTurn {
                role: "assistant".into(),
                text: "Warm light on stone.".into(),
                at: 0,
            }],
            updated_at: 0,
            bundle: Default::default(),
            request: String::new(),
        };
        h.app
            .agents
            .sessions
            .insert(note, std::sync::Arc::new(session));
        assert_eq!(
            h.app.agent_note_reply(note).as_deref(),
            Some("Warm light on stone.")
        );
        h.app.sync_agent_text_output(note);
        for _ in 0..200 {
            h.app.pump_agent_text_outputs();
            if std::fs::read_to_string(&path)
                .ok()
                .is_some_and(|text| !text.is_empty())
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(on_disk.trim(), "Warm light on stone.");
    }

    /// A wire dropped on empty board from any media with the Agent squircle
    /// opens the modality menu, and the new wire leaves from that grip.
    #[test]
    fn a_wire_dropped_from_a_picture_offers_an_agent() {
        let (mut h, _) = generator_with_output("media_drop");
        let photo = place(&mut h, "court.png", -600.0);
        let grip = (photo, Side::Top, 0.5);
        assert!(h.app.flow_wire_released(grip, Pos2::new(-500.0, 200.0)));
        let menu = h.app.agents.flow.menu.as_ref().unwrap();
        assert!(menu.dropped && menu.grip == Some((Side::Top, 0.5)));
        let detail = serde_json::json!({
            "source": photo.0, "kind": "image", "at": [-500.0, 200.0], "from": menu.grip,
        });
        h.app.agents.flow.menu = None;
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let made = *h.app.board_sel.iter().next().unwrap();
        let (wire, slot) = wire_into(&h, made).unwrap();
        assert_eq!(slot, "media");
        let NodeKind::Connector(c) = &h.app.doc().scene.node(wire).unwrap().kind else {
            panic!("wire")
        };
        let from = if c.binding.as_ref().unwrap().input_b {
            &c.a
        } else {
            &c.b
        };
        assert!(matches!(
            from,
            ConnectorEnd::Anchored {
                side: Side::Top,
                ..
            }
        ));
        assert_eq!(MODALITIES.len(), 2, "Text and Image today");
    }

    /// A placed file on the board, like a drop.
    pub(crate) fn place(h: &mut super::super::tests::Harness, name: &str, x: f32) -> NodeId {
        let path = h.base.join(name);
        if name.ends_with(".png") {
            image::RgbImage::new(8, 8).save(&path).unwrap();
        } else {
            std::fs::write(&path, "a courtyard in morning light").unwrap();
        }
        let item = h.app.doc_mut().add_item(path, name, 10, 0, "k");
        let node = h.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(x, 800.0, 240.0, 160.0),
            NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
        );
        let id = node.id;
        h.app.add_nodes(vec![node]);
        id
    }

    #[test]
    fn submitting_from_a_picture_spawns_a_wired_generator_with_its_settings_and_runs_it() {
        let (mut h, generator) = generator_with_output("media_agent");
        let picture = place(&mut h, "site.png", -900.0);
        assert!(h.app.is_agent_media(picture));
        assert!(
            h.app.is_agent_media(generator),
            "a generated frame is media too"
        );
        let before = h.app.doc().scene.nodes.len();
        let depth = h.app.tab().journal.undo_depth();
        let detail = serde_json::json!({
            "source": picture.0, "kind": "image", "prompt": "a watercolor of this site",
            "provider": "comfy", "model": null,
            "image": {"count": 3, "aspect": "wide", "seed": 42}, "live": false,
        });
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        assert_eq!(h.app.doc().scene.nodes.len(), before + 2);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
        let spawned = h.app.doc().scene.nodes[before].id;
        assert!(agent_inputs::is_image_generator(
            &h.app.doc().scene,
            spawned
        ));
        assert_eq!(wire_into(&h, spawned).unwrap().1, "media");
        let agent = h
            .app
            .doc()
            .scene
            .node(spawned)
            .and_then(slate_doc::agent_chat::agent)
            .unwrap()
            .clone();
        assert_eq!(agent.instruction, "a watercolor of this site");
        assert_eq!(agent.image.count, 3);
        assert_eq!(agent.image.aspect, atlas_ai::agent::Aspect::Wide);
        assert_eq!(agent.image.seed, Some(42));
        let (id, request) = h.app.agents.dispatched.last().unwrap().clone();
        assert_eq!(id, spawned, "a submitted agent runs at once");
        assert_eq!(request.prompt, "a watercolor of this site");
        let params = request.image.unwrap();
        assert_eq!(
            (params.count, params.aspect, params.seed),
            (3, atlas_ai::agent::Aspect::Wide, Some(42))
        );
        assert!(request
            .inputs
            .on(atlas_agent::InputSlot::Media)
            .any(|i| i.images[0].ends_with("site.png")));

        // ChatGPT through the Codex sign-in is the same frame on another engine.
        let detail = serde_json::json!({
            "source": picture.0, "kind": "image", "prompt": "night version",
            "provider": "codex-image",
        });
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let chatgpt = h.app.doc().scene.nodes[before + 2].id;
        let provider = h
            .app
            .doc()
            .scene
            .node(chatgpt)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.provider.clone());
        assert_eq!(provider.as_deref(), Some("codex-image"));
        assert_eq!(h.app.agents.dispatched.last().unwrap().0, chatgpt);
    }

    #[test]
    fn a_text_document_drives_a_text_agent_through_its_words() {
        let (mut h, _) = generator_with_output("text_agent");
        let brief = place(&mut h, "brief.md", -900.0);
        let NodeKind::Image(image) = &h.app.doc().scene.node(brief).unwrap().kind else {
            unreachable!()
        };
        let item = image.item;
        h.app
            .complete_document_preview_for_test(item, "a courtyard in morning light");
        assert!(h.app.is_agent_media(brief));
        let detail = serde_json::json!({
            "source": brief.0, "kind": "text", "prompt": "Rewrite this as a one-line caption",
        });
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let block = *h.app.board_sel.iter().next().unwrap();
        assert!(h.app.is_text_block(block));
        assert_eq!(
            wire_into(&h, block).unwrap().1,
            "prompt",
            "words land on Prompt"
        );
        let (id, request) = h.app.agents.dispatched.last().unwrap().clone();
        assert_eq!(id, block);
        assert!(request.oneshot);
        assert!(
            request.prompt.starts_with("a courtyard in morning light"),
            "the document's words come first: {}",
            request.prompt
        );
        assert!(request
            .prompt
            .ends_with("Rewrite this as a one-line caption"));
    }

    #[test]
    fn a_selected_picture_offers_the_agent_and_its_editor_paints_the_choices() {
        let (mut h, _) = generator_with_output("agent_editor");
        let picture = place(&mut h, "site.png", -900.0);
        h.app.agent_blur();
        h.app.board_sel = std::iter::once(picture).collect();
        let r = h.app.doc().scene.node(picture).unwrap().rect;
        h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
        h.app.tab_mut().cam.z = 1.0;
        h.frame();
        h.frame();
        h.app.shape_properties.panel = Some(super::super::board_properties::Panel::Agent);
        crate::app::tests::pause(&mut h); // Reveal the lower prompt face before inspecting its hint.
                                          // egui sizes a new area on its first frame without painting it.
        painted(&mut h, Pos2::new(2.0, 890.0));
        let shown = painted(&mut h, Pos2::new(2.0, 890.0));
        // The image blister's bare menus replace generic Text/Image/Submit capsules.
        for label in ["ComfyUI · Auto", "Source", "Single", "Settings"] {
            assert!(
                shown.iter().any(|t| t == label),
                "missing {label}: {shown:?}"
            );
        }
        assert!(
            shown.iter().any(|t| t.starts_with("Describe the picture")),
            "the prompt face shows its hint: {shown:?}"
        );
    }

    fn wheel_at(h: &mut super::super::tests::Harness, at: Pos2) {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
            ..Default::default()
        };
        input.events.push(egui::Event::PointerMoved(at));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -80.0),
            modifiers: egui::Modifiers::default(),
        });
        let ctx = h.ctx.clone();
        let app = &mut h.app;
        let _ = ctx.run(input, |c| app.update_app(c));
    }

    #[test]
    fn an_open_model_list_keeps_the_wheel_and_the_picture_has_no_model_chip() {
        let (mut h, generator) = generator_with_output("menu_wheel");
        h.app.agent_blur();
        h.app.board_sel.clear();
        h.frame();
        h.frame();
        let empty = Pos2::new(1300.0, 700.0);
        let before = h.app.tab().cam.z;
        wheel_at(&mut h, empty);
        assert_ne!(h.app.tab().cam.z, before, "the empty board zooms");
        let before = h.app.tab().cam.z;
        let list = Rect::from_center_size(empty, egui::vec2(200.0, 240.0));
        atlas_shell::menu_wheel::claim(&h.ctx, list);
        wheel_at(&mut h, empty);
        assert_eq!(h.app.tab().cam.z, before, "an open list scrolls instead");

        let r = h.app.doc().scene.node(generator).unwrap().rect;
        let over = h.app.board_xf().rect_w2s(r).center();
        painted(&mut h, over);
        let shown = painted(&mut h, over);
        assert!(
            !shown.iter().any(|t| t == "ComfyUI · Auto") && !shown.iter().any(|t| t == "Generate"),
            "an idle image is naked; model controls belong to its blister: {shown:?}"
        );
    }

    #[test]
    fn a_chosen_model_becomes_the_default_for_new_frames_from_any_spawn() {
        let (mut h, generator) = generator_with_output("model_default");
        let models = h.app.agent_models(true);
        assert!(models.iter().any(|m| m.0 == "comfy"));
        assert!(models.iter().any(|m| m.0 == "openai-image"), "{models:?}");
        let texts = h.app.agent_models(false);
        assert!(texts.iter().any(|m| m.0 == "openai-text"), "{texts:?}");

        // Choosing on a frame's chip switches it and becomes the default.
        h.app
            .set_flow_model(generator, true, "openai-image", "gpt-image-2.5-flare");
        let agent = |h: &super::super::tests::Harness, id| {
            h.app
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .cloned()
                .unwrap()
        };
        assert_eq!(agent(&h, generator).provider, "openai-image");
        assert_eq!(
            h.app.default_model(true),
            Some(("openai-image".into(), "gpt-image-2.5-flare".into()))
        );
        // A wire dropped on the board spawns with that default.
        let detail = serde_json::json!({"source": generator.0, "kind": "image"});
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let next = *h.app.board_sel.iter().next().unwrap();
        assert_eq!(agent(&h, next).provider, "openai-image");
        assert_eq!(
            agent(&h, next).model.as_deref(),
            Some("gpt-image-2.5-flare")
        );

        h.app.remember_model(false, "openai-text", "gpt-5.5");
        let detail = serde_json::json!({"source": generator.0, "kind": "text"});
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let block = *h.app.board_sel.iter().next().unwrap();
        assert!(h.app.is_text_block(block));
        assert_eq!(agent(&h, block).provider, "openai-text");
        assert_eq!(agent(&h, block).model.as_deref(), Some("gpt-5.5"));
    }

    #[test]
    fn a_prompt_typed_in_the_frame_is_journaled_and_runs() {
        let (mut h, generator) = generator_with_output("typed_prompt");
        *h.app.agents.prompt_mut(generator) = "a red barn at dusk".into();
        h.app.commit_instruction(generator);
        let instruction = h
            .app
            .doc()
            .scene
            .node(generator)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.instruction.clone());
        assert_eq!(instruction.as_deref(), Some("a red barn at dusk"));
        h.app.queue_generation(generator);
        let (id, request) = h.app.agents.dispatched.last().unwrap().clone();
        assert_eq!(id, generator);
        assert_eq!(request.prompt, "a red barn at dusk");
    }

    #[test]
    fn a_web_page_feeds_its_picture_to_both_agents() {
        let (mut h, _) = generator_with_output("web_agent");
        let page = h.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(-1400.0, 0.0, 800.0, 500.0),
            NodeKind::Portal(PortalNode::bound_web("Map", "https://maps.example.com/")),
        );
        let page = {
            let id = page.id;
            h.app.add_nodes(vec![page]);
            id
        };
        assert!(h.app.is_agent_media(page));
        let detail = serde_json::json!({
            "source": page.0, "kind": "image", "prompt": "remove every road marking",
            "provider": "comfy",
        });
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let generator = *h.app.board_sel.iter().next().unwrap();
        let (wire, slot) = wire_into(&h, generator).unwrap();
        assert_eq!(slot, "media", "the page lands on the picture port");
        let NodeKind::Connector(c) = &h.app.doc().scene.node(wire).unwrap().kind else {
            panic!("wire")
        };
        assert_eq!(c.binding.as_ref().unwrap().kind, InputKind::Images);
        let view = h.app.generator_view(generator);
        assert_eq!(view.inputs[0].label, "Web page");
        assert_eq!(view.task().action(), "Transform");
        // Headless there is no WebView2 to capture from, and the run says so.
        assert!(h
            .app
            .agent_failure_reason(generator)
            .is_some_and(|r| r.contains("WebView2")));

        // The text agent reads the page's visible text, once, on Submit.
        #[derive(Default)]
        struct Page {
            asked: bool,
        }
        impl super::super::board_web::WebHost for Page {
            fn available(&self) -> bool {
                true
            }
            fn admit(&mut self, _: NodeId, _: &super::super::board_web::WebRequest) {}
            fn evict(&mut self, _: NodeId) {}
            fn take_frame(&mut self, _: NodeId) -> Option<crate::app::board_web::WebFrame> {
                None
            }
            fn capture_poster(&mut self, _: NodeId) -> Option<crate::app::board_web::WebFrame> {
                None
            }
            fn send_input(&mut self, _: NodeId, _: super::super::board_web::WebInput) {}
            fn cursor(&self, _: NodeId) -> Option<egui::CursorIcon> {
                None
            }
            fn load_error(&self, _: NodeId) -> Option<String> {
                None
            }
            fn request_text(&mut self, _: NodeId) -> bool {
                self.asked = true;
                true
            }
            fn take_text(&mut self, _: NodeId) -> Option<Result<String, String>> {
                std::mem::take(&mut self.asked)
                    .then(|| Ok("Penn Station is a railway station in Manhattan.".into()))
            }
        }
        h.app.web.set_host(Box::new(Page::default()));
        let detail = serde_json::json!({
            "source": page.0, "kind": "text", "prompt": "Summarize this page",
        });
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let block = *h.app.board_sel.iter().next().unwrap();
        assert_eq!(
            wire_into(&h, block).unwrap().1,
            "prompt",
            "words land on Prompt"
        );
        // The first pump asks the page; the next one has its text.
        h.app.pump_comfy_queue();
        let (id, request) = h.app.agents.dispatched.last().unwrap().clone();
        assert_eq!(id, block);
        assert!(request.oneshot);
        assert_eq!(
            request.prompt,
            "Penn Station is a railway station in Manhattan.\n\nSummarize this page",
            "the page's text, not its address"
        );
    }

    #[test]
    fn chat_cards_and_wires_are_not_media() {
        let mut h = super::super::tests::Harness::new("not_media");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::new(0.0, 0.0));
        let chat = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(chat, "codex");
        assert!(!h.app.is_agent_media(chat));
        let picture = place(&mut h, "a.png", 600.0);
        let wire = h.app.add_connector(
            ConnectorEnd::Anchored {
                node: picture,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Free {
                point: [2000.0, 0.0],
            },
        );
        assert!(!h.app.is_agent_media(wire.unwrap()));
    }

    #[test]
    fn a_picture_on_the_style_port_guides_without_becoming_the_source() {
        let (mut h, generator) = generator_with_output("flow_style");
        let detail = serde_json::json!({"source": generator.0, "kind": "image"});
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let styled = *h.app.board_sel.iter().next().unwrap();
        // Move that wire from Media to Style, as a drag onto the lower port would.
        let (wire, _) = wire_into(&h, styled).unwrap();
        let style_t = agent_inputs::input_ports(&h.app.doc().scene, styled)[2].t;
        let before = h.app.doc().scene.node(wire).unwrap().clone();
        let mut after = before.clone();
        if let NodeKind::Connector(c) = &mut after.kind {
            let end = if c.binding.as_ref().unwrap().input_b {
                &mut c.b
            } else {
                &mut c.a
            };
            *end = ConnectorEnd::Anchored {
                node: styled,
                side: Side::Left,
                t: style_t,
            };
            let NodeKind::Connector(old) = &before.kind else {
                unreachable!()
            };
            c.binding = agent_inputs::rebind(&h.app.doc().scene, old, c);
        }
        assert!(h.app.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]));
        assert_eq!(wire_into(&h, styled).unwrap().1, "style");
        let view = h.app.generator_view(styled);
        assert_eq!(view.inputs.len(), 1);
        assert_eq!(view.inputs[0].role, InputRole::Style);
        assert_eq!(view.task().action(), "Generate", "style alone never varies");
    }
}
