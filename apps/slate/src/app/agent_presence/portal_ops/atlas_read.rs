//! Existing Atlas read path, extracted to keep portal lifecycle ownership small.
use super::*;
impl SlateApp {
    pub(super) fn read_atlas_file(&mut self, op: &mut PortalOp) -> Result<Option<Value>, String> {
        if let Some(rx) = &op.text_rx {
            let locator = op.arguments["locator"]
                .as_str()
                .ok_or("Read_file needs one relative locator")?;
            let (_, _, current_generation, size, mtime) =
                self.agent_atlas_file_ref(op.portal, locator)?;
            if Some(current_generation) != op.source_generation
                || Some((size, mtime)) != op.source_snapshot
            {
                return Err("Atlas scan changed during this file read".into());
            }
            return match rx.try_recv() {
                Ok(result) => result.map(Some),
                Err(mpsc::TryRecvError::Empty) => Ok(None),
                Err(mpsc::TryRecvError::Disconnected) => Err("Text read worker stopped".into()),
            };
        }
        match op
            .arguments
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("text")
        {
            "image" => return image_file::read_image(self, op),
            "text" => {}
            _ => return Err("Read_file mode must be text or image".into()),
        }
        let locator = op.arguments["locator"]
            .as_str()
            .ok_or("Read_file needs one relative locator")?;
        let (root, path, generation, size, mtime) =
            self.agent_atlas_file_ref(op.portal, locator)?;
        let cursor = number(&op.arguments, "cursor", 0, 4 * 1024 * 1024)?;
        if cursor > 0 && op.arguments.get("generation").and_then(Value::as_u64) != Some(generation)
        {
            return Err(
                "Continuation generation is missing or stale; restart at cursor zero".into(),
            );
        }
        let limit = number(&op.arguments, "limit", 4096, 16 * 1024)?;
        if !(4..=16 * 1024).contains(&limit) {
            return Err("Text page limit must be 4–16384 bytes".into());
        }
        self.agent_atlas_highlight(op.portal, locator, &op.call.request_id, &op.observer)?;
        let (tx, rx) = mpsc::channel();
        op.text_rx = Some(rx);
        op.source_generation = Some(generation);
        op.source_snapshot = Some((size, mtime));
        let locator = locator.to_owned();
        std::thread::Builder::new()
            .name("atlas-agent-portal-text".into())
            .spawn(move || {
                let result =
                    read_text_page(root, path, &locator, generation, size, mtime, cursor, limit);
                let _ = tx.send(result);
            })
            .map_err(|e| format!("Could not start bounded text read: {e}"))?;
        Ok(None)
    }
}
