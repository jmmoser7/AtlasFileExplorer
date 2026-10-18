//! Synthetic full-app hosts must not discover the developer's installed providers.
use super::*;
use crate::app::tests::Harness;

const CATALOGS: [&str; 6] = [
    "codex",
    "cursor",
    "ollama",
    "comfy",
    "openai-image",
    "openai-text",
];

fn host() -> Harness {
    let mut host = Harness::new("catalog_probe_hold");
    host.app.set_agent_programs_for_test(&[]);
    host.app.at_home = false;
    host.app.ensure_work_tab();
    host.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    host
}

#[test]
fn synthetic_full_app_frames_keep_wanted_and_refreshed_catalogs_unready_without_probes() {
    let mut host = host();
    assert!(host.app.agents.hold_catalog_probes);
    for _ in 0..3 {
        host.frame();
        for key in CATALOGS {
            host.app.agents.want_catalog(key);
            host.app.agents.models_started.insert(key.into());
            host.app.agents.refresh_catalog(key);
        }
        host.frame();
        for key in CATALOGS {
            assert!(host.app.agents.models_wanted.contains(key));
            assert!(!host.app.agents.catalog_ready(key));
            assert!(host.app.agents.catalog_error(key).is_none());
            assert!(!host.app.agents.models_started.contains(key));
        }
        assert!(
            host.app.agents.models_rx.is_none(),
            "no automatic probe receiver may be launched"
        );
        assert!(
            host.app.agents.models.is_empty(),
            "held catalogs are absent, not ready-empty"
        );
    }
}

#[test]
fn held_catalog_pump_ingests_pending_success_and_error_without_launching_a_follow_on() {
    let mut host = host();
    for key in CATALOGS {
        host.app.agents.want_catalog(key);
    }
    let (tx, rx) = unbounded();
    host.app.agents.models_rx = Some(rx);
    host.app.agents.models_started.insert("codex".into());
    host.app.pump_agent_models();
    assert!(
        host.app.agents.models_rx.is_some(),
        "an existing pending receiver is retained"
    );
    assert!(!host.app.agents.catalog_ready("codex"));
    tx.send((
        "codex".into(),
        Ok(vec![atlas_ai::agent::AgentModel {
            id: "injected-model".into(),
            name: "Injected model".into(),
            ..Default::default()
        }]),
    ))
    .unwrap();
    host.app.pump_agent_models();
    assert!(host.app.agents.catalog_ready("codex"));
    assert_eq!(host.app.agents.catalog("codex")[0].id, "injected-model");
    assert!(host.app.agents.models_started.contains("codex"));
    assert!(host.app.agents.catalog_error("codex").is_none());
    assert!(host.app.agents.models_rx.is_none());
    for key in CATALOGS.into_iter().filter(|key| *key != "codex") {
        assert!(!host.app.agents.catalog_ready(key));
        assert!(!host.app.agents.models_started.contains(key));
    }
    let (tx, rx) = unbounded();
    host.app.agents.models_rx = Some(rx);
    tx.send(("cursor".into(), Err("Injected catalog failure".into())))
        .unwrap();
    host.app.pump_agent_models();
    assert!(!host.app.agents.catalog_ready("cursor"));
    assert_eq!(
        host.app.agents.catalog_error("cursor"),
        Some("Injected catalog failure")
    );
    assert!(host.app.agents.models_rx.is_none());
    assert_eq!(host.app.agents.models.len(), 1);
    host.app.agents.refresh_catalog("cursor");
    host.app.pump_agent_models();
    assert!(host.app.agents.hold_catalog_probes);
    assert!(host.app.agents.catalog_error("cursor").is_none());
    assert!(!host.app.agents.catalog_ready("cursor"));
    assert!(host.app.agents.models_rx.is_none());
}
