//! Process-owned agent state follows the active document runtime.
use super::AgentRuntime;

impl AgentRuntime {
    /// Move what belongs to the process rather than to one document: sidecar
    /// and Codex processes, provider catalogs, staged proposals, and this
    /// bookkeeping. Everything else, NodeId-keyed or not, stays per document.
    pub(super) fn adopt_process(&mut self, other: &mut AgentRuntime) {
        use std::mem::swap;
        other.clear_generation_preview_residents();
        swap(&mut self.sidecar_spawned, &mut other.sidecar_spawned);
        swap(&mut self.sidecar_child, &mut other.sidecar_child);
        swap(&mut self.sidecar_booting, &mut other.sidecar_booting);
        swap(&mut self.sidecar_policies, &mut other.sidecar_policies);
        swap(&mut self.sidecar_startups, &mut other.sidecar_startups);
        swap(&mut self.sidecar_restart, &mut other.sidecar_restart);
        swap(&mut self.sidecar_boot_tx, &mut other.sidecar_boot_tx);
        swap(&mut self.sidecar_boot_rx, &mut other.sidecar_boot_rx);
        swap(&mut self.codex, &mut other.codex);
        swap(&mut self.live_decodes, &mut other.live_decodes);
        swap(&mut self.models, &mut other.models);
        swap(&mut self.models_rx, &mut other.models_rx);
        swap(&mut self.models_started, &mut other.models_started);
        swap(&mut self.models_error, &mut other.models_error);
        swap(&mut self.programs, &mut other.programs);
        swap(&mut self.programs_started, &mut other.programs_started);
        swap(&mut self.recents, &mut other.recents);
        swap(&mut self.provider_recents, &mut other.provider_recents);
        swap(&mut self.recents_rx, &mut other.recents_rx);
        swap(&mut self.recents_started, &mut other.recents_started);
        swap(&mut self.ide, &mut other.ide);
        swap(&mut self.ide_rx, &mut other.ide_rx);
        swap(&mut self.ide_inflight, &mut other.ide_inflight);
        swap(&mut self.ide_next, &mut other.ide_next);
        swap(&mut self.pending, &mut other.pending);
        swap(&mut self.stage, &mut other.stage);
        swap(&mut self.full_access, &mut other.full_access);
        swap(
            &mut self.dashboard_full_access,
            &mut other.dashboard_full_access,
        );
        swap(&mut self.access_snapshot, &mut other.access_snapshot);
        swap(&mut self.access_path, &mut other.access_path);
        swap(&mut self.schedules, &mut other.schedules);
        swap(&mut self.schedule_dialog, &mut other.schedule_dialog);
        swap(&mut self.life.doc, &mut other.life.doc);
        swap(&mut self.life.parked, &mut other.life.parked);
        swap(&mut self.life.rejoined, &mut other.life.rejoined);
        swap(&mut self.life.sidecar_dirs, &mut other.life.sidecar_dirs);
        #[cfg(test)]
        {
            swap(
                &mut self.hold_catalog_probes,
                &mut other.hold_catalog_probes,
            );
            swap(&mut self.dispatched, &mut other.dispatched);
            swap(&mut self.life.stopped, &mut other.life.stopped);
            swap(&mut self.life.hold_loads, &mut other.life.hold_loads);
            swap(&mut self.life.loads, &mut other.life.loads);
        }
        // Caches keyed by these epochs must not match across documents.
        self.output_epoch = self.output_epoch.max(other.output_epoch).wrapping_add(1);
        self.prompt_epoch = self.prompt_epoch.max(other.prompt_epoch).wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;
    use slate_doc::scene::NodeId;

    #[test]
    fn workbook_switches_carry_policy_and_startup_but_keep_document_drafts() {
        let mut first = AgentRuntime::default();
        let mut second = AgentRuntime::default();
        first.dashboard_full_access = true;
        first.access_snapshot = serde_json::from_value(serde_json::json!({
            "artifact_default_full": true, "overrides": {"denied": false}
        }))
        .unwrap();
        let policy = atlas_ai::sidecar::SidecarPolicy {
            artifact: true,
            full_access: Some(true),
            ..Default::default()
        };
        first.sidecar_policies.insert("dashboard".into(), policy);
        let startup = first
            .sidecar_startups
            .entry("dashboard".into())
            .or_default();
        let generation = startup.advance();
        first.sidecar_restart.insert("dashboard".into());
        first.prompts.insert(NodeId(1), "first workbook".into());
        second.prompts.insert(NodeId(1), "second workbook".into());

        second.adopt_process(&mut first);
        assert!(second.dashboard_full_access);
        assert!(second.access_snapshot.artifact_granted("new"));
        assert!(!second.access_snapshot.artifact_granted("denied"));
        assert_eq!(second.sidecar_policies.get("dashboard"), Some(&policy));
        assert!(second.sidecar_startups["dashboard"].current(generation));
        assert!(second.sidecar_restart.contains("dashboard"));
        assert_eq!(first.prompts[&NodeId(1)], "first workbook");
        assert_eq!(second.prompts[&NodeId(1)], "second workbook");

        first.adopt_process(&mut second);
        assert!(first.dashboard_full_access);
        assert!(first.sidecar_startups["dashboard"].current(generation));
        assert!(!first.access_snapshot.artifact_granted("denied"));
    }

    #[test]
    fn process_adoption_carries_test_catalog_hold_without_inventing_readiness() {
        let mut host = Harness::new("catalog_hold_adoption");
        let mut first = std::mem::take(&mut host.app.agents);
        let mut second = AgentRuntime::default();
        assert!(first.hold_catalog_probes);
        assert!(
            !second.hold_catalog_probes,
            "raw runtime retains automatic discovery"
        );
        second.adopt_process(&mut first);
        assert!(second.hold_catalog_probes);
        assert!(!first.hold_catalog_probes);
        second.refresh_catalog("codex");
        assert!(second.hold_catalog_probes);
        assert!(!second.catalog_ready("codex"));
        assert!(second.models_rx.is_none());
        first.adopt_process(&mut second);
        assert!(first.hold_catalog_probes);
        assert!(!second.hold_catalog_probes);
        assert!(!first.catalog_ready("codex"));
        assert!(first.models_rx.is_none());
    }
}
