//! Z-order, add-to-frame, export, and canvas section timing.

use super::*;

impl SlateApp {
    /// Move nodes to the front or back of the z-list (one undo group).
    pub fn reorder_nodes(&mut self, ids: &[NodeId], to_front: bool) {
        let mut cmds = Vec::new();
        // Plan against a scratch z-list; only Scene::apply may mutate the
        // real list, so its spatial/paint order caches are invalidated too.
        let mut z_list: Vec<NodeId> = self.doc().scene.nodes.iter().map(|n| n.id).collect();
        let mut ordered: Vec<NodeId> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .map(|n| n.id)
            .collect();
        // Inserting at zero requires reverse traversal to retain the
        // selection's internal stacking order.
        if !to_front {
            ordered.reverse();
        }
        for id in ordered {
            let index = z_list
                .iter()
                .position(|candidate| *candidate == id)
                .unwrap();
            let new_index = if to_front { z_list.len() - 1 } else { 0 };
            if index == new_index {
                continue;
            }
            let node = self.doc().scene.node(id).unwrap().clone();
            z_list.remove(index);
            z_list.insert(new_index, id);
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
            self.commit_scene(cmds);
        }
    }

    // ----- dialogs ------------------------------------------------------------------

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
