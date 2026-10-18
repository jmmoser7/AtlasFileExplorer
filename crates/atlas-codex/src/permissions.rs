use super::*;

/// Native collaborators change the board through host tools only. Their model
/// filesystem sandbox is independent of the board command/autonomy grant.
pub fn filesystem_policy(request: &AgentRequest) -> TurnPolicy {
    if request.collaborator.is_some() {
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
    fn collaborator_filesystem_is_read_only_even_when_provider_settings_allow_writes() {
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
                let mut permissions = TurnPermissions::default();
                permissions.own = Some(Permissions {
                    approval: json!("never"),
                    sandbox: json!({"type":configured}),
                });
                let filesystem = filesystem_policy(&request);
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
                assert_eq!(params["sandboxPolicy"], json!({"type":"readOnly"}));
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
    fn ordinary_chat_keeps_its_existing_provider_permission_behavior() {
        for policy in [TurnPolicy::Default, TurnPolicy::ReadOnly] {
            let request = AgentRequest {
                policy,
                ..Default::default()
            };
            assert_eq!(filesystem_policy(&request), policy);
        }
        let request = AgentRequest::default();
        let params = turn_params(
            "t",
            vec![],
            "r",
            None,
            filesystem_policy(&request),
            false,
            None,
        );
        assert!(params.get("sandboxPolicy").is_none());
        let full = turn_params(
            "t",
            vec![],
            "r",
            None,
            filesystem_policy(&request),
            true,
            None,
        );
        assert_eq!(full["sandboxPolicy"]["type"], "dangerFullAccess");
    }
}
