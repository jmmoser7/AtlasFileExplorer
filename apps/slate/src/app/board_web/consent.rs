//! Machine-local consent for remote Web portal origins.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use slate_doc::scene::{web_origin, NodeKind, PortalKind};

use super::{SlateApp, WebRuntime};

/// `web-consent.json` in the per-user Atlas data folder.
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedConsent {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    workbooks: HashMap<String, HashSet<String>>,
    /// Explicit per-user global Web permission. Missing in older files.
    #[serde(default)]
    allow_all_websites: bool,
}

impl WebRuntime {
    pub fn has_consent(&self, origin: &str) -> bool {
        self.allow_all_websites
            || self.consent.contains(origin)
            || self.workbook_consent.contains(origin)
    }

    pub fn allows_all_websites(&self) -> bool {
        self.allow_all_websites
    }

    /// Update the per-user policy and persist it beside the workbook map.
    pub(crate) fn set_allow_all_websites(&mut self, allow: bool) {
        if self.allow_all_websites == allow {
            return;
        }
        self.allow_all_websites = allow;
        self.save_consent();
    }

    /// Permit one origin: for this session, and for the active saved workbook
    /// in later sessions. Local state only — never a journal command, never
    /// saved into the workbook (D32).
    pub fn grant_consent(&mut self, origin: impl Into<String>) {
        let origin = origin.into();
        self.consent.insert(origin.clone());
        if let Some(key) = self.consent_key() {
            self.workbook_consent.insert(origin.clone());
            self.saved_consent.entry(key).or_default().insert(origin);
            self.save_consent();
        }
    }

    pub fn revoke_consent(&mut self, origin: &str) {
        self.consent.remove(origin);
        if let Some(key) = self.consent_key() {
            self.workbook_consent.remove(origin);
            if let Some(origins) = self.saved_consent.get_mut(&key) {
                origins.remove(origin);
            }
            self.save_consent();
        }
    }

    /// Keep grants across sessions in `path`, loading what is already there.
    pub fn use_consent_file(&mut self, path: PathBuf) {
        let saved = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<SavedConsent>(&bytes).ok());
        self.saved_consent = saved
            .as_ref()
            .map(|saved| saved.workbooks.clone())
            .unwrap_or_default();
        self.allow_all_websites = saved.is_some_and(|saved| saved.allow_all_websites);
        self.consent_file = Some(path);
        self.workbook_consent = self.saved_for_active();
    }

    /// Follow the active tab. Cheap when unchanged.
    pub fn set_consent_workbook(&mut self, workbook: Option<&Path>) {
        if self.consent_workbook.as_deref() == workbook {
            return;
        }
        self.consent_workbook = workbook.map(Path::to_path_buf);
        self.workbook_consent = self.saved_for_active();
    }

    fn consent_key(&self) -> Option<String> {
        self.consent_file.as_ref()?;
        Some(
            self.consent_workbook
                .as_ref()?
                .to_string_lossy()
                .into_owned(),
        )
    }

    fn saved_for_active(&self) -> HashSet<String> {
        self.consent_key()
            .and_then(|key| self.saved_consent.get(&key).cloned())
            .unwrap_or_default()
    }

    fn save_consent(&self) {
        let Some(path) = &self.consent_file else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let saved = SavedConsent {
            version: 1,
            workbooks: self.saved_consent.clone(),
            allow_all_websites: self.allow_all_websites,
        };
        let _ = atlas_ai::agent::atomic_write_json(path, &saved);
    }
}

impl SlateApp {
    pub(crate) fn web_consent_command(&mut self, command: &str) -> bool {
        match command {
            "portal.web.allow_origin" => self.web_allow_selected_origin(),
            "portal.web.revoke_origin" => self.web_revoke_selected_origin(),
            "portal.web.allow_all_sites" => self.set_global_web_consent(true),
            "portal.web.restrict_sites" => self.set_global_web_consent(false),
            "portal.web.access_settings" => {
                self.portal_restore();
                self.tab_mut().chrome.advanced_open = true;
                true
            }
            _ => false,
        }
    }

    /// Set the explicit per-user Web policy. Disabling it immediately evicts
    /// live pages whose authored or redirected remote origin lacks a grant.
    pub(crate) fn set_global_web_consent(&mut self, allow: bool) -> bool {
        if self.web.allows_all_websites() == allow {
            return false;
        }
        self.web.set_allow_all_websites(allow);
        if allow {
            return true;
        }

        let mut release = Vec::new();
        for id in self.web.views.keys().copied() {
            let authored = self.doc().scene.node(id).and_then(|node| match &node.kind {
                NodeKind::Portal(portal) if portal.kind == PortalKind::Web => portal
                    .source
                    .as_ref()
                    .and_then(|source| web_origin(&source.locator)),
                _ => None,
            });
            let current = self
                .web
                .host
                .current_url(id)
                .and_then(|url| web_origin(&url));
            let resume = self.web.views[&id]
                .resume_url
                .as_deref()
                .and_then(web_origin);
            if [authored, current, resume]
                .into_iter()
                .flatten()
                .any(|origin| !self.web.has_consent(&origin))
            {
                release.push(id);
            }
        }

        for id in release {
            if let Some(view) = self.web.views.get_mut(&id) {
                view.resume_url = None;
            }
            if self.web.focused == Some(id) {
                self.web_blur();
            } else if let Some((leased, request)) = self.web.agent_focus_lease.clone() {
                if leased == id {
                    self.agent_web_release_focus(&request);
                }
            }
            self.release_web_host(id);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consent_file(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "slate_web_global_consent_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (dir.join("web-consent.json"), dir)
    }

    #[test]
    fn legacy_file_defaults_global_policy_to_false() {
        let (file, dir) = consent_file("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, r#"{"version":1,"workbooks":{}}"#).unwrap();
        let mut web = WebRuntime::default();
        web.use_consent_file(file);
        assert!(!web.allows_all_websites());
        assert!(!web.has_consent("https://novel.example"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn global_policy_persists_across_workbooks_and_preserves_site_grants() {
        let (file, dir) = consent_file("roundtrip");
        let first_book = dir.join("one.slate");
        let second_book = dir.join("two.slate");
        let site = "https://individually-allowed.example";

        let mut first = WebRuntime::default();
        first.use_consent_file(file.clone());
        first.set_consent_workbook(Some(&first_book));
        first.grant_consent(site);
        first.set_allow_all_websites(true);
        assert!(first.has_consent("https://global.example"));

        let mut later = WebRuntime::default();
        later.use_consent_file(file.clone());
        later.set_consent_workbook(Some(&second_book));
        assert!(later.allows_all_websites());
        assert!(later.has_consent("https://global.example"));
        later.set_allow_all_websites(false);
        assert!(!later.has_consent("https://global.example"));
        later.set_consent_workbook(Some(&first_book));
        assert!(later.has_consent(site), "the per-workbook grant survives");

        let mut restarted = WebRuntime::default();
        restarted.use_consent_file(file);
        restarted.set_consent_workbook(Some(&second_book));
        assert!(!restarted.allows_all_websites());
        assert!(!restarted.has_consent("https://global.example"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn global_policy_admits_agent_navigation_and_input_across_new_origins() {
        use super::super::WebInput;
        use crate::app::tests::{only_portal, web_board, web_settle, with_fake_host};
        let mut h = web_board("web_global_agent_navigation");
        let host = with_fake_host(&mut h);
        h.app
            .paste_web_url("https://first.example/page", eframe::egui::Pos2::ZERO);
        let (id, _) = only_portal(&h);
        h.app.set_web_portal_consent(id, false);
        assert!(h.app.agent_web_check_scope(id).is_err());

        assert!(h.app.web_consent_command("portal.web.allow_all_sites"));
        web_settle(&mut h, &[(id, 540.0)], 3);
        assert!(
            h.app.web.is_live(id),
            "global permission must admit the host"
        );
        for url in [
            "https://second.example/search",
            "https://third.example/result",
        ] {
            h.app.agent_web_navigate(id, url).unwrap();
            h.app.agent_web_check_scope(id).unwrap();
            assert_eq!(host.current_url(id).as_deref(), Some(url));
        }
        h.app.agent_web_focus(id, "global-agent-request").unwrap();
        h.app
            .agent_web_input(id, WebInput::Text("q".into()))
            .unwrap();
        assert!(host
            .inputs()
            .iter()
            .any(|input| matches!(input, WebInput::Text(text) if text == "q")));
        assert!(!h.app.agent_web_needs_file_grant(id).unwrap());

        assert!(h.app.web_consent_command("portal.web.restrict_sites"));
        assert!(h.app.agent_web_check_scope(id).is_err());
        assert!(!h.app.web.is_live(id));
        assert_eq!(h.app.web.focused, None);
    }

    #[test]
    fn disabling_global_policy_forgets_ungranted_evicted_visits() {
        use crate::app::tests::{only_portal, web_board, with_fake_host};
        let mut h = web_board("web_global_evicted_visit");
        let host = with_fake_host(&mut h);
        h.app.place_web_portal_at(eframe::egui::Pos2::ZERO);
        let (id, portal) = only_portal(&h);
        h.frame();
        h.app.set_global_web_consent(true);
        h.app.web.views.get_mut(&id).unwrap().resume_url =
            Some("https://ungranted.example/last-visit".into());
        h.app.release_web_host(id);
        host.0.borrow_mut().current_urls.remove(&id);
        assert!(h.app.set_global_web_consent(false));
        assert_eq!(h.app.web.views[&id].resume_url, None);
        let authored = &portal.source.as_ref().unwrap().locator;
        assert_eq!(h.app.resume_target(id, authored), *authored);
    }

    #[test]
    fn disabling_global_policy_evicts_ungranted_redirect_and_releases_focus() {
        use crate::app::tests::{only_portal, web_board, web_settle, with_fake_host};
        use eframe::egui::Pos2;

        let mut h = web_board("web_global_policy_revoke");
        let host = with_fake_host(&mut h);
        h.app.place_web_portal_at(Pos2::ZERO);
        let (blocked, _) = only_portal(&h);
        h.app.place_web_portal_at(Pos2::new(900.0, 0.0));
        let kept = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .find(|node| {
                node.id != blocked
                    && matches!(&node.kind, NodeKind::Portal(portal) if portal.kind == PortalKind::Web)
            })
            .expect("second Web portal")
            .id;
        web_settle(&mut h, &[(blocked, 540.0), (kept, 540.0)], 3);
        host.set_current_url(blocked, "https://ungranted-redirect.example/page");
        host.set_current_url(kept, "https://individually-allowed.example/page");
        h.app
            .web
            .grant_consent("https://individually-allowed.example");
        h.app.web.focused = Some(blocked);
        h.app.web.agent_focus_lease = Some((blocked, "revocation-test".into()));
        h.app.set_global_web_consent(true);
        assert!(h.app.web.has_consent("https://second-origin.example"));

        assert!(h.app.set_global_web_consent(false));
        assert!(!h.app.web.has_consent("https://second-origin.example"));
        assert!(!h.app.web.is_live(blocked));
        assert!(h.app.web.is_live(kept));
        assert_eq!(h.app.web.focused, None);
        assert_eq!(h.app.web.agent_focus_lease, None);
        assert_eq!(h.app.web.open_count(), 1);
        assert!(h
            .app
            .web
            .has_consent("https://individually-allowed.example"));
    }
}
