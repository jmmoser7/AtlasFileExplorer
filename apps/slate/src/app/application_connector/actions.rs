//! Source-bound actions; native pixels and all I/O are prepared by the shared worker.
use super::*;
impl SlateApp {
    pub(crate) fn connector_capture(&mut self, id: NodeId, destination: Option<PathBuf>) {
        self.connector_capture_as(id, destination, slate_doc::scene::CmdAuthor::Human);
    }
    pub(crate) fn connector_capture_as(
        &mut self,
        id: NodeId,
        destination: Option<PathBuf>,
        author: slate_doc::scene::CmdAuthor,
    ) {
        let tab = self.tab().id;
        let path = destination.clone().unwrap_or_else(|| {
            super::super::board_portal::capture::output::capture_path(
                self.tab().path.as_deref(),
                id,
                "png",
            )
        });
        if let Some(s) = self.connectors.sessions.get_mut(&(tab, id)) {
            if s.tx
                .try_send(worker::Action::Capture {
                    path,
                    canvas: destination.is_none(),
                    author,
                })
                .is_err()
            {
                self.toast("Rhino is busy. Retry Capture shortly.");
            } else {
                s.failure = None;
                s.pending += 1;
            }
        }
    }
    pub(crate) fn connector_restore_path(&mut self, id: NodeId, path: PathBuf) {
        let tab = self.tab().id;
        if let Some(s) = self.connectors.sessions.get_mut(&(tab, id)) {
            match s.tx.try_send(worker::Action::Restore(path)) {
                Ok(()) => {
                    s.failure = None;
                    s.pending += 1;
                }
                Err(_) => {
                    self.toast("Rhino is busy or disconnected. Retry the saved view shortly.")
                }
            }
        }
    }

    pub(crate) fn connector_ready(&self, id: NodeId) -> bool {
        self.connectors
            .sessions
            .get(&(self.tab().id, id))
            .is_some_and(|s| !s.ended && s.binding.is_some() && s.descriptor.is_some())
    }
    pub(crate) fn connector_pending(&self, id: NodeId) -> bool {
        self.connectors
            .sessions
            .get(&(self.tab().id, id))
            .is_some_and(|s| s.pending > 0)
    }
    pub(crate) fn connector_failure(&self, id: NodeId) -> Option<String> {
        self.connectors
            .sessions
            .get(&(self.tab().id, id))
            .and_then(|s| s.failure.clone())
    }
    pub(crate) fn connector_invocation_receipt(&self, id: NodeId) -> Option<serde_json::Value> {
        self.connectors
            .sessions
            .get(&(self.tab().id, id))
            .and_then(|s| s.invocation_receipt.clone())
    }
    pub(crate) fn connector_invoke_scoped(
        &mut self,
        id: NodeId,
        command: String,
        parameters: serde_json::Value,
    ) -> Result<(), String> {
        self.queue_connector_invocation(id, command, parameters, true)
    }
    fn queue_connector_invocation(
        &mut self,
        id: NodeId,
        command: String,
        parameters: serde_json::Value,
        agent: bool,
    ) -> Result<(), String> {
        let tab = self.tab().id;
        let session = self
            .connectors
            .sessions
            .get_mut(&(tab, id))
            .ok_or("No native session")?;
        session
            .tx
            .try_send(worker::Action::Invoke {
                agent,
                command,
                parameters,
            })
            .map_err(|_| "Native command queue is full")?;
        session.failure = None;
        session.invocation_receipt = None;
        session.pending += 1;
        Ok(())
    }
    pub(crate) fn dispatch_application_connector(
        &mut self,
        command: &str,
        detail: Option<&str>,
    ) -> bool {
        let args = detail
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .unwrap_or_default();
        let id = args["node"]
            .as_u64()
            .or_else(|| detail.and_then(|s| s.parse().ok()))
            .map(NodeId)
            .or_else(|| self.board_sel.iter().next().copied());
        let Some(id) = id else {
            return false;
        };
        match command {
            "board.model.connector.open_surface" => {
                self.open_application_surface(id, "grasshopper", "org.slate.rhino.grasshopper_show")
            }
            "board.model.connector.activate" => self.connector_activate(id),
            "board.model.connector.activate_draft" => self.connector_activate_isolated(id),
            "board.model.connector.inspect" => {
                if let Err(error) = self.queue_connector_invocation(
                    id,
                    "org.slate.rhino.inspect".into(),
                    serde_json::json!({}),
                    false,
                ) {
                    self.toast(error);
                }
            }
            "board.model.connector.deactivate" => self.connector_deactivate(id),
            "board.model.connector.capture" => self.connector_capture(id, None),
            "board.model.connector.restore_view" => {
                let Some(path) = args["path"].as_str() else {
                    return false;
                };
                self.connector_restore_path(id, PathBuf::from(path));
            }
            "board.model.connector.invoke" => {
                let Some(command) = args["command"].as_str() else {
                    return false;
                };
                if let Err(error) = self.queue_connector_invocation(
                    id,
                    command.into(),
                    args["parameters"].clone(),
                    false,
                ) {
                    self.toast(error);
                }
            }
            _ => return false,
        }
        true
    }
    /// A generator waits for pixels from the real native view, never the cached mesh.
    /// Its request-specific directory identifies one immutable input snapshot.
    pub(crate) fn capture_connector_inputs(
        &mut self,
        id: NodeId,
        directory: &std::path::Path,
        agent_id: &str,
    ) -> Result<Option<super::super::model3d::ModelCapture>, String> {
        let tab = self.tab().id;
        let session = self
            .connectors
            .sessions
            .get_mut(&(tab, id))
            .ok_or("No native model is active")?;
        if let Some(error) = session
            .failure
            .as_ref()
            .filter(|_| session.ended || session.binding.is_none())
        {
            return Err(error.clone());
        }
        match session.input_captures.get(directory) {
            Some(InputCapture::Pending) => return Ok(None),
            Some(InputCapture::Complete(result)) => {
                return result
                    .clone()
                    .map(|view| Some(super::super::model3d::ModelCapture { view, depth: None }))
            }
            None => {}
        }
        if session.input_captures.len() >= 8 {
            session
                .input_captures
                .retain(|_, value| matches!(value, InputCapture::Pending));
            if session.input_captures.len() >= 8 {
                return Err("Native input capture budget is full".into());
            }
        }
        let directory = directory.to_path_buf();
        let path = directory.join(format!("node-{}-native-view.png", id.0));
        match session.tx.try_send(worker::Action::InputCapture {
            directory: directory.clone(),
            path,
            agent_id: agent_id.into(),
        }) {
            Ok(()) => {
                session
                    .input_captures
                    .insert(directory, InputCapture::Pending);
                Ok(None)
            }
            Err(crossbeam_channel::TrySendError::Full(_)) => Ok(None),
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                Err("Native capture connection is unavailable".into())
            }
        }
    }
}
