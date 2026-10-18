//! Scoped collaborator access to an already-running Atlas portal session.
use super::*;
use serde_json::{json, Value};
use std::path::Path;

impl SlateApp {
    pub(crate) fn agent_atlas_validate_locator(&self, locator: &str) -> Result<(), String> {
        validate_locator(locator)
    }

    /// State of the existing scan owner for this bound portal. `None` means
    /// the normal Atlas pump has not admitted the binding yet.
    pub(crate) fn agent_atlas_scan_state(&self, portal: NodeId) -> Option<(usize, bool)> {
        let view = self.atlas_lenses.views.get(&portal)?;
        let session = self.atlas_lenses.sessions.get(&view.session_key)?;
        Some((session.entries.len(), session.done))
    }

    pub(crate) fn agent_atlas_locator_known(
        &self,
        portal: NodeId,
        locator: &str,
    ) -> Result<bool, String> {
        validate_locator(locator)?;
        let Some(view) = self.atlas_lenses.views.get(&portal) else {
            return Ok(false);
        };
        let Some(session) = self.atlas_lenses.sessions.get(&view.session_key) else {
            return Ok(false);
        };
        let normalized = locator.replace('/', "\\");
        let Some(&file) = session.rel_to_id.get(&normalized) else {
            return Ok(false);
        };
        Ok(session
            .entries
            .get(file as usize)
            .is_some_and(|entry| !entry.dead && entry.rel.replace('/', "\\") == normalized))
    }

    pub(crate) fn agent_atlas_page(
        &mut self,
        portal: NodeId,
        offset: usize,
        limit: usize,
    ) -> Result<Value, String> {
        if !(1..=100).contains(&limit) {
            return Err("Atlas page size must be between 1 and 100".into());
        }
        let view = self
            .atlas_lenses
            .views
            .get(&portal)
            .ok_or("Atlas portal is not active")?;
        let session = self
            .atlas_lenses
            .sessions
            .get(&view.session_key)
            .ok_or("Atlas scan is not active")?;
        let end = offset.saturating_add(limit).min(session.entries.len());
        let rows: Vec<Value> = session
            .entries
            .get(offset.min(end)..end)
            .unwrap_or_default()
            .iter()
            .filter(|entry| !entry.dead)
            .map(|entry| {
                json!({
                    "locator": entry.rel.replace('\\', "/"),
                    "name": entry.name,
                    "extension": entry.ext,
                    "size": entry.size,
                    "modified": entry.mtime,
                    "family": format!("{:?}", entry.family),
                })
            })
            .collect();
        Ok(json!({
            "offset": offset,
            "files": rows,
            "next_offset": (end < session.entries.len()).then_some(end),
            "total_seen": session.entries.len(),
            "generation": session.generation,
            "complete": session.done,
        }))
    }

    pub(crate) fn agent_atlas_highlight(
        &mut self,
        portal: NodeId,
        locator: &str,
        request_id: &str,
        observer: &str,
    ) -> Result<Value, String> {
        validate_locator(locator)?;
        let normalized = locator.replace('/', "\\");
        let view = self
            .atlas_lenses
            .views
            .get(&portal)
            .ok_or("Atlas portal is not active")?;
        let session = self
            .atlas_lenses
            .sessions
            .get(&view.session_key)
            .ok_or("Atlas scan is not active")?;
        let Some(&file) = session.rel_to_id.get(&normalized) else {
            return Err("That file is not in the current Atlas scan".into());
        };
        let Some(entry) = session
            .entries
            .get(file as usize)
            .filter(|entry| !entry.dead && entry.rel.replace('/', "\\") == normalized)
        else {
            return Err("That file is not in the current Atlas scan".into());
        };
        let name = entry.name.clone();
        let visible_locator = entry.rel.replace('\\', "/");
        let view = self
            .atlas_lenses
            .views
            .get_mut(&portal)
            .ok_or("Atlas portal changed")?;
        view.agent_highlights
            .insert(request_id.to_owned(), (file, observer.to_owned()));
        view.rebuild_agent_highlight_union();
        Ok(json!({
            "highlighted": visible_locator,
            "name": name,
            "observer": observer,
            "status": "overlay_registered"
        }))
    }

    pub(crate) fn agent_atlas_clear_observer(&mut self, request_id: &str) {
        for view in self.atlas_lenses.views.values_mut() {
            if view.agent_highlights.remove(request_id).is_some() {
                view.rebuild_agent_highlight_union();
            }
        }
    }

    pub(crate) fn agent_atlas_file_ref(
        &self,
        portal: NodeId,
        locator: &str,
    ) -> Result<(PathBuf, PathBuf, u64, u64, i64), String> {
        validate_locator(locator)?;
        let view = self
            .atlas_lenses
            .views
            .get(&portal)
            .ok_or("Atlas portal is not active")?;
        let session = self
            .atlas_lenses
            .sessions
            .get(&view.session_key)
            .ok_or("Atlas scan is not active")?;
        let normalized = locator.replace('/', "\\");
        let file = session
            .rel_to_id
            .get(&normalized)
            .copied()
            .ok_or("That file is not in the current Atlas scan")?;
        let entry = session
            .entries
            .get(file as usize)
            .filter(|entry| !entry.dead && entry.rel.replace('/', "\\") == normalized)
            .ok_or("That file is not in the current Atlas scan")?;
        Ok((
            session.root.clone(),
            entry.path.clone(),
            session.generation,
            entry.size,
            entry.mtime,
        ))
    }

    pub(crate) fn build_bound_atlas(
        &self,
        rect: WorldRect,
        path: &Path,
        files: Vec<String>,
    ) -> Node {
        let locator = source_locator(self.tab().path.as_deref(), path);
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("File Atlas");
        let mut portal = PortalNode::bound_file_atlas(title, locator);
        portal.atlas.files = files;
        let mut prepared = self.doc().scene.node_preparation();
        prepared.build_node(rect, NodeKind::Portal(portal))
    }
}

impl AtlasView {
    fn rebuild_agent_highlight_union(&mut self) {
        self.agent_highlight_union = std::sync::Arc::new(
            self.agent_highlights
                .values()
                .map(|(file, _)| *file)
                .collect(),
        );
    }
}

fn validate_locator(locator: &str) -> Result<(), String> {
    if locator.is_empty()
        || locator.len() > 1024
        || Path::new(locator).is_absolute()
        || locator.contains(':')
        || locator.starts_with('/')
        || locator
            .split(['/', '\\'])
            .any(|part| part == ".." || part.is_empty())
    {
        Err("Use one exact relative file locator from this Atlas portal".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;
    use std::collections::HashSet;

    #[test]
    fn atlas_locator_must_be_one_relative_enumerated_path() {
        assert!(validate_locator("reports/annual.csv").is_ok());
        for bad in [
            "",
            "../secret.txt",
            "reports/../secret.txt",
            "C:/secret.txt",
            "/secret.txt",
            "reports//file.txt",
        ] {
            assert!(validate_locator(bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn agent_highlight_is_separate_from_human_selection() {
        let mut h = Harness::new("portal_agent_highlight");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_bound_atlas_at(egui::pos2(0.0, 0.0), &h.base);
        let portal = h.app.doc().scene.nodes[0].id;
        let root = canonical(&h.base);
        let entry =
            FileEntry::from_abs(&root, root.join("readme.md"), 5, 0, 0, String::new()).unwrap();
        let (_tx, rx) = unbounded();
        h.app.atlas_lenses.sessions.insert(
            root.clone(),
            AtlasSession {
                root,
                generation: 2,
                entries: vec![entry],
                rel_to_id: HashMap::from([("readme.md".into(), 0)]),
                tree: None,
                tree_dirty: true,
                last_tree_at: Instant::now(),
                last_tree_n: 0,
                scan_rx: rx,
                scan_handle: None,
                scan_hold: None,
                watch: None,
                fs_backlog: VecDeque::new(),
                done: true,
                subscribers: 1,
            },
        );
        h.frame();
        assert!(h
            .app
            .agent_atlas_locator_known(portal, "readme.md")
            .unwrap());
        assert!(!h
            .app
            .agent_atlas_locator_known(portal, "missing.md")
            .unwrap());
        h.app
            .atlas_lenses
            .views
            .get_mut(&portal)
            .unwrap()
            .selection
            .insert(7);
        let before = h.app.atlas_lenses.views[&portal].selection.clone();
        h.app
            .agent_atlas_highlight(portal, "readme.md", "request-a", "Dot")
            .unwrap();
        assert_eq!(
            h.app.atlas_lenses.views[&portal]
                .agent_highlight_union
                .as_ref(),
            &HashSet::from([0])
        );
        h.app
            .agent_atlas_highlight(portal, "readme.md", "request-b", "Sage")
            .unwrap();
        h.app.agent_atlas_clear_observer("request-a");
        assert_eq!(
            h.app.atlas_lenses.views[&portal].agent_highlights["request-b"].1,
            "Sage"
        );
        assert_eq!(h.app.atlas_lenses.views[&portal].selection, before);
        h.app.agent_atlas_clear_observer("request-b");
        assert!(h.app.atlas_lenses.views[&portal]
            .agent_highlight_union
            .is_empty());
    }

    #[test]
    fn atlas_sync_warms_a_new_binding_without_paint_or_viewport_admission() {
        let mut h = Harness::new("portal_atlas_warm");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app
            .place_bound_atlas_at(egui::pos2(100_000.0, 100_000.0), &h.base);
        let portal = h.app.doc().scene.nodes[0].id;

        h.app.atlas_sync_bindings();

        assert!(h.app.atlas_lenses.views.contains_key(&portal));
        assert_eq!(h.app.agent_atlas_scan_state(portal), Some((0, false)));
        assert!(h
            .app
            .agent_atlas_validate_locator("../outside.png")
            .is_err());
    }
}
