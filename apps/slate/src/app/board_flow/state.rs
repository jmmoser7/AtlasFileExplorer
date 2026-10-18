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
pub(super) struct AgentDraft {
    pub(super) kind: SpawnKind,
    pub(super) prompt: String,
    /// (provider, model) picked in this editor; `None` follows the default.
    pub(super) choice: Option<(String, String)>,
    pub(super) model_open: bool,
    pub(super) count: u8,
    pub(super) aspect: usize,
    pub(super) aspect_open: bool,
    pub(super) seed: Option<u64>,
    pub(super) live: bool,
    pub(super) take_focus: bool,
    /// An OpenAI key being pasted, shown masked.
    pub(super) key: Option<String>,
}

impl AgentDraft {
    pub(super) fn new() -> Self {
        Self {
            kind: SpawnKind::Image,
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

/// Derived view state; nothing here is journaled.
#[derive(Default)]
pub(crate) struct FlowUi {
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
