//! Blocking file transport, called only by the link worker.
use super::*;
use std::{collections::BTreeMap, time::SystemTime};

#[derive(Debug, Default)]
pub(super) struct FileAgentLink {
    last_write_attempt: Option<Instant>,
    last_fingerprint: u64,
    last_read_attempt: Option<Instant>,
    last_session_mtime: Option<SystemTime>,
    readmes: BTreeMap<String, bool>,
}

impl FileAgentLink {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn tick_write_context(
        &mut self,
        ai_workspace: &Path,
        session: &str,
        ctx: &AgentContext,
    ) -> bool {
        self.tick_write_context_in(&agent_dir(ai_workspace, session), ctx)
    }
    pub(super) fn tick_write_context_in(&mut self, dir: &Path, ctx: &AgentContext) -> bool {
        if let Some(t) = self.last_write_attempt {
            if t.elapsed() < WRITE_INTERVAL {
                return false;
            }
        }
        self.last_write_attempt = Some(Instant::now());

        let fp = ctx.fingerprint();
        if fp == self.last_fingerprint {
            return false;
        }
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
        if atomic_write_json(&dir.join("context.json"), ctx).is_err() {
            return false;
        }
        self.last_fingerprint = fp;
        self.write_readme_if_needed(&dir.to_string_lossy(), dir);
        true
    }

    #[cfg(test)]
    pub fn send_request(
        &mut self,
        ai_workspace: &Path,
        session: &str,
        req: &AgentRequest,
    ) -> std::io::Result<()> {
        self.send_request_in(&agent_dir(ai_workspace, session), req)
    }
    pub(super) fn send_request_in(
        &mut self,
        dir: &Path,
        req: &AgentRequest,
    ) -> std::io::Result<()> {
        req.validate_artifact_request()
            .map_err(std::io::Error::other)?;
        if let Some(run) = &req.artifact {
            crate::versions::validate_prepared_artifact_run(run).map_err(|error| {
                std::io::Error::new(
                    error.kind(),
                    format!(
                        "Could not validate artifact output {}: {error}",
                        run.staging_output.display()
                    ),
                )
            })?;
        }
        std::fs::create_dir_all(dir).map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!(
                    "Could not create agent request directory {}: {error}",
                    dir.display()
                ),
            )
        })?;
        let path = dir.join("request.json");
        atomic_write_json(&path, req).map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("Could not write agent request {}: {error}", path.display()),
            )
        })?;
        self.write_readme_if_needed(&dir.to_string_lossy(), dir);
        Ok(())
    }

    #[cfg(test)]
    pub fn tick_read_session(
        &mut self,
        ai_workspace: &Path,
        session: &str,
    ) -> Option<AgentSession> {
        self.tick_read_session_file(&agent_dir(ai_workspace, session).join("session.json"))
    }
    pub(super) fn tick_read_session_file(&mut self, path: &Path) -> Option<AgentSession> {
        if let Some(t) = self.last_read_attempt {
            if t.elapsed() < STREAM_READ_INTERVAL {
                return None;
            }
        }
        self.last_read_attempt = Some(Instant::now());

        let metadata = std::fs::metadata(path).ok()?;
        let mtime = metadata.modified().ok()?;
        if self.last_session_mtime == Some(mtime) {
            return None;
        }
        if metadata.len() > 16 * 1024 * 1024 {
            self.last_session_mtime = Some(mtime);
            return Some(AgentSession { activity: Vec::new(), usage: None, approval:None, conversation: String::new(), artifacts: Vec::new(),request:String::new(),provider:String::new(),turns:vec![],updated_at:0,bundle:Default::default(),status:AgentStatus::Error("The sidecar session exceeds 16 MB. Archive its older turns in the source program.".into())});
        }
        let text = std::fs::read_to_string(path).ok()?;
        let session = serde_json::from_str::<AgentSession>(&text).ok()?;
        self.last_session_mtime = Some(mtime);
        Some(session)
    }

    fn write_readme_if_needed(&mut self, session: &str, dir: &Path) {
        if self.readmes.get(session).copied().unwrap_or(false) {
            return;
        }
        let path = dir.join("README.md");
        if path.exists() || std::fs::write(&path, README).is_ok() {
            self.readmes.insert(session.to_string(), true);
        }
    }

    #[cfg(test)]
    pub(super) fn force_elapsed(&mut self) {
        self.last_write_attempt =
            Some(Instant::now() - WRITE_INTERVAL - Duration::from_millis(100));
        self.last_read_attempt = Some(Instant::now() - READ_INTERVAL - Duration::from_millis(100));
    }
}
