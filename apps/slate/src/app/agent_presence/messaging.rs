use super::*;
use serde_json::{json, Value};
use slate_doc::scene::{NodeKind, SceneCmd};

pub(crate) fn message_text_changed(
    before: Option<&slate_doc::Node>,
    after: &slate_doc::Node,
) -> bool {
    let NodeKind::Text(text) = &after.kind else {
        return false;
    };
    !matches!(before.map(|n| &n.kind), Some(NodeKind::Text(old)) if old.text == text.text)
}

impl SlateApp {
    pub(super) fn presence_command_schema(&self) -> Value {
        let mut schema = Self::targeted_command_schema();
        schema["presence.send"] = json!({"targets":[],"arguments":{"recipient":"one permitted conversation ID","text":"message; <=16384 bytes"}});
        schema["presence.helper"] = json!({"targets":[],"arguments":{"task":"bounded helper task; <=8192 bytes"},"limits":"at most4 children, no grandchildren,300seconds; read participant results"});
        schema["presence.read_image"] = json!({"targets":["one readable image node ID"],"arguments":{},"requires":"human wired-file input grant; returns one prepared image_source locator, never hydrates cloud-only sources"});
        schema
    }
    pub(super) fn presence_conversations(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        self.doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| slate_doc::agent_chat::agent(n).map(|a| a.session.clone()))
            .chain(
                self.presence
                    .agents
                    .iter()
                    .filter(|a| a.tab == self.tab().id && a.departure.is_none())
                    .map(|a| a.profile.id.clone()),
            )
            .filter(|id| seen.insert(id.clone()))
            .take(129)
            .collect()
    }
    pub(super) fn relay_presence_message(
        &mut self,
        from: &mut Collaborator,
        targets: &[String],
        arguments: &Value,
    ) -> Result<Value, String> {
        if !targets.is_empty() {
            return Err(
                "Conversation recipients belong in arguments.recipient, not board node targets"
                    .into(),
            );
        }
        let target = arguments["recipient"]
            .as_str()
            .ok_or("Choose one explicitly permitted conversation")?;
        let grant = from.grant.as_ref().ok_or("Grant revoked")?;
        if !grant.conversation_ids.iter().any(|id| id == target) {
            return Err("Conversation is outside the collaborator grant".into());
        }
        let text = arguments["text"]
            .as_str()
            .filter(|s| !s.trim().is_empty() && s.len() <= 16_384)
            .ok_or("Supply a bounded message")?;
        if from.relays >= 32 {
            return Err(
                "This session reached its 32-message relay limit; start a fresh human task".into(),
            );
        }
        if let Some(recipient) = self.presence.agents.iter().find(|a| a.profile.id == target) {
            if recipient.relays >= 32 {
                return Err("Recipient reached its relay limit".into());
            }
            self.send_presence_message_from(target, text, Some(&from.profile.name))?;
            if let Some(recipient) = self
                .presence
                .agents
                .iter_mut()
                .find(|a| a.profile.id == target)
            {
                recipient.relays += 1;
            }
        } else {
            let portal = self
                .doc()
                .scene
                .nodes
                .iter()
                .rev()
                .find(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.session == target))
                .map(|n| n.id)
                .ok_or("Conversation no longer exists")?;
            let inputs = self.agent_input_snapshot(portal)?;
            if inputs.context.iter().chain(&inputs.wired).any(|item| {
                !grant.allows_node(&item.node.to_string())
                    || ((!item.images.is_empty() || item.depth.is_some())
                        && !grant
                            .readable_sources
                            .iter()
                            .any(|s| s == "board-file-inputs"))
            }) {
                return Err("This conversation's inputs exceed the collaborator grant".into());
            }
            self.send_scoped_collaborator_relay(portal, &from.profile.name, text)?;
        }
        from.relays += 1;
        from.activity = "exchanging".into();
        self.push_history_as(
            atlas_commands::CommandId("presence.send"),
            Some(format!("to {target}")),
            atlas_commands::CmdAuthor::Agent(from.profile.name.clone()),
        );
        Ok(json!({"status":"sent","recipient":target}))
    }
    /// Human-authored message cards can address a named board collaborator.
    /// Edits made by providers never auto-route more messages.
    pub(crate) fn route_presence_mentions(&mut self, cmds: &[SceneCmd]) {
        let texts: Vec<_> = cmds
            .iter()
            .filter_map(|cmd| match cmd {
                SceneCmd::Add { node, .. } => Some(node),
                SceneCmd::Patch { after, .. } => Some(after.as_ref()),
                _ => None,
            })
            .filter_map(|node| match &node.kind {
                NodeKind::Text(t) => Some((node.id, t.text.clone())),
                _ => None,
            })
            .collect();
        for (node, text) in texts {
            use std::hash::{Hash, Hasher};
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut hash);
            let hash = hash.finish();
            if self.presence.mention_hashes.get(&node) == Some(&hash) {
                continue;
            }
            self.presence.mention_hashes.insert(node, hash);
            if self.presence.mention_hashes.len() > 1024 {
                self.presence.mention_hashes.clear();
            }
            let roster: Vec<_> = self
                .presence
                .agents
                .iter()
                .filter(|a| a.tab == self.tab().id && a.departure.is_none())
                .map(|a| (a.profile.id.as_str(), a.profile.name.as_str()))
                .collect();
            match resolve_mention(&text, &roster) {
                Ok(Some((who, message))) => {
                    let who = who.to_owned();
                    let message = message.to_owned();
                    if let Err(e) = self.send_presence_message(&who, &message) {
                        self.toast(e);
                    }
                }
                Err(e) => self.toast(e),
                _ => {}
            }
        }
    }
}

/// Longest exact display-name prefix; duplicate names require the stable @id.
pub(super) fn resolve_mention<'a>(
    text: &'a str,
    roster: &[(&'a str, &str)],
) -> Result<Option<(&'a str, &'a str)>, String> {
    let Some(rest) = text.trim_start().strip_prefix('@') else {
        return Ok(None);
    };
    let matches: Vec<_> = roster
        .iter()
        .filter_map(|(id, name)| {
            [*id, *name].into_iter().find_map(|name| {
                let prefix = rest.get(..name.len())?;
                if !prefix.eq_ignore_ascii_case(name) {
                    return None;
                }
                let tail = &rest[name.len()..];
                if !tail.starts_with(|c: char| c.is_whitespace() || c == ':') {
                    return None;
                }
                Some((
                    *id,
                    tail.trim_start_matches(|c: char| c.is_whitespace() || c == ':'),
                    name.len(),
                ))
            })
        })
        .collect();
    let longest = matches.iter().map(|m| m.2).max();
    let best: Vec<_> = matches.iter().filter(|m| Some(m.2) == longest).collect();
    match best.as_slice() {
        [] => Ok(None),
        [one] => {
            if one.1.is_empty() {
                Ok(None)
            } else {
                Ok(Some((one.0, one.1)))
            }
        }
        _ => Err(
            "Several collaborators share this name; address the stable @id from its capsule".into(),
        ),
    }
}
