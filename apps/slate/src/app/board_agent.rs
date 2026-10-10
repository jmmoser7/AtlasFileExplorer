//! Agent portal runtime: file-link context, session status, and staged proposal
//! handling. Everything here is derived state until a human accepts a proposal.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use atlas_ai::agent::{
    launch_provider, AgentContext, AgentRequest, AgentSession, AgentStatus, AgentTurn,
};
use atlas_ai::cursor_chats::CursorChat;
use atlas_ai::launch::CursorIdeStatus;
use atlas_shell::file_picker::{self, PickRequest};
use atlas_shell::home::{image_album, AlbumImage};
use atlas_shell::recent::{RecentEntry, RecentList};
use atlas_shell::tokens::OverlayInk;
use atlas_shell::{canvas_scale, canvas_text};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Align2, Color32, FontId, Id, Pos2, Rect, Sense};
use slate_doc::reject;
use slate_doc::scene::{AgentContextScope, Node, NodeId, NodeKind, PortalKind, PortalNode};
use slate_doc::stage::{self, Proposal, ProposalResult, StageFeed};

use super::board::BoardXf;
use super::board_portal::resolve_source;
use super::board_portal_chrome::layout_for_portal;
use super::{PickerMsg, SlateApp};

pub(crate) mod crosstalk;
mod life;
#[path = "board_agent_outputs.rs"]
mod outputs;
mod schedule;
mod train_ux;

// Concern modules. Names stay on this module via `use`.
mod agent_stage;
mod artifacts;
mod artifacts_open;
mod awaits;
mod bundle;
mod card_fit;
mod card_metrics;
mod composer;
mod connection;
mod context;
mod cursor_ide;
mod focus;
mod generator;
mod generator_pump;
mod inputs;
mod models;
mod picture;
mod portal_bound;
mod portal_composer;
mod portal_header;
mod portal_picker;
mod portal_unbound;
mod programs;
mod prompt;
mod results;
mod runtime;
mod session;
mod spawn;
mod spawn_chrome;
mod train;

use card_metrics::{
    agent_pick_columns, bundle_view, card_fold, collapsed_card_height, composer_text_height,
    composer_wrap, context_label, conversation_card_height, hugging_composer_card,
    output_circle_center, paint_chevron_glyph, paint_pick_button, paint_responding,
    parse_artifact_open, tracked_galley, write_fold, CARD_TEXT_PAD, COLLAPSED_ROWS,
    COMPOSER_BOTTOM, COMPOSER_GAP, COMPOSER_TOP, PICK_BAR_RESERVE, STOP_REACH, STOP_SIDE,
    SUMMARY_TEXT_TOP,
};
pub(crate) use card_metrics::{
    agent_presentations, paint_agent_spinner, paint_handle_dot, paint_overlay_pill,
    record_agent_resize, ArtifactBuild, RespondingLabel,
};
use cursor_ide::{
    agent_status_chip, await_new_reply, chat_key, classify_agent_failure,
    collect_agent_project_recents, fit_frame_height, model_default, model_fallback, recover_label,
};
#[cfg(not(test))]
use runtime::SidecarBoot;
#[cfg(test)]
pub(crate) use runtime::LINK_PROBES_ON_THIS_THREAD;
use runtime::{
    stable_seed, AgentAwait, AgentRecover, AGENT_RECENTS_KEY, AWAIT_TIMEOUT, CAPTURE_TIMEOUT,
    GENERATION_QUEUE, LIVE_TYPING_SETTLE,
};
pub use runtime::{AgentRuntime, ChatPicker};
pub(crate) use runtime::{
    GeneratorInput, GeneratorView, ImageWheel, InputRole, LivePreview, GENERATOR_CHIP_PX,
};

/// What [`SlateApp::program_binding`] resolved for one bind.
pub(crate) struct ProgramBinding {
    session: String,
    bundle: Option<slate_doc::SourceUri>,
    locator: Option<String>,
}

impl ProgramBinding {
    pub(crate) fn set_locator(&mut self, locator: String) {
        self.locator = Some(locator);
    }
}

/// The size a freshly bound program's card takes.
pub(crate) fn program_card_size(view: atlas_ai::agent::PortalView) -> egui::Vec2 {
    use atlas_ai::agent::PortalView;
    match view {
        PortalView::Chat => egui::vec2(slate_doc::agent_chat::CARD_WIDTH, 540.0),
        PortalView::Images => egui::vec2(960.0, 540.0),
        PortalView::Text => egui::vec2(440.0, 320.0),
    }
}

/// Bind a program onto an agent portal: provider identity, a fresh session,
/// the view and its card size.
pub(crate) fn bind_program(
    node: &mut Node,
    program: &atlas_ai::agent::AgentProvider,
    binding: &ProgramBinding,
) {
    let Some(a) = slate_doc::agent_chat::agent_mut(node) else {
        return;
    };
    a.provider = program.id.clone();
    a.chat.linear = atlas_ai::runtime::linear_provider(&program.id);
    a.session = binding.session.clone();
    a.channel = None;
    a.bundle = binding.bundle.clone();
    a.seed = None;
    a.view = program.view;
    // New conversations start as message pairs; a card already placed as a
    // chat train (the wire-drop Agent) keeps its presentation.
    if program.view == atlas_ai::agent::PortalView::Chat && !a.chat.train {
        a.chat.train = true;
        a.chat.detail = slate_doc::agent_chat::Detail::Pair;
    }
    let size = program_card_size(program.view);
    node.rect.w = size.x;
    node.rect.h = size.y;
    if let NodeKind::Portal(p) = &mut node.kind {
        p.title = program.display_name.clone();
        if let Some(locator) = &binding.locator {
            p.source = Some(slate_doc::SourceUri {
                locator: locator.clone(),
            });
        }
    }
    // An image or text engine makes media, not a chat card.
    slate_doc::scene::agent_card_as_media(node);
}

#[cfg(test)]
mod await_tests;
#[cfg(test)]
mod cover_tests;
#[cfg(test)]
mod draft_tests;
#[cfg(test)]
mod fork_tests;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod paint_probe;
#[cfg(test)]
mod picker_tests;
#[cfg(test)]
mod present_tests;
#[cfg(test)]
mod stop_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod train_setup;
#[cfg(test)]
mod train_tests;
