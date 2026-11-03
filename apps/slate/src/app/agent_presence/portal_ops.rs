//! Scoped, witnessed operations on existing Atlas and Web portal hosts.
use super::*;
use crate::app::board_web::WebInput;
use serde_json::{json, Value};
use slate_doc::scene::{NodeKind, PortalKind};
use std::{path::PathBuf, sync::mpsc, time::Duration};
mod args;
use args::{finite, number, page_coord, web_key};

fn portal_reply(call: &BoardCall, result: Value) -> BoardReply {
    BoardReply {
        id: call.id.clone(),
        session_id: call.session.session_id.clone(),
        request_id: call.request_id.clone(),
        success: true,
        result,
    }
}

mod read_file;
use read_file::read_text_page;
mod atlas_read;
mod image_file;
mod native;

const SOURCE_INPUTS: &str = "board-file-inputs";

pub(super) struct PortalOp {
    pub(super) call: BoardCall,
    portal: slate_doc::NodeId,
    node: slate_doc::scene::Node,
    command: String,
    arguments: Value,
    observer: String,
    contact: Option<(f64, f64)>,
    text_rx: Option<mpsc::Receiver<Result<Value, String>>>,
    image_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    image_path: Option<PathBuf>,
    image_verified: bool,
    source_generation: Option<u64>,
    source_snapshot: Option<(u64, i64)>,
    started: bool,
    native: Option<native::NativeOp>,
}
pub(super) fn recognizes(command: &str) -> bool {
    native::recognizes(command)
        || matches!(
            command,
            "portal.web.focus"
                | "portal.atlas.list"
                | "portal.atlas.highlight"
                | "portal.atlas.read_file"
                | "portal.web.read_text"
                | "portal.web.capture"
                | "portal.web.navigate"
                | "portal.web.click"
                | "portal.web.scroll"
                | "portal.web.type"
                | "portal.web.key"
        )
}

impl SlateApp {
    pub(super) fn begin_presence_portal_call(
        &mut self,
        ctx: Option<&egui::Context>,
        a: &mut Collaborator,
        call: &BoardCall,
    ) -> Result<Option<Value>, String> {
        let BoardOperation::Invoke {
            command,
            targets,
            arguments,
        } = &call.operation
        else {
            return Ok(None);
        };
        if !recognizes(command) {
            return Ok(None);
        }
        SlateApp::validate_targeted_arguments(command, arguments)?;
        let grant = a.grant.as_ref().ok_or("Board grant revoked")?;
        grant
            .authorize(call, &a.binding, &a.request)
            .map_err(str::to_owned)?;
        if a.portal.is_some() || a.work.is_some() || a.read.is_some() {
            return Err("Wait for the current action".into());
        }
        if a.paused || a.departure.is_some() || a.tab != self.tab().id {
            return Err("Board is inactive or collaborator paused".into());
        }
        if targets.len() != 1 {
            return Err("Portal operations target exactly one granted portal".into());
        }
        let id = targets[0]
            .parse::<u64>()
            .map(slate_doc::NodeId)
            .map_err(|_| "Use one explicit portal ID")?;
        if !grant.allows_node(&id.0.to_string()) {
            return Err("Portal is outside this collaborator's board scope".into());
        }
        let node = self
            .doc()
            .scene
            .node(id)
            .cloned()
            .ok_or("Portal no longer exists")?;
        let is_native = native::recognizes(command);
        let (is_atlas, is_web) = match &node.kind {
            NodeKind::Portal(portal) => (
                command.starts_with("portal.atlas.") && portal.kind == PortalKind::FileAtlas,
                command.starts_with("portal.web.") && portal.kind == PortalKind::Web,
            ),
            _ => (false, false),
        };
        if !is_atlas && !is_web && !is_native {
            return Err("Command and portal kind do not match".into());
        }
        let needs_file_grant =
            is_native || is_atlas || (is_web && self.agent_web_needs_file_grant(id)?);
        if needs_file_grant
            && !grant
                .readable_sources
                .iter()
                .any(|scope| scope == SOURCE_INPUTS)
        {
            return Err("This grant does not include linked portal sources".into());
        }
        if is_web {
            self.agent_web_check_scope(id)?;
        }
        let native = if is_native {
            Some(native::NativeOp::prepare(self, a, call, id)?)
        } else {
            None
        };
        if command == "portal.atlas.read_file" {
            let locator = arguments["locator"]
                .as_str()
                .ok_or("Read_file needs one relative locator")?;
            self.agent_atlas_validate_locator(locator)?;
            let mode = arguments
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("text");
            if !matches!(mode, "text" | "image") {
                return Err("Read_file mode must be text or image".into());
            }
            let cursor = number(arguments, "cursor", 0, 4 * 1024 * 1024)?;
            let limit = number(arguments, "limit", 4096, 16 * 1024)?;
            if !(4..=16 * 1024).contains(&limit) {
                return Err("Text page limit must be 4–16384 bytes".into());
            }
            if mode == "image" && cursor != 0 {
                return Err("Image preview reads do not support text cursors".into());
            }
        }
        if command == "portal.atlas.highlight" {
            let locator = arguments["locator"]
                .as_str()
                .ok_or("Highlight needs one relative locator")?;
            self.agent_atlas_validate_locator(locator)?;
        }
        let contact = if command == "portal.web.click" || command == "portal.web.scroll" {
            Some(self.agent_web_contact(
                ctx.ok_or("Pointer portal actions need the live board context")?,
                id,
                page_coord(arguments, "x")?,
                page_coord(arguments, "y")?,
            )?)
        } else {
            None
        };
        let (x, y) = contact.unwrap_or_else(|| {
            let (x, y) = node.rect.center();
            (f64::from(x), f64::from(y))
        });
        let motion = self
            .presence
            .roster
            .session_mut(&a.motion)
            .ok_or("Collaborator presence ended")?;
        let target = WorldPoint { x, y };
        witness::enqueue_motion(motion, &id.0.to_string(), target, target)?;
        let op = PortalOp {
            call: call.clone(),
            portal: id,
            node,
            command: command.clone(),
            arguments: arguments.clone(),
            observer: a.profile.name.clone(),
            contact,
            text_rx: None,
            image_rx: None,
            image_path: None,
            image_verified: false,
            source_generation: None,
            source_snapshot: None,
            started: false,
            native,
        };
        a.portal = Some(op);
        a.observed_target = Some(id);
        a.activity = "approaching".into();
        a.status = "Approaching portal · visible action".into();
        Ok(None)
    }

    pub(super) fn advance_presence_portal_call(
        &mut self,
        ctx: &egui::Context,
        a: &mut Collaborator,
        elapsed: Duration,
    ) -> Option<BoardReply> {
        let mut op = a.portal.take()?;
        if let Some(native) = op.native.as_ref() {
            if let Err(reason) = native.validate(self, a, &op.call, op.portal, &op.node) {
                return Some(native::failed_reply(&op.call, "authority_changed", &reason));
            }
            if a.paused {
                native.cancel();
                return Some(native::failed_reply(
                    &op.call,
                    "revoked_before_or_after_dispatch",
                    "Native work was paused by the human; no rollback is claimed",
                ));
            }
        }
        if a.paused {
            self.release_presence_portal_focus(&op.call.request_id);
            a.portal = Some(op);
            a.activity = "paused".into();
            a.status = "Portal action paused · resume to continue".into();
            return None;
        }
        let result = (|| -> Result<Option<Value>, String> {
            let grant = a.grant.as_ref().ok_or("Board grant revoked")?;
            grant
                .authorize(&op.call, &a.binding, &a.request)
                .map_err(str::to_owned)?;
            if a.tab != self.tab().id || a.departure.is_some() {
                return Err("Board action was paused, revoked, or moved to another tab".into());
            }
            if !grant.allows_node(&op.portal.0.to_string())
                || self.doc().scene.node(op.portal) != Some(&op.node)
            {
                return Err("Portal changed or left the granted scope".into());
            }
            if op.command.starts_with("portal.web.") {
                self.agent_web_check_scope(op.portal)?;
            }
            let needs_file_grant = op.command.starts_with("portal.atlas.")
                || (op.command.starts_with("portal.web.")
                    && self.agent_web_needs_file_grant(op.portal)?);
            if needs_file_grant && !grant.readable_sources.iter().any(|s| s == SOURCE_INPUTS) {
                return Err("Portal source grant revoked".into());
            }
            let motion = self
                .presence
                .roster
                .session_mut(&a.motion)
                .ok_or("Collaborator presence ended")?;
            if !op.started {
                let sample = motion.advance(elapsed);
                if sample.phase != MotionPhase::ReadyToCommit {
                    return Ok(None);
                }
                motion.resolve_ready(|_| true);
                op.started = true;
                a.activity = "working".into();
                a.status = "At portal · working".into();
            }
            if op.command.starts_with("portal.web.") {
                self.agent_web_focus(op.portal, &a.request)?;
                if !self.web.is_live(op.portal) {
                    a.activity = "working".into();
                    a.status = "Starting Web portal · visible, cancelable focus lease".into();
                    return Ok(None);
                }
            }
            if op.command.starts_with("portal.atlas.") {
                let Some((seen, complete)) = self.agent_atlas_scan_state(op.portal) else {
                    a.activity = "working".into();
                    a.status = "Starting Atlas portal scan · visible, cancelable action".into();
                    return Ok(None);
                };
                let locator = op.arguments.get("locator").and_then(Value::as_str);
                let waiting_for_index = if op.command == "portal.atlas.list" {
                    seen == 0 && !complete
                } else {
                    locator.is_some_and(|path| {
                        !self
                            .agent_atlas_locator_known(op.portal, path)
                            .unwrap_or(false)
                            && !complete
                    })
                };
                if waiting_for_index {
                    a.activity = "working".into();
                    a.status = "Waiting for Atlas enumeration · visible, cancelable action".into();
                    return Ok(None);
                }
            }
            if op.command == "portal.web.click" || op.command == "portal.web.scroll" {
                let contact = self.agent_web_contact(
                    ctx,
                    op.portal,
                    page_coord(&op.arguments, "x")?,
                    page_coord(&op.arguments, "y")?,
                )?;
                let previous = op
                    .contact
                    .ok_or("Portal pointer contact was not prepared")?;
                let distance =
                    ((previous.0 - contact.0).powi(2) + (previous.1 - contact.1).powi(2)).sqrt();
                if distance > 0.25 {
                    let motion = self
                        .presence
                        .roster
                        .session_mut(&a.motion)
                        .ok_or("Collaborator presence ended")?;
                    let target = WorldPoint {
                        x: contact.0,
                        y: contact.1,
                    };
                    witness::enqueue_motion(motion, &op.portal.0.to_string(), target, target)?;
                    op.contact = Some(contact);
                    op.started = false;
                    a.activity = "approaching".into();
                    a.status = "Portal moved · repositioning before input".into();
                    return Ok(None);
                }
            }
            if op.native.is_some() {
                native::advance(self, a, &mut op)
            } else {
                self.execute_portal_operation(&mut op)
            }
        })();
        match result {
            Ok(Some(value)) => {
                let success = op.native.is_none() || value["status"] == "completed";
                let mut reply = portal_reply(&op.call, value);
                reply.success = success;
                Some(reply)
            }
            Ok(None) => {
                a.portal = Some(op);
                None
            }
            Err(error) => {
                self.release_presence_portal_focus(&op.call.request_id);
                self.clear_portal_highlight(&op);
                a.activity = "blocked".into();
                a.status = error.clone();
                if let Some(native) = op.native.as_ref() {
                    Some(native::failed_reply(
                        &op.call,
                        if native.admitted() {
                            "native_action_stopped_after_admission"
                        } else {
                            "not_dispatched"
                        },
                        &error,
                    ))
                } else {
                    Some(BoardReply::error(&op.call, &error))
                }
            }
        }
    }

    fn execute_portal_operation(&mut self, op: &mut PortalOp) -> Result<Option<Value>, String> {
        match op.command.as_str() {
            "portal.web.focus" => Ok(Some(json!({"status":"focused","portal_id":op.portal.0}))),
            "portal.atlas.list" => {
                let offset = number(&op.arguments, "offset", 0, usize::MAX / 2)?;
                let limit = number(&op.arguments, "limit", 50, 100)?;
                if !(1..=100).contains(&limit) {
                    return Err("Atlas page limit must be 1–100".into());
                }
                self.agent_atlas_page(op.portal, offset, limit).map(Some)
            }
            "portal.atlas.highlight" => {
                let locator = op.arguments["locator"]
                    .as_str()
                    .ok_or("Highlight needs one relative locator")?;
                self.agent_atlas_highlight(op.portal, locator, &op.call.request_id, &op.observer)
                    .map(Some)
            }
            "portal.atlas.read_file" => self.read_atlas_file(op),
            "portal.web.read_text" => self.agent_web_text(op.portal).map(|value| {
                value.map(|text| json!({"text":text,"url":self.agent_web_current_url(op.portal)}))
            }),
            "portal.web.capture" => {
                let dir = self
                    .ai
                    .config
                    .valid_workspace()
                    .map(PathBuf::from)
                    .ok_or("Establish the AI workspace before capturing a page")?;
                self.agent_web_capture(op.portal, &dir.join("portal-captures"), &op.call.id)
                    .map(|p| {
                        p.map(|path| {
                            json!({"image_source":path,"url":self.agent_web_current_url(op.portal)})
                        })
                    })
            }
            "portal.web.navigate" => {
                let url = op.arguments["url"]
                    .as_str()
                    .filter(|s| s.len() <= 2048)
                    .ok_or("Navigation URL is missing or too long")?;
                self.agent_web_navigate(op.portal, url)?;
                Ok(Some(json!({"status":"navigated","url":url})))
            }
            "portal.web.click" => {
                let x = page_coord(&op.arguments, "x")?;
                let y = page_coord(&op.arguments, "y")?;
                self.agent_web_input(op.portal, WebInput::Down { x, y, button: 0 })?;
                self.agent_web_input(op.portal, WebInput::Up { x, y, button: 0 })?;
                Ok(Some(json!({"status":"clicked","x":x,"y":y})))
            }
            "portal.web.scroll" => {
                let x = page_coord(&op.arguments, "x")?;
                let y = page_coord(&op.arguments, "y")?;
                let delta = finite(&op.arguments, "delta")?;
                let horizontal = op.arguments["horizontal"].as_bool().unwrap_or(false);
                self.agent_web_input(
                    op.portal,
                    WebInput::Wheel {
                        x,
                        y,
                        delta: delta.clamp(-2400.0, 2400.0),
                        horizontal,
                    },
                )?;
                Ok(Some(json!({"status":"scrolled"})))
            }
            "portal.web.type" => {
                let text = op.arguments["text"]
                    .as_str()
                    .filter(|s| s.chars().count() <= 512)
                    .ok_or("Typed text exceeds 512 characters")?;
                self.agent_web_input(op.portal, WebInput::Text(text.to_owned()))?;
                Ok(Some(
                    json!({"status":"typed","characters":text.chars().count()}),
                ))
            }
            "portal.web.key" => {
                let key = web_key(op.arguments["key"].as_str().ok_or("Key name is required")?)
                    .ok_or("Unsupported key")?;
                let pressed = op.arguments["pressed"]
                    .as_bool()
                    .ok_or("Key pressed must be boolean")?;
                self.agent_web_input(
                    op.portal,
                    WebInput::Key {
                        key,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                )?;
                Ok(Some(json!({"status":"key_sent"})))
            }
            _ => Err("Unknown portal operation".into()),
        }
    }

    /// Lifecycle hook for stop/revoke/request completion, including when the
    /// explicit focus operation has already returned and no PortalOp remains.
    pub(super) fn release_presence_portal_focus(&mut self, request: &str) -> bool {
        self.agent_web_release_focus(request)
    }

    fn clear_portal_highlight(&mut self, op: &PortalOp) {
        if op.command.starts_with("portal.atlas.") {
            self.agent_atlas_clear_observer(&op.call.request_id);
        }
    }
}
