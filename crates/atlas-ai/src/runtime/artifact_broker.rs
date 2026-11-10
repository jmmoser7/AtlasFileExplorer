//! Live parent callback for Artifact I/O. Descriptors and helper files are routing.
use super::*;
use atlas_agent::{
    artifact_tools::{ArtifactCall, ArtifactIdentity, ArtifactReply},
    collaborator::{mailbox::write_packet, transport::read_json},
    helper::ArtifactToolsLaunch,
};
use std::{collections::HashSet, sync::Mutex, time::Instant};

pub(super) struct ArtifactBroker {
    running: Arc<AtomicBool>,
    binding: PathBuf,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl ArtifactBroker {
    pub fn start(
        launch: &ArtifactToolsLaunch,
        request: &AgentRequest,
        link_dir: &Path,
        stop: Arc<AtomicBool>,
        current: Arc<Mutex<String>>,
    ) -> Result<Self, String> {
        launch.validate_for(request)?;
        if launch.link_dir != link_dir {
            return Err("Artifact helper link does not match its live runtime".into());
        }
        let scope = crate::versions::ArtifactExecutionScope::prepare(request, &launch.link_dir)?;
        // Register only the fingerprint of the request the prepared parent has
        // just validated, never a descriptor-supplied assertion on its own.
        let expected = ArtifactIdentity::from_request(request)?;
        let remaining = launch.remaining_time()?;
        if remaining
            > std::time::Duration::from_secs(atlas_agent::collaborator::tool_task_budget_seconds(
                request,
            ))
        {
            return Err("Artifact parent scope cannot exceed its validated task budget".into());
        }
        // The parent retains the originally admitted Instant, not a new phase.
        let deadline = launch.expires;
        let identity = expected.clone();
        let dir = launch.transport_dir();
        let running = Arc::new(AtomicBool::new(true));
        let live = running.clone();
        let ensure = move || {
            if !live.load(Ordering::Acquire)
                || stop.load(Ordering::Acquire)
                || Instant::now() >= deadline
                || current
                    .lock()
                    .map_err(|_| "Artifact request witness unavailable")?
                    .as_str()
                    != identity.request_id
            {
                Err("Artifact request stopped, expired, or was replaced; staged output cannot be published".into())
            } else {
                Ok(())
            }
        };
        ensure()?;
        std::fs::create_dir_all(dir.join("artifact-calls")).map_err(|e| e.to_string())?;
        let binding = dir.join("artifact-session.json");
        crate::agent::atomic_write_json(&binding, &expected).map_err(|e| e.to_string())?;
        let worker = std::thread::spawn(move || {
            let mut sequence = 0;
            let mut seen = HashSet::new();
            while ensure().is_ok() && seen.len() < 64 {
                let paths = std::fs::read_dir(dir.join("artifact-calls"))
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .take(8)
                    .map(|entry| entry.path())
                    .collect::<Vec<_>>();
                let mut calls = paths
                    .into_iter()
                    .filter_map(|path| {
                        let value = read_json(&path, Some(atlas_core::cloud::is_dehydrated))?;
                        let call = serde_json::from_value::<ArtifactCall>(value).ok()?;
                        (call.identity == expected
                            && call.validate().is_ok()
                            && path.file_stem().and_then(|name| name.to_str())
                                == Some(call.id.as_str()))
                        .then_some(call)
                    })
                    .collect::<Vec<_>>();
                calls.sort_by_key(|call| call.sequence);
                for call in calls {
                    if seen.contains(&call.id) {
                        continue;
                    }
                    // A directory snapshot can observe a later atomic rename
                    // first. Leave future calls queued until their predecessor.
                    if call.sequence > sequence && sequence < 32 {
                        continue;
                    }
                    let reply = if call.sequence != sequence || sequence >= 32 {
                        ArtifactReply::error(
                            &call,
                            "Artifact request sequence or 32-action budget rejected this call",
                        )
                    } else {
                        sequence += 1;
                        match scope.execute(&call.operation, &ensure) {
                            Ok(result) => ArtifactReply {
                                id: call.id.clone(),
                                identity: expected.clone(),
                                success: true,
                                result,
                            },
                            Err(error) => ArtifactReply::error(&call, &error),
                        }
                    };
                    seen.insert(call.id.clone());
                    // An admitted write may have finished during Stop. Preserve
                    // its uncertain outcome in a reply; never publish from here.
                    if let Err(error) = write_packet(&dir, "artifact-replies", &call.id, &reply) {
                        let _ = write_packet(
                            &dir,
                            "artifact-replies",
                            &call.id,
                            &ArtifactReply::error(&call, &error),
                        );
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            let _ = std::fs::remove_file(dir.join("artifact-session.json"));
        });
        Ok(Self {
            running,
            binding,
            worker: Some(worker),
        })
    }
}
impl Drop for ArtifactBroker {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        let _ = std::fs::remove_file(&self.binding);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(test)]
mod tests;
