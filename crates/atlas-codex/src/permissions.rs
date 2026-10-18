use super::*;

/// `provider_full_access` is resolved by atlas-ai::access, never by a board grant
/// or model-authored payload. Explicit read-only turns always remain read-only.
pub fn filesystem_policy(request: &AgentRequest, provider_full_access: bool) -> TurnPolicy {
    if request.collaborator.is_some() && !provider_full_access {
        TurnPolicy::ReadOnly
    } else {
        request.policy
    }
}
pub fn thread_params(cwd: &Path, thread: Option<&str>) -> Value {
    // The installed provider owns workspace permissions and project instructions.
    // Attaching must not replace them with Slate's former read-only persona.
    let mut p = json!({"cwd":cwd});
    if let Some(id) = thread {
        p["threadId"] = json!(id);
        p["excludeTurns"] = json!(true);
    }
    p
}

/// Without a grant the provider's own approval and sandbox settings apply.
/// A read-only turn overrides the grant. `restore` puts the thread's own
/// settings back on a turn that follows an override ([`Permissions`]).
pub fn turn_params(
    thread: &str,
    input: Vec<Value>,
    request: &str,
    model: Option<&str>,
    policy: TurnPolicy,
    full_access: bool,
    restore: Option<&Permissions>,
) -> Value {
    let mut p =
        json!({"threadId":thread,"input":input,"clientUserMessageId":request,"model":model});
    if let Some(sandbox) = turn_override(policy, full_access) {
        p["approvalPolicy"] = json!("never");
        p["sandboxPolicy"] = json!({ "type": sandbox });
    } else if let Some(own) = restore {
        p["approvalPolicy"] = own.approval.clone();
        p["sandboxPolicy"] = own.sandbox.clone();
    }
    p
}

fn turn_override(policy: TurnPolicy, full_access: bool) -> Option<&'static str> {
    match policy {
        TurnPolicy::ReadOnly => Some("readOnly"),
        TurnPolicy::Default if full_access => Some("dangerFullAccess"),
        TurnPolicy::Default => None,
    }
}

/// A thread's own approval and sandbox settings, as `thread/start` or
/// `thread/resume` reported them. Codex keeps a turn's overrides for the
/// turns after it, so a turn that should run under the provider's settings
/// after a read-only or full-access turn sends these back explicitly.
#[derive(Clone, Debug, PartialEq)]
pub struct Permissions {
    pub approval: Value,
    pub sandbox: Value,
}

impl Permissions {
    pub fn from_thread_response(response: &Value) -> Option<Self> {
        let approval = response.get("approvalPolicy").filter(|v| !v.is_null())?;
        let sandbox = match response.get("sandbox")? {
            Value::String(mode) => json!({ "type": match mode.as_str() {
                "read-only" => "readOnly",
                "workspace-write" => "workspaceWrite",
                "danger-full-access" => "dangerFullAccess",
                other => other,
            } }),
            object @ Value::Object(_) => object.clone(),
            _ => return None,
        };
        Some(Self {
            approval: approval.clone(),
            sandbox,
        })
    }

    /// Codex's own defaults, for a thread that did not report its settings:
    /// ask before acting, write only inside the workspace.
    pub fn provider_default() -> Self {
        Self {
            approval: json!("on-request"),
            sandbox: json!({ "type": "workspaceWrite" }),
        }
    }
}

/// Which settings each turn of one thread sends. Only a turn after an
/// override restores; a thread that was never overridden sends nothing.
#[derive(Clone, Debug, Default)]
pub struct TurnPermissions {
    pub own: Option<Permissions>,
    overridden: bool,
}

impl TurnPermissions {
    /// The `restore` argument of [`turn_params`] for the next turn.
    pub fn next(&mut self, policy: TurnPolicy, full_access: bool) -> Option<Permissions> {
        let overrides = turn_override(policy, full_access).is_some();
        let restore = (!overrides && self.overridden).then(|| {
            self.own
                .clone()
                .unwrap_or_else(Permissions::provider_default)
        });
        self.overridden = overrides;
        restore
    }
}

#[cfg(test)]
mod board_permission_tests {
    use super::*;
    #[test]
    fn collaborator_filesystem_requires_separate_trusted_provider_permission() {
        let request = AgentRequest {
            collaborator: Some(atlas_agent::collaborator::BoardLinkSession {
                session_id: "s".into(),
                grant_id: "g".into(),
                workspace_id: "w".into(),
            }),
            ..Default::default()
        };
        for configured in ["workspaceWrite", "dangerFullAccess"] {
            for full_access in [false, true] {
                let mut permissions = TurnPermissions {
                    own: Some(Permissions {
                        approval: json!("never"),
                        sandbox: json!({"type":configured}),
                    }),
                    ..Default::default()
                };
                let filesystem = filesystem_policy(&request, full_access);
                let restore = permissions.next(filesystem, full_access);
                let params = turn_params(
                    "t",
                    vec![],
                    "r",
                    None,
                    filesystem,
                    full_access,
                    restore.as_ref(),
                );
                assert_eq!(
                    params["sandboxPolicy"],
                    json!({"type":if full_access { "dangerFullAccess" } else { "readOnly" }})
                );
                assert_eq!(params["approvalPolicy"], "never");
                assert_eq!(
                    request.policy,
                    TurnPolicy::Default,
                    "board command policy remains distinct from model filesystem permissions"
                );
            }
        }
    }
    #[test]
    fn explicit_read_only_collaborator_turn_overrides_provider_full_access() {
        let request = AgentRequest {
            collaborator: Some(atlas_agent::collaborator::BoardLinkSession {
                session_id: "s".into(),
                grant_id: "g".into(),
                workspace_id: "w".into(),
            }),
            policy: TurnPolicy::ReadOnly,
            ..Default::default()
        };
        assert_eq!(filesystem_policy(&request, true), TurnPolicy::ReadOnly);
        let params = turn_params(
            "t",
            vec![],
            "r",
            None,
            filesystem_policy(&request, true),
            true,
            None,
        );
        assert_eq!(params["sandboxPolicy"]["type"], "readOnly");
    }
    #[test]
    fn ordinary_chat_keeps_its_existing_provider_permission_behavior() {
        for policy in [TurnPolicy::Default, TurnPolicy::ReadOnly] {
            let request = AgentRequest {
                policy,
                ..Default::default()
            };
            assert_eq!(filesystem_policy(&request, false), policy);
            assert_eq!(filesystem_policy(&request, true), policy);
        }
        let request = AgentRequest::default();
        let params = turn_params(
            "t",
            vec![],
            "r",
            None,
            filesystem_policy(&request, false),
            false,
            None,
        );
        assert!(params.get("sandboxPolicy").is_none());
        let full = turn_params(
            "t",
            vec![],
            "r",
            None,
            filesystem_policy(&request, true),
            true,
            None,
        );
        assert_eq!(full["sandboxPolicy"]["type"], "dangerFullAccess");
    }
}
