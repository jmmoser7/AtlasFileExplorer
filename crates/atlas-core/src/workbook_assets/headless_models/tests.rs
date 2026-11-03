use super::*;

#[test]
fn collect_only_provenanced_linked_models_and_save_as_owned_copies() {
    let base = std::env::temp_dir().join(format!(
        "headless-model-assets-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let data = base.join("data");
    let board = base.join("board/Board.slate");
    let models = data.join("headless-rhino/models");
    std::fs::create_dir_all(&models).unwrap();
    let mut sources = Vec::new();
    for (id, bytes) in [
        ("empty-fixture", b"empty".as_slice()),
        ("native-a", b"model A".as_slice()),
        ("native-b", b"model B".as_slice()),
    ] {
        let path = models.join(format!("{id}.3dm"));
        std::fs::write(&path, bytes).unwrap();
        write_headless_model_provenance(&path, id).unwrap();
        sources.push(path);
    }
    let unproven = models.join("linked-original.3dm");
    std::fs::write(&unproven, b"do not collect").unwrap();
    sources.push(unproven.clone());
    let private = data.join("headless-rhino/jobs/run/input.3dm");
    std::fs::create_dir_all(private.parent().unwrap()).unwrap();
    std::fs::write(&private, b"private snapshot").unwrap();
    write_headless_model_provenance(&private, "input").unwrap();
    sources.push(private.clone());
    let transcript = data.join("headless-rhino/models/session.json");
    std::fs::write(&transcript, b"private transcript").unwrap();
    sources.push(transcript.clone());
    let outcome = collect_data_dir_assets(&data, &board, &sources, 0);
    assert_eq!(outcome.collected.len(), 3);
    assert_eq!(outcome.refused.len(), 3);
    assert!(outcome.collected.iter().all(|a| a
        .locator
        .starts_with("assets/generated/headless-rhino/")
        && a.to.extension().unwrap() == "3dm"));
    let copied = outcome
        .collected
        .iter()
        .map(|a| a.to.clone())
        .collect::<Vec<_>>();
    let moved = base.join("moved/Board.slate");
    let again = copy_referenced_assets(&board, &moved, &copied);
    assert_eq!(again.collected.len(), 3);
    for asset in &outcome.collected {
        std::fs::remove_file(&asset.from).unwrap();
    }
    for (index, asset) in again.collected.iter().enumerate() {
        assert_eq!(
            std::fs::read(&asset.to).unwrap(),
            [
                b"empty".as_slice(),
                b"model A".as_slice(),
                b"model B".as_slice()
            ][index]
        );
    }
    assert!(private.exists() && transcript.exists() && unproven.exists());
    assert!(!assets_root(&board).join("jobs").exists());
}
