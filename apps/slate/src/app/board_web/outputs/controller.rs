//! State is scoped to the placed Web portal. Revisions serialize user gestures and Undo.
use super::*;
use slate_doc::dashboard_outputs::controller::ControllerState;

pub(crate) fn install_script(authored: &str, saved: Option<&ControllerState>) -> String {
    let Ok(url) = url::Url::from_file_path(authored) else {
        return String::new();
    };
    let quoted = |s: &str| serde_json::to_string(s).expect("quoted JSON");
    let payload = serde_json::to_string(&saved).expect("JSON dashboard data");
    // Gate before any data is parsed or dispatched. A local page can follow
    // links, and shared runtime hooks must never deliver state to a new origin.
    format!(";if(location.protocol==='file:'&&location.host==={}&&location.pathname==={}&&location.search===''){{{}{}));}}",
        quoted(url.host_str().unwrap_or("")), quoted(url.path()), include_str!("controller.js"), quoted(&payload))
}

impl SlateApp {
    pub(super) fn commit_dashboard_parameters(&mut self, id: NodeId, edit: ControllerEdit) {
        if self.tab().read_only {
            return;
        }
        let Some(node) = self.doc().scene.node(id) else {
            return;
        };
        let NodeKind::Portal(portal) = &node.kind else {
            return;
        };
        let current = portal
            .web
            .as_ref()
            .and_then(|web| web.controller.as_deref());
        if portal.kind != PortalKind::Web
            || portal
                .web
                .as_ref()
                .is_some_and(|web| !web.interactive_allowed)
            || self.web.agent_focus_lease.is_some()
            || current.map(|s| &s.revision) != edit.base.as_ref()
        {
            return;
        }
        self.patch_nodes(&[id], |node| {
            if let NodeKind::Portal(portal) = &mut node.kind {
                portal.web.get_or_insert_with(Default::default).controller =
                    Some(Box::new(edit.state.clone()));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;
    use slate_doc::SlateDoc;

    #[test]
    fn dashboard_parameters_stay_with_their_portal_across_save_as_reopen_and_rebind() {
        let mut h = Harness::new("dashboard-portable-parameters");
        for x in [0., 200.] {
            h.app
                .paste_web_url("https://example.com/dashboard", egui::pos2(x, 0.));
        }
        let ids: Vec<_> = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| matches!(&n.kind, NodeKind::Portal(_)))
            .map(|n| n.id)
            .collect();
        assert_eq!(ids.len(), 2);
        for &id in &ids {
            assert!(h
                .app
                .bind_web_source(id, "controllers/Spline graph.html".into()));
        }
        let parameters = |doc: &SlateDoc, id| {
            let NodeKind::Portal(p) = &doc.scene.node(id).unwrap().kind else {
                panic!("expected portal")
            };
            p.web_ref().controller
        };
        let edit = |i: u32| ControllerEdit {
            base: None,
            state: ControllerState {
                schema: "d7436103-4e1b-46c7-a2ab-983d454fc101".parse().unwrap(),
                revision: format!("d7436103-4e1b-46c7-a2ab-{i:012x}").parse().unwrap(),
                value: serde_json::json!({"control":i}).to_string(),
            },
        };
        assert!(parameters(h.app.doc(), ids[0]).is_none());
        h.app.commit_dashboard_parameters(ids[0], edit(1));
        assert!(parameters(h.app.doc(), ids[1]).is_none());
        h.app.commit_dashboard_parameters(ids[1], edit(2));
        let depth = h.app.tab().journal.undo_depth();
        h.app.commit_dashboard_parameters(ids[0], edit(3));
        assert_eq!(
            depth,
            h.app.tab().journal.undo_depth(),
            "stale baseline must not dirty the workbook"
        );
        let folder = h.base.join("copy");
        std::fs::create_dir(&folder).unwrap();
        let file = folder.join("Restored.slate");
        h.app.doc().save_to(&file).unwrap();
        let reopened = SlateDoc::load_from(&file).unwrap();
        for (i, &id) in ids.iter().enumerate() {
            assert_eq!(
                *parameters(&reopened, id).unwrap(),
                edit(i as u32 + 1).state
            );
        }
        assert!(h
            .app
            .bind_web_source(ids[0], "controllers/Other.html".into()));
        assert!(parameters(h.app.doc(), ids[0]).is_none());
        assert!(parameters(h.app.doc(), ids[1]).is_some());
    }
}
