//! Agent portal boards and the thinking-state send.

use super::*;

pub(crate) fn agent_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h
}

pub(crate) fn assert_send_shows_thinking(h: &Harness) {
    let awaiting = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .any(|n| h.app.agent_is_awaiting(n.id));
    assert!(awaiting, "Send must enter Thinking — never a blank wait");
    assert!(h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .all(|n| h.app.agent_failure_reason(n.id).is_none()));
}

/// Hermetic: `local` writes `request.json` only; it never starts Ollama/Codex runtimes.
pub(crate) fn send_thinking_harness(tag: &str) -> (Harness, slate_doc::NodeId) {
    let mut h = agent_board(tag);
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    let ws = h.base.join("ai-ws");
    std::fs::create_dir_all(&ws).unwrap();
    h.app.ai.config.workspace_dir = Some(ws);
    *h.app.agents.prompt_mut(id) = "what is 2+2?".into();
    (h, id)
}

pub(crate) fn walk_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk_files(&p));
        } else {
            out.push(p);
        }
    }
    out
}

// --- what happens once you are inside the page ------------------------------
