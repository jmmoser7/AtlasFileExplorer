//! Provider-neutral scoped board command bridge. Grants are machine-local authority.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_BOARD_MESSAGE_BYTES: u64 = 65_536;
pub const MAX_BOARD_TARGETS: usize = 64;
pub const MAX_BOARD_CALLS: usize = 128;
pub const BOARD_LINK_MARKER: &str = "\n\nSlate board collaborator:\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardLinkSession {
    pub session_id: String,
    pub grant_id: String,
    pub workspace_id: String,
}

impl BoardLinkSession {
    pub fn validate(&self) -> Result<(), &'static str> {
        if [&self.session_id, &self.grant_id, &self.workspace_id]
            .iter()
            .all(|id| valid_id(id))
        {
            Ok(())
        } else {
            Err("Invalid board session identity.")
        }
    }
}

/// Created by human UI only. A provider permission is never a board grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaboratorGrant {
    pub id: String,
    pub workspace_id: String,
    pub readable_board: bool,
    #[serde(default)]
    pub readable_nodes: Vec<String>,
    #[serde(default)]
    pub readable_sources: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub conversation_ids: Vec<String>,
    #[serde(default)]
    pub mutation_autonomy: bool,
    #[serde(default)]
    pub helper_limit: u8,
    pub max_actions: u16,
}

impl CollaboratorGrant {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_id(&self.id)
            || !valid_id(&self.workspace_id)
            || self.readable_nodes.len() > 4096
            || self.readable_sources.len() > 128
            || self.commands.len() > 128
            || self.conversation_ids.len() > 128
            || self.helper_limit > 8
            || !(1..=128).contains(&self.max_actions)
            || self
                .readable_nodes
                .iter()
                .chain(&self.readable_sources)
                .chain(&self.commands)
                .chain(&self.conversation_ids)
                .any(|v| !valid_id(v))
            || serde_json::to_vec(self)
                .map_or(true, |v| v.len() as u64 > MAX_BOARD_MESSAGE_BYTES - 2048)
        {
            return Err("Invalid or unbounded board grant.");
        }
        Ok(())
    }

    pub fn allows_node(&self, id: &str) -> bool {
        self.readable_board || self.readable_nodes.iter().any(|allowed| allowed == id)
    }

    pub fn authorize(
        &self,
        call: &BoardCall,
        session: &BoardLinkSession,
        request: &str,
    ) -> Result<(), &'static str> {
        self.validate()?;
        call.validate()?;
        if &call.session != session
            || call.request_id != request
            || self.id != session.grant_id
            || self.workspace_id != session.workspace_id
        {
            return Err("Board session or request is no longer current.");
        }
        if call.sequence >= u64::from(self.max_actions) {
            return Err("This collaborator's task action budget is exhausted.");
        }
        match &call.operation {
            BoardOperation::Read { node_ids, .. } => {
                if node_ids.iter().any(|id| !self.allows_node(id)) {
                    return Err("Board read is outside this collaborator's scope.");
                }
            }
            BoardOperation::Invoke {
                command, targets, ..
            } => {
                if !self.commands.contains(command)
                    || targets.iter().any(|id| !self.allows_node(id))
                {
                    return Err("Board command or target is outside this collaborator's scope.");
                }
            }
        }
        Ok(())
    }

    /// A helper receives a new identity and an explicit subset, never inherited autonomy.
    pub fn allows_helper(&self, child: &Self) -> bool {
        let subset = |a: &[String], b: &[String]| a.iter().all(|v| b.contains(v));
        child.validate().is_ok()
            && self.helper_limit > 0
            && child.id != self.id
            && child.workspace_id == self.workspace_id
            && (!child.readable_board || self.readable_board)
            && (self.readable_board || subset(&child.readable_nodes, &self.readable_nodes))
            && subset(&child.readable_sources, &self.readable_sources)
            && subset(&child.commands, &self.commands)
            && subset(&child.conversation_ids, &self.conversation_ids)
            && !child.mutation_autonomy
            && child.helper_limit == 0
            && child.max_actions <= self.max_actions
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardCall {
    pub id: String,
    pub session: BoardLinkSession,
    pub request_id: String,
    pub sequence: u64,
    pub operation: BoardOperation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BoardOperation {
    /// Empty IDs ask for the grant's readable scope. The host filters every object.
    Read {
        #[serde(default)]
        node_ids: Vec<String>,
        #[serde(default)]
        offset: usize,
        #[serde(default = "default_read_limit")]
        limit: usize,
    },
    Invoke {
        command: String,
        #[serde(default)]
        targets: Vec<String>,
        #[serde(default)]
        arguments: Value,
    },
}

impl BoardCall {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.session.validate()?;
        if !valid_call_id(&self.id) || !valid_id(&self.request_id) || self.sequence >= 128 {
            return Err("Invalid board call identity or action budget.");
        }
        let targets = match &self.operation {
            BoardOperation::Read {
                node_ids,
                offset,
                limit,
            } => {
                if *offset > 1_000_000 || !(1..=64).contains(limit) {
                    return Err("Board read pagination exceeds protocol limits.");
                }
                node_ids
            }
            BoardOperation::Invoke {
                command, targets, ..
            } => {
                if !valid_id(command) {
                    return Err("Invalid registered command.");
                }
                targets
            }
        };
        if targets.len() > MAX_BOARD_TARGETS
            || targets.iter().any(|v| !valid_id(v))
            || serde_json::to_vec(self).map_or(true, |v| v.len() as u64 > MAX_BOARD_MESSAGE_BYTES)
        {
            return Err("Board command exceeds protocol limits.");
        }
        Ok(())
    }
}

fn default_read_limit() -> usize {
    32
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardReply {
    pub id: String,
    pub session_id: String,
    pub request_id: String,
    pub success: bool,
    pub result: Value,
}

impl BoardReply {
    pub fn error(call: &BoardCall, message: &str) -> Self {
        Self {
            id: call.id.clone(),
            session_id: call.session.session_id.clone(),
            request_id: call.request_id.clone(),
            success: false,
            result: serde_json::json!({"error": message}),
        }
    }
    pub fn matches(&self, call: &BoardCall) -> bool {
        self.id == call.id
            && self.session_id == call.session.session_id
            && self.request_id == call.request_id
    }
}

/// Opaque IDs and registered names, not paths or arbitrary shell programs.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
}

pub fn valid_call_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}

/// Only transport instructions and current identity; never board contents.
pub fn board_link_guide(request: &crate::AgentRequest, dir: &str) -> String {
    let Some(link) = &request.collaborator else {
        return String::new();
    };
    let profile = include_str!("../../../docs/agent/cursor-sidecar/board-link-v1.md");
    let profile = profile
        .replace(
            "{{session}}",
            &serde_json::to_string(link).unwrap_or_default(),
        )
        .replace(
            "{{request_id}}",
            &serde_json::to_string(&request.id).unwrap_or_default(),
        )
        .replace(
            "{{link_dir}}",
            &serde_json::to_string(dir).unwrap_or_default(),
        );
    format!("{BOARD_LINK_MARKER}{profile}")
}

#[cfg(test)]
mod tests;
