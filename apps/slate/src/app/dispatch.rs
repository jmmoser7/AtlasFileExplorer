//! Command dispatch: the keyboard front-end of the registry.
//!
//! `hotkeys` builds [`atlas_commands::Chord`]s from egui key events, looks
//! them up in the registry (respecting the pre-existing suppression gates:
//! presenting, typing, text editing, board-only keys), and routes matches
//! through [`SlateApp::dispatch`] — the same entry the canvas palette, the
//! dock toggles, and the menu bar use, so every input modality drives one
//! command surface (Constitution Art. VII/VIII). Every dispatched command
//! pushes a [`HistoryEntry`] (the F2 window's data and the Space/Enter
//! repeat source).
//!
//! Specially handled (not chord table rows): Space-tap / Enter-idle repeat,
//! the Esc cancel stack ([`atlas_commands::cancel_target`]), Tab cycling,
//! and arrows (nudge with a selection, pan without).

use super::{board, commands, SlateApp};
use atlas_commands::{
    cancel_target, Availability, CancelLayer, Chord, CmdAuthor, CommandId, HistoryEntry, Key,
};
use eframe::egui;
use slate_doc::scene::NodeKind;
use slate_doc::{ItemId, NodeId, ViewKind};
use std::time::{Duration, Instant};

/// Space taps longer than this are holds (pan chords), not repeat requests.
const SPACE_TAP_MAX: Duration = Duration::from_millis(250);

/// Tracks a live Space press so release can decide tap (repeat) vs hold (pan).
#[derive(Default)]
pub struct SpaceTap {
    pressed_at: Option<Instant>,
    /// A pointer button went down while Space was held — it was a pan chord.
    pointer_used: bool,
}

impl SlateApp {
    /// The availability context for this frame: active view + GLOBAL +
    /// NEEDS_SELECTION while the view's selection is non-empty.
    pub(crate) fn command_ctx(&self) -> Availability {
        let view = match self.doc().view.active_view {
            ViewKind::Board => Availability::BOARD_VIEW,
            ViewKind::Lens => Availability::LENS,
            // Grid, Venn, and the legacy/unknown kinds all present the
            // item-canvas surface.
            _ => Availability::GRID_VENN,
        };
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

    /// Record an executed command (keyboard, palette, menu, dock, or mouse
    /// site). The history is the intent log (Art. VI) and the repeat source.
    pub(crate) fn push_history(&mut self, id: CommandId, detail: Option<String>) {
        let Some(spec) = self.registry.by_id(id) else {
            return;
        };
        self.cmd_history.push(HistoryEntry {
            id,
            name: spec.name,
            author: CmdAuthor::Human,
            detail,
            at: std::time::SystemTime::now(),
        });
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
        let Some(spec) = self.registry.by_id(id) else {
            return false;
        };
        if !spec.when.matches(self.command_ctx()) {
            return false; // unavailable commands are no-ops (registry contract)
        }
        let board = self.doc().view.active_view == ViewKind::Board;
        let mut detail = detail;
        let ran = match id.0 {
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
            "app.new_tab" => {
                self.new_tab();
                true
            }
            "app.export" => {
                self.export_artifact_dialog();
                true
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
            "app.help" => {
                // F1: the Advanced window (the Commands & shortcuts reference
                // is its dedicated section; P1 has no per-section scroll).
                self.chrome_mut().advanced_open = true;
                true
            }
            "app.preferences" => {
                self.chrome_mut().advanced_open = true;
                true
            }
            "app.history" => {
                self.history_open = !self.history_open;
                true
            }
            "app.properties" => {
                let on = !self.chrome().tool(super::chrome::ToolPanel::Selection);
                self.chrome_mut()
                    .set_tool(super::chrome::ToolPanel::Selection, on);
                detail = detail.or(Some(if on { "shown" } else { "hidden" }.into()));
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
            "board.tool.rect" => {
                self.set_board_tool(board::BoardTool::RectShape);
                true
            }
            "board.tool.ellipse" => {
                self.set_board_tool(board::BoardTool::Ellipse);
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
            "board.tool.text" => {
                self.set_board_tool(board::BoardTool::Text);
                true
            }
            "board.tool.brush" => {
                // Re-arming also breaks the Shift+click straight chain.
                self.set_board_tool(board::BoardTool::Brush);
                true
            }
            "board.tool.eraser" => {
                self.set_board_tool(board::BoardTool::Eraser);
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
                let (w, eraser) = self.step_active_width(id.0 == "board.brush.width_up");
                detail = detail.or(Some(format!(
                    "{} → {w:.1}u",
                    if eraser { "eraser" } else { "brush" }
                )));
                true
            }
            // ----- path editing -----------------------------------------------------
            "board.path.join" => {
                let ran = self.cmd_join();
                if ran {
                    self.toast("Joined");
                }
                ran
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
            "board.delete" => {
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
                // C: one selected croppable image → the same crop mode the
                // double-click path enters. No-op otherwise.
                let single =
                    (self.board_sel.len() == 1).then(|| *self.board_sel.iter().next().unwrap());
                match single {
                    Some(node_id) if self.croppable_image(node_id) => {
                        self.enter_crop_mode(node_id);
                        true
                    }
                    _ => false,
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
            "board.image.invert" => {
                let images = self.selected_image_nodes();
                if images.is_empty() {
                    false
                } else {
                    let target = !self
                        .doc()
                        .scene
                        .node(images[0])
                        .and_then(|n| match &n.kind {
                            NodeKind::Image(img) => Some(img.adjust.invert),
                            _ => None,
                        })
                        .unwrap_or(false);
                    self.patch_nodes(&images, move |n| {
                        if let NodeKind::Image(img) = &mut n.kind {
                            img.adjust.invert = target;
                        }
                    });
                    self.last_board_edit = None; // toggles never coalesce
                    detail = detail.or(Some(format!(
                        "{} → {}",
                        images.len(),
                        if target { "on" } else { "off" }
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
                // OS clipboard text (from the platform Paste event, when one
                // arrived this frame) wins over the app-internal buffer so
                // selections round-trip between Slate instances.
                let os_text = self.pending_paste_text.take();
                let n = self.board_paste(os_text.as_deref(), at);
                detail = detail.or(Some(format!("{n} node(s)")));
                n > 0
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
    /// its own `board.fit`; Lens keeps `F` local to the laid-out graph.
    fn fit_active_view(&mut self) {
        match self.doc().view.active_view {
            ViewKind::Board => self.fit_board(),
            ViewKind::Lens => {}
            _ => {
                if let Some(bounds) = self.layout_bounds_now() {
                    self.fit_view(bounds);
                }
            }
        }
    }

    // ---------- the Esc cancel stack ----------

    /// Build the live cancel layers and pop exactly one
    /// (`atlas_commands::cancel_target`). The Lens focus clear stays first,
    /// exactly as the pre-registry cascade did. Text editing is *not* a
    /// layer here: the edit overlay owns Esc (commit) itself.
    fn cancel_pop(&mut self) -> bool {
        // Lens focus: today's first Escape target, kept ahead of the stack.
        if self.doc().view.active_view == ViewKind::Lens && self.lens.focus.is_some() {
            self.lens.focus = None;
            return true;
        }
        let board = self.doc().view.active_view == ViewKind::Board;
        let mut live: Vec<CancelLayer> = Vec::new();
        // Running drag operations (wire drags, eraser scrubs, direct-
        // selection edits, the zoom-window marquee) cancel first — restore,
        // no journal.
        if self.zoom_marquee.is_some()
            || matches!(
                self.board_drag,
                Some(
                    board::BoardDrag::Wire(_)
                        | board::BoardDrag::Erase { .. }
                        | board::BoardDrag::Direct(_)
                )
            )
        {
            live.push(CancelLayer::ActiveOperation);
        }
        let direct_live = board
            && self.board_tool == board::BoardTool::DirectSelect
            && (!self.direct.anchors.is_empty() || self.direct.node.is_some());
        if self.board_crop.is_some() || self.board_path_draft.is_some() || direct_live {
            live.push(CancelLayer::Draft);
        }
        if self.zoom_tool_active() || (board && self.board_tool != board::BoardTool::Select) {
            live.push(CancelLayer::Mode);
        }
        let has_selection = if board {
            !self.board_sel.is_empty()
        } else {
            !self.selection.is_empty()
        };
        if has_selection {
            live.push(CancelLayer::Selection);
        }
        // Chrome: open context menus, the adjust popover, and the inline
        // new-tag editor. The palette and the search strip own their Esc
        // (focused text fields); the minimap is pinned chrome — excluded.
        if self.menu.is_some()
            || self.board_menu.is_some()
            || self.board_empty_menu.is_some()
            || self.adjust_popover_open
            || self.new_tag_edit.is_some()
        {
            live.push(CancelLayer::Chrome);
        }
        match cancel_target(&live) {
            Some(CancelLayer::ActiveOperation) => {
                if self.zoom_marquee.is_some() {
                    // Cancel the zoom window; the tool stays armed.
                    self.zoom_marquee = None;
                    return true;
                }
                match self.board_drag.take() {
                    Some(board::BoardDrag::Wire(wd)) => self.cancel_wire_drag(wd),
                    Some(board::BoardDrag::Direct(d)) => self.cancel_direct_drag(d),
                    // Eraser: nothing was mutated — dropping the drag
                    // restores full opacity.
                    _ => {}
                }
                true
            }
            Some(CancelLayer::Draft) => {
                if self.board_crop.is_some() {
                    // First Escape only exits crop mode; the node stays
                    // selected (press again to clear the selection).
                    self.board_crop = None;
                } else if self.board_path_draft.is_some() {
                    self.cancel_path_draft();
                } else if !self.direct.anchors.is_empty() {
                    // Direct-selection Esc order: anchors → node → tool.
                    self.direct.anchors.clear();
                } else {
                    self.direct_set_target(None);
                }
                true
            }
            Some(CancelLayer::Mode) => {
                if self.zoom_tool_active() {
                    // Disarm Z first: the previous tool re-arms untouched.
                    self.zoom_armed = false;
                    self.zoom_marquee = None;
                } else {
                    self.set_board_tool(board::BoardTool::Select);
                }
                true
            }
            Some(CancelLayer::Selection) => {
                self.board_sel.clear();
                self.selection.clear();
                true
            }
            Some(CancelLayer::Chrome) => {
                self.menu = None;
                self.board_menu = None;
                self.board_empty_menu = None;
                self.adjust_popover_open = false;
                self.new_tag_edit = None;
                true
            }
            None => false,
        }
    }

    // ---------- the keyboard front-end ----------

    pub(crate) fn hotkeys(&mut self, ctx: &egui::Context) {
        // Presentation mode owns the keyboard (handled in present_frame).
        if self.presenting.is_some() {
            self.space_tap = SpaceTap::default();
            return;
        }
        let wants_kb = ctx.wants_keyboard_input();
        let board = self.doc().view.active_view == ViewKind::Board;
        let editing = self.text_edit.is_some();
        let palette_open = self.palette_state.open;
        let cmd_ctx = self.command_ctx();

        struct Keys {
            matched: Vec<CommandId>,
            escape: bool,
            enter: bool,
            tab: Option<i64>,
            arrows: (f32, f32),
            shift: bool,
            repeat_via_space: bool,
            paste_text: Option<String>,
        }
        let keys = ctx.input(|i| {
            let mut k = Keys {
                matched: Vec::new(),
                escape: false,
                enter: false,
                tab: None,
                arrows: (0.0, 0.0),
                shift: i.modifiers.shift,
                repeat_via_space: false,
                paste_text: None,
            };
            fn push_unique(v: &mut Vec<CommandId>, id: CommandId) {
                if !v.contains(&id) {
                    v.push(id);
                }
            }

            // --- Space tap = repeat-last (release-fired; Space+drag stays pan) ---
            if i.key_pressed(egui::Key::Space)
                && self.space_tap.pressed_at.is_none()
                && !wants_kb
                && !editing
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
                    if tapped && !wants_kb && !editing && !palette_open {
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
                if suppressed(chord, wants_kb, editing, palette_open) {
                    continue;
                }
                if spec.when.matches(cmd_ctx) {
                    push_unique(&mut k.matched, spec.id);
                }
            }
            for (chord, id) in commands::ALIAS_CHORDS {
                if !chord_pressed(i, *chord) {
                    continue;
                }
                if suppressed(*chord, wants_kb, editing, palette_open) {
                    continue;
                }
                if let Some(spec) = self.registry.by_id(*id) {
                    if spec.when.matches(cmd_ctx) {
                        push_unique(&mut k.matched, *id);
                    }
                }
            }

            // The platform integration may deliver Ctrl+C/X/V as
            // Copy/Cut/Paste events (with the OS clipboard payload) instead
            // of — or as well as — key events. Map them onto the same
            // commands; `push_unique` keeps double delivery harmless.
            if !wants_kb && !editing && !palette_open {
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

            // --- specially handled keys ---
            k.escape = i.key_pressed(egui::Key::Escape);
            // Bare Enter only: modified Enter belongs to overlays (e.g.
            // Shift+Enter cycles search backwards) or is reserved.
            k.enter = i.key_pressed(egui::Key::Enter)
                && !i.modifiers.ctrl
                && !i.modifiers.shift
                && !i.modifiers.alt;
            if i.key_pressed(egui::Key::Tab) && !wants_kb && !editing && !palette_open {
                k.tab = Some(if i.modifiers.shift { -1 } else { 1 });
            }
            if !wants_kb && !editing && !palette_open && !i.modifiers.ctrl {
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

        // --- Escape: overlays with focused text own Esc; else the stack ---
        if keys.escape && !palette_open {
            if self.search.open {
                self.search.open = false;
            } else if !editing {
                // The text-edit overlay commits on its own Esc.
                self.dispatch(ctx, CommandId("app.cancel"), None);
            }
        }

        // --- Enter: crop / path drafts first (as before), else idle repeat ---
        if keys.enter && !wants_kb && !editing && !palette_open {
            if board && self.board_crop.is_some() {
                self.board_crop = None;
            } else if board && self.board_path_draft.is_some() {
                self.path_tool_try_finish();
            } else {
                self.dispatch(ctx, CommandId("app.repeat_last"), None);
            }
        }
        if keys.repeat_via_space {
            self.dispatch(ctx, CommandId("app.repeat_last"), None);
        }

        // --- registry chords ---
        self.pending_paste_text = keys.paste_text;
        for id in keys.matched {
            self.dispatch(ctx, id, None);
        }
        self.pending_paste_text = None;

        // --- Tab cycling (suppressed while typing/presenting/crop) ---
        if let Some(dir) = keys.tab {
            if self.board_crop.is_none() {
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
                let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
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

/// Chord match against this frame's input: exact modifier state + key press.
fn chord_pressed(i: &egui::InputState, chord: Chord) -> bool {
    let Some(key) = to_egui_key(chord.key) else {
        return false;
    };
    i.key_pressed(key)
        && i.modifiers.ctrl == chord.ctrl
        && i.modifiers.shift == chord.shift
        && i.modifiers.alt == chord.alt
}

/// The pre-registry suppression gates, applied per chord shape:
/// - Ctrl chords are suppressed while a text field wants the keyboard
///   (exactly the old `!wants_kb` gate on the Ctrl block);
/// - bare (modifier-free) non-F-keys are suppressed while typing or text
///   editing (the old board-keys gate — view scoping now comes from
///   `Availability`);
/// - F-keys stay live while typing (F5/F11 always were);
/// - everything is suppressed while the palette popup is open (it owns
///   the keyboard).
fn suppressed(chord: Chord, wants_kb: bool, editing: bool, palette_open: bool) -> bool {
    if palette_open {
        return true;
    }
    let fkey = matches!(
        chord.key,
        Key::F1
            | Key::F2
            | Key::F3
            | Key::F4
            | Key::F5
            | Key::F6
            | Key::F7
            | Key::F8
            | Key::F9
            | Key::F10
            | Key::F11
            | Key::F12
    );
    if fkey {
        return false;
    }
    if chord.ctrl {
        return wants_kb;
    }
    wants_kb || editing
}

/// Registry [`Key`] → egui key (the app-edge mapping the pure crate defers).
fn to_egui_key(key: Key) -> Option<egui::Key> {
    use egui::Key as E;
    Some(match key {
        Key::A => E::A,
        Key::B => E::B,
        Key::C => E::C,
        Key::D => E::D,
        Key::E => E::E,
        Key::F => E::F,
        Key::G => E::G,
        Key::H => E::H,
        Key::I => E::I,
        Key::J => E::J,
        Key::K => E::K,
        Key::L => E::L,
        Key::M => E::M,
        Key::N => E::N,
        Key::O => E::O,
        Key::P => E::P,
        Key::Q => E::Q,
        Key::R => E::R,
        Key::S => E::S,
        Key::T => E::T,
        Key::U => E::U,
        Key::V => E::V,
        Key::W => E::W,
        Key::X => E::X,
        Key::Y => E::Y,
        Key::Z => E::Z,
        Key::Num0 => E::Num0,
        Key::Num1 => E::Num1,
        Key::Num2 => E::Num2,
        Key::Num3 => E::Num3,
        Key::Num4 => E::Num4,
        Key::Num5 => E::Num5,
        Key::Num6 => E::Num6,
        Key::Num7 => E::Num7,
        Key::Num8 => E::Num8,
        Key::Num9 => E::Num9,
        Key::F1 => E::F1,
        Key::F2 => E::F2,
        Key::F3 => E::F3,
        Key::F4 => E::F4,
        Key::F5 => E::F5,
        Key::F6 => E::F6,
        Key::F7 => E::F7,
        Key::F8 => E::F8,
        Key::F9 => E::F9,
        Key::F10 => E::F10,
        Key::F11 => E::F11,
        Key::F12 => E::F12,
        Key::ArrowUp => E::ArrowUp,
        Key::ArrowDown => E::ArrowDown,
        Key::ArrowLeft => E::ArrowLeft,
        Key::ArrowRight => E::ArrowRight,
        Key::Space => E::Space,
        Key::Enter => E::Enter,
        Key::Escape => E::Escape,
        Key::Tab => E::Tab,
        Key::Delete => E::Delete,
        Key::Backspace => E::Backspace,
        Key::Home => E::Home,
        Key::End => E::End,
        Key::PageUp => E::PageUp,
        Key::PageDown => E::PageDown,
        Key::OpenBracket => E::OpenBracket,
        Key::CloseBracket => E::CloseBracket,
        Key::Comma => E::Comma,
        Key::Period => E::Period,
        Key::Plus => E::Plus,
        Key::Minus => E::Minus,
    })
}
