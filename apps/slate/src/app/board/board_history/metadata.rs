//! Workbook metadata and linked-cell steps in the board's shared undo order.
use super::*;

impl SlateApp {
    pub(super) fn revert_tag_mark(&mut self, mark: slate_doc::TagCommand, to_redo: bool) {
        let inverse = mark.inverted();
        let applied = inverse.apply(self.doc_mut());
        let step = BoardMark::Tags(if applied { inverse } else { mark });
        let tab = self.tab_mut();
        if to_redo == applied {
            tab.edit_redo.push(step);
        } else {
            tab.edits.push(step);
        }
        if !applied {
            self.toast("Couldn't restore that tag: its metadata or assignments changed.");
            return;
        }
        self.tab_mut().dirty = true;
        self.last_board_edit = None;
        self.publish_session_tags();
    }

    pub(super) fn revert_sheet_mark(&mut self, mark: SheetMark, to_redo: bool) {
        let Some(prior) =
            atlas_core::table::revert_sheet_cell(&mark.path, mark.row, mark.col, &mark.prior)
        else {
            self.toast("Couldn't change that spreadsheet");
            let tab = self.tab_mut();
            if to_redo {
                tab.edits.push(BoardMark::Sheet(mark));
            } else {
                tab.edit_redo.push(BoardMark::Sheet(mark));
            }
            return;
        };
        self.invalidate_document_card(mark.item);
        let inverted = SheetMark { prior, ..mark };
        let tab = self.tab_mut();
        if to_redo {
            tab.edit_redo.push(BoardMark::Sheet(inverted));
        } else {
            tab.edits.push(BoardMark::Sheet(inverted));
        }
    }

    pub(super) fn revert_locator_mark(&mut self, mark: slate_doc::RewriteLocators, to_redo: bool) {
        let inverse = mark.inverted();
        if !inverse.apply(self.doc_mut()) {
            self.toast("Couldn't restore those asset links");
            let tab = self.tab_mut();
            if to_redo {
                tab.edits.push(BoardMark::Locators(mark));
            } else {
                tab.edit_redo.push(BoardMark::Locators(mark));
            }
            return;
        }
        let tab = self.tab_mut();
        tab.dirty = true;
        if to_redo {
            tab.edit_redo.push(BoardMark::Locators(inverse));
        } else {
            tab.edits.push(BoardMark::Locators(inverse));
        }
        self.note_scene_change();
    }
}
