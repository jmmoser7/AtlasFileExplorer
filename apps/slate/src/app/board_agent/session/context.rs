//! Wired context metadata for the session pump.
use super::*;

impl SlateApp {
    pub(super) fn agent_context_for(
        &self,
        session: &str,
        provider: &str,
        _scope: AgentContextScope,
    ) -> AgentContext {
        let tab = self.tab();
        AgentContext {
            app: "slate",
            session: session.to_string(),
            provider: provider.to_string(),
            workbook: tab.path.clone(),
            format_version: tab.doc.format_version,
            scope: "wired".into(),
            selection: vec![],
            viewport: None,
            board_summary: String::new(),
            generated_at: atlas_ai::context::now_secs(),
        }
    }
}
