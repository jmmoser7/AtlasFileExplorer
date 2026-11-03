//! Bounded public Images conversation pages; no activity or private reasoning.
use super::*;
use std::hash::{Hash, Hasher};

pub(in crate::app::agent_presence) fn recognizes(call: &BoardCall) -> bool {
    matches!(&call.operation,BoardOperation::Invoke{command,arguments,..} if command=="board.image.history" && arguments["action"]=="view")
}
pub(super) struct HistoryPage {
    target: slate_doc::NodeId,
    args: Value,
    generation: u64,
    token: Option<String>,
    response: Option<Value>,
    session: Option<Arc<AgentSession>>,
}
impl HistoryPage {
    pub(super) fn parse(call: &BoardCall, generation: u64) -> Result<Option<Self>, String> {
        if !recognizes(call) {
            return Ok(None);
        }
        let BoardOperation::Invoke {
            targets, arguments, ..
        } = &call.operation
        else {
            unreachable!()
        };
        let object = arguments
            .as_object()
            .ok_or("History page arguments must be an object")?;
        if targets.len() != 1
            || object.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "action" | "cursor" | "text_cursor" | "limit" | "generation"
                )
            })
        {
            return Err("Use one Image and only action/cursor/text_cursor/limit/generation".into());
        }
        let target = targets[0]
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .map(slate_doc::NodeId)
            .ok_or("Use one positive stable Image ID")?;
        let page = Self {
            target,
            args: arguments.clone(),
            generation,
            token: None,
            response: None,
            session: None,
        };
        if !(4..=4096).contains(&page.integer("limit", 4096)?) {
            return Err("History page limit must be 4–4096 bytes".into());
        }
        if (page.integer("cursor", 0)? > 0 || page.integer("text_cursor", 0)? > 0)
            && !arguments["generation"].is_string()
        {
            return Err("Continuation requires the exact history generation".into());
        }
        Ok(Some(page))
    }
    fn integer(&self, key: &str, default: usize) -> Result<usize, String> {
        self.args.get(key).map_or(Ok(default), |v| {
            v.as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| format!("{key} must be an unsigned integer"))
        })
    }
    pub(super) fn initialize(
        &mut self,
        app: &SlateApp,
        grant: &CollaboratorGrant,
    ) -> Result<(), String> {
        let value = self.value(app, grant)?;
        self.token = value["generation"].as_str().map(str::to_owned);
        self.session = app.agents.sessions.get(&self.target).cloned();
        self.response = Some(value);
        Ok(())
    }
    pub(super) fn value(&self, app: &SlateApp, grant: &CollaboratorGrant) -> Result<Value, String> {
        if self.generation != app.doc().scene.scene_gen() {
            return Err("Board changed; restart the history page".into());
        }
        app.validate_image_history_scope(grant, &[self.target.0.to_string()], &self.args)?;
        if let Some(response) = &self.response {
            let current = app.agents.sessions.get(&self.target);
            if match (&self.session, current) {
                (Some(before), Some(after)) => !Arc::ptr_eq(before, after),
                (None, None) => false,
                _ => true,
            } {
                return Err(
                    "Public history changed during the read; restart at cursor zero".into(),
                );
            }
            return Ok(response.clone());
        }
        let turns = app.public_image_history(self.target)?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.generation.hash(&mut hash);
        for turn in &turns {
            turn.role.hash(&mut hash);
            turn.text.hash(&mut hash);
            turn.at.hash(&mut hash);
        }
        let token = format!("{:016x}", hash.finish());
        if self.token.as_ref().is_some_and(|v| v != &token)
            || self
                .args
                .get("generation")
                .is_some_and(|v| v.as_str() != Some(token.as_str()))
        {
            return Err("Public history changed; restart at cursor zero".into());
        }
        let cursor = self.integer("cursor", 0)?;
        let text_cursor = self.integer("text_cursor", 0)?;
        if cursor > turns.len() || (cursor == turns.len() && text_cursor != 0) {
            return Err("History cursor is out of range".into());
        }
        let Some(turn) = turns.get(cursor) else {
            return Ok(
                json!({"node_id":self.target.0.to_string(),"messages":[],"generation":token,"next_cursor":null,"public_only":true}),
            );
        };
        let end =
            super::text_page::utf8_page_end(&turn.text, text_cursor, self.integer("limit", 4096)?)?;
        let next = if end < turn.text.len() {
            Some((cursor, end))
        } else if cursor + 1 < turns.len() {
            Some((cursor + 1, 0))
        } else {
            None
        };
        Ok(
            json!({"node_id":self.target.0.to_string(),"messages":[{"role":turn.role,"text":&turn.text[text_cursor..end],"at":turn.at}],"generation":token,"next_cursor":next.map(|v|v.0),"next_text_cursor":next.map(|v|v.1),"public_only":true}),
        )
    }
}
