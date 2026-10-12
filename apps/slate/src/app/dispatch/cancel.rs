//! Layered Escape cancellation on the existing command owner.
use super::*;

impl SlateApp {
    /// Build the live cancel layers and pop exactly one
    /// (`atlas_commands::cancel_target`). Text editing is *not* a
    /// layer here: the edit overlay owns Esc (commit) itself.
    pub(super) fn cancel_pop(&mut self) -> bool {
        if self.atlas_cancel_carry() {
            return true;
        }
        if self.doc().view.active_view == ViewKind::Board
            && (self.cancel_image_layer_opacity()
                || self.cancel_image_result_drag()
                || self.cancel_image_layer_drag())
        {
            return true;
        }
        // Maximize peels first: Esc returns the portal to the board without
        // dropping page focus (P1.portal.maximize / P0.1).
        if self.portal_chrome.maximized.is_some() {
            return self.portal_restore();
        }
        // Web portal input focus. Releasing it leaves the page running with
        // its scroll position and form contents intact (D12) — this peels
        // where the keyboard goes, nothing more.
        if self.doc().view.active_view == ViewKind::Board && self.web.focused.is_some() {
            return self.web_blur();
        }
        if self.doc().view.active_view == ViewKind::Board && self.dismiss_agent_picker() {
            return true;
        }
        if self.doc().view.active_view == ViewKind::Board && self.agents.focused.is_some() {
            return self.agent_blur();
        }
        if self.doc().view.active_view == ViewKind::Board && self.atlas_lenses.focused.is_some() {
            return self.atlas_blur();
        }
        // A saved view held over a model cancels before the viewport it
        // opened (media D38).
        if self.doc().view.active_view == ViewKind::Board
            && (self.dismiss_view_drop_preview() || self.dismiss_image_segment())
        {
            return true;
        }
        // Any drag that edits nodes live (move, Alt copy, resize, rotate,
        // crop, grip, fillet) goes back to its press-time nodes before an
        // entered viewport peels (P0.1).
        if self.doc().view.active_view == ViewKind::Board && self.cancel_node_drag() {
            return true;
        }
        // A live 3D viewport is entered contents: its measure state, then
        // the viewport itself, peel before the board's own layers.
        if self.doc().view.active_view == ViewKind::Board && self.model_cancel_step() {
            return true;
        }
        let board = self.doc().view.active_view == ViewKind::Board;
        let mut live: Vec<CancelLayer> = Vec::new();
        // Running drag operations (wire drags, eraser scrubs, direct-
        // selection edits, the zoom-window marquee) cancel first — restore,
        // no journal.
        if self.brush_hud.is_some()
            || self.zoom_marquee.is_some()
            || matches!(
                self.board_drag,
                Some(
                    board::BoardDrag::Wire(_)
                        | board::BoardDrag::Erase { .. }
                        | board::BoardDrag::Smooth { .. }
                        | board::BoardDrag::Direct(_)
                        | board::BoardDrag::DeckStroke { .. }
                        | board::BoardDrag::Marquee { .. }
                        | board::BoardDrag::BezierAnchor { .. }
                        | board::BoardDrag::BezierEdit { .. }
                )
            )
        {
            live.push(CancelLayer::ActiveOperation);
        }
        let direct_live = board
            && self.board_tool == board::BoardTool::DirectSelect
            && (!self.direct.anchors.is_empty() || self.direct.node.is_some());
        if self.board_crop.is_some()
            || self.board_path_draft.is_some()
            || self.line_draft.is_some()
            || self.trim_live_draft()
            || direct_live
        {
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
        if self.board_menu.is_some()
            || self.board_empty_menu.is_some()
            || self.adjust_popover_open
            || self.new_tag_edit.is_some()
        {
            live.push(CancelLayer::Chrome);
        }
        match cancel_target(&live) {
            Some(CancelLayer::ActiveOperation) => {
                if self.brush_hud.is_some() {
                    self.cancel_brush_hud();
                    return true;
                }
                if self.zoom_marquee.is_some() {
                    // Cancel the zoom window; the tool stays armed.
                    self.zoom_marquee = None;
                    return true;
                }
                match self.board_drag.take() {
                    Some(board::BoardDrag::Wire(wd)) => self.cancel_wire_drag(wd),
                    Some(board::BoardDrag::Direct(d)) => self.cancel_direct_drag(d),
                    Some(board::BoardDrag::Smooth { .. }) => {
                        self.smooth_preview.clear();
                        self.smooth_polylines.clear();
                    }
                    // Bézier draft: drop the anchor being placed, or put an
                    // edited anchor back. Placed anchors stay.
                    Some(board::BoardDrag::BezierAnchor { .. }) => {
                        if let Some(board_path::BoardPathDraft::Bezier { placing, .. }) =
                            &mut self.board_path_draft
                        {
                            *placing = None;
                        }
                    }
                    Some(board::BoardDrag::BezierEdit { anchors0, .. }) => {
                        if let Some(board_path::BoardPathDraft::Bezier { anchors, .. }) =
                            &mut self.board_path_draft
                        {
                            *anchors = anchors0;
                        }
                    }
                    // Eraser: nothing was mutated — dropping the drag and its
                    // live preview restores the ink.
                    _ => {
                        self.erase_live.clear();
                        self.brush_tiles
                            .forget_erase_lines(&self.erase_settle.jobs());
                    }
                }
                true
            }
            Some(CancelLayer::Draft) => {
                if self.image_paint.is_some() {
                    self.clear_image_paint_session();
                } else if self.board_crop.is_some() {
                    // First Escape only exits crop mode; the node stays
                    // selected (press again to clear the selection).
                    self.board_crop = None;
                } else if self.line_draft.is_some() {
                    // Line draft Esc layering (contract D12): a live numeric
                    // entry clears first, the next press removes the first
                    // point; the Mode layer below then disarms to Select.
                    self.line_cancel_step();
                } else if self.board_path_draft.is_some() {
                    self.escape_path_draft();
                } else if self.trim_live_draft() {
                    self.trim_cancel_step();
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
                self.board_menu = None;
                self.board_empty_menu = None;
                self.adjust_popover_open = false;
                self.new_tag_edit = None;
                true
            }
            // Slate has no readout-owned selection (File Atlas' activity
            // timeline registers that layer), so it is never live here.
            Some(CancelLayer::Readout) | None => false,
        }
    }
}
