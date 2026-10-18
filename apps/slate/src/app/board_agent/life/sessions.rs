//! Saved-session membership and completed exchange predicates.
use crate::app::SlateTab;
use atlas_ai::agent::AgentTurn;

pub(super) fn tab_sessions(tab: &SlateTab) -> impl Iterator<Item = &str> {
    tab.doc
        .scene
        .nodes
        .iter()
        .filter_map(slate_doc::agent_chat::agent)
        .filter(|a| !a.provider.is_empty())
        .map(|a| a.session.as_str())
}
/// At least one user message has been answered.
pub(super) fn completed_exchange(turns: &[AgentTurn]) -> bool {
    turns
        .iter()
        .position(|t| t.role == "user")
        .is_some_and(|first| turns[first..].iter().any(|t| t.role == "assistant"))
}
