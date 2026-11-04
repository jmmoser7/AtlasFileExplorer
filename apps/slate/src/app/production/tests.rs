use super::*;
use crate::app::tests::Harness;
fn config(h: &Harness, workbook: &Path, command: &str) -> PathBuf {
    let path = h.base.join("production.json");
    let value = json!({"version":1,"workspace_root":h.base,"workbook":workbook,"author":"Design team","actions":[{"command":command,"arguments":{"x":0,"y":0,"width":600,"height":400,"title":"Design"}}]});
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    path
}
#[test]
fn production_refuses_existing_workbook_without_touching_it() {
    let h = Harness::new("production-preserve");
    let workbook = h.base.join("existing.slate");
    std::fs::write(&workbook, b"preserve").unwrap();
    let path = config(&h, &workbook, "board.objects.create_frame");
    assert!(load_plan(&path).is_err());
    assert_eq!(std::fs::read(workbook).unwrap(), b"preserve");
}
#[test]
fn production_command_list_cannot_execute_arbitrary_source_code() {
    let h = Harness::new("production-closed-commands");
    let workbook = h.base.join("new.slate");
    let path = config(&h, &workbook, "powershell");
    assert!(load_plan(&path).is_err());
    assert!(!workbook.exists());
}
#[test]
fn production_rejects_output_outside_its_explicit_root() {
    let h = Harness::new("production-output-scope");
    let workbook = h.base.parent().unwrap().join("outside.slate");
    let path = config(&h, &workbook, "board.objects.create_frame");
    assert!(load_plan(&path).is_err());
}

#[test]
fn production_aliases_and_index_wait_for_the_witnessed_receipt() {
    let mut h = Harness::new("production-witness-pending");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let workbook = h.base.join("witnessed.slate");
    h.app.doc().save_to(&workbook).unwrap();
    h.app.tab_mut().path = Some(workbook.clone());
    let (events, _replies) = crossbeam_channel::bounded(16);
    let mut runtime = ProductionRuntime {
        plan: Some(Plan {
            config: Configuration {
                version: 1,
                workspace_root: h.base.clone(),
                workbook,
                author: "Architect".into(),
                interval_ms: 120,
                actions: vec![Action {
                    id: "first-frame".into(),
                    author: None,
                    command: "board.objects.create_frame".into(),
                    arguments: json!({"x":24,"y":32,"width":240,"height":160}),
                    targets: vec![],
                    focus: None,
                    file: None,
                    capture_destination: None,
                }],
            },
            events,
        }),
        ..Default::default()
    };
    for _ in 0..2 {
        h.app.production_step(&h.ctx, &mut runtime).unwrap();
        assert_eq!(runtime.index, 0);
        assert!(runtime.witness_pending.is_some());
        assert!(runtime.aliases.is_empty());
        assert!(runtime.last.is_empty());
        assert!(h.app.doc().scene.nodes.is_empty());
    }
}
