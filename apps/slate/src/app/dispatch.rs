//! Command dispatch: the keyboard front-end of the registry.
//!
//! `hotkeys` builds [`atlas_commands::Chord`]s from egui key events, looks
//! them up in the registry (respecting the pre-existing suppression gates:
//! presenting, typing, text editing, board-only keys), and routes matches
//! through [`SlateApp::dispatch`] — the same entry the canvas palette, the
//! dock toggles, and the menu bar use, so every input modality drives one
//! command surface (Constitution Art. VII/VIII). Every dispatched command
//! pushes a [`HistoryEntry`] for F2 and Space/Enter repeat.
//!
//! Specially handled (not chord table rows): Space-tap / Enter-idle repeat,
//! the Esc cancel stack ([`atlas_commands::cancel_target`]), Tab cycling,
//! arrows (nudge with a selection, pan without), and Board type-to-command
//! (bare letter shortcuts hold ~700 ms so a second character can open the
//! canvas palette instead).
mod feedback;
mod input_state;
mod keys;
pub use input_state::{BareLetterHold, SpaceTap};
mod media;
use super::{board, board_align, board_path, commands, SlateApp};
use atlas_commands::{
    cancel_target, Availability, CancelLayer, Chord, CmdAuthor, CommandId, HistoryEntry, Key,
};
use eframe::egui;
use keys::*;
use slate_doc::scene::NodeKind;
use slate_doc::{ItemId, NodeId, ViewKind};
use std::time::{Duration, Instant};
/// Space taps longer than this are holds (pan chords), not repeat requests.
mod cancel;
mod capture;
mod context_menu;
mod history;
mod image_layers;
pub(crate) mod targeted;
pub(crate) use targeted::PreparedBoardCommand;

const SPACE_TAP_MAX: Duration = Duration::from_millis(250);

/// After a bare letter shortcut, wait this long before committing so a
/// following character can promote the keystroke into type-to-command entry.
const BARE_LETTER_HOLD: Duration = Duration::from_millis(700);

impl SlateApp {
    /// The availability context for this frame: active view + GLOBAL +
    /// NEEDS_SELECTION while the view's selection is non-empty.
    pub(crate) fn command_ctx(&self) -> Availability {
        let view = Availability::BOARD_VIEW;
        let has_selection = if self.doc().view.active_view == ViewKind::Board {
            !self.board_sel.is_empty()
        } else {
            !self.selection.is_empty()
        };
        let mut ctx = view | Availability::GLOBAL;
        if has_selection {
            ctx |= Availability::NEEDS_SELECTION;
        }
        ctx
    }

    /// Dispatch a command by id: availability-gated, handler bodies are the
    /// pre-registry implementations, one history push per execution.
    /// Returns whether the command ran.
    pub(crate) fn dispatch(
        &mut self,
        ctx: &egui::Context,
        id: CommandId,
        detail: Option<String>,
    ) -> bool {
        self.feedback.enter_command(id.0);
        atlas_core::session_log::event("slate.command.begin", id.0);
        let ran = self.dispatch_inner(ctx, id, detail);
        atlas_core::session_log::event("slate.command.end", id.0);
        self.feedback.exit_command();
        ran
    }

    fn dispatch_inner(
        &mut self,
        ctx: &egui::Context,
        id: CommandId,
        detail: Option<String>,
    ) -> bool {
        let Some(spec) = self.registry.by_id(id) else {
            return false;
        };
        if !spec.when.matches(self.command_ctx()) {
            return false; // unavailable commands are no-ops (registry contract)
        }
        // A command lands after the glide it interrupts, never inside it.
        self.finish_bumper_glide();
        let board = self.doc().view.active_view == ViewKind::Board;
        let mut detail = detail;
        let ran = match id.0 {
            command if command.starts_with("app.recovery.") => {
                self.dispatch_recovery(ctx, command, detail.as_deref())
            }
            command if command.starts_with("presence.") => {
                self.dispatch_presence(ctx, command, detail.as_deref())
            }
            command if command.starts_with("board.objects.") => {
                return self.dispatch_targeted_human_with_context(ctx, command, detail.as_deref());
            }
            "board.wire.canvas" => self.open_canvas_wire_settings(ctx),
            "board.image.shape_fill" => return self.shape_fill_command(detail.as_deref()),
            "board.image.layer.rename"
            | "board.image.layer.visibility"
            | "board.image.layer.delete"
            | "board.image.layer.duplicate"
            | "board.image.layer.reorder" => self.dispatch_image_layer(id.0, detail.as_deref()),
            "board.shape.edit" | "board.wire.edit" if detail.is_none() => {
                self.open_object_properties()
            }
            "board.sheet.edit" => self.edit_selected_sheet(),
            "board.open_file" => self.open_selected_image_file(),
            "board.open_location" => self.open_selected_file_location(),
            "board.tags.add" => self.dispatch_tag_add(detail.as_deref()),
            "board.tags.visibility" => self.dispatch_tag_visibility(detail.as_deref()),
            "board.tags.create" | "board.tags.assign" | "board.tags.unassign" => {
                return self.dispatch_targeted_human_with_context(ctx, id.0, detail.as_deref());
            }
            "portal.agent.run" => self.dispatch_image_generate(detail.as_deref()),
            command if command.starts_with("portal.artifact.") => {
                self.dispatch_artifact(command, detail.as_deref())
            }
            "portal.agent.image_count" => self.dispatch_image_count(detail.as_deref()),
            "board.curve.fade" => self.dispatch_curve_fade(detail.as_deref()),
            "board.image.layer.add_image" => self.pick_image_layer_command(detail.as_deref()),
            "board.shape.edit" | "board.wire.edit" => {
                self.shape_property_command(detail.as_deref())
            }
            "board.shape.dimension" => self.shape_dimension_command(detail.as_deref()),
            "board.shape.fillet" => self.shape_fillet_command(detail.as_deref()),
            "board.shape.sides" => self.shape_sides_command(detail.as_deref()),
            "board.color.desktop" => {
                self.start_tool_desktop_sample(detail.as_deref() == Some("background"), false);
                true
            }
            "app.updates.check" => {
                self.updater.check(true);
                true
            }
            "app.updates.download" => {
                self.updater.download();
                true
            }
            "app.updates.later" => {
                self.updater.visible = false;
                true
            }
            "app.updates.install" => {
                if let Some(reason) = self.update_close_blocked() {
                    self.toast(reason);
                } else {
                    self.updater.install();
                }
                true
            }
            // ----- app / workbook -------------------------------------------------
            "app.open" => {
                self.open_doc_dialog();
                true
            }
            "app.save" => {
                self.save_doc();
                true
            }
            "app.save_as" => {
                self.save_doc_as_dialog();
                true
            }
            "board.assets.collect" => self.collect_assets_into_workbook(),
            "app.new_tab" => {
                self.new_tab();
                true
            }
            "app.export" => {
                self.export_artifact_dialog();
                true
            }
            command
                if matches!(
                    command,
                    "board.media.image"
                        | "board.media.model"
                        | "board.media.video"
                        | "board.media.text"
                        | "board.media.text_new"
                        | "board.media.page"
                ) =>
            {
                self.dispatch_media_intake(ctx, command, detail.as_deref())
            }
            command if Self::image_action_command(command) => {
                self.dispatch_image_result_action(ctx, command, detail.as_deref())
            }
            "board.objects.link_media" => self.dispatch_link_media(detail.as_deref()),
            command if command.starts_with("board.model.connector.") => {
                self.dispatch_application_connector(command, detail.as_deref())
            }
            "board.model_display" => self.dispatch_model_display(detail.as_deref()),
            "board.model_materials" => self.dispatch_model_materials(detail.as_deref()),
            "board.model_measure" => self.dispatch_model_measure(detail.as_deref()),
            command if capture::owns_command(command) => {
                self.dispatch_portal_capture(ctx, id.0, detail.as_deref())
            }
            "board.model_view_restore" => false,
            "board.media.unbundle" => {
                let (node, focus) = if let Some(detail) = detail.as_deref() {
                    let mut parts = detail.split(':');
                    let node = parts
                        .next()
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(slate_doc::NodeId);
                    let focus = parts.next().and_then(|s| s.parse::<u16>().ok());
                    (node, focus)
                } else {
                    (self.selected_paged_node(), None)
                };
                node.is_some_and(|id| self.unbundle_paged_media(id, focus))
            }
            "app.add_files" => {
                self.add_files_dialog();
                true
            }
            "app.home" => {
                self.go_home();
                true
            }
            "app.undo" => {
                self.board_undo();
                true
            }
            "app.redo" => {
                self.board_redo();
                true
            }
            "app.select_all" => {
                if board {
                    // Hidden and locked nodes are never selectable
                    // (scene-flags semantics matrix).
                    self.board_sel = self
                        .doc()
                        .scene
                        .nodes
                        .iter()
                        .filter(|n| !n.hidden && !n.locked)
                        .map(|n| n.id)
                        .collect();
                    detail = detail.or(Some(format!("{} node(s)", self.board_sel.len())));
                } else {
                    let all: Vec<ItemId> = self.doc().items.iter().map(|it| it.id).collect();
                    detail = detail.or(Some(format!("{} item(s)", all.len())));
                    self.selection = all.into_iter().collect();
                }
                true
            }
            "app.present" => {
                self.start_present(None);
                true
            }
            "app.fullscreen" => {
                self.toggle_canvas_fullscreen();
                true
            }
            "app.feedback.open" => self.feedback_open_command(),
            "app.feedback.report" => self.feedback_report_command(),
            "app.feedback.finish_recording" => {
                self.feedback.finish_recording();
                self.feedback.phase = self.feedback.form_after_recording();
                true
            }
            "app.help" => {
                // F1: the Advanced window (the Commands & shortcuts reference
                // is its dedicated section; P1 has no per-section scroll).
                self.chrome_mut().advanced_open = true;
                true
            }
            "board.hover_highlight" => {
                if let Some(kind) = detail.as_deref() {
                    if !slate_doc::scene::NodeKind::KIND_NAMES.contains(&kind) {
                        return false;
                    }
                    if !self.settings.hover_highlight_disabled.remove(kind) {
                        self.settings
                            .hover_highlight_disabled
                            .insert(kind.to_owned());
                    }
                    self.board_hover_glow.clear();
                    self.settings.save();
                } else {
                    self.chrome_mut().advanced_open = true;
                }
                true
            }
            "app.preferences" => {
                self.chrome_mut().advanced_open = true;
                true
            }
            "app.optional.bumper_cars" => {
                let on = !self.settings.optional_bumper_cars;
                self.settings.optional_bumper_cars = on;
                self.settings.save();
                detail = detail.or(Some(if on { "on" } else { "off" }.into()));
                true
            }
            "app.session.mark" => {
                self.session_log.mark("F4");
                self.toast("Marked this moment in the session log");
                true
            }
            "app.session.reveal" => {
                self.session_log.reveal();
                true
            }
            "app.crash_reports.open" => self.open_crash_reports(),
            "app.history" => {
                self.history_open = !self.history_open;
                true
            }
            "app.properties" => {
                // Selection editing lives on the per-node strips. The bottom
                // dock no longer hosts a Selection icon or inspector.
                detail = detail.or(Some("object strip".into()));
                true
            }
            "app.repeat_last" => {
                if let Some(last) = self.cmd_history.last_repeatable(&self.registry) {
                    self.dispatch(ctx, last, Some("repeat".into()))
                } else {
                    false
                }
            }
            "app.cancel" => self.cancel_pop(),
            // ----- canvas overlays ------------------------------------------------
            "canvas.minimap" => {
                self.toggle_minimap();
                detail = detail.or(Some(if self.minimap_on { "on" } else { "off" }.into()));
                true
            }
            "canvas.search" => {
                self.open_search();
                true
            }
            "canvas.cycle_next" => {
                self.cycle_objects(1);
                true
            }
            "canvas.tool.zoom" => {
                // Z toggles the transient zoom mode (camera-only, never
                // journaled); the underlying tool re-arms on disarm.
                self.zoom_armed = !self.zoom_armed;
                if !self.zoom_armed {
                    self.zoom_marquee = None;
                }
                detail = detail.or(Some(if self.zoom_armed { "armed" } else { "off" }.into()));
                true
            }
            "canvas.fit" => {
                self.fit_active_view();
                true
            }
            // ----- board tools ------------------------------------------------------
            "board.tool.select" => {
                self.set_board_tool(board::BoardTool::Select);
                true
            }
            "board.tool.pan" => {
                self.set_board_tool(board::BoardTool::Pan);
                true
            }
            "board.tool.frame" => {
                self.set_board_tool(board::BoardTool::Frame);
                true
            }
            "board.tool.deck" => {
                self.set_board_tool(board::BoardTool::Deck);
                true
            }
            "board.tool.rect" => {
                self.set_board_tool(board::BoardTool::RectShape);
                true
            }
            "board.tool.ellipse" => {
                self.set_board_tool(board::BoardTool::Ellipse);
                true
            }
            "board.tool.polygon" => {
                self.set_board_tool(board::BoardTool::Polygon);
                true
            }
            "board.tool.line" => {
                self.set_board_tool(board::BoardTool::Line);
                true
            }
            "board.tool.pen" => {
                self.set_board_tool(board::BoardTool::Pen);
                true
            }
            "board.tool.polyline" => {
                self.set_board_tool(board::BoardTool::Polyline);
                true
            }
            "board.tool.arc" => {
                self.set_board_tool(board::BoardTool::Arc);
                true
            }
            "board.tool.bezier" => {
                self.set_board_tool(board::BoardTool::BezierSpan);
                true
            }
            "board.tool.text" => {
                self.set_board_tool(board::BoardTool::Text);
                true
            }
            "board.tool.brush" => {
                // Re-arming also forgets where the next Shift segment starts.
                if let Some(image) = self.selected_image_nodes().first().copied() {
                    self.arm_image_brush(image);
                } else {
                    self.set_board_tool(board::BoardTool::Brush);
                }
                true
            }
            "board.tool.eraser" => {
                self.set_board_tool(board::BoardTool::Eraser);
                true
            }
            "board.tool.smooth" => {
                self.set_board_tool(board::BoardTool::Smooth);
                true
            }
            "board.tool.eyedropper" => {
                self.set_board_tool(board::BoardTool::Eyedropper);
                true
            }
            "board.tool.sticky" => {
                self.set_board_tool(board::BoardTool::Sticky);
                true
            }
            "board.tool.direct_select" => {
                self.set_board_tool(board::BoardTool::DirectSelect);
                true
            }
            "board.portal.agent" => {
                self.set_board_tool(board::BoardTool::AgentPortal);
                true
            }
            "board.portal.web" => {
                self.set_board_tool(board::BoardTool::WebPortal);
                true
            }
            "board.portal.atlas" => {
                self.set_board_tool(board::BoardTool::AtlasPortal);
                true
            }
            "board.portal.slate" => {
                self.set_board_tool(board::BoardTool::SlatePortal);
                true
            }
            "portal.slate.source" => self.slate_pick_source_for_selection(),
            "portal.slate.refresh" => {
                if let Some(id) = self.selected_slate_portal() {
                    self.slate_refresh(ctx, id);
                } else {
                    self.toast("Select a Slate board portal first.");
                }
                true
            }
            "portal.slate.open" => {
                if !self.open_selected_slate_portal() {
                    self.toast("Select a Slate board portal first.");
                }
                true
            }
            "portal.atlas.source" => self.atlas_pick_source_for_selection(),
            "portal.atlas.refresh" => self.atlas_refresh_selected(),
            "portal.atlas.bake" => self.atlas_bake_selected(),
            "portal.atlas.focus" => self.atlas_toggle_focus(),
            "portal.atlas.open" => self.atlas_open_in_file_atlas(ctx, None),
            "portal.atlas.fit" => self.atlas_fit_selected(),
            "board.tool.trim" => {
                self.set_board_tool(board::BoardTool::Trim);
                true
            }
            "board.tool.split" => {
                self.set_board_tool(board::BoardTool::Split);
                true
            }
            "portal.web.source" => self.web_source_command(detail.as_deref()),
            command if self.web_consent_command(command) => true,
            "portal.web.home" => self.web_home_selected(),
            "portal.web.reload" => self.web_reload_selected(),
            "portal.web.back" => self.web_history_step(false),
            "portal.web.forward" => self.web_history_step(true),
            "portal.web.recapture" => self.web_recapture_selected(),
            "portal.web.focus" => self.web_toggle_focus(),
            "portal.web.open_external" => self.web_open_external(),
            "portal.web.package" => self.web_package_selected(),
            "portal.web.bake" => self.web_bake_selected(),
            "portal.web.copy_url" => self.web_copy_url(ctx),
            "portal.web.paste_url" => self.web_paste_url(),
            "portal.maximize" => self.portal_maximize_selected(),
            "portal.chrome.toggle" => self.portal_toggle_chrome_selected(),
            "portal.agent.bind" => self.pick_selected_agent_project(),
            "portal.agent.send" => self.send_selected_agent_prompt(),
            "portal.agent.wire_output" => self.toggle_agent_wire_output(),
            "portal.agent.unbundle" => self.unbundle_selected_agent(),
            "portal.agent.stop" => self.stop_selected_agent(),
            "portal.agent.train" => self.agent_show_train(),
            "portal.agent.pairs" => self.agent_show_pairs(),
            "portal.agent.model" => self.agent_set_model(detail.as_deref().unwrap_or("")),
            "portal.agent.live" => self.agent_toggle_live(),
            "portal.agent.keep" => self.agent_keep_live(),
            "portal.agent.rename" => match detail.as_deref() {
                Some(title) => self.agent_rename(title),
                None => self.begin_agent_rename(),
            },
            "portal.agent.continue" => self.agent_spawn_command(detail.as_deref()),
            "portal.agent.spawn" => self.spawn_flow_node(detail.as_deref()),
            "portal.agent.approval" => self.answer_agent_approval(detail.as_deref()),
            "portal.agent.artifacts" => self.toggle_agent_artifacts(detail.as_deref()),
            "portal.agent.open_artifact" => self.open_agent_artifact(detail.as_deref()),
            "portal.agent.spawn_output" => self.agent_spawn_output(detail.as_deref()),
            "portal.agent.spawn_outputs" => self.agent_spawn_outputs(detail.as_deref()),
            "portal.agent.evolution" => self.agent_evolution(detail.as_deref()),
            "portal.agent.chat" => self.agent_show_chat(),
            "portal.agent.bundle_chat" => self.agent_bundle_selection(),
            "portal.agent.expand_chat" => self.agent_expand_bundle(),
            "portal.agent.fork" => self.agent_fork_selected(detail.as_deref()),
            "portal.agent.collapse" => self.agent_fold(ctx, detail.as_deref()),
            "portal.agent.fit" => self.agent_fit_to_text(ctx),
            "portal.agent.board_access" => self.open_chat_train_board_access(),
            "portal.agent.full_access" => self.agent_toggle_full_access(),
            "portal.artifact.full_access" => self.dashboard_set_full_access(detail.as_deref()),
            "portal.agent.model_parameter" => self.generator_set_parameter(detail.as_deref()),
            "portal.agent.schedule" => self.agent_set_schedule(detail.as_deref()),
            "portal.agent.pocket" => self.agent_toggle_pocket(detail.as_deref()),
            "portal.agent.crosstalk.link" => self.crosstalk_link_selected(detail.as_deref()),
            "portal.agent.crosstalk.start" => self.crosstalk_start(detail.as_deref()),
            "portal.agent.crosstalk.capsule" => self.crosstalk_capsule_command(detail.as_deref()),
            "portal.agent.crosstalk.pause" => self.crosstalk_pause(detail.as_deref()),
            "portal.agent.crosstalk.resume" => self.crosstalk_resume(detail.as_deref()),
            "portal.agent.crosstalk.stop" => self.crosstalk_stop(detail.as_deref()),
            "portal.agent.crosstalk.edit" => self.crosstalk_edit(detail.as_deref()),
            "portal.agent.crosstalk.send" => self.crosstalk_send(detail.as_deref()),
            "portal.agent.crosstalk.skip" => self.crosstalk_skip(detail.as_deref()),
            "portal.agent.crosstalk.more" => self.crosstalk_more(detail.as_deref()),
            "portal.agent.crosstalk.trust" => self.crosstalk_trust(detail.as_deref()),
            "portal.agent.identity" => {
                self.agent_set_detail(slate_doc::agent_chat::Detail::Identity)
            }
            "portal.agent.summary" => self.agent_set_detail(slate_doc::agent_chat::Detail::Summary),
            "portal.agent.full" => self.agent_set_detail(slate_doc::agent_chat::Detail::Full),
            "portal.agent.provider" => self.toggle_selected_agent_provider(),
            "portal.agent.reveal" => self.reveal_selected_agent_link(),
            "portal.agent.launch" => self.launch_selected_agent_provider(),
            "portal.agent.focus" => self.agent_toggle_focus(),
            "portal.agent.get_key" => self.open_cursor_api_key_page(),
            "portal.agent.switch_chat" => self.switch_selected_agent_chat(),
            "stage.accept" => self.accept_selected_stage_proposal(),
            "stage.reject" => self.reject_selected_stage_proposal(),
            // ----- color state + widths -------------------------------------------
            "board.colors.default" => {
                self.reset_board_colors();
                true
            }
            "board.colors.swap" => {
                self.swap_board_colors();
                true
            }
            "board.brush.width_down" | "board.brush.width_up" => {
                let (w, _eraser) = self.step_active_width(id.0 == "board.brush.width_up");
                let label = match self.board_tool {
                    board::BoardTool::Eraser => "eraser",
                    board::BoardTool::Smooth => "smooth",
                    _ => "brush",
                };
                detail = detail.or(Some(format!("{label} → {w:.1}u")));
                true
            }
            "board.brush.softness_down" | "board.brush.softness_up" => {
                if !matches!(
                    self.board_tool,
                    board::BoardTool::Brush | board::BoardTool::Eraser | board::BoardTool::Smooth
                ) {
                    false
                } else {
                    let softness = self.step_brush_softness(id.0 == "board.brush.softness_up");
                    detail = detail.or(Some(format!("{:.0}%", softness * 100.0)));
                    true
                }
            }
            // ----- path editing -----------------------------------------------------
            "board.path.join" => {
                let ran = self.cmd_join();
                if ran {
                    self.toast("Joined");
                }
                ran
            }
            "board.mirror.horizontal" | "board.mirror.vertical" => {
                let axis = if id.0 == "board.mirror.horizontal" {
                    slate_doc::mirror::MirrorAxis::Horizontal
                } else {
                    slate_doc::mirror::MirrorAxis::Vertical
                };
                let ids: Vec<NodeId> = self
                    .board_sel
                    .iter()
                    .copied()
                    .filter(|id| {
                        self.doc()
                            .scene
                            .node(*id)
                            .is_some_and(|n| slate_doc::mirror::node_mirrors(self.doc(), n))
                    })
                    .collect();
                if ids.is_empty() {
                    false
                } else {
                    self.patch_nodes(&ids, move |n| {
                        slate_doc::mirror::mirror_on_board(n, axis);
                    });
                    self.last_board_edit = None; // toggles never coalesce
                    detail = detail.or(Some(format!("{} node(s)", ids.len())));
                    true
                }
            }
            // ----- scene flags ------------------------------------------------------
            "board.group" => {
                let n = self.cmd_group_selection();
                detail = detail.or(Some(format!("{n} node(s)")));
                n > 0
            }
            "board.ungroup" => {
                let n = self.cmd_ungroup_selection();
                detail = detail.or(Some(format!("{n} node(s)")));
                n > 0
            }
            "board.hide" => {
                let n = self.cmd_hide_selection();
                detail = detail.or(Some(format!("{n} node(s)")));
                n > 0
            }
            "board.show_all" => {
                let n = self.cmd_show_all_hidden();
                detail = detail.or(Some(format!("{n} node(s)")));
                n > 0
            }
            "board.lock" => {
                let n = self.cmd_lock_selection();
                detail = detail.or(Some(format!("{n} node(s)")));
                n > 0
            }
            "board.unlock_all" => {
                let n = self.cmd_unlock_all();
                detail = detail.or(Some(format!("{n} node(s)")));
                n > 0
            }
            // ----- board commands -----------------------------------------------------
            "board.fit" => {
                self.fit_board();
                true
            }
            "board.duplicate" => {
                let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
                if ids.is_empty() {
                    false
                } else {
                    detail = detail.or(Some(format!("{} node(s)", ids.len())));
                    !self.duplicate_board_nodes(&ids, 24.0, 24.0).is_empty()
                }
            }
            "board.align.left"
            | "board.align.center_h"
            | "board.align.right"
            | "board.align.top"
            | "board.align.middle_v"
            | "board.align.bottom"
            | "board.distribute.horizontal"
            | "board.distribute.vertical" => {
                let Some(action) = board_align::AlignAction::from_command_id(id.0) else {
                    return false;
                };
                let n = self.board_sel.len();
                detail = detail.or(Some(format!("{} node(s)", n)));
                self.apply_align_action(action)
            }
            "board.delete" => {
                if self.delete_picked_vertices() {
                    return true;
                }
                use super::board_image_layers::ImageStripFocus;
                if let Some(session) = self.image_paint.as_ref() {
                    if self.board_sel.len() == 1 && self.board_sel.contains(&session.image) {
                        if let ImageStripFocus::Layer(idx) = session.focus {
                            let image = session.image;
                            if self.image_layer_index_exists(image, idx) {
                                self.delete_paint_layer(image, idx);
                                return true;
                            }
                        }
                    }
                }
                let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
                if ids.is_empty() {
                    false
                } else {
                    detail = detail.or(Some(format!("{} node(s)", ids.len())));
                    self.delete_board_nodes(&ids);
                    true
                }
            }
            "board.to_front" | "board.to_back" => {
                let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
                if ids.is_empty() {
                    false
                } else {
                    detail = detail.or(Some(format!("{} node(s)", ids.len())));
                    self.reorder_nodes(&ids, id.0 == "board.to_front");
                    true
                }
            }
            "board.dock.drop" => {
                let palette = self
                    .dock_pins
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "tool.shapes".into());
                self.drop_dock_strip(ctx, &palette);
                detail = detail.or(Some(palette));
                true
            }
            "board.dock.advanced" => {
                // Retired from palette chrome; the id stays on the command
                // surface so Advanced → Commands does not go stale.
                true
            }
            "dock.bar.toggle" => {
                let on = atlas_shell::dock::bar_collapsed(ctx, "slate_tools").unwrap_or(false);
                atlas_shell::dock::set_bar_collapsed(ctx, "slate_tools", !on);
                self.dock_bar_collapsed = !on;
                self.save_chrome_prefs();
                true
            }
            "board.grid" => {
                self.board_show_grid = !self.board_show_grid;
                detail = detail.or(Some(if self.board_show_grid { "on" } else { "off" }.into()));
                true
            }
            "board.snap_grid" => {
                self.board_snap_grid = !self.board_snap_grid;
                detail = detail.or(Some(if self.board_snap_grid { "on" } else { "off" }.into()));
                true
            }
            "board.smart_guides" => {
                self.board_smart_guides = !self.board_smart_guides;
                self.persist_osnap();
                detail = detail.or(Some(
                    if self.board_smart_guides { "on" } else { "off" }.into(),
                ));
                true
            }
            "board.snap_reach" => {
                self.board_snap_reach = self.board_snap_reach.next();
                self.persist_osnap();
                detail = detail.or(Some(self.board_snap_reach.label().into()));
                true
            }
            "board.snap_reach.tight" | "board.snap_reach.nearby" | "board.snap_reach.wide" => {
                self.board_snap_reach = match id.0 {
                    "board.snap_reach.tight" => super::settings::SnapReach::Tight,
                    "board.snap_reach.wide" => super::settings::SnapReach::Wide,
                    _ => super::settings::SnapReach::Nearby,
                };
                self.persist_osnap();
                detail = detail.or(Some(self.board_snap_reach.label().into()));
                true
            }
            "board.osnap" => {
                self.board_osnap.enabled = !self.board_osnap.enabled;
                self.persist_osnap();
                let state = if self.board_osnap.enabled {
                    "on"
                } else {
                    "off"
                };
                detail = detail.or(Some(state.into()));
                true
            }
            "board.osnap.end" | "board.osnap.mid" | "board.osnap.center" | "board.osnap.near"
            | "board.osnap.int" | "board.osnap.quad" | "board.osnap.perp" | "board.osnap.tan" => {
                let kind = match id.0 {
                    "board.osnap.end" => slate_doc::SnapKind::End,
                    "board.osnap.mid" => slate_doc::SnapKind::Mid,
                    "board.osnap.center" => slate_doc::SnapKind::Center,
                    "board.osnap.near" => slate_doc::SnapKind::Near,
                    "board.osnap.int" => slate_doc::SnapKind::Intersection,
                    "board.osnap.quad" => slate_doc::SnapKind::Quadrant,
                    "board.osnap.perp" => slate_doc::SnapKind::Perpendicular,
                    "board.osnap.tan" => slate_doc::SnapKind::Tangent,
                    _ => unreachable!(),
                };
                self.board_osnap.toggle(kind);
                self.persist_osnap();
                let state = if self.board_osnap.is_kind_remembered(kind) {
                    "on"
                } else {
                    "off"
                };
                detail = detail.or(Some(format!("{} {state}", kind.token())));
                true
            }
            "board.wire.routing" | "board.wire.bezier" | "board.wire.orthogonal" => {
                let wires = self.wire_routing_targets(detail.as_deref());
                let routing = match id.0 {
                    "board.wire.bezier" => slate_doc::WireRouting::Bezier,
                    "board.wire.orthogonal" => slate_doc::WireRouting::Orthogonal,
                    _ => wires
                        .iter()
                        .find_map(|wire| match &self.doc().scene.node(*wire)?.kind {
                            slate_doc::scene::NodeKind::Connector(c) => {
                                Some(c.effective_routing(self.board_wire_routing))
                            }
                            _ => None,
                        })
                        .unwrap_or(self.board_wire_routing)
                        .toggle(),
                };
                let label = Some(routing.label().to_ascii_lowercase());
                detail = if wires.is_empty() {
                    detail.or(label)
                } else {
                    label
                };
                self.set_wire_routing(routing, wires)
            }
            "board.ortho" => {
                self.board_ortho = !self.board_ortho;
                self.settings.board_ortho = self.board_ortho;
                self.settings.save();
                let state = if self.board_ortho { "on" } else { "off" };
                detail = detail.or(Some(state.into()));
                self.toast(format!("Ortho {state}"));
                true
            }
            "board.crop" => {
                // C: enter crop on the selected croppable images. The rest of
                // the selection stays. No-op when none of them can crop.
                if let Some(node_id) = self
                    .board_sel
                    .iter()
                    .copied()
                    .find(|id| self.croppable_image(*id))
                {
                    self.enter_crop_mode(node_id);
                    true
                } else {
                    false
                }
            }
            "board.image.adjust" => {
                if self.selected_image_nodes().is_empty() {
                    false
                } else {
                    self.adjust_popover_open = !self.adjust_popover_open;
                    true
                }
            }
            "board.image.segment.commit" => self.commit_hover_segment(None).is_some(),
            "board.image.selection.lasso" => self.dispatch_image_selection(true),
            "board.image.selection.layer" => self.dispatch_image_selection(false),
            "board.image.selection.copy" => self.dispatch_image_selection_copy(ctx),
            "board.image.invert" => {
                let images = self.selected_image_nodes();
                if images.is_empty() {
                    false
                } else {
                    let current = self
                        .doc()
                        .scene
                        .node(images[0])
                        .and_then(|n| match &n.kind {
                            NodeKind::Image(img) => Some(img.adjust.invert),
                            _ => None,
                        })
                        .unwrap_or(0.0);
                    let target = if current > 0.0 { 0.0 } else { 1.0 };
                    self.patch_nodes(&images, move |n| {
                        if let NodeKind::Image(img) = &mut n.kind {
                            img.adjust.invert = target;
                        }
                    });
                    self.last_board_edit = None; // toggles never coalesce
                    detail = detail.or(Some(format!(
                        "{} → {}",
                        images.len(),
                        if target > 0.0 { "on" } else { "off" }
                    )));
                    true
                }
            }
            "board.copy" => {
                let n = self.board_copy(ctx);
                detail = detail.or(Some(format!("{n} node(s)")));
                n > 0
            }
            "board.cut" => {
                let n = self.board_cut(ctx);
                detail = detail.or(Some(format!("{n} node(s)")));
                n > 0
            }
            "board.paste" | "board.paste_in_place" => {
                let at = if id.0 == "board.paste" {
                    Some(self.paste_target_world(ctx))
                } else {
                    None
                };
                // Slate's own clipboard format comes first: a Slate copy also
                // carries a bitmap, files, or text for other apps. Otherwise
                // an outside image or file list lands, then OS clipboard text
                // (JSON from an older Slate still parses), then the buffer.
                let os_text = self.pending_paste_text.take();
                let at_media = at.unwrap_or_else(|| self.paste_target_world(ctx));
                if let Some(n) = self.paste_slate_clipboard(at) {
                    detail = detail.or(Some(format!("{n} node(s)")));
                    n > 0
                } else if self.paste_os_clipboard(at_media, os_text.as_deref()) {
                    detail = detail.or(Some("from clipboard".into()));
                    true
                } else {
                    let n = self.board_paste(os_text.as_deref(), at);
                    detail = detail.or(Some(format!("{n} node(s)")));
                    n > 0
                }
            }
            "board.palette" => {
                let screen = self.canvas_rect.center();
                let world = self.screen_to_world(screen);
                self.palette_state.open_at(screen, world);
                true
            }
            // Documentation-only rows (mouse gestures, per-view keys) fall
            // through: nothing to run here.
            _ => false,
        };
        if ran && id.0 != "board.palette" {
            // board.palette pushes from its double-click site (open_board_palette).
            self.push_history(id, detail);
        }
        ran
    }

    /// Image nodes in the current board selection.
    pub(crate) fn selected_image_nodes(&self) -> Vec<NodeId> {
        self.board_sel
            .iter()
            .copied()
            .filter(|id| {
                matches!(
                    self.doc().scene.node(*id).map(|n| &n.kind),
                    Some(NodeKind::Image(_))
                )
            })
            .collect()
    }
    /// Where Ctrl+V lands: the pointer when it hovers the canvas, else the
    /// view center.
    fn paste_target_world(&self, ctx: &egui::Context) -> egui::Pos2 {
        ctx.input(|i| i.pointer.hover_pos())
            .filter(|p| self.canvas_rect.contains(*p))
            .map(|p| self.screen_to_world(p))
            .unwrap_or_else(|| self.tab().cam.offset.to_pos2())
    }
    /// Fit the active view (used by the palette's `canvas.fit`). Board uses
    fn fit_active_view(&mut self) {
        self.fit_board();
    }

    // ---------- the keyboard front-end ----------

    pub(crate) fn hotkeys(&mut self, ctx: &egui::Context) {
        // Route the native bitmap-only Ctrl+V edge to focused chat or the board.
        let focused = ctx.input(|i| i.focused);
        let mut paste_edge = self.take_paste_chord_edge(focused);
        if self.presence_poll_attachments(ctx, paste_edge.is_some()) {
            paste_edge = None;
        }
        // Presentation mode owns the keyboard (handled in present_frame).
        if self.presenting.is_some() {
            self.space_tap = SpaceTap::default();
            self.bare_letter_hold = None;
            return;
        }
        let tag_editor_open = super::ui::tools::tags::editor_open(self, ctx);
        if !tag_editor_open && self.image_selection_keys(ctx) {
            self.space_tap = SpaceTap::default();
            self.bare_letter_hold = None;
            return;
        }
        let wants_kb = ctx.wants_keyboard_input() || tag_editor_open;
        let board = self.doc().view.active_view == ViewKind::Board;
        let editing = self.text_compose_active() || self.sheet_edit.is_some();
        // A focused web portal is a keyboard sink, like an inline editor: bare
        // letters, digits, Tab, and arrows belong to the page, so typing in a
        // form cannot switch tools (D22). Ctrl chords except clipboard stay
        // Slate's — save/undo work — and Escape stays out so it can still peel
        // the focus back off.
        let web_focus = board && self.web.focused.is_some();
        let agent_focus = board && self.agents.focused.is_some();
        let typing_sink = editing || web_focus || agent_focus;
        // A selected chat card stays deletable while it holds the keyboard,
        // unless the person is typing into its composer.
        let agent_delete = if board && !editing && !web_focus {
            self.agent_delete_keys(wants_kb)
        } else {
            (false, false)
        };
        let delete_passes = |chord: Chord, id: CommandId| {
            id.0 == "board.delete"
                && !chord.ctrl
                && ((chord.key == Key::Delete && agent_delete.0)
                    || (chord.key == Key::Backspace && agent_delete.1))
        };
        if board && !typing_sink && !wants_kb && self.selected_slate_enter(ctx) {
            return;
        }
        let palette_open = self.palette_state.open;
        let cmd_ctx = self.command_ctx();
        // Type-to-command / bare-letter hold: Board only, and never while a
        // draft or inline editor owns digits/letters.
        let command_typing_ok = board
            && !wants_kb
            && !typing_sink
            && !palette_open
            && self.line_draft.is_none()
            && self.board_path_draft.is_none()
            && self.board_crop.is_none()
            && self.wire_label_edit.is_none()
            && !self.text_compose_active()
            && !self.search.open;

        struct Keys {
            matched: Vec<CommandId>,
            /// Bare A–Z chords deferred for the type-to-command hold window.
            bare_letter: Option<(CommandId, char)>,
            /// Printable text typed this frame (lowercase), for command entry.
            typed: String,
            escape: bool,
            enter: bool,
            tab: Option<i64>,
            arrows: (f32, f32),
            shift: bool,
            repeat_via_space: bool,
            paste_text: Option<String>,
            pointer_pressed: bool,
        }
        let mut keys = ctx.input(|i| {
            let mut k = Keys {
                matched: Vec::new(),
                bare_letter: None,
                typed: String::new(),
                escape: false,
                enter: false,
                tab: None,
                arrows: (0.0, 0.0),
                shift: i.modifiers.shift,
                repeat_via_space: false,
                paste_text: None,
                pointer_pressed: i.pointer.any_pressed(),
            };
            // --- Space tap = repeat-last (release-fired; Space+drag stays pan) ---
            if i.key_pressed(egui::Key::Space)
                && self.space_tap.pressed_at.is_none()
                && !wants_kb
                && !typing_sink
                && !palette_open
            {
                self.space_tap.pressed_at = Some(Instant::now());
                self.space_tap.pointer_used = i.pointer.any_down();
            }
            if self.space_tap.pressed_at.is_some() && i.pointer.any_down() {
                self.space_tap.pointer_used = true;
            }
            if i.key_released(egui::Key::Space) {
                if let Some(at) = self.space_tap.pressed_at.take() {
                    let tapped = at.elapsed() < SPACE_TAP_MAX && !self.space_tap.pointer_used;
                    if tapped && !wants_kb && !typing_sink && !palette_open {
                        k.repeat_via_space = true;
                    }
                }
                self.space_tap.pointer_used = false;
            }

            // --- chord lookup over the registry ---
            for spec in self.registry.iter() {
                let Some(chord) = spec.chord else { continue };
                if !chord_pressed(i, chord) {
                    continue;
                }
                if suppressed(chord, wants_kb, typing_sink, palette_open)
                    && (palette_open || !delete_passes(chord, spec.id))
                {
                    continue;
                }
                if !spec.when.matches(cmd_ctx) || (web_focus && web_clipboard_command(spec.id)) {
                    continue;
                }
                // On the board, bare letters wait out the hold window so a
                // second character can open type-to-command instead. A held
                // letter's auto-repeat is neither.
                if command_typing_ok && chord.is_bare_letter() {
                    if let Some(ch) = chord.key.as_letter().filter(|_| chord_fresh(i, chord)) {
                        k.bare_letter = Some((spec.id, ch));
                    }
                    continue;
                }
                push_unique(&mut k.matched, spec.id);
            }
            for (chord, id) in commands::ALIAS_CHORDS {
                if !chord_pressed(i, *chord) {
                    continue;
                }
                if suppressed(*chord, wants_kb, typing_sink, palette_open)
                    && (palette_open || !delete_passes(*chord, *id))
                {
                    continue;
                }
                if let Some(spec) = self.registry.by_id(*id) {
                    if !spec.when.matches(cmd_ctx) || (web_focus && web_clipboard_command(spec.id))
                    {
                        continue;
                    }
                    if command_typing_ok && chord.is_bare_letter() {
                        if let Some(ch) = chord.key.as_letter().filter(|_| chord_fresh(i, *chord)) {
                            k.bare_letter = Some((*id, ch));
                        }
                        continue;
                    }
                    push_unique(&mut k.matched, *id);
                }
            }

            // The platform integration may deliver Ctrl+C/X/V as
            // Copy/Cut/Paste events (with the OS clipboard payload) instead
            // of — or as well as — key events. Map them onto the same
            // commands; `push_unique` keeps double delivery harmless.
            if !wants_kb && !typing_sink && !palette_open {
                for e in &i.events {
                    match e {
                        egui::Event::Copy => {
                            push_unique(&mut k.matched, CommandId("board.copy"));
                        }
                        egui::Event::Cut => {
                            push_unique(&mut k.matched, CommandId("board.cut"));
                        }
                        egui::Event::Paste(text) => {
                            k.paste_text = Some(text.clone());
                            push_unique(
                                &mut k.matched,
                                CommandId(if i.modifiers.shift {
                                    "board.paste_in_place"
                                } else {
                                    "board.paste"
                                }),
                            );
                        }
                        _ => {}
                    }
                }
            }

            // Printable text for type-to-command (letters/digits; drafts
            // already gate command_typing_ok). Space is allowed only after
            // the first character so Space-tap pan/repeat is not stolen.
            // The character a held key auto-repeats is not typing: holding
            // a tool letter never opens command entry.
            if command_typing_ok && !i.modifiers.ctrl && !i.modifiers.alt && !i.modifiers.command {
                let mut repeated: Vec<char> = i
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Key {
                            key,
                            pressed: true,
                            repeat: true,
                            ..
                        } => key_char(*key),
                        _ => None,
                    })
                    .collect();
                for e in &i.events {
                    if let egui::Event::Text(t) = e {
                        for c in t.chars() {
                            if k.typed.starts_with(['\'', '"']) {
                                k.typed.push(c);
                                continue;
                            }
                            let c = c.to_ascii_lowercase();
                            if let Some(at) = repeated.iter().position(|r| *r == c) {
                                repeated.swap_remove(at);
                                continue;
                            }
                            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                                k.typed.push(c);
                            } else if matches!(c, '"' | '\'')
                                && k.typed.is_empty()
                                && self.bare_letter_hold.is_none()
                            {
                                // Grasshopper: a leading quote opens the
                                // panel entry (a blank text document).
                                k.typed.push(c);
                            } else if c == ' ' && !k.typed.is_empty() {
                                k.typed.push(' ');
                            }
                        }
                    }
                }
            }

            // --- specially handled keys ---
            k.escape = i.key_pressed(egui::Key::Escape);
            // Bare Enter only: modified Enter belongs to overlays (e.g.
            // Shift+Enter cycles search backwards) or is reserved.
            k.enter = i.key_pressed(egui::Key::Enter)
                && !i.modifiers.ctrl
                && !i.modifiers.shift
                && !i.modifiers.alt;
            if i.key_pressed(egui::Key::Tab) && !wants_kb && !typing_sink && !palette_open {
                k.tab = Some(if i.modifiers.shift { -1 } else { 1 });
            }
            if !wants_kb && !typing_sink && !palette_open && !i.modifiers.ctrl {
                let step = 1.0;
                if i.key_pressed(egui::Key::ArrowLeft) {
                    k.arrows.0 -= step;
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    k.arrows.0 += step;
                }
                if i.key_pressed(egui::Key::ArrowUp) {
                    k.arrows.1 -= step;
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    k.arrows.1 += step;
                }
            }
            k
        });

        if let Some(shift) = paste_edge {
            if board && !self.at_home && !wants_kb && !typing_sink && !palette_open {
                let already = keys
                    .matched
                    .iter()
                    .any(|id| id.0 == "board.paste" || id.0 == "board.paste_in_place");
                if !already {
                    keys.matched.push(CommandId(if shift {
                        "board.paste_in_place"
                    } else {
                        "board.paste"
                    }));
                }
            }
        }

        // --- Escape: cancel a pending bare-letter hold first; else stack ---
        let mut cancelled_hold = false;
        if keys.escape && !tag_editor_open && self.portal_chrome.maximized.is_some() {
            // Maximize is an app-level mode, even if an editor or palette
            // retained keyboard focus when the portal was maximized.
            self.dispatch(ctx, CommandId("app.cancel"), None);
        } else if keys.escape && !tag_editor_open && !palette_open {
            if self.bare_letter_hold.take().is_some() {
                cancelled_hold = true;
            } else if self.search.open {
                self.search.open = false;
            } else if !editing {
                // The text-edit overlay commits on its own Esc.
                self.dispatch(ctx, CommandId("app.cancel"), None);
            }
        }

        // --- bare-letter hold / type-to-command ---
        // Resolve the ambiguity window before dispatching other chords so a
        // follow-up letter opens the palette instead of firing the shortcut.
        let mut open_entry: Option<String> = None;
        let mut commit_hold: Option<CommandId> = None;
        // Enter/Space that only existed to flush the hold must not also
        // trigger repeat-last in the same frame.
        let mut suppress_repeat = false;
        if cancelled_hold {
            // nothing pending
        } else if let Some(hold) = self.bare_letter_hold.as_mut() {
            if !keys.typed.is_empty() {
                hold.buffer.push_str(&keys.typed);
                open_entry = Some(std::mem::take(&mut hold.buffer));
                self.bare_letter_hold = None;
            } else if keys.pointer_pressed
                || !keys.matched.is_empty()
                || keys.tab.is_some()
                || keys.arrows != (0.0, 0.0)
            {
                // Pointer or another command: commit so tool-then-click stays
                // snappy inside the hold window.
                commit_hold = Some(hold.pending_id);
                self.bare_letter_hold = None;
            } else if keys.enter || keys.repeat_via_space {
                commit_hold = Some(hold.pending_id);
                self.bare_letter_hold = None;
                suppress_repeat = true;
            } else if hold.started_at.elapsed() >= BARE_LETTER_HOLD {
                commit_hold = Some(hold.pending_id);
                self.bare_letter_hold = None;
            } else {
                let left = BARE_LETTER_HOLD.saturating_sub(hold.started_at.elapsed());
                ctx.request_repaint_after(left);
            }
        } else if command_typing_ok {
            if let Some((id, ch)) = keys.bare_letter {
                // Same frame may already carry more than the first letter
                // (fast typists) — go straight to command entry.
                let mut buf = String::new();
                if keys.typed.is_empty() {
                    buf.push(ch);
                } else {
                    buf.push_str(&keys.typed);
                    // Ensure the chord letter is represented if Text lagged.
                    if !buf.starts_with(ch) {
                        buf.insert(0, ch);
                    }
                }
                if buf.chars().count() > 1 {
                    open_entry = Some(buf);
                } else {
                    self.bare_letter_hold = Some(BareLetterHold {
                        pending_id: id,
                        buffer: buf,
                        started_at: Instant::now(),
                    });
                    ctx.request_repaint_after(BARE_LETTER_HOLD);
                }
            } else if !keys.typed.is_empty() {
                // Unbound first character(s): open command entry immediately.
                open_entry = Some(keys.typed.clone());
            }
        }
        if let Some(id) = commit_hold {
            self.dispatch(ctx, id, None);
        }
        if let Some(q) = open_entry {
            self.open_command_entry(ctx, q);
        }

        // --- Enter: crop / line / path drafts first (as before), else idle repeat ---
        if keys.enter && !wants_kb && !typing_sink && !palette_open && !suppress_repeat {
            if board && self.board_crop.is_some() {
                self.board_crop = None;
            } else if board
                && matches!(
                    self.board_tool,
                    board::BoardTool::Trim | board::BoardTool::Split
                )
            {
                self.trim_enter();
            } else if board && self.line_draft.is_some() {
                // Typed length places the end point along the current
                // direction; plain Enter commits at the cursor (D08).
                self.line_enter_commit();
            } else if board && self.board_path_draft.is_some() {
                self.path_tool_try_finish();
            } else {
                self.dispatch(ctx, CommandId("app.repeat_last"), None);
            }
        }
        if keys.repeat_via_space && !suppress_repeat {
            self.dispatch(ctx, CommandId("app.repeat_last"), None);
        }

        // --- registry chords ---
        self.pending_paste_text = keys.paste_text;
        for id in keys.matched {
            self.dispatch(ctx, id, None);
        }
        self.pending_paste_text = None;

        // --- line draft numeric entry (typed digits set length — D08) ---
        if board && self.line_draft.is_some() && !wants_kb && !editing && !palette_open {
            let (typed, backspaces) = ctx.input(|i| {
                let mut typed = String::new();
                let mut backspaces = 0u32;
                for e in &i.events {
                    let egui::Event::Key {
                        key, pressed: true, ..
                    } = e
                    else {
                        continue;
                    };
                    let c = match key {
                        egui::Key::Num0 => Some('0'),
                        egui::Key::Num1 => Some('1'),
                        egui::Key::Num2 => Some('2'),
                        egui::Key::Num3 => Some('3'),
                        egui::Key::Num4 => Some('4'),
                        egui::Key::Num5 => Some('5'),
                        egui::Key::Num6 => Some('6'),
                        egui::Key::Num7 => Some('7'),
                        egui::Key::Num8 => Some('8'),
                        egui::Key::Num9 => Some('9'),
                        egui::Key::Period => Some('.'),
                        egui::Key::Backspace => {
                            backspaces += 1;
                            None
                        }
                        _ => None,
                    };
                    if let Some(c) = c {
                        typed.push(c);
                    }
                }
                (typed, backspaces)
            });
            for _ in 0..backspaces {
                self.line_pop_digit();
            }
            for c in typed.chars() {
                self.line_push_digit(c);
            }
        }

        // --- Tab cycling (suppressed while typing/presenting/crop) ---
        if let Some(dir) = keys.tab {
            if board && self.board_tool == board::BoardTool::Deck {
                // Deck has no direction lock and does not cycle the selection.
            } else if board && {
                // Mid-segment Tab locks the direction instead of cycling the
                // selection (P2.RhinoDraft.tab; KEYMAP Tab note).
                let pointer = ctx.pointer_latest_pos().map(|p| self.board_xf().s2w(p));
                self.toggle_segment_lock(pointer)
            } {
                self.board_took_tab = Some(ctx.memory(|m| m.focused()));
            } else if self.board_crop.is_none() {
                self.cycle_objects(dir);
                self.push_history(
                    CommandId("canvas.cycle_next"),
                    Some(if dir > 0 { "next" } else { "prev" }.into()),
                );
            }
        }

        // --- arrows: nudge with a selection (as before), pan without ---
        let (dx, dy) = keys.arrows;
        if (dx != 0.0 || dy != 0.0) && board {
            // Direct selection: arrows nudge the selected anchors instead
            // of the node (coalesced via amend_last_patch).
            let step0 = if keys.shift { 10.0 } else { 1.0 };
            if self.board_tool == board::BoardTool::DirectSelect
                && self.direct_nudge(dx * step0, dy * step0)
            {
                // handled
            } else if !self.board_sel.is_empty() {
                let step = if keys.shift { 10.0 } else { 1.0 };
                let (mx, my) = (dx * step, dy * step);
                let ids =
                    self.expand_with_members(&self.board_sel.iter().copied().collect::<Vec<_>>());
                self.patch_nodes(&ids, move |n| n.rect = n.rect.translated(mx, my));
            } else {
                // Pan: a comfortable screen-space step, Shift = ×4.
                let px = if keys.shift { 240.0 } else { 60.0 };
                let z = self.tab().cam.z.max(f32::EPSILON);
                self.tab_mut().cam.offset += egui::Vec2::new(dx * px, dy * px) / z;
                let now = self.frame_time;
                self.bump_grid_fade(now);
            }
        }
    }
}

#[cfg(test)]
mod shortcut_tests;
