//! Local generator view and menu state.
use super::*;

pub(super) struct FlowMenu {
    pub(super) source: NodeId,
    /// World point the menu opened at: the port for a click, the release
    /// point for a dropped wire.
    pub(super) at: Pos2,
    pub(super) dropped: bool,
    /// The grip a dropped wire left from; the new wire keeps it.
    pub(super) grip: Option<(Side, f32)>,
    pub(super) armed: bool,
}

/// The agent editor's unsent choices for one source. Submitting journals them
/// on the new node; until then they are view state.
#[derive(Clone)]
pub(super) struct AgentDraft {
    pub(super) kind: SpawnKind,
    pub(super) prompt: String,
    /// (provider, model) picked in this editor; `None` follows the default.
    pub(super) choice: Option<(String, String)>,
    pub(super) model_parameters: std::collections::BTreeMap<String, String>,
    pub(super) model_open: bool,
    pub(super) count: u8,
    pub(super) aspect: usize,
    pub(super) custom_aspect: Option<[u16; 2]>,
    pub(super) aspect_open: bool,
    pub(super) seed: Option<u64>,
    pub(super) live: bool,
    pub(super) preserve_history: bool,
    pub(super) take_focus: bool,
    /// An OpenAI key being pasted, shown masked.
    pub(super) key: Option<String>,
}

impl AgentDraft {
    pub(super) fn image_aspect(&self) -> atlas_ai::agent::Aspect {
        if self.aspect == atlas_ai::agent::Aspect::ALL.len() {
            if let Some([width, height]) = self.custom_aspect {
                return atlas_ai::agent::Aspect::Custom { width, height };
            }
        }
        atlas_ai::agent::Aspect::ALL
            .get(self.aspect)
            .copied()
            .unwrap_or_default()
    }
    pub(super) fn new() -> Self {
        Self {
            kind: SpawnKind::Image,
            prompt: String::new(),
            choice: None,
            model_parameters: Default::default(),
            model_open: false,
            count: 1,
            aspect: 0,
            custom_aspect: None,
            aspect_open: false,
            seed: None,
            live: false,
            preserve_history: false,
            take_focus: true,
            key: None,
        }
    }
    pub(super) fn from_node(node: &slate_doc::scene::Node) -> Self {
        let mut draft = Self::new();
        if let Some(agent) = slate_doc::agent_chat::agent(node) {
            draft.count = agent.generator.count.unwrap_or(agent.image.count).max(1);
            draft.preserve_history = agent.generator.preserve_history.unwrap_or(false);
        }
        draft
    }
}

/// Installed sign-in adapter availability, read once rather than per frame.
#[derive(Clone, Copy)]
pub(super) struct Engines {
    pub(super) codex: bool,
}

impl Engines {
    pub(super) fn probe() -> Self {
        Self {
            codex: atlas_ai::runtime::codex_executable().is_some(),
        }
    }
}

/// Immutable selected passage belonging to one submitted request.
#[derive(Clone, Debug)]
pub(crate) struct TextAnswerRequest {
    pub(crate) group: String,
    pub(crate) context_epoch: u64,
    pub(crate) source: String,
    pub(crate) range: std::ops::Range<usize>,
    pub(crate) question: String,
    pub(crate) session: String,
    pub(crate) source_revision: Option<String>,
    pub(crate) review: bool,
    pub(crate) selected: bool,
}

type ConnectedInputCache = (
    (u64, u64, u64),
    HashMap<NodeId, std::collections::HashSet<atlas_agent::InputSlot>>,
);

/// Derived view state; nothing here is journaled.
#[derive(Default)]
pub(crate) struct FlowUi {
    /// Derived visibility cache, rebuilt once per scene revision rather than
    /// scanning every connector for every painted native source.
    pub(super) connected_inputs: std::cell::RefCell<Option<ConnectedInputCache>>,
    pub(super) graph_menu: Option<super::graph_ui::GraphMenu>,
    pub(super) history_view: Option<super::graph_history::HistoryView>,
    pub(super) history_press: Option<(u64, NodeId, bool, Pos2)>,
    pub(crate) picker_key_entry: Option<(u64, NodeId)>,
    pub(crate) text_selections: HashMap<(u64, NodeId), (String, std::ops::Range<usize>)>,
    pub(crate) text_answer_requests: HashMap<(u64, NodeId, String), TextAnswerRequest>,
    /// Open the existing generator dialogue at this response; never resume inference.
    pub(crate) text_answer_navigation:
        Option<(u64, NodeId, slate_doc::text_answers::TextOutputRef)>,
    pub(crate) viewed_dialogues: HashMap<(u64, NodeId), slate_doc::text_answers::TextOutputRef>,
    pub(crate) text_query_runs: HashMap<(u64, NodeId), String>,
    /// Shared ephemeral generator menus and public activity, keyed by workbook.
    pub(crate) presentations:
        HashMap<(u64, NodeId), atlas_shell::selection_tools::generator::GeneratorPresentation>,
    pub(super) image_result_drag: Option<image_editor::ImageResultDrag>,
    pub(super) results: result_actions::ResultController,
    pub(super) menu: Option<FlowMenu>,
    pub(super) press: Option<(NodeId, Pos2)>,
    pub(super) drafts: HashMap<NodeId, AgentDraft>,
    pub(super) engines: Option<Engines>,
    /// The prompt capsule of this frame is expanded for editing.
    pub(super) capsule: Option<NodeId>,
    /// A generator's own prompt field has the caret.
    pub(super) typing: Option<NodeId>,
    /// A freshly spawned agent's prompt takes the caret on its first paint.
    pub(super) focus_prompt: Option<NodeId>,
    /// Pictures being cover-flowed: their labels, dock and ports stay hidden
    /// until the picture is clicked again.
    pub(super) hud_hidden: std::collections::HashSet<NodeId>,
    /// When the shown run of a frame started: (request, instant).
    pub(super) started: HashMap<NodeId, (String, std::time::Instant)>,
    /// The instruction each text block last had in the document, so an undo
    /// or a reload replaces the draft instead of being overwritten by it.
    pub(super) synced: HashMap<NodeId, String>,
    /// Reply a text block was last fitted to.
    pub(super) fitted: HashMap<NodeId, String>,
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
