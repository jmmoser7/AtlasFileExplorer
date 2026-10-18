//! The existing approval command's bounded, observable file delivery.
use super::*;

const MAX_DELIVERIES: usize = 4;

#[derive(Default)]
pub(super) struct ApprovalDelivery {
    pending: HashMap<NodeId, ApprovalIdentity>,
    sent: HashMap<NodeId, ApprovalIdentity>,
    errors: HashMap<NodeId, (ApprovalIdentity, String)>,
    done: Option<(Sender<Delivered>, Receiver<Delivered>)>,
}

#[derive(Clone, PartialEq, Eq)]
struct ApprovalIdentity {
    session: String,
    request: String,
    approval: String,
}

struct Delivered {
    tab: u64,
    node: NodeId,
    identity: ApprovalIdentity,
    result: Result<(), String>,
}

impl ApprovalDelivery {
    fn prune(&mut self, sessions: &HashMap<NodeId, std::sync::Arc<AgentSession>>) {
        let current = |node: &NodeId, id: &ApprovalIdentity| {
            sessions.get(node).is_some_and(|session| {
                session.request == id.request
                    && session
                        .approval
                        .as_ref()
                        .is_some_and(|approval| approval.id == id.approval)
            })
        };
        self.sent.retain(|node, id| current(node, id));
        self.errors.retain(|node, (id, _)| current(node, id));
    }
    fn receive(&mut self, limit: usize) -> Vec<Delivered> {
        let mut completed = Vec::new();
        for _ in 0..limit {
            let Some(done) = self.done.as_ref().and_then(|(_, rx)| rx.try_recv().ok()) else {
                break;
            };
            if self.pending.get(&done.node) != Some(&done.identity) {
                continue;
            }
            self.pending.remove(&done.node);
            match &done.result {
                Ok(()) => {
                    self.sent.insert(done.node, done.identity.clone());
                }
                Err(error) => {
                    self.errors
                        .insert(done.node, (done.identity.clone(), error.clone()));
                }
            }
            completed.push(done);
        }
        completed
    }
}

impl SlateApp {
    fn agent_approval_identity(&self, node: NodeId) -> Option<ApprovalIdentity> {
        let session = self.agents.session(node)?;
        if self
            .agents
            .requests
            .get(&node)
            .is_some_and(|request| request != &session.request)
        {
            return None;
        }
        let approval = session.approval.as_ref()?;
        Some(ApprovalIdentity {
            session: self.agent_session_for(node)?.0,
            request: session.request.clone(),
            approval: approval.id.clone(),
        })
    }

    pub(crate) fn agent_approval_description(&self, node: NodeId) -> Option<String> {
        let identity = self.agent_approval_identity(node)?;
        let approval = self.agents.session(node)?.approval.as_ref()?;
        let delivery = &self.agents.approvals;
        let prefix = if let Some((_, error)) =
            delivery.errors.get(&node).filter(|(id, _)| id == &identity)
        {
            Some(error.as_str())
        } else if delivery.pending.contains_key(&node) {
            Some("Sending your decision…")
        } else if delivery.sent.get(&node) == Some(&identity) {
            Some("Decision sent. Waiting for the harness…")
        } else {
            None
        };
        Some(match prefix {
            Some(prefix) => format!("{prefix}\n\n{}", approval.description),
            None => approval.description.clone(),
        })
    }

    pub(crate) fn agent_approval_interactive(&self, node: NodeId) -> bool {
        let Some(identity) = self.agent_approval_identity(node) else {
            return false;
        };
        !self.agents.approvals.pending.contains_key(&node)
            && self.agents.approvals.sent.get(&node) != Some(&identity)
    }

    pub(crate) fn answer_agent_approval(&mut self, detail: Option<&str>) -> bool {
        let Some((raw, allow)) = detail.and_then(|s| serde_json::from_str::<(u64, bool)>(s).ok())
        else {
            self.toast("This approval command needs the agent and an Allow once or Deny decision.");
            return false;
        };
        let node = NodeId(raw);
        let Some(approval) = self.agents.session(node).and_then(|s| s.approval.clone()) else {
            self.toast("This agent is not waiting for approval.");
            return false;
        };
        let Some(identity) = self.agent_approval_identity(node) else {
            self.toast("This approval belongs to an earlier run. Wait for the current harness.");
            return false;
        };
        if !self.agent_approval_interactive(node) {
            self.toast("Your approval decision is already being delivered. Wait for the harness.");
            return false;
        }
        let Some(ws) = self.ai.config.workspace_dir.as_ref() else {
            self.toast("Set an AI workspace before answering this approval.");
            return false;
        };
        let Some(dir) = self.agent_link_dir(node, ws) else {
            self.toast("Reconnect this agent's source before answering its approval.");
            return false;
        };
        let active = self.agents.approvals.pending.len()
            + self
                .agents
                .life
                .parked
                .values()
                .map(|runtime| runtime.approvals.pending.len())
                .sum::<usize>();
        if active >= MAX_DELIVERIES {
            self.toast("Approval delivery is busy. Try Allow once or Deny again shortly.");
            return false;
        }
        let tab = self.tab().id;
        let delivery = &mut self.agents.approvals;
        delivery.errors.remove(&node);
        delivery.pending.insert(node, identity.clone());
        let tx = delivery.done.get_or_insert_with(unbounded).0.clone();
        std::thread::spawn(move || {
            let path = dir.join("approval.json");
            let result = atlas_ai::agent::atomic_write_json(
                &path,
                &serde_json::json!({"id":approval.id,"allow":allow}),
            )
            .map_err(|error| {
                format!(
                    "Could not deliver approval to {}: {error}. Try Allow once or Deny again.",
                    path.display()
                )
            });
            let _ = tx.send(Delivered {
                tab,
                node,
                identity,
                result,
            });
        });
        self.toast("Sending approval decision…");
        true
    }

    pub(super) fn pump_agent_approval_deliveries(&mut self, ctx: &egui::Context) {
        self.agents.approvals.prune(&self.agents.sessions);
        let mut completed = self.agents.approvals.receive(MAX_DELIVERIES);
        for parked in self.agents.life.parked.values_mut() {
            parked.approvals.prune(&parked.sessions);
            if completed.len() >= MAX_DELIVERIES {
                break;
            }
            completed.extend(parked.approvals.receive(MAX_DELIVERIES - completed.len()));
        }
        for delivered in completed {
            let current = self
                .tabs
                .iter()
                .find(|tab| tab.id == delivered.tab)
                .and_then(|tab| tab.doc.scene.node(delivered.node))
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|agent| agent.session == delivered.identity.session);
            let runtime = if self.tab().id == delivered.tab {
                Some(&self.agents)
            } else {
                self.agents.life.parked.get(&delivered.tab)
            };
            let same_request = runtime
                .and_then(|runtime| runtime.sessions.get(&delivered.node))
                .is_some_and(|session| session.request == delivered.identity.request);
            if current && same_request {
                self.toast(match delivered.result {
                    Ok(()) => "Approval decision sent. Waiting for the harness.".into(),
                    Err(error) => error,
                });
            }
            ctx.request_repaint();
        }
        if !self.agents.approvals.pending.is_empty()
            || self
                .agents
                .life
                .parked
                .values()
                .any(|runtime| !runtime.approvals.pending.is_empty())
        {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }

    #[cfg(test)]
    pub(crate) fn wait_agent_approval_delivery_for_test(&mut self) {
        let (tx, rx) = self
            .agents
            .approvals
            .done
            .as_ref()
            .expect("an approval was queued");
        let completed = rx.recv().expect("the approval worker reports completion");
        // Keep state changes and notifications on the production pump path.
        tx.send(completed)
            .expect("the approval completion queue remains open");
    }
}
