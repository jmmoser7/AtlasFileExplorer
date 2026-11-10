//! Scoped task capture reuses Slate's viewport owner, never the headless SDK.
use super::*;
use atlas_agent::collaborator::BoardCall;
use serde_json::{json, Value};
use std::io::Write;

pub(super) struct Pending {
    pub(super) call: BoardCall,
    directory: PathBuf,
    destination: PathBuf,
    writing: Option<Receiver<Result<Value, String>>>,
    delivered: bool,
}
impl Pending {
    pub(super) fn new(task: &ModelTaskAuthority, call: BoardCall) -> Self {
        Self {
            directory: task.dir.join("captures").join(&call.id),
            destination: task
                .workspace
                .join("portal-captures")
                .join(format!("task-{}.png", call.id)),
            call,
            writing: None,
            delivered: false,
        }
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if !self.delivered {
            if let Some(writing) = self.writing.take() {
                let destination = self.destination.clone();
                // Retain the cleanup owner across Stop/stale/closed workbook.
                std::thread::spawn(move || {
                    if matches!(writing.recv(), Ok(Ok(_))) {
                        let _ = std::fs::remove_file(destination);
                    }
                });
            }
        }
    }
}

pub(super) fn poll(
    app: &mut SlateApp,
    task: &ModelTaskAuthority,
    pending: &mut Pending,
) -> Result<Option<Value>, String> {
    task.validate(app, true)?;
    if let Some(writing) = &pending.writing {
        return match writing.try_recv() {
            Ok(result) => {
                let value = match result {
                    Ok(value) => value,
                    Err(error) => {
                        pending.delivered = true;
                        return Err(error);
                    }
                };
                if let Err(error) = task.validate(app, true) {
                    let _ = std::fs::remove_file(&pending.destination);
                    pending.delivered = true;
                    return Err(error);
                }
                pending.delivered = true;
                Ok(Some(value))
            }
            Err(crossbeam_channel::TryRecvError::Empty) => Ok(None),
            Err(_) => Err("The scoped viewport capture worker ended without a result".into()),
        };
    }
    // GPU state and live cameras belong to the visible workbook. A parked task
    // waits without changing the selected tab, focus, selection or camera.
    if app.at_home || app.tab().id != task.doc {
        return Ok(None);
    }
    let Some(captured) =
        app.capture_model_inputs(task.node, &pending.directory, &task.permit.identity.agent)?
    else {
        return Ok(None);
    };
    let permit = task.permit.clone();
    let destination = pending.destination.clone();
    let workspace = task.workspace.clone();
    let directory = pending.directory.clone();
    let (send, receive) = crossbeam_channel::bounded(1);
    pending.writing = Some(receive);
    std::thread::spawn(move || {
        let result = finalize(&captured.view, &destination, &workspace, &permit);
        let created = result.is_ok();
        // Only the files created by this exact viewport request are temporary.
        let _ = std::fs::remove_file(&captured.view);
        if let Some(depth) = captured.depth {
            let _ = std::fs::remove_file(depth);
        }
        let _ = std::fs::remove_dir(directory);
        if send.send(result).is_err() && created {
            let _ = std::fs::remove_file(destination);
        }
    });
    Ok(None)
}

pub(super) fn finalize(
    view: &PathBuf,
    destination: &PathBuf,
    workspace: &PathBuf,
    permit: &ModelTaskPermit,
) -> Result<Value, String> {
    let live = || {
        if permit.live() {
            Ok(())
        } else {
            Err("The Model task ended before viewport capture publication".to_string())
        }
    };
    live()?;
    let bytes = atlas_agent::collaborator::images::read_png_bytes(
        view,
        Some(atlas_core::cloud::is_dehydrated),
    )?;
    atlas_core::workbook_assets::require_copy_destination(destination)
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(
        destination
            .parent()
            .ok_or("Viewport capture owner is missing")?,
    )
    .map_err(|e| e.to_string())?;
    atlas_core::workbook_assets::require_copy_destination(destination)
        .map_err(|e| e.to_string())?;
    live()?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        // Use the established direct capture-directory and PNG bounds policy.
        let actual = atlas_agent::collaborator::images::read_workspace_png(
            workspace,
            destination,
            Some(atlas_core::cloud::is_dehydrated),
        )?;
        live()?;
        if actual != bytes {
            return Err("Viewport capture bytes changed before publication".into());
        }
        let width = u32::from_be_bytes(actual[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(actual[20..24].try_into().unwrap());
        Ok(
            json!({"status":"completed","image_source":destination,"image_sha256":model_preview::view_meta::hash_bytes(&actual),"width":width,"height":height,
            "source":permit.binding.binding.source.scope(),"document_id":permit.binding.binding.document_id,"mode":"isolated_draft","capture_owner":"slate_native_viewport"}),
        )
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(destination);
    }
    result
}
