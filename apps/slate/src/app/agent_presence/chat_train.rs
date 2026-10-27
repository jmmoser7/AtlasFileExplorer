//! The chat train is a representation of its real provider session. Its board
//! tools reuse membership, grants, witnessed actions, and reply transport.
use super::*;
use slate_doc::scene::{NodeId, NodeKind};
use std::path::Path;

const NATIVE_COMMANDS: [&str; 3] = [
    "board.model.connector.inspect",
    "board.model.connector.capture",
    "board.model.connector.invoke",
];
const WIRED_INPUT_READ_COMMANDS: [&str; 2] = ["presence.read_image", "presence.read_text"];

#[derive(Clone, PartialEq, Eq)]
struct WiredSource {
    wire: NodeId,
    source: NodeId,
    target: NodeId,
    path: PathBuf,
    native: bool,
}

pub(super) struct TrainBinding {
    portal: NodeId,
    tab: u64,
    wires: Vec<WiredSource>,
    checked_generation: (u64, u64),
}

impl SlateApp {
    fn chat_membership_can_run(&self, session: &str) -> bool {
        self.presence.agents.iter().any(|a| {
            a.profile.id == session
                && a.tab == self.tab().id
                && !a.paused
                && a.departure.is_none()
                && a.grant.is_some()
        })
    }

    pub(crate) fn validate_chat_train_send(&self, portal: NodeId) -> Result<(), String> {
        if let Some((session, provider)) = self.agent_session_for(portal) {
            if provider == "codex"
                && self
                    .chat_train_wires(&session)
                    .iter()
                    .any(|wire| wire.native)
                && !self.chat_membership_can_run(&session)
            {
                return Err("Open this chat's Board access and explicitly Resume its membership before using native tools".into());
            }
        }
        Ok(())
    }
    fn chat_train_wires(&self, session: &str) -> Vec<WiredSource> {
        let cards: HashSet<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|node| {
                slate_doc::agent_chat::agent(node)
                    .filter(|agent| agent.session == session && agent.provider == "codex")
                    .map(|_| node.id)
            })
            .collect();
        let mut wires: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|node| {
                let NodeKind::Connector(connector) = &node.kind else {
                    return None;
                };
                let binding = connector.binding.as_ref()?;
                if !matches!(
                    binding.kind,
                    slate_doc::agent_inputs::InputKind::Images
                        | slate_doc::agent_inputs::InputKind::Text
                ) {
                    return None;
                }
                let source = slate_doc::agent_inputs::endpoint_node(binding.source_end(connector))?;
                let target = slate_doc::agent_inputs::endpoint_node(binding.target_end(connector))?;
                if !cards.contains(&target) {
                    return None;
                }
                let path = self.image_item_path(source)?;
                let native = self.application_connector_eligible(source);
                let csv = path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"));
                if !native && !csv && slate_doc::media_kind(&path) != slate_doc::MediaKind::Image {
                    return None;
                }
                Some(WiredSource {
                    wire: node.id,
                    source,
                    target,
                    path,
                    native,
                })
            })
            .collect();
        wires.sort_by_key(|wire| wire.wire);
        wires
    }

    pub(in crate::app::agent_presence) fn native_explicit_inputs(
        &self,
        who: &str,
        model: NodeId,
    ) -> Vec<(NodeId, NodeId, PathBuf)> {
        if self.is_chat_train_identity(who) {
            return self
                .chat_train_wires(who)
                .into_iter()
                .map(|wire| (wire.wire, wire.source, wire.path))
                .collect();
        }
        self.doc()
            .scene
            .nodes
            .iter()
            .filter_map(|node| {
                let NodeKind::Connector(connector) = &node.kind else {
                    return None;
                };
                let binding = connector.binding.as_ref()?;
                if !matches!(
                    binding.kind,
                    slate_doc::agent_inputs::InputKind::Images
                        | slate_doc::agent_inputs::InputKind::Text
                ) || slate_doc::agent_inputs::endpoint_node(binding.target_end(connector))
                    != Some(model)
                {
                    return None;
                }
                let source = slate_doc::agent_inputs::endpoint_node(binding.source_end(connector))?;
                Some((node.id, source, self.image_item_path(source)?))
            })
            .collect()
    }

    fn ensure_chat_train_membership(
        &mut self,
        portal: NodeId,
        workspace: &Path,
        explicit_human_access: bool,
    ) -> Result<String, String> {
        let (session, provider) = self
            .agent_session_for(portal)
            .ok_or("Choose a chat train first")?;
        if provider != "codex" {
            return Err("Native chat-train tools require the Codex harness".into());
        }
        let existing = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == session);
        if !explicit_human_access && !self.chat_membership_can_run(&session) {
            return Err("Open this chat's Board access and explicitly Resume its membership before using native tools".into());
        }
        if explicit_human_access
            && existing.is_some_and(|a| a.grant.is_none() && a.departure.is_none())
        {
            self.retire_presence(&session, chrome::RANDOM_DEPARTURE)?;
        }
        let wires = self.chat_train_wires(&session);
        if !wires.iter().any(|wire| wire.native) {
            return Err(
                "Wire a Rhino model into this chat before granting native board access".into(),
            );
        }
        if !self
            .presence
            .agents
            .iter()
            .any(|a| a.profile.id == session && a.departure.is_none())
        {
            self.join_scoped_presence_at(
                StoredAgent {
                    id: session.clone(),
                    name: "Codex chat train".into(),
                    hat: "cap".into(),
                    provider,
                    retired: false,
                },
                None,
                workspace.to_path_buf(),
            )?;
        }
        let dir = self
            .agent_link_dir(portal, workspace)
            .ok_or("The chat train has no runtime folder")?;
        let tab = self.tab().id;
        let a = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == session)
            .unwrap();
        if a.tab != tab {
            return Err("This chat membership belongs to another open board".into());
        }
        if a.dir != dir {
            self.presence.links.revoke(&a.dir);
            a.dir = dir;
        }
        a.workspace = workspace.to_path_buf();
        let grant = a
            .grant
            .as_mut()
            .ok_or("This chat's board grant was revoked; reopen Board access")?;
        grant.readable_board = false;
        grant.readable_nodes = wires
            .iter()
            .flat_map(|wire| [wire.source.0.to_string(), wire.target.0.to_string()])
            .collect();
        grant.readable_nodes.sort();
        grant.readable_nodes.dedup();
        if !grant.readable_nodes.contains(&portal.0.to_string()) {
            grant.readable_nodes.push(portal.0.to_string());
        }
        grant.commands = NATIVE_COMMANDS
            .iter()
            .chain(WIRED_INPUT_READ_COMMANDS.iter())
            .map(|command| (*command).into())
            .collect();
        grant.conversation_ids.clear();
        grant.helper_limit = 0;
        let checked_generation = (self.scene_gen, self.doc().scene.scene_gen());
        self.presence.chat_trains.insert(
            session.clone(),
            TrainBinding {
                portal,
                tab,
                wires,
                checked_generation,
            },
        );
        Ok(session)
    }

    pub(crate) fn open_chat_train_board_access(&mut self) -> bool {
        let Some(portal) = self.selected_agent_portal() else {
            return false;
        };
        let Some(workspace) = self
            .agent_folder_for(portal)
            .or_else(|| self.ai.config.valid_workspace().map(Path::to_path_buf))
        else {
            self.toast("Choose an AI workspace before granting board access.");
            return true;
        };
        match self.ensure_chat_train_membership(portal, &workspace, true) {
            Ok(who) => self.presence.chrome.open_agent_settings(&who),
            Err(error) => self.toast(error),
        }
        true
    }

    pub(crate) fn bind_chat_train_request(
        &mut self,
        portal: NodeId,
        workspace: &Path,
        request: &str,
    ) -> Result<Option<BoardLinkSession>, String> {
        let Some((session, provider)) = self.agent_session_for(portal) else {
            return Ok(None);
        };
        if provider != "codex"
            || !self
                .chat_train_wires(&session)
                .iter()
                .any(|wire| wire.native)
        {
            return Ok(None);
        }
        let who = self.ensure_chat_train_membership(portal, workspace, false)?;
        let readonly = self.crosstalk_policy(&session) == atlas_agent::TurnPolicy::ReadOnly;
        let a = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == who)
            .unwrap();
        if a.board_action_active() {
            return Err("Wait for this chat's pending native action before sending again".into());
        }
        let mut grant = a
            .grant
            .clone()
            .ok_or("This chat's board grant was revoked")?;
        if readonly {
            grant.mutation_autonomy = false;
        }
        self.presence
            .links
            .set_session(&a.dir, a.binding.clone(), grant.clone(), request.into())
            .map_err(str::to_owned)?;
        self.presence
            .roster
            .session_mut(&a.motion)
            .ok_or("Chat membership ended")?
            .begin_request(id(request)?)
            .map_err(|error| error.to_string())?;
        a.grant = Some(grant);
        a.request = request.into();
        a.calls = 0;
        a.sequences.clear();
        a.control_epoch = a.control_epoch.wrapping_add(1);
        a.inflight = true;
        a.seen_running = false;
        a.had_response = false;
        a.launched = vec![(portal, request.into())];
        a.status = "Working in chat train".into();
        a.activity = "reasoning".into();
        self.presence.sources.expect(&a.dir, request);
        Ok(Some(a.binding.clone()))
    }

    /// Fence the membership before cancelling its existing provider owner.
    pub(crate) fn revoke_chat_train_request(&mut self, portal: NodeId, request: &str) {
        let who = self
            .presence
            .chat_trains
            .iter()
            .find(|(_, binding)| binding.portal == portal)
            .map(|(who, _)| who.clone());
        let Some(who) = who else {
            return;
        };
        let Some(a) = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == who && a.request == request)
        else {
            return;
        };
        a.launched.clear();
        self.presence.chat_trains.remove(&who);
        let _ = self.stop_presence(&who, false);
    }

    pub(super) fn maintain_chat_train_memberships(&mut self) {
        let generation = (self.scene_gen, self.doc().scene.scene_gen());
        let stale: Vec<_> = self
            .presence
            .chat_trains
            .iter()
            .filter_map(|(who, binding)| {
                if binding.tab == self.tab().id && binding.checked_generation == generation {
                    return None;
                }
                let valid = binding.tab == self.tab().id
                    && self
                        .agent_session_for(binding.portal)
                        .is_some_and(|(session, provider)| session == *who && provider == "codex")
                    && self.chat_train_wires(who) == binding.wires;
                (!valid).then(|| who.clone())
            })
            .collect();
        for who in stale {
            self.presence.chat_trains.remove(&who);
            let _ = self.stop_presence(&who, false);
        }
        for binding in self.presence.chat_trains.values_mut() {
            binding.checked_generation = generation;
        }
    }

    pub(super) fn is_chat_train_identity(&self, who: &str) -> bool {
        self.doc().scene.nodes.iter().any(|node| {
            slate_doc::agent_chat::agent(node)
                .is_some_and(|agent| agent.session == who && agent.provider == "codex")
        })
    }

    /// Source-state reconciliation follows only this live membership's actual
    /// request. The provider locator may change; local identity and grants do not.
    pub(crate) fn sync_chat_train_provider_channel(
        &mut self,
        portal: NodeId,
        state: &atlas_agent::AgentSession,
    ) {
        let Some((who, provider)) = self.agent_session_for(portal) else {
            return;
        };
        if provider != "codex"
            || state.provider != "codex"
            || state.conversation.is_empty()
            || state.request.is_empty()
        {
            return;
        }
        if !self
            .presence
            .chat_trains
            .get(&who)
            .is_some_and(|binding| binding.portal == portal && binding.tab == self.tab().id)
            || !self.presence.agents.iter().any(|actor| {
                actor.profile.id == who
                    && actor.tab == self.tab().id
                    && actor.departure.is_none()
                    && actor.grant.is_some()
                    && actor.request == state.request
            })
        {
            return;
        }
        let changed: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|node| {
                slate_doc::agent_chat::agent(node)
                    .filter(|agent| {
                        agent.session == who
                            && agent.provider == "codex"
                            && agent
                                .channel
                                .as_ref()
                                .is_some_and(|channel| channel != &state.conversation)
                    })
                    .map(|_| node.id)
            })
            .collect();
        if !changed.is_empty() {
            self.patch_nodes(&changed, |node| {
                if let Some(agent) = slate_doc::agent_chat::agent_mut(node) {
                    agent.channel = Some(state.conversation.clone());
                }
            });
        }
    }
}

#[cfg(test)]
mod tests;
