//! Dataflow on image generators and text blocks, after Flora and xFigura:
//! typed input ports down the left edge, one output port whose click (or a
//! wire dropped on empty board) offers Text or Image, and the text block card
//! that runs a local language model once per press.
//!
//! Port positions come from `WireHost::ports`, which reads the table in
//! `slate_doc::agent_inputs`. Runs go through the generator queue. See
//! `docs/keymap/contracts/portal-agent-link.md` (Flow ports, 23 September 2026).

use std::collections::HashMap;

use atlas_ai::agent::{AgentRequest, PortalView};
use atlas_shell::{canvas_scale, canvas_text};
use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect};
use slate_doc::agent_inputs::{self, InputKind, OUTPUT_T};
use slate_doc::scene::{ConnectorEnd, NodeId, NodeKind, PortalNode, SceneCmd, Side};
use slate_doc::WireHost;

use super::board::{BoardDrag, BoardXf};
use super::board_agent::{bind_program, program_card_size, InputRole};
use super::SlateApp;

/// Designed width of a picture an agent generates.
const AGENT_PICTURE_W: f32 = 480.0;
/// Designed port radius; it scales with the board like the card does.
const PORT_RADIUS: f32 = 5.0;
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

/// Where `choice` sits in a model list: the exact entry, else that provider's
/// default, else the first (local) entry.
fn model_index(models: &[(String, String, String)], choice: Option<&(String, String)>) -> usize {
    let Some((provider, model)) = choice else {
        return 0;
    };
    models
        .iter()
        .position(|m| &m.0 == provider && &m.1 == model)
        .or_else(|| {
            models
                .iter()
                .position(|m| &m.0 == provider && m.1.is_empty())
        })
        .or_else(|| models.iter().position(|m| &m.0 == provider))
        .unwrap_or(0)
}

/// The key field shows one bullet per key character: the bullets left are
/// the characters kept, anything else was typed or pasted.
fn absorb_key(draft: &mut AgentDraft, shown: &str) {
    let kept = shown.chars().filter(|c| *c == '•').count();
    let typed: String = shown.chars().filter(|c| *c != '•').collect();
    let key = draft.key.get_or_insert_with(String::new);
    *key = key.chars().take(kept).chain(typed.chars()).collect();
}

/// A seed to lock, from a fresh request id.
fn fresh_seed() -> u64 {
    let id = atlas_ai::agent::request_id();
    id.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    }) >> 1
}

/// The instruction a text block spawned from a picture starts with.
const DESCRIBE_STYLE: &str = "Describe the visual style of this image as a comma-separated image prompt of 15 to 30 words: medium, palette, lighting, texture and mood. Leave out the subject.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpawnKind {
    Text,
    Image,
}

/// One thing an agent can make from media. The wire-drop menu and the agent
/// editor's mode switch both read [`MODALITIES`]; 3D and video join it when
/// an engine makes them.
pub(crate) struct Modality {
    pub kind: SpawnKind,
    pub id: &'static str,
    pub label: &'static str,
    pub menu: &'static str,
    pub icon: atlas_shell::menu::MenuIcon,
}

pub(crate) const MODALITIES: [Modality; 2] = [
    Modality {
        kind: SpawnKind::Text,
        id: "text",
        label: "Text",
        menu: "Text · language model",
        icon: atlas_shell::menu::MenuIcon::Chat,
    },
    Modality {
        kind: SpawnKind::Image,
        id: "image",
        label: "Image",
        menu: "Image · generator",
        icon: atlas_shell::menu::MenuIcon::Image,
    },
];

impl SpawnKind {
    fn modality(self) -> &'static Modality {
        MODALITIES.iter().find(|m| m.kind == self).unwrap()
    }

    fn id(self) -> &'static str {
        self.modality().id
    }

    fn from_id(id: &str) -> Option<Self> {
        MODALITIES.iter().find(|m| m.id == id).map(|m| m.kind)
    }

    fn index(self) -> usize {
        MODALITIES.iter().position(|m| m.kind == self).unwrap()
    }
}

struct FlowMenu {
    source: NodeId,
    /// World point the menu opened at: the port for a click, the release
    /// point for a dropped wire.
    at: Pos2,
    dropped: bool,
    /// The grip a dropped wire left from; the new wire keeps it.
    grip: Option<(Side, f32)>,
    armed: bool,
}

/// The agent editor's unsent choices for one source. Submitting journals them
/// on the new node; until then they are view state.
struct AgentDraft {
    image: bool,
    prompt: String,
    /// (provider, model) picked in this editor; `None` follows the default.
    choice: Option<(String, String)>,
    model_open: bool,
    count: u8,
    aspect: usize,
    aspect_open: bool,
    seed: Option<u64>,
    live: bool,
    take_focus: bool,
    /// An OpenAI key being pasted, shown masked.
    key: Option<String>,
}

impl AgentDraft {
    fn new() -> Self {
        Self {
            image: true,
            prompt: String::new(),
            choice: None,
            model_open: false,
            count: 1,
            aspect: 0,
            aspect_open: false,
            seed: None,
            live: false,
            take_focus: true,
            key: None,
        }
    }
}

/// Which remote engines this machine can reach. Read once, never per frame;
/// saving a key refreshes it.
#[derive(Clone, Copy)]
struct Engines {
    codex: bool,
    openai: bool,
}

impl Engines {
    fn probe() -> Self {
        Self {
            codex: atlas_ai::runtime::codex_executable().is_some(),
            openai: atlas_core::secrets::health(atlas_ai::runtime::OPENAI_KEY_SLOT)
                == atlas_core::secrets::SecretHealth::Ok,
        }
    }
}

/// Derived view state; nothing here is journaled.
#[derive(Default)]
pub(crate) struct FlowUi {
    menu: Option<FlowMenu>,
    press: Option<(NodeId, Pos2)>,
    drafts: HashMap<NodeId, AgentDraft>,
    engines: Option<Engines>,
    /// The prompt capsule of this frame is expanded for editing.
    capsule: Option<NodeId>,
    /// A generator's own prompt field has the caret.
    typing: Option<NodeId>,
    /// A freshly spawned agent's prompt takes the caret on its first paint.
    focus_prompt: Option<NodeId>,
    /// Pictures being cover-flowed: their labels, dock and ports stay hidden
    /// until the picture is clicked again.
    hud_hidden: std::collections::HashSet<NodeId>,
    /// When the shown run of a frame started: (request, instant).
    started: HashMap<NodeId, (String, std::time::Instant)>,
    /// The instruction each text block last had in the document, so an undo
    /// or a reload replaces the draft instead of being overwritten by it.
    synced: HashMap<NodeId, String>,
    /// Reply a text block was last fitted to.
    fitted: HashMap<NodeId, String>,
}

impl FlowUi {
    pub(crate) fn typing(&self, id: NodeId) -> bool {
        self.typing == Some(id)
    }

    pub(crate) fn hud_hidden(&self, id: NodeId) -> bool {
        self.hud_hidden.contains(&id)
    }

    pub(crate) fn set_hud_hidden(&mut self, id: NodeId, hidden: bool) {
        if hidden {
            self.hud_hidden.insert(id);
        } else {
            self.hud_hidden.remove(&id);
        }
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
                xf.w2s(menu.at) + egui::vec2(10.0, -12.0),
                dark,
                240.0,
                Some(&mut menu.armed),
                |ui| {
                    atlas_shell::menu::heading(ui, "Make with an agent", dark);
                    let mut chosen = None;
                    for m in &MODALITIES {
                        if atlas_shell::menu::item(ui, m.icon, m.menu, dark).clicked() {
                            chosen = Some(m.kind);
                        }
                    }
                    chosen
                },
            );
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if let Some(kind) = shown.inner {
                let detail = serde_json::json!({
                    "source": menu.source.0,
                    "kind": kind.id(),
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
                        let at = WireHost::from_node(node).anchor(Side::Right, OUTPUT_T);
                        self.open_flow_menu(id, Pos2::new(at[0], at[1]), false, None);
                    }
                }
            }
        }
        captured
    }

    /// A wire from media an agent can start from, released away from every
    /// node, spawns an agent there: the modality menu opens at the release
    /// point. A generator or note offers it from its output only, since its
    /// left edge holds inputs. False when `from` offers no agent.
    pub(crate) fn flow_wire_released(&mut self, from: (NodeId, Side, f32), cursor: Pos2) -> bool {
        let offers = if agent_inputs::is_flow_node(&self.doc().scene, from.0) {
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

    /// `portal.agent.spawn`: a text block or generator wired from the source,
    /// as one journal step. The output-port menu sends only `source`, `kind`
    /// and `at`; the agent editor also sends the prompt and settings, and the
    /// new node runs at once.
    pub(crate) fn spawn_flow_node(&mut self, detail: Option<&str>) -> bool {
        #[derive(serde::Deserialize)]
        struct Spawn {
            source: u64,
            kind: String,
            #[serde(default)]
            at: Option<[f32; 2]>,
            #[serde(default)]
            prompt: String,
            /// Engine for an image agent (`comfy`, `codex-image`, `openai-image`).
            #[serde(default)]
            provider: Option<String>,
            #[serde(default)]
            model: Option<String>,
            #[serde(default)]
            image: slate_doc::scene::ImageSettings,
            #[serde(default)]
            live: bool,
            /// The source grip a dropped wire left from.
            #[serde(default)]
            from: Option<(Side, f32)>,
        }
        let Some(spawn) = detail.and_then(|d| serde_json::from_str::<Spawn>(d).ok()) else {
            return false;
        };
        let source = NodeId(spawn.source);
        let Some(kind) = SpawnKind::from_id(&spawn.kind) else {
            return false;
        };
        if !self.is_agent_media(source) {
            return false;
        }
        let doc = self.doc();
        let Some(carries) = agent_inputs::wire_kind(&doc.scene, source, &|id| {
            doc.item(id).map(|item| item.path.as_path())
        }) else {
            return false;
        };
        // Without an explicit choice the new frame takes the model the person
        // last chose for this kind of agent.
        let (provider, model) = match spawn.provider.clone() {
            Some(provider) => (Some(provider), spawn.model.clone()),
            None => match self.default_model(kind == SpawnKind::Image) {
                Some((provider, model)) => (Some(provider), Some(model)),
                None => (None, None),
            },
        };
        let program = match kind {
            SpawnKind::Image => atlas_ai::agent::provider_by_id(
                provider
                    .as_deref()
                    .filter(|p| atlas_ai::agent::local_image_engine(p))
                    .unwrap_or("comfy"),
            ),
            SpawnKind::Text => atlas_ai::agent::AgentProvider {
                view: PortalView::Text,
                display_name: "Text".into(),
                ..atlas_ai::agent::provider_by_id(
                    provider
                        .as_deref()
                        .filter(|p| TEXT_ENGINES.contains(p))
                        .unwrap_or("ollama"),
                )
            },
        };
        // The agent makes media: a picture shaped like the one it will make,
        // or a sticky it writes into.
        let size = match kind {
            SpawnKind::Image => {
                let source_ratio = self
                    .doc()
                    .scene
                    .node(source)
                    .map(|n| n.rect.w / n.rect.h.max(1.0))
                    .filter(|r| r.is_finite() && *r > 0.0);
                let ratio = spawn
                    .image
                    .aspect
                    .ratio()
                    .or(source_ratio)
                    .unwrap_or(1.0)
                    .clamp(0.25, 4.0);
                egui::vec2(AGENT_PICTURE_W, AGENT_PICTURE_W / ratio)
            }
            SpawnKind::Text => program_card_size(program.view),
        };
        let binding = self.program_binding(None);
        let mut node = self.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(0.0, 0.0, size.x, size.y),
            NodeKind::Portal(PortalNode::unbound_agent("", program.id.clone())),
        );
        bind_program(&mut node, &program, &binding);
        node.rect.w = size.x;
        node.rect.h = size.y;
        // A web page gives the image agent its picture and the text agent its
        // visible text (read as the run starts).
        let doc = self.doc();
        let reads_text = if agent_inputs::is_web_page(&doc.scene, source) {
            kind == SpawnKind::Text
        } else {
            !agent_inputs::feeds_picture(&doc.scene, source, &|id| {
                doc.item(id).map(|item| item.path.as_path())
            })
        };
        let Some(port) = agent_inputs::input_ports_of(&node)
            .iter()
            .find(|p| p.slot.takes_text() == reads_text)
            .copied()
        else {
            self.toast("This node has no input for that output.");
            return false;
        };
        // A dropped wire's end becomes the new node's port.
        let drop = spawn.at.map(|[x, y]| Pos2::new(x, y - port.t * size.y));
        let zoom = self.tab().cam.z;
        let Some(rect) = self.agent_spawn_rect(source, drop, size, zoom) else {
            return false;
        };
        node.rect = rect;
        let prompt = spawn.prompt.trim().to_string();
        let run = !prompt.is_empty();
        if let Some(a) = slate_doc::agent_chat::agent_mut(&mut node) {
            a.instruction = if run {
                prompt
            } else if kind == SpawnKind::Text && carries == InputKind::Images {
                DESCRIBE_STYLE.into()
            } else {
                String::new()
            };
            a.model = model.clone().filter(|m| !m.is_empty());
            a.image = spawn.image;
            a.live = spawn.live && kind == SpawnKind::Image;
        }
        let id = node.id;
        // The wire binds against the planned node; one journal step adds both.
        let plan = SceneCmd::Add {
            index: self.doc().scene.nodes.len(),
            node: node.clone(),
        };
        if !self.doc_mut().scene.apply(&plan) {
            return false;
        }
        let (side, t) = spawn.from.unwrap_or((Side::Right, OUTPUT_T));
        let wire = self.build_connector(
            ConnectorEnd::Anchored {
                node: source,
                side,
                t,
            },
            ConnectorEnd::Anchored {
                node: id,
                side: Side::Left,
                t: port.t,
            },
        );
        self.doc_mut().scene.apply(&plan.inverted());
        if self.add_nodes(vec![node, wire]).is_empty() {
            return false;
        }
        self.board_sel = std::iter::once(id).collect();
        if !run {
            self.agents.flow.focus_prompt = Some(id);
        }
        if run {
            self.warm_text_sources(id);
            match kind {
                SpawnKind::Image if spawn.live => {}
                SpawnKind::Image => self.queue_generation(id),
                SpawnKind::Text => self.queue_text_block(id),
            }
        }
        true
    }

    /// Read the words of placed text documents wired into `id` before a run,
    /// so its snapshot can carry them without reading files itself.
    fn warm_text_sources(&mut self, id: NodeId) {
        let sources: Vec<(slate_doc::ItemId, std::path::PathBuf)> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Connector(c) => {
                    let binding = c.binding.as_ref()?;
                    let (from, to) = if binding.input_b {
                        (&c.a, &c.b)
                    } else {
                        (&c.b, &c.a)
                    };
                    (agent_inputs::endpoint_node(to) == Some(id) && binding.kind == InputKind::Text)
                        .then(|| agent_inputs::endpoint_node(from))
                        .flatten()
                }
                _ => None,
            })
            .filter_map(|source| match &self.doc().scene.node(source)?.kind {
                NodeKind::Image(image) => {
                    let item = self.doc().item(image.item)?;
                    Some((image.item, item.path.clone()))
                }
                _ => None,
            })
            .collect();
        for (item, path) in sources {
            let _ = self.snippet_for(item, &path);
        }
    }

    /// Typed ports of flow nodes the pointer is near, that are selected, or
    /// while a wire is being drawn.
    pub(crate) fn paint_flow_ports(&self, painter: &egui::Painter, xf: &BoardXf) {
        let z = xf.z;
        let radius = canvas_scale::px(PORT_RADIUS, z);
        if radius < 1.5 {
            return;
        }
        let palette = self.palette();
        let pointer = painter.ctx().pointer_latest_pos();
        let wiring = matches!(self.board_drag, Some(BoardDrag::Wire(_)));
        let font = canvas_scale::font(PORT_LABEL_PX, z);
        let labels = canvas_text::legible(font.size);
        let scene = &self.doc().scene;
        for node in &scene.nodes {
            let ports = agent_inputs::input_ports_of(node);
            if node.hidden || ports.is_empty() {
                continue;
            }
            let screen = xf.rect_w2s(node.rect);
            let near = pointer
                .is_some_and(|p| screen.expand(canvas_scale::px(PORT_REVEAL, z)).contains(p));
            if !(near || wiring || self.board_sel.contains(&node.id))
                || (self.agents.flow.hud_hidden(node.id) && !wiring)
            {
                continue;
            }
            let bound = agent_inputs::bound_slots(scene, node.id);
            let sites = WireHost::from_node(node).ports();
            for (port, site) in ports.iter().zip(&sites) {
                let center = xf.w2s(Pos2::new(site.point[0], site.point[1]));
                let (_, color) = InputRole::of_slot(port.slot).look();
                if bound.contains(&port.slot) {
                    painter.circle_filled(center, radius, color);
                } else {
                    painter.circle_filled(center, radius, palette.card);
                    painter.circle_stroke(
                        center,
                        radius,
                        egui::Stroke::new(canvas_scale::px(1.5, z), color),
                    );
                }
                if labels {
                    canvas_text::text(
                        painter,
                        center - egui::vec2(radius + canvas_scale::px(6.0, z), 0.0),
                        Align2::RIGHT_CENTER,
                        port.label,
                        font.clone(),
                        color,
                    );
                }
            }
            if let Some(out) = sites.last() {
                let center = xf.w2s(Pos2::new(out.point[0], out.point[1]));
                let hot = pointer.is_some_and(|p| p.distance(center) <= radius * 3.0);
                let ring = if hot { palette.accent } else { palette.sub };
                painter.circle_filled(center, radius * 1.2, palette.card);
                painter.circle_stroke(
                    center,
                    radius * 1.2,
                    egui::Stroke::new(canvas_scale::px(1.5, z), ring),
                );
                let arm = radius * 0.6;
                let stroke = egui::Stroke::new(canvas_scale::px(1.5, z), ring);
                painter.line_segment(
                    [center - egui::vec2(arm, 0.0), center + egui::vec2(arm, 0.0)],
                    stroke,
                );
                painter.line_segment(
                    [center - egui::vec2(0.0, arm), center + egui::vec2(0.0, arm)],
                    stroke,
                );
            }
        }
        // A dropped wire waits at its menu until a choice is made.
        if let Some(menu) = self.agents.flow.menu.as_ref().filter(|m| m.dropped) {
            if let Some(source) = scene.node(menu.source) {
                let (side, t) = menu.grip.unwrap_or((Side::Right, OUTPUT_T));
                let from = WireHost::from_node(source).anchor(side, t);
                painter.line_segment(
                    [xf.w2s(Pos2::new(from[0], from[1])), xf.w2s(menu.at)],
                    egui::Stroke::new(canvas_scale::px(1.5, z), palette.sub.gamma_multiply(0.6)),
                );
            }
        }
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

    /// Keep the draft and the journaled instruction in step: the document
    /// wins when it changed (undo, reload); otherwise the draft is committed
    /// once its field loses the caret.
    fn sync_instruction(&mut self, id: NodeId, journaled: &str) {
        let doc_changed = self.agents.flow.synced.get(&id).map(String::as_str) != Some(journaled);
        if doc_changed || !self.agents.has_draft(id) {
            *self.agents.prompt_mut(id) = journaled.to_string();
            self.agents.flow.synced.insert(id, journaled.to_string());
            return;
        }
        let draft = self.agents.prompt_mut(id).clone();
        if self.agents.composing(id) || draft == journaled || self.tab().read_only {
            return;
        }
        self.patch_nodes(&[id], |n| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                a.instruction = draft.clone();
            }
        });
        self.agents.flow.synced.insert(id, draft);
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

    /// Run a text block once: wired prompts, then its instruction, with the
    /// pictures on its Image port.
    pub(crate) fn queue_text_block(&mut self, id: NodeId) {
        // A block spawned by the agent editor runs before it is ever painted.
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
        // Running again replaces words the person made their own, as one undo.
        let baked = matches!(
            self.doc().scene.node(id).map(|n| &n.kind),
            Some(NodeKind::Text(t)) if !t.text.is_empty()
        );
        if baked && !self.tab().read_only {
            self.patch_nodes(&[id], |n| {
                if let NodeKind::Text(t) = &mut n.kind {
                    t.text.clear();
                }
            });
        }
        let model = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|a| a.model.clone());
        let request = AgentRequest {
            id: atlas_ai::agent::request_id(),
            prompt,
            model,
            at: atlas_ai::context::now_secs(),
            inputs,
            history: vec![],
            image: None,
            output_dir: None,
            oneshot: true,
        };
        self.enqueue_generation(id, request);
    }

    /// A note an agent writes: its newest reply shows in the note itself (the
    /// board paints it), and the prompt docks along the bottom with Run. An
    /// empty note is a prompt field until its first reply.
    pub(crate) fn paint_agent_note(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &slate_doc::scene::Node,
        body: Rect,
    ) {
        let id = node.id;
        let z = xf.z;
        let px = |v: f32| canvas_scale::px(v, z);
        let font = canvas_scale::font(BLOCK_TEXT_PX, z);
        if !canvas_text::legible(font.size) {
            return;
        }
        let running = self.agent_is_running(id);
        let waiting = self.generations_waiting(id);
        let reply = self.agent_reply(id);
        let own = matches!(&node.kind, NodeKind::Text(t) if !t.text.trim().is_empty());
        if let (Some(reply), NodeKind::Text(t)) = (&reply, &node.kind) {
            if !own && !running {
                let laid = canvas_text::layout(
                    painter,
                    reply.clone(),
                    canvas_scale::font(t.size, z),
                    Color32::WHITE,
                    body.width(),
                );
                self.fit_agent_note(id, reply, laid.size().y / z.max(0.01));
            }
        }
        let hovered = ui
            .ctx()
            .pointer_latest_pos()
            .is_some_and(|p| body.expand(px(8.0)).contains(p));
        let reveal = hovered || self.board_sel.contains(&id) || self.flow_capsule_open(id);
        let empty = reply.is_none() && !own;
        if empty && !(running || waiting > 0) {
            let wired = self
                .generator_view(id)
                .inputs
                .iter()
                .any(|i| i.role == InputRole::Prompt);
            let hint = if wired {
                "Add to the wired prompt, or press Run"
            } else {
                "Wire a note to Prompt, or type here"
            };
            let field = Rect::from_center_size(
                body.center(),
                egui::vec2(body.width() - px(32.0), px(64.0)),
            );
            painter.rect_filled(
                field.expand(px(8.0)),
                px(10.0),
                Color32::from_black_alpha(150),
            );
            self.paint_generator_prompt(ui, id, field, z, hint);
        }
        let failure = self
            .agent_failure_reason(id)
            .map(str::to_string)
            .filter(|_| !running);
        if let Some(reason) = failure {
            let laid = canvas_text::layout(
                painter,
                reason,
                canvas_scale::font(super::board_agent::GENERATOR_CHIP_PX, z),
                Color32::from_rgb(255, 190, 130),
                body.width() - px(24.0),
            );
            let at = Pos2::new(body.left() + px(12.0), body.top() + px(12.0));
            painter.rect_filled(
                Rect::from_min_size(at, laid.size()).expand(px(5.0)),
                px(6.0),
                Color32::from_black_alpha(175),
            );
            laid.paint(painter, at, Color32::from_rgb(255, 190, 130));
        }
        self.paint_run_progress(ui, painter, id, body, z, "Writing", false);
        if !reveal {
            return;
        }
        let at = Pos2::new(body.left() + px(12.0), body.bottom() - px(40.0));
        let (run, rect) = Self::generator_button(
            ui,
            painter,
            at,
            if running { "Stop" } else { "Run" },
            Id::new(("text-block-run", id.0)),
            Color32::from_black_alpha(175),
            z,
        );
        if run.clicked() {
            self.board_sel = std::iter::once(id).collect();
            let command = if running {
                "portal.agent.stop"
            } else {
                "portal.agent.send"
            };
            self.dispatch(ui.ctx(), atlas_commands::CommandId(command), None);
        }
        if !empty {
            let capsule = Rect::from_min_max(
                Pos2::new(rect.right() + px(6.0), rect.top()),
                Pos2::new(body.right() - px(12.0), rect.bottom()),
            );
            self.paint_prompt_capsule(ui, painter, id, capsule, body, z);
        }
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
        match self.doc().scene.node(id).map(|n| &n.kind) {
            Some(NodeKind::Text(t)) if t.agent.is_some() && t.text.trim().is_empty() => {
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
        if image {
            self.agents.want_catalog("comfy");
            if engines.openai {
                self.agents.want_catalog("openai-image");
            }
            return atlas_ai::runtime::image_models(
                self.agents.catalog("comfy"),
                engines.codex,
                engines.openai,
                self.agents.catalog("openai-image"),
            );
        }
        self.agents.want_catalog("ollama");
        if engines.codex {
            self.agents.want_catalog("codex");
        }
        if engines.openai {
            self.agents.want_catalog("openai-text");
        }
        atlas_ai::runtime::text_models(
            self.agents.catalog("ollama"),
            engines.codex.then(|| self.agents.catalog("codex")),
            engines.openai,
            self.agents.catalog("openai-text"),
        )
    }

    /// The model the person last chose for this kind of agent.
    pub(crate) fn default_model(&self, image: bool) -> Option<(String, String)> {
        if image {
            self.settings.agent_image_model.clone()
        } else {
            self.settings.agent_text_model.clone()
        }
    }

    /// A chosen model becomes the default for every new agent of its kind.
    pub(crate) fn remember_model(&mut self, image: bool, provider: &str, model: &str) {
        let choice = Some((provider.to_string(), model.to_string()));
        let slot = if image {
            &mut self.settings.agent_image_model
        } else {
            &mut self.settings.agent_text_model
        };
        if *slot != choice {
            *slot = choice;
            self.settings.save();
        }
    }

    /// A saved key opens the OpenAI catalogs.
    fn key_saved(&mut self) {
        self.agents.flow.engines = None;
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
        if agent_inputs::is_flow_node(&self.doc().scene, source) {
            return self.attached_editor_body(ui, rect, source, z, theme);
        }
        let mut draft = self
            .agents
            .flow
            .drafts
            .remove(&source)
            .unwrap_or_else(AgentDraft::new);
        let models = self.agent_models(draft.image);
        let modes: Vec<&str> = MODALITIES.iter().map(|m| m.label).collect();
        let mode = if draft.image {
            SpawnKind::Image
        } else {
            SpawnKind::Text
        }
        .index();
        let current = draft
            .choice
            .clone()
            .or_else(|| self.default_model(draft.image));
        let index = model_index(&models, current.as_ref());
        let labels: Vec<&str> = models.iter().map(|m| m.2.as_str()).collect();
        let aspects: Vec<&str> = atlas_ai::agent::Aspect::ALL
            .iter()
            .map(|a| a.label())
            .collect();
        let pasting = draft.key.is_some();
        let hint = if pasting {
            "Paste your OpenAI API key, then Submit. It stays in Windows Credential Manager."
        } else if draft.image {
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
        let out = atlas_shell::selection_tools::agent_editor(
            ui,
            rect,
            atlas_shell::selection_tools::AgentEdit {
                modes: &modes,
                mode,
                image: draft.image,
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
            draft.image = MODALITIES
                .get(mode)
                .is_some_and(|m| m.kind == SpawnKind::Image);
            draft.choice = None;
            draft.key = None;
        }
        if let Some((provider, model, _)) = out.model.and_then(|i| models.get(i)) {
            draft.choice = Some((provider.clone(), model.clone()));
            self.remember_model(draft.image, provider, model);
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
        if provider.starts_with("openai") && !self.engines().openai {
            draft.key = Some(String::new());
            draft.take_focus = true;
            return;
        }
        self.remember_model(draft.image, &provider, &model);
        if draft.prompt.trim().is_empty() {
            self.toast("Write a prompt for the agent first.");
            draft.take_focus = true;
            return;
        }
        let aspect = atlas_ai::agent::Aspect::ALL
            .get(draft.aspect)
            .copied()
            .unwrap_or_default();
        let detail = serde_json::json!({
            "source": source.0,
            "kind": if draft.image { "image" } else { "text" },
            "prompt": draft.prompt.trim(),
            "provider": provider,
            "model": (!model.is_empty()).then_some(model),
            "image": {"count": draft.count, "aspect": aspect, "seed": draft.seed},
            "live": draft.live && draft.image,
        });
        self.dispatch(
            ctx,
            atlas_commands::CommandId("portal.agent.spawn"),
            Some(detail.to_string()),
        );
        // The source stays selected, so the next variation is one Submit away.
        self.board_sel = std::iter::once(source).collect();
    }

    /// A pasted OpenAI key goes to Credential Manager. True when a key was
    /// being pasted, saved or not.
    fn save_pasted_key(&mut self, draft: &mut AgentDraft) -> bool {
        let Some(key) = draft.key.take() else {
            return false;
        };
        match atlas_core::secrets::store(atlas_ai::runtime::OPENAI_KEY_SLOT, &key) {
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
        let out = atlas_shell::selection_tools::agent_editor(
            ui,
            rect,
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
            if provider.starts_with("openai") && !self.engines().openai {
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
        let laid = canvas_text::layout_no_wrap(painter, text, font, Color32::WHITE);
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
        painter.rect_filled(pill, size.y * 0.5, Color32::from_black_alpha(185));
        let time = ui.input(|i| i.time) as f32;
        super::board_agent::paint_agent_spinner(
            painter,
            Pos2::new(pill.left() + pad + spin, pill.center().y),
            spin,
            time,
            Color32::WHITE,
        );
        laid.paint_anchored(
            painter,
            Pos2::new(pill.left() + pad * 2.0 + spin * 2.0, pill.center().y),
            Align2::LEFT_CENTER,
            Color32::WHITE,
        );
        if running && !previews {
            // No step count to show: a segment sweeps along the bottom edge.
            let h = canvas_scale::px(3.0, z);
            let track = Rect::from_min_size(
                Pos2::new(body.left(), body.bottom() - h),
                egui::vec2(body.width(), h),
            );
            let span = track.width() * 0.3;
            let phase = (time * 0.6).fract();
            let x = track.left() - span + (track.width() + span) * phase;
            let bar = Rect::from_min_max(
                Pos2::new(x.max(track.left()), track.top()),
                Pos2::new((x + span).min(track.right()), track.bottom()),
            );
            painter.rect_filled(bar, 0.0, Color32::from_rgb(92, 204, 170));
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
        // The field sits on a dark scrim in both themes.
        theme.ink = Color32::WHITE;
        theme.sub = Color32::from_gray(170);
        let mut text = self.agents.prompt_mut(id).clone();
        let take_focus = self.agents.flow.focus_prompt == Some(id);
        if take_focus {
            self.agents.flow.focus_prompt = None;
        }
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
        if out.focused {
            self.agents.flow.typing = Some(id);
            self.web_release_keyboard();
        } else if self.agents.flow.typing == Some(id) {
            self.agents.flow.typing = None;
        }
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

    /// Run the agent a picture or note was summoned with, once.
    pub(crate) fn run_agent_media(&mut self, id: NodeId) {
        if self.is_text_block(id) {
            self.queue_text_block(id);
        } else {
            self.queue_generation(id);
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
        painter.rect_filled(
            capsule,
            capsule.height() * 0.5,
            Color32::from_black_alpha(175),
        );
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
                Color32::from_gray(170)
            } else {
                Color32::WHITE
            },
            capsule.width() - px(24.0),
            1,
        );
        laid.paint_anchored(
            &painter.with_clip_rect(capsule),
            Pos2::new(capsule.left() + px(12.0), capsule.center().y),
            Align2::LEFT_CENTER,
            Color32::WHITE,
        );
        if !open {
            return;
        }
        painter.rect_filled(expanded, px(10.0), Color32::from_black_alpha(215));
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
mod tests {
    use super::*;
    use slate_doc::agent_inputs::InputKind;

    /// A ComfyUI generator with one finished picture in its album.
    fn generator_with_output(tag: &str) -> (super::super::tests::Harness, NodeId) {
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
                let end = if binding.input_b { &c.b } else { &c.a };
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
        // A left-edge input is not an output.
        assert!(!h
            .app
            .flow_wire_released((generator, Side::Left, 0.5), Pos2::new(1400.0, 0.0)));

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

    #[test]
    fn ports_show_near_a_generator_and_the_text_block_paints_its_controls() {
        let (mut h, generator) = generator_with_output("flow_paint");
        let detail = serde_json::json!({"source": generator.0, "kind": "text"});
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let block = *h.app.board_sel.iter().next().unwrap();
        h.frame();
        let r = h.app.doc().scene.node(generator).unwrap().rect;
        h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w, r.y + r.h * 0.5);
        h.app.tab_mut().cam.z = 0.6;
        h.frame();
        h.app.agent_blur();
        h.app.board_sel.clear();
        let rest = painted(&mut h, Pos2::new(2.0, 890.0));
        assert!(
            !rest.iter().any(|t| t == "Style"),
            "quiet at rest: {rest:?}"
        );
        let xf = h.app.board_xf();
        let near = xf.rect_w2s(r).center();
        let hover = painted(&mut h, near);
        for label in ["Media", "Prompt", "Style"] {
            assert!(
                hover.iter().any(|t| t == label),
                "missing {label}: {hover:?}"
            );
        }
        // The text choice is a note: its ports, Run, and the prompt docked
        // under it. The model lives on the Agent squircle.
        assert!(matches!(
            h.app.doc().scene.node(block).map(|n| &n.kind),
            Some(NodeKind::Text(t)) if t.agent.is_some()
        ));
        h.app.board_sel = std::iter::once(block).collect();
        let selected = painted(&mut h, Pos2::new(2.0, 890.0));
        // Before its first reply the note is its prompt field.
        for label in ["Image", "Prompt", "Run", "Describe the visual style"] {
            assert!(
                selected.iter().any(|t| t.starts_with(label)),
                "missing {label}: {selected:?}"
            );
        }
        assert!(
            !selected.iter().any(|t| t.starts_with("Local ·")),
            "no model chip: {selected:?}"
        );
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

    /// Cover flow belongs to a selected picture with several results; hover
    /// never flows. Copying it copies only the picture it shows, as a plain
    /// placed image.
    #[test]
    fn a_selected_picture_flows_and_copies_as_its_shown_image() {
        let (mut h, generator) = generator_with_output("media_flow");
        let second = h.base.join("hall-2.png");
        image::RgbImage::new(8, 8).save(&second).unwrap();
        let session = atlas_ai::agent::AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: vec![],
            status: atlas_ai::agent::AgentStatus::Idle,
            provider: "comfy".into(),
            turns: vec![],
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
        assert_eq!(h.app.agent_shown_path(generator).as_deref(), Some(second.as_path()));

        h.app.board_sel.clear();
        assert_eq!(h.app.image_wheel(generator), super::super::board_agent::ImageWheel::Board);
        h.app.board_sel.insert(generator);
        assert_eq!(h.app.image_wheel(generator), super::super::board_agent::ImageWheel::Album);
        // Flowing hides the labels until the picture is clicked again.
        h.app.agents.flow.set_hud_hidden(generator, true);
        assert!(h.app.agents.flow.hud_hidden(generator));
        h.app.board_sel.clear();
        h.frame();
        assert!(!h.app.agents.flow.hud_hidden(generator), "deselecting shows it again");

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
        assert!(matches!(
            &h.app.doc().scene.node(generator).unwrap().kind,
            NodeKind::Image(i) if i.agent.is_some()
        ), "the source keeps its agent");
    }

    /// The Text choice is a sticky note. It shows the agent's reply until the
    /// person edits it; running again replaces their words as one undo.
    #[test]
    fn the_text_choice_is_a_note_that_shows_its_reply_until_edited() {
        let (mut h, generator) = generator_with_output("media_note");
        let detail = serde_json::json!({"source": generator.0, "kind": "text"});
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let note = *h.app.board_sel.iter().next().unwrap();
        assert!(matches!(
            &h.app.doc().scene.node(note).unwrap().kind,
            NodeKind::Text(t) if t.text.is_empty() && t.fill.is_some()
        ));
        let session = atlas_ai::agent::AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: vec![],
            status: atlas_ai::agent::AgentStatus::Idle,
            provider: "ollama".into(),
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

        // Opening and closing the editor unchanged keeps the words the agent's.
        let depth = h.app.tab().journal.undo_depth();
        h.app.text_edit = Some((note, "Warm light on stone.".into()));
        h.app.commit_text_edit();
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        // Editing makes them the person's own.
        h.app.text_edit = Some((note, "Warm light on old stone.".into()));
        h.app.commit_text_edit();
        assert!(h.app.agent_note_reply(note).is_none());
        let depth = h.app.tab().journal.undo_depth();
        *h.app.agents.prompt_mut(note) = "Describe it".into();
        h.app.queue_text_block(note);
        assert!(matches!(
            &h.app.doc().scene.node(note).unwrap().kind,
            NodeKind::Text(t) if t.text.is_empty()
        ));
        assert!(
            h.app.tab().journal.undo_depth() > depth,
            "undo brings the words back"
        );
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
    fn place(h: &mut super::super::tests::Harness, name: &str, x: f32) -> NodeId {
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
        // egui sizes a new area on its first frame without painting it.
        painted(&mut h, Pos2::new(2.0, 890.0));
        let shown = painted(&mut h, Pos2::new(2.0, 890.0));
        for label in [
            "Text",
            "Image",
            "Submit",
            "ComfyUI · Auto",
            "Source",
            "1 img",
            "Seed",
            "Live",
        ] {
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
        h.app.agents.note_menu_popup(&h.ctx, list);
        wheel_at(&mut h, empty);
        assert_eq!(h.app.tab().cam.z, before, "an open list scrolls instead");

        let r = h.app.doc().scene.node(generator).unwrap().rect;
        let over = h.app.board_xf().rect_w2s(r).center();
        painted(&mut h, over);
        let shown = painted(&mut h, over);
        assert!(
            shown.iter().any(|t| t == "Generate") && !shown.iter().any(|t| t == "ComfyUI · Auto"),
            "the model is chosen on the Agent squircle: {shown:?}"
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
    fn a_running_frame_shows_what_it_is_doing_and_for_how_long() {
        let (mut h, generator) = generator_with_output("progress");
        h.app.patch_nodes(&[generator], |n| {
            let a = slate_doc::agent_chat::agent_mut(n).unwrap();
            a.instruction = "a red barn".into();
            a.image.count = 3;
        });
        h.app.queue_generation(generator);
        h.app.agent_blur();
        h.app.board_sel.clear();
        let r = h.app.doc().scene.node(generator).unwrap().rect;
        h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
        h.app.tab_mut().cam.z = 1.0;
        h.frame();
        let shown = painted(&mut h, Pos2::new(2.0, 890.0));
        assert!(
            shown
                .iter()
                .any(|t| t.starts_with("Generating image 1 of 3 · 0:0")),
            "progress without hovering: {shown:?}"
        );
        // Selected, the picture shows its dock.
        h.app.board_sel.insert(generator);
        let shown = painted(&mut h, Pos2::new(2.0, 890.0));
        assert!(shown.iter().any(|t| t == "Stop"), "{shown:?}");
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
            fn take_frame(&mut self, _: NodeId) -> Option<egui::ColorImage> {
                None
            }
            fn capture_poster(&mut self, _: NodeId) -> Option<egui::ColorImage> {
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
