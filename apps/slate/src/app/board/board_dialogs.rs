//! Z-order, add-to-frame, export, and canvas section timing.

use super::*;

impl SlateApp {
    /// Move nodes to the front or back of the z-list (one undo group).
    pub fn reorder_nodes(&mut self, ids: &[NodeId], to_front: bool) {
        let mut cmds = Vec::new();
        // Stable: process in current z-order.
        let ordered: Vec<NodeId> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .map(|n| n.id)
            .collect();
        for id in ordered {
            let Some(index) = self.doc().scene.index_of(id) else {
                continue;
            };
            let node = self.doc().scene.node(id).unwrap().clone();
            // Apply immediately so subsequent indices are correct.
            let scene = &mut self.doc_mut().scene;
            scene.nodes.remove(index);
            let new_index = if to_front { scene.nodes.len() } else { 0 };
            scene.nodes.insert(new_index, node.clone());
            cmds.push(SceneCmd::Remove {
                index,
                node: node.clone(),
            });
            cmds.push(SceneCmd::Add {
                index: new_index,
                node,
            });
        }
        if !cmds.is_empty() {
            self.tab_mut().journal.record(cmds);
            self.tab_mut().dirty = true;
            self.note_scene_change();
        }
    }

    // ----- dialogs ------------------------------------------------------------------

    /// Frame "+ images": pick files, place them inside the frame, inherit tags.
    pub fn add_to_frame_dialog(&mut self, frame: NodeId) {
        self.picker.open(PickRequest::files(), move |paths| {
            super::super::PickerMsg::AddToFrame { frame, paths }
        });
    }

    pub fn export_artifact_dialog(&mut self) {
        self.picker.open(PickRequest::folder(), |picked| {
            super::super::PickerMsg::ExportArtifact(file_picker::first(picked))
        });
    }
}

/// Section timings inside `board_canvas`. Off unless a bench calls [`brush_prof::begin`].
pub(crate) mod brush_prof {
    use std::cell::RefCell;
    use std::time::Instant;

    struct Prof {
        last: Instant,
        laps: Vec<(&'static str, f32)>,
    }

    thread_local! {
        static PROF: RefCell<Option<Prof>> = const { RefCell::new(None) };
    }

    #[cfg(test)]
    pub fn begin() {
        let now = Instant::now();
        PROF.with(|p| {
            *p.borrow_mut() = Some(Prof {
                last: now,
                laps: Vec::new(),
            });
        });
    }

    pub fn lap(name: &'static str) {
        PROF.with(|p| {
            let mut slot = p.borrow_mut();
            let Some(prof) = slot.as_mut() else {
                return;
            };
            let now = Instant::now();
            let ms = now.duration_since(prof.last).as_secs_f32() * 1000.0;
            prof.laps.push((name, ms));
            prof.last = now;
        });
    }

    #[cfg(test)]
    #[cfg(test)]
    pub fn take() -> Vec<(&'static str, f32)> {
        PROF.with(|p| {
            p.borrow_mut()
                .take()
                .map(|prof| prof.laps)
                .unwrap_or_default()
        })
    }
}
