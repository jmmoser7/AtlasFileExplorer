//! Author Grasshopper revisions as inert files until the person chooses to run one.

use super::*;
use slate_doc::scene::ModelOutputRef;
use std::io::Read;
use std::path::Path;

const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_REVISIONS: usize = 128;
const REVISION: &str = "definition.ghx";

pub(super) struct Run {
    session: String,
    source: slate_doc::ItemId,
    request: String,
    expires: Instant,
    phase: Phase,
}

enum Phase {
    Staging {
        ready: Receiver<Result<PathBuf, String>>,
        request: Box<AgentRequest>,
        authored: String,
        workspace: PathBuf,
    },
    Writing {
        draft: PathBuf,
        publish: PathBuf,
    },
    Checking(Receiver<Result<(PathBuf, u64, i64), String>>),
}

impl SlateApp {
    pub(crate) fn request_grasshopper_output_confirmation(
        &mut self,
        node: NodeId,
        id: &str,
    ) -> bool {
        if !self.grasshopper_viewer(node) || !self.grasshopper_model_output(node, id) {
            return false;
        }
        self.agents.grasshopper_confirm = Some((node, id.to_owned()));
        true
    }

    pub(super) fn validate_grasshopper_authoring(&self, node: NodeId) -> Result<(), String> {
        let target = self
            .doc()
            .scene
            .node(node)
            .ok_or("Grasshopper viewer was removed")?;
        if self.tab().read_only || target.locked {
            return Err("Grasshopper viewer is read-only or locked".into());
        }
        let agent = slate_doc::agent_chat::agent(target).ok_or("Choose Codex before sending")?;
        if agent.provider != "codex" || agent.session.is_empty() {
            return Err("Grasshopper authoring requires a connected Codex agent".into());
        }
        if agent.generator.count.unwrap_or(agent.image.count).max(1) > 1 {
            return Err("Grasshopper authoring supports one revision per request".into());
        }
        if agent.generator.model_outputs.len() >= MAX_REVISIONS {
            return Err("The Grasshopper revision history is full".into());
        }
        if self.agents.grasshopper_runs.contains_key(&node) {
            return Err("Stop the current Grasshopper request before sending again".into());
        }
        if !self.agents.catalog("codex").iter().any(|model| {
            agent
                .model
                .as_deref()
                .map_or(model.is_default, |id| id == model.id)
        }) {
            return Err("Choose an available Codex model for Grasshopper authoring".into());
        }
        Ok(())
    }

    pub(super) fn queue_grasshopper_authoring(
        &mut self,
        node: NodeId,
        workspace: PathBuf,
        authored: String,
        request: AgentRequest,
    ) -> Result<(), String> {
        let source = self
            .image_item(node)
            .ok_or("Grasshopper source is missing")?;
        let source_path = self
            .item_path(source)
            .ok_or("Grasshopper source is missing")?;
        let (session, _) = self
            .agent_session_for(node)
            .ok_or("Choose Codex before sending")?;
        let work_dir = self
            .agent_folder_for(node)
            .unwrap_or_else(|| workspace.clone());
        let draft = work_dir
            .join(".slate-grasshopper-authoring")
            .join(&request.id);
        let (tx, ready) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send(stage(&source_path, &draft));
        });
        self.agents.grasshopper_runs.insert(
            node,
            Run {
                session,
                source,
                request: request.id.clone(),
                expires: Instant::now() + Duration::from_secs(1850),
                phase: Phase::Staging {
                    ready,
                    request: Box::new(request),
                    authored,
                    workspace,
                },
            },
        );
        Ok(())
    }

    pub(super) fn pump_grasshopper_authoring(&mut self, ctx: &egui::Context) {
        let ids: Vec<_> = self.agents.grasshopper_runs.keys().copied().collect();
        for node in ids {
            let Some(mut run) = self.agents.grasshopper_runs.remove(&node) else {
                continue;
            };
            if self.tab().read_only
                || self.doc().scene.node(node).is_none_or(|n| n.locked)
                || self.image_item(node) != Some(run.source)
                || self.agent_session_for(node).as_ref().map(|(s, _)| s) != Some(&run.session)
            {
                continue;
            }
            if Instant::now() >= run.expires {
                self.fail_agent_await(node, "Grasshopper authoring timed out".into());
                continue;
            }
            match self.advance_grasshopper_authoring(node, &mut run) {
                Ok(false) => {
                    self.agents.grasshopper_runs.insert(node, run);
                }
                Ok(true) => {}
                Err(error) => self.fail_agent_await(node, error),
            }
        }
        if !self.agents.grasshopper_runs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }

    fn advance_grasshopper_authoring(
        &mut self,
        node: NodeId,
        run: &mut Run,
    ) -> Result<bool, String> {
        match &mut run.phase {
            Phase::Staging {
                ready,
                request,
                authored,
                workspace,
            } => {
                let Some(source) = worker_reply(ready)? else {
                    return Ok(false);
                };
                let draft = source
                    .parent()
                    .ok_or("Grasshopper staging has no directory")?
                    .to_path_buf();
                let publish = self
                    .tab()
                    .path
                    .as_deref()
                    .map(atlas_core::workbook_assets::assets_root)
                    .unwrap_or_else(|| atlas_core::index::data_dir().join("assets"))
                    .join("generated/grasshopper")
                    .join(&run.request)
                    .join(REVISION);
                request.output_dir = Some(draft.to_string_lossy().into_owned());
                request.policy = atlas_agent::TurnPolicy::StagedFiles;
                request.pending_prompt_projection = None;
                request
                    .prompt
                    .push_str(&authoring_instructions(&source, &draft.join(REVISION)));
                let deadline = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
                    + 1_800_000;
                if !self.dispatch_model_task_request(
                    node,
                    (**request).clone(),
                    workspace,
                    authored,
                    deadline,
                ) {
                    return Ok(true);
                }
                run.phase = Phase::Writing { draft, publish };
                Ok(false)
            }
            Phase::Writing { draft, publish } => {
                if self.agents.requests.get(&node) != Some(&run.request)
                    || matches!(
                        self.agents.awaiting.get(&node),
                        Some(AgentAwait::Failed { .. })
                    )
                {
                    return Ok(true);
                }
                let complete = self.agents.sessions.get(&node).is_some_and(|session| {
                    session.request == run.request && session.status == AgentStatus::Idle
                });
                if !complete {
                    return Ok(false);
                }
                let draft = draft.clone();
                let publish = publish.clone();
                let (tx, rx) = crossbeam_channel::bounded(1);
                std::thread::spawn(move || {
                    let _ = tx.send(validate_and_publish(&draft, &publish));
                });
                run.phase = Phase::Checking(rx);
                Ok(false)
            }
            Phase::Checking(ready) => {
                if self.agents.requests.get(&node) != Some(&run.request) {
                    return Ok(true);
                }
                let Some((path, size, mtime)) = worker_reply(ready)? else {
                    return Ok(false);
                };
                let output = ModelOutputRef {
                    id: run.request.clone(),
                    source: slate_doc::SourceUri {
                        locator: slate_doc::scene::source_locator(
                            self.tab().path.as_deref(),
                            &path,
                        ),
                    },
                    size,
                    mtime,
                };
                let original = self.doc().item(run.source).map(|item| ModelOutputRef {
                    id: format!("grasshopper-original-{}", node.0),
                    source: slate_doc::SourceUri {
                        locator: item.path.to_string_lossy().into_owned(),
                    },
                    size: item.size,
                    mtime: item.mtime,
                });
                self.patch_nodes(&[node], |target| {
                    if let Some(agent) = slate_doc::agent_chat::agent_mut(target) {
                        if agent.generator.model_outputs.is_empty() {
                            if let Some(original) = &original {
                                agent.generator.shown_model = Some(original.id.clone());
                                agent.generator.model_outputs.push(original.clone());
                            }
                        }
                        agent.generator.model_outputs.push(output.clone());
                    }
                });
                self.toast(
                    "Grasshopper revision ready. Select it to review and run the definition.",
                );
                Ok(true)
            }
        }
    }

    pub(super) fn paint_grasshopper_confirmation(&mut self, ctx: &egui::Context) {
        let Some((node, output)) = self.agents.grasshopper_confirm.clone() else {
            return;
        };
        let response = egui::Modal::new(Id::new("confirm-grasshopper-revision")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading("Run Grasshopper definition?");
            ui.label("Opening this revision in the viewer will run any embedded scripts. The original file stays on disk.");
            ui.horizontal(|ui| {
                let cancel = ui.button("Cancel").clicked();
                let run = ui.button("Run definition").clicked();
                (cancel, run)
            }).inner
        });
        if response.inner.0 || response.inner.1 || response.should_close() {
            self.agents.grasshopper_confirm = None;
        }
        if response.inner.1 {
            if let Err(error) = self.select_approved_grasshopper_output(node, &output) {
                self.toast(error);
            }
        }
    }
}

fn worker_reply<T>(ready: &Receiver<Result<T, String>>) -> Result<Option<T>, String> {
    match ready.try_recv() {
        Ok(result) => result.map(Some),
        Err(crossbeam_channel::TryRecvError::Empty) => Ok(None),
        Err(crossbeam_channel::TryRecvError::Disconnected) => {
            Err("Grasshopper file preparation stopped".into())
        }
    }
}

fn bounded_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ARCHIVE_BYTES {
        return Err("Expected a regular nonempty Grasshopper file up to 64 MiB".into());
    }
    if atlas_core::cloud::is_dehydrated(path) {
        return Err("Download the Grasshopper file before using it".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_ARCHIVE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err("The Grasshopper file size changed while reading it".into());
    }
    Ok(bytes)
}

fn stage(source: &Path, draft: &Path) -> Result<PathBuf, String> {
    if !slate_doc::media::is_grasshopper(source) {
        return Err("The selected source is not a Grasshopper definition".into());
    }
    let source_name = if source
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gh"))
    {
        "source.gh"
    } else {
        "source.ghx"
    };
    let bytes = bounded_bytes(source)
        .map_err(|error| format!("Could not copy Grasshopper source: {error}"))?;
    std::fs::create_dir_all(draft.parent().ok_or("Grasshopper staging has no parent")?)
        .and_then(|()| std::fs::create_dir(draft))
        .map_err(|error| format!("Could not create Grasshopper staging: {error}"))?;
    let copy = draft.join(source_name);
    std::fs::write(&copy, bytes)
        .map_err(|error| format!("Could not copy Grasshopper source: {error}"))?;
    Ok(copy)
}

fn validate_and_publish(draft: &Path, output: &Path) -> Result<(PathBuf, u64, i64), String> {
    let missing = |error| format!("Codex did not return a valid Grasshopper definition: {error}");
    let candidate = draft.join(REVISION);
    let bytes = bounded_bytes(&candidate).map_err(missing)?;
    validate_archive(&bytes).map_err(missing)?;
    let dir = output
        .parent()
        .ok_or("Grasshopper revision has no output directory")?;
    let saved = || -> std::io::Result<(u64, i64)> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join("definition.tmp");
        let mut file = std::fs::File::create_new(&tmp)?;
        std::io::Write::write_all(&mut file, &bytes)?;
        drop(file);
        std::fs::rename(tmp, output)?;
        let metadata = std::fs::metadata(output)?;
        Ok((metadata.len(), atlas_core::metadata::mtime_of(&metadata)))
    };
    let (size, mtime) =
        saved().map_err(|error| format!("Could not save Grasshopper revision: {error}"))?;
    Ok((output.to_path_buf(), size, mtime))
}

fn validate_archive(bytes: &[u8]) -> Result<(), String> {
    let xml = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let document = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
    let root = document.root_element();
    let definition = root
        .children()
        .find(|node| node.has_tag_name("chunks"))
        .and_then(|chunks| {
            chunks.children().find(|node| {
                node.has_tag_name("chunk") && node.attribute("name") == Some("Definition")
            })
        });
    let has_definition = root.has_tag_name("Archive")
        && definition.is_some_and(|node| {
            node.children()
                .find(|node| node.has_tag_name("chunks"))
                .is_some_and(|chunks| {
                    chunks.children().any(|child| {
                        child.has_tag_name("chunk")
                            && child.attribute("name") == Some("DefinitionObjects")
                    })
                })
        });
    has_definition
        .then_some(())
        .ok_or_else(|| "The GHX archive is missing its Grasshopper definition objects".into())
}

fn authoring_instructions(source: &Path, output: &Path) -> String {
    let marker = atlas_agent::GRASSHOPPER_AUTHORING_MARKER;
    format!(
        "{marker}Return exactly one editable Grasshopper definition. \
         The copied input definition is {source:?}. For edits, preserve unaffected components, \
         stable control names, IDs, defaults and wires. Save the new GHX archive at {output:?}. \
         Never change the original or substitute a baked 3DM. Prefer an embedded C# Script component \
         unless the person asks otherwise. Expose requested controls as named typed Grasshopper inputs \
         with defaults, appropriate ranges and wires, such as Number Sliders or a Colour Swatch. \
         Use RhinoCommon for parametric geometry and return visible geometry and preview materials. \
         For example, a chair definition should expose Seat Height, Back Height, Chair Width and Chair Color \
         when requested. Use GH_IO/Grasshopper SDK serialization (GH_Archive.AppendObject(document, \
         \"Definition\"), then WriteToFile). Rhino 8 exposes \
         RhinoCodePluginGH.Components.CSharpComponent.Create(title, source, icon: null, emptyParams: true), \
         SetSource and typed ScriptVariableParam registrations. Do not execute any authored or input \
         Grasshopper scripts, start a solve, issue board commands, or place the result. \
         Slate validates and lists the file for the person to open explicitly. Report any authoring failure clearly."
    )
}

#[cfg(test)]
mod tests;
