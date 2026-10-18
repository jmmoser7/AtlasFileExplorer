use super::*;
#[path = "messaging_relay.rs"]
mod relay;
#[cfg(test)]
#[path = "messaging_relay_tests.rs"]
mod relay_tests;
pub(super) use relay::PreparedRelay;
#[cfg(test)]
#[path = "messaging_mention_tests.rs"]
mod mention_tests;
#[path = "messaging_mentions.rs"]
mod mentions;
pub(super) use mentions::MentionQueue;
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
                NodeKind::Text(t) if t.text.len() <= submission::MESSAGE_BYTES => {
                    Some((node.id, t.text.clone()))
                }
                _ => None,
            })
            .collect();
        for (node, text) in texts {
            let hash = mentions::text_hash(&text);
            if self.presence.mention_hashes.get(&node) == Some(&hash) {
                continue;
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
                    match self.queue_presence_mention(&who, node, hash, &message) {
                        Ok(()) => {
                            if self.presence.mention_hashes.len() >= 1024 {
                                self.presence.mention_hashes.clear();
                            }
                            self.presence.mention_hashes.insert(node, hash);
                        }
                        Err(e) => self.toast(e),
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
