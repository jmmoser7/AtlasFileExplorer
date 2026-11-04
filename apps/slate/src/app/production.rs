//! Explicit local command-file entry for recorded production sessions.
//! The CLI file is declarative command data, never workbook-attached code.
use super::{items::ReadyFile, SlateApp};
use crossbeam_channel::{Receiver, Sender};
use serde::Deserialize;
use serde_json::{json, Value};
use slate_doc::{scene::WorldRect, NodeId};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
mod native_result;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    version: u32,
    workspace_root: PathBuf,
    workbook: PathBuf,
    author: String,
    #[serde(default = "interval")]
    interval_ms: u64,
    actions: Vec<Action>,
}
fn interval() -> u64 {
    120
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    #[serde(default)]
    id: String,
    #[serde(default)]
    author: Option<String>,
    command: String,
    #[serde(default)]
    arguments: Value,
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default)]
    focus: Option<String>,
    #[serde(skip)]
    file: Option<ReadyFile>,
    #[serde(skip)]
    capture_destination: Option<PathBuf>,
}
struct Plan {
    config: Configuration,
    events: Sender<Value>,
}
#[derive(Default)]
pub(crate) struct ProductionRuntime {
    load: Option<Receiver<Result<Plan, String>>>,
    plan: Option<Plan>,
    index: usize,
    aliases: HashMap<String, NodeId>,
    last: Vec<NodeId>,
    next: Option<Instant>,
    native_pending: bool,
    native_discovery: Option<Instant>,
    witness_pending: Option<String>,
}
impl ProductionRuntime {
    pub(crate) fn from_args() -> Self {
        let args: Vec<_> = std::env::args_os().collect();
        let path = args
            .iter()
            .position(|a| a == "--production-session")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from);
        let Some(path) = path else {
            return Self::default();
        };
        let (tx, load) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send(load_plan(&path));
        });
        Self {
            load: Some(load),
            ..Default::default()
        }
    }
}
const COMMANDS: &[&str] = &[
    "board.objects.create_frame",
    "board.objects.create_text",
    "board.objects.create_shape",
    "board.paths.draw",
    "board.shape.edit",
    "board.objects.move",
    "board.objects.resize",
    "board.objects.set_text",
    "board.objects.order",
    "board.objects.wire",
    "board.objects.delete",
    "board.objects.link_media",
    "board.objects.create_generator",
    "portal.agent.run",
    "board.model.connector.activate",
    "board.model.connector.invoke",
    "board.model.connector.capture",
    "board.model.connector.deactivate",
];
fn approved(root: &Path, path: &Path, existing: bool) -> Result<PathBuf, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let resolved = if existing {
        path.canonicalize()
    } else {
        path.parent()
            .ok_or("Output needs a parent folder")?
            .canonicalize()
            .map(|p| p.join(path.file_name().unwrap_or_default()))
    }
    .map_err(|e| e.to_string())?;
    if !resolved.starts_with(root) {
        return Err(
            "Production paths must stay inside the explicitly selected workspace root".into(),
        );
    }
    Ok(resolved)
}
fn load_plan(path: &Path) -> Result<Plan, String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.len() > 8 * 1024 * 1024 {
        return Err("Production command file exceeds 8 MiB".into());
    }
    let mut config: Configuration =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if config.version != 1 || config.actions.is_empty() || config.actions.len() > 4096 {
        return Err("Use production version 1 with 1–4096 actions".into());
    }
    if config.author.trim().is_empty()
        || config.author.len() > 128
        || !(16..=2000).contains(&config.interval_ms)
    {
        return Err("Production author or action cadence is invalid".into());
    }
    config.workspace_root = config
        .workspace_root
        .canonicalize()
        .map_err(|e| e.to_string())?;
    config.workbook = approved(&config.workspace_root, &config.workbook, false)?;
    if config.workbook.exists() {
        return Err("Production creates a new workbook. Choose a new output filename to preserve the existing project.".into());
    }
    if config
        .workbook
        .extension()
        .is_none_or(|e| !e.eq_ignore_ascii_case("slate"))
    {
        return Err("Production output must be a .slate workbook".into());
    }
    for action in &mut config.actions {
        if !COMMANDS.contains(&action.command.as_str())
            || action.targets.len() > 128
            || action.id.len() > 128
            || action
                .author
                .as_ref()
                .is_some_and(|a| a.trim().is_empty() || a.len() > 128)
        {
            return Err("Production action is outside its declared command/identity scope".into());
        }
        if action.command == "board.model.connector.capture" {
            if let Some(path) = action.arguments["path"].as_str() {
                if path.len() > 4096 {
                    return Err("Capture path exceeds bound".into());
                }
                let path = approved(&config.workspace_root, Path::new(path), false)?;
                if !path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                {
                    return Err("Native capture output must be a .png file".into());
                }
                action.capture_destination = Some(path);
            }
        }
        if action.command == "board.objects.link_media" {
            let path = action.arguments["path"]
                .as_str()
                .ok_or("Linked media needs a bounded local path")?;
            if path.len() > 4096 {
                return Err("Media path exceeds bound".into());
            }
            let path = approved(&config.workspace_root, Path::new(path), true)?;
            if atlas_core::cloud::is_dehydrated(&path) {
                return Err("Production media must be locally available".into());
            }
            if !matches!(
                slate_doc::media_kind(&path),
                slate_doc::MediaKind::Image
                    | slate_doc::MediaKind::Video
                    | slate_doc::MediaKind::Model
                    | slate_doc::MediaKind::Pdf
                    | slate_doc::MediaKind::Text
                    | slate_doc::MediaKind::Doc
            ) {
                return Err("Unsupported linked media type".into());
            }
            action.file = Some(ReadyFile::read(path)?);
        }
    }
    let event_path = config.workbook.with_extension("production.jsonl");
    let (events, rx) = crossbeam_channel::bounded::<Value>(256);
    std::thread::spawn(move || {
        use std::io::Write;
        let Ok(mut out) = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(event_path)
        else {
            return;
        };
        for event in rx {
            let _ = writeln!(out, "{event}");
            let _ = out.flush();
        }
    });
    Ok(Plan { config, events })
}
impl SlateApp {
    pub(crate) fn pump_production_session(&mut self, ctx: &eframe::egui::Context) {
        let mut runtime = std::mem::take(&mut self.production);
        if let Some(rx) = &runtime.load {
            match rx.try_recv() {
                Ok(Ok(plan)) => {
                    runtime.load = None;
                    self.leave_home();
                    self.ensure_work_tab();
                    if self.doc().scene.nodes.is_empty() {
                        self.save_doc_to(self.tab().id, plan.config.workbook.clone());
                        if self.tab().path.as_deref() == Some(plan.config.workbook.as_path()) {
                            let _=plan.events.try_send(json!({"status":"started","author":plan.config.author,"actions":plan.config.actions.len()}));
                            runtime.plan = Some(plan);
                            runtime.next = Some(Instant::now() + Duration::from_millis(500));
                        } else {
                            self.toast("Production could not reserve its new workbook.");
                        }
                    } else {
                        self.toast(
                            "Production requires a fresh workbook; existing content is preserved.",
                        );
                    }
                }
                Ok(Err(error)) => {
                    runtime.load = None;
                    self.toast(format!("Production: {error}"));
                }
                Err(_) => {
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
            }
        }
        if runtime.plan.is_some() && runtime.next.is_none_or(|due| Instant::now() >= due) {
            let result = self.production_step(ctx, &mut runtime);
            if let Err(error) = result {
                if let Some(plan) = runtime.plan.take() {
                    let _ = plan
                        .events
                        .try_send(json!({"status":"failed","index":runtime.index,"error":error}));
                }
                self.toast(format!("Production paused: {error}"));
            }
        }
        if runtime.plan.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        self.production = runtime;
    }
    fn production_step(
        &mut self,
        ctx: &eframe::egui::Context,
        runtime: &mut ProductionRuntime,
    ) -> Result<(), String> {
        let plan = runtime.plan.as_mut().ok_or("Production plan missing")?;
        if runtime.index >= plan.config.actions.len() {
            self.save_doc_to(self.tab().id, plan.config.workbook.clone());
            let _=plan.events.try_send(json!({"status":"completed","actions":runtime.index,"objects":self.doc().scene.nodes.len(),"workbook":plan.config.workbook}));
            self.toast("Recorded production session completed and saved.");
            runtime.plan = None;
            return Ok(());
        }
        if self.tab().path.as_deref() != Some(plan.config.workbook.as_path()) {
            return Err("Production workbook is no longer active".into());
        }
        let action = &mut plan.config.actions[runtime.index];
        let author = action.author.as_deref().unwrap_or(&plan.config.author);
        if let Some(focus) = action
            .focus
            .as_ref()
            .filter(|_| runtime.witness_pending.is_none() && !runtime.native_pending)
            .and_then(|s| runtime.aliases.get(s))
        {
            if let Some(rect) = self.doc().scene.node(*focus).map(|n| n.rect) {
                self.zoom_to_rect(rect);
            }
        }
        let targets: Result<Vec<_>, _> = action
            .targets
            .iter()
            .map(|s| {
                if s == "last" {
                    runtime
                        .last
                        .first()
                        .copied()
                        .ok_or("No previous created object".into())
                } else {
                    runtime
                        .aliases
                        .get(s)
                        .copied()
                        .or_else(|| s.parse().ok().map(NodeId))
                        .ok_or_else(|| format!("Unknown target {s}"))
                }
            })
            .collect();
        let targets = targets?;
        let mut native_receipt = None;
        let created = if action.command.starts_with("board.model.connector.") {
            let node = *targets
                .first()
                .ok_or("Native actions require one explicit model target")?;
            match action.command.as_str() {
                "board.model.connector.activate" => {
                    if self.ai.packs.probe_pending() {
                        let started = *runtime.native_discovery.get_or_insert_with(Instant::now);
                        if started.elapsed() >= Duration::from_secs(120) {
                            return Err(
                                "Rhino connector discovery did not finish within its bound".into(),
                            );
                        }
                        runtime.next = Some(Instant::now() + Duration::from_millis(50));
                        return Ok(());
                    }
                    runtime.native_discovery = None;
                    if action.arguments["mode"] != "isolated_draft" {
                        return Err(
                            "CLI native modeling requires an explicitly isolated draft".into()
                        );
                    }
                    let commands: Vec<String> =
                        serde_json::from_value(action.arguments["commands"].clone())
                            .map_err(|_| "Native activation must list its scoped command IDs")?;
                    if commands.is_empty() || commands.len() > 64 {
                        return Err("Native grant needs 1–64 bounded commands".into());
                    }
                    self.connector_activate_draft(
                        node,
                        format!("production-{}", std::process::id()),
                        format!(
                            "agent-{}",
                            model_preview::view_meta::hash_bytes(author.as_bytes())
                        ),
                        commands,
                    );
                    if !self.application_connector_active(node) {
                        return Err("Rhino activation could not start for the selected model; see the portal status".into());
                    }
                }
                "board.model.connector.deactivate" => self.connector_deactivate(node),
                command => {
                    if let Some(error) = self.connector_failure(node) {
                        return Err(error);
                    }
                    if !self.connector_ready(node) {
                        runtime.next = Some(Instant::now() + Duration::from_millis(50));
                        return Ok(());
                    }
                    if !runtime.native_pending {
                        if command == "board.model.connector.capture" {
                            self.connector_capture_as(
                                node,
                                action.capture_destination.clone(),
                                slate_doc::scene::CmdAuthor::Agent(author.into()),
                            );
                        } else {
                            let command = action.arguments["command"]
                                .as_str()
                                .ok_or("Native invoke requires its registered command ID")?;
                            self.connector_invoke_scoped(
                                node,
                                command.into(),
                                action.arguments["parameters"].clone(),
                            )?;
                        }
                        runtime.native_pending = true;
                        runtime.next = Some(Instant::now() + Duration::from_millis(16));
                        return Ok(());
                    }
                    if self.connector_pending(node) {
                        runtime.next = Some(Instant::now() + Duration::from_millis(16));
                        return Ok(());
                    }
                    runtime.native_pending = false;
                    if command == "board.model.connector.invoke" {
                        let receipt = self.connector_invocation_receipt(node).ok_or(
                            "Native command finished without its validated result receipt",
                        )?;
                        if receipt["command"] != action.arguments["command"] {
                            return Err("Native result belongs to a different command".into());
                        }
                        native_result::require_complete_clear(
                            &receipt,
                            runtime.index,
                            author,
                            &plan.events,
                        )?;
                        native_receipt = Some(receipt);
                    }
                }
            }
            Vec::new()
        } else if action.command == "board.objects.link_media" {
            let file = action
                .file
                .take()
                .ok_or("Media action was already consumed")?;
            let number = |key: &str| {
                action.arguments[key]
                    .as_f64()
                    .filter(|n| n.is_finite() && n.abs() < 1e9)
                    .map(|n| n as f32)
                    .ok_or_else(|| format!("Invalid media {key}"))
            };
            let rect = WorldRect::new(
                number("x")?,
                number("y")?,
                number("width")?,
                number("height")?,
            );
            if rect.w <= 0.0 || rect.h <= 0.0 {
                return Err("Media dimensions must be positive".into());
            }
            vec![self.link_ready_media(
                rect,
                file,
                slate_doc::scene::CmdAuthor::Agent(author.into()),
            )?]
        } else {
            let targets = targets.iter().map(|n| n.0.to_string()).collect::<Vec<_>>();
            if let Some(ticket) = runtime.witness_pending.as_deref() {
                match self.production_witness_poll(ticket)? {
                    Some(created) => {
                        runtime.witness_pending = None;
                        created
                    }
                    None => {
                        runtime.next = Some(Instant::now() + Duration::from_millis(16));
                        return Ok(());
                    }
                }
            } else {
                let (_, ticket) = self.production_witness_command(
                    ctx,
                    &plan.config.workspace_root,
                    author,
                    &action.command,
                    &targets,
                    &action.arguments,
                )?;
                runtime.witness_pending = Some(ticket);
                runtime.next = Some(Instant::now() + Duration::from_millis(16));
                return Ok(());
            }
        };
        if !created.is_empty() {
            runtime.last = created.clone();
            self.board_sel = created.iter().copied().collect();
            if !action.id.is_empty() {
                runtime.aliases.insert(action.id.clone(), created[0]);
            }
            if action.command == "board.objects.create_frame" {
                if let Some(rect) = self.doc().scene.node(created[0]).map(|n| n.rect) {
                    self.zoom_to_rect(rect);
                }
            }
        }
        let _=plan.events.try_send(json!({"status":"applied","index":runtime.index,"author":author,"command":action.command,"created":created.iter().map(|n|n.0).collect::<Vec<_>>(),"native_receipt":native_receipt,"at_unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()}));
        runtime.index += 1;
        runtime.next = Some(Instant::now() + Duration::from_millis(plan.config.interval_ms));
        Ok(())
    }
}

#[cfg(test)]
mod tests;
