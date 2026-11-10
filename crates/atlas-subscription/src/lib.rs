//! Subscription CLI protocols stay outside the application and its renderer.
mod catalog;
mod claude_cli;
mod cursor_acp;
mod discovery;
pub mod host_tools;
mod payload;
mod process;
mod progress;
mod tool_client;
pub mod vision;
use atlas_agent::{AgentModel, AgentRequest, AgentSession, AgentStatus, AgentTurn};
pub use discovery::{discover, executable, AuthStatus, Discovery, Provider};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub fn provider(id: &str) -> Option<Provider> {
    match id {
        "cursor-cli" => Some(Provider::Cursor),
        "claude-code" => Some(Provider::ClaudeCode),
        _ => None,
    }
}
pub fn available(provider: Provider) -> bool {
    executable(provider).is_some()
}

/// No guessed model IDs or parameters: empty ID means the vendor's account default.
pub fn models(provider: Provider) -> Result<Vec<AgentModel>, String> {
    let found = discover(provider)?;
    match found.auth {
        AuthStatus::LoggedIn => {
            let cwd=std::env::temp_dir().join("slate-subscription-catalog");
            std::fs::create_dir_all(&cwd).map_err(|error|error.to_string())?;
            match provider {Provider::Cursor=>cursor_acp::models(&found.executable,&cwd),Provider::ClaudeCode=>claude_cli::models(&found.executable,&cwd)}
        },
        AuthStatus::LoginRequired => Err(format!("Sign in to your {} subscription with {}, then refresh.", provider.label(), provider.login_command())),
        AuthStatus::Unknown => Err(format!("Could not verify your {} subscription sign-in. Run {} and refresh; update the native CLI if necessary.", provider.label(), provider.login_command())),
    }
}

pub struct Client {
    provider: Provider,
    executable: PathBuf,
    cwd: PathBuf,
    link_dir: PathBuf,
    cloud_guard: Option<vision::CloudGuard>,
}
impl Client {
    pub fn start(provider: Provider, dir: &Path, cwd: &Path) -> Result<Self, String> {
        if !cwd.is_dir() {
            return Err("The subscription agent's working folder is unavailable.".into());
        }
        let found = discover(provider)?;
        if found.auth != AuthStatus::LoggedIn {
            return Err(format!(
                "Sign in to your {} subscription with {}, then refresh.",
                provider.label(),
                provider.login_command()
            ));
        }
        let executable = found.executable;
        Ok(Self {
            provider,
            executable,
            cwd: cwd.into(),
            link_dir: dir.into(),
            cloud_guard: None,
        })
    }
    pub fn run(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        stop: &AtomicBool,
        changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        self.run_with_tools(request, session, stop, None, changed)
    }
    pub fn set_cloud_guard(&mut self, guard: vision::CloudGuard) {
        self.cloud_guard = Some(guard);
    }
    pub fn run_with_tools(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        stop: &AtomicBool,
        tools: Option<&atlas_agent::collaborator::HostToolsLaunch>,
        changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        let helper = tools.cloned().map(atlas_agent::helper::HelperLaunch::Model);
        self.run_with_helper(request, session, stop, helper.as_ref(), changed)
    }
    pub fn run_with_helper(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        stop: &AtomicBool,
        tools: Option<&atlas_agent::helper::HelperLaunch>,
        mut changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        validate_request_shape(request, tools)?;
        if stop.load(Ordering::Relaxed) {
            return Err("Response stopped.".into());
        }
        session.provider = self.provider.id().into();
        session.request = request.id.clone();
        session.conversation.clear();
        session.status = AgentStatus::Thinking;
        session.updated_at = request.at;
        session.turns.push(AgentTurn {
            role: "user".into(),
            text: request.prompt.clone(),
            at: request.at,
        });
        let answer = session.turns.len();
        session.turns.push(AgentTurn {
            role: "assistant".into(),
            text: String::new(),
            at: request.at,
        });
        changed(session);
        // The task renderer preserves each canonical source/history projection.
        let mut payload = payload::TextPayload::new(request, self.provider, &self.link_dir);
        payload.capture_images(request, self.cloud_guard)?;
        let result = match self.provider {
            Provider::Cursor => cursor_acp::run(
                &self.executable,
                &self.cwd,
                request,
                tools,
                &mut payload,
                session,
                answer,
                stop,
                &mut changed,
            ),
            Provider::ClaudeCode => claude_cli::run(
                &self.executable,
                &self.cwd,
                request,
                tools,
                &mut payload,
                session,
                answer,
                stop,
                &mut changed,
            ),
        };
        let result = result.map_err(|error| match tools {
            Some(atlas_agent::helper::HelperLaunch::Artifact(scope))
                if error == process::RESPONSE_TIMEOUT && scope.remaining_time().is_err() =>
            {
                // A shorter startup/protocol timeout is still a connection error.
                // Only the actual admitted scope expiry gets a task deadline cause.
                format!(
                    "Artifact generation reached its captured {}-second deadline. Its staged draft was not published.",
                    atlas_agent::collaborator::tool_task_budget_seconds(request)
                )
            }
            _ => error,
        });
        payload.terminal(
            result.is_ok() && (tools.is_some() || !session.turns[answer].text.trim().is_empty()),
        );
        match result {
            Ok(()) if tools.is_some() || !session.turns[answer].text.trim().is_empty() => {
                session.status = AgentStatus::Idle;
                changed(session);
                Ok(())
            }
            Ok(()) => Err("The subscription provider completed without a text answer.".into()),
            Err(error) => Err(error),
        }
    }
}

/// Task shape and helper identity are checked before any paid model prompt.
fn validate_request_shape(
    request: &AgentRequest,
    tools: Option<&atlas_agent::helper::HelperLaunch>,
) -> Result<(), String> {
    if let Some(tools) = tools {
        tools.validate_for(request)?;
    }
    if request.image.is_some() || request.goal.is_some() {
        return Err("This subscription connection supports Text, native Model and Dashboard tasks; image generation is unavailable.".into());
    }
    if request.artifact.is_some()
        && !matches!(tools, Some(atlas_agent::helper::HelperLaunch::Artifact(_)))
    {
        return Err("Dashboard output requires its exact staged artifact file helper.".into());
    }
    request.validate_artifact_request().map_err(str::to_owned)?;
    if request.collaborator.is_some()
        && !matches!(tools, Some(atlas_agent::helper::HelperLaunch::Model(_)))
    {
        return Err("This native Model task has no exact host tool binding.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// No user turn or tool call: validates real CLI initialization against an explicit helper.
pub fn probe_claude_host_tools(
    cwd: &Path,
    request: &AgentRequest,
    tools: &atlas_agent::collaborator::HostToolsLaunch,
) -> Result<serde_json::Value, String> {
    tools.validate_for(request)?;
    let found = discover(Provider::ClaudeCode)?;
    if found.auth != AuthStatus::LoggedIn {
        return Err("Claude subscription sign-in required".into());
    }
    claude_cli::probe_tools(
        &found.executable,
        cwd,
        &atlas_agent::helper::HelperLaunch::Model(tools.clone()),
    )
}

#[cfg(test)]
mod tool_tests;
