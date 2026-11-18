use super::*;
use atlas_agent::{ContextItem, InputSnapshot};
#[cfg(windows)]
use std::process::Command;

#[test]
fn isolated_artifact_copies_frozen_inputs_and_accepts_only_the_matching_single_html() {
    let base = std::env::temp_dir().join(atlas_agent::request_id());
    fs::create_dir_all(&base).unwrap();
    let owner = prepare_artifact_run(&base, "actual-family", "first", None).unwrap();
    let source = base.join("table.csv");
    fs::write(&source, b"n,value\nA,5").unwrap();
    let tables = HashMap::from([(41, source.clone())]);
    let wired = ContextItem {
        node: 41,
        text: String::new(),
        images: vec![],
        depth: None,
        outputs: Default::default(),
        active: None,
        slot: None,
    };
    let initial = InputSnapshot {
        wired: vec![wired],
        ..Default::default()
    };
    let inputs = freeze_artifact_inputs(&owner, &initial, &tables).unwrap().0;
    fs::write(&source, b"later edit").unwrap();
    let mut request = AgentRequest {
        id: owner.run_id.clone(),
        artifact: Some(owner.clone()),
        output_dir: Some(owner.staging_output.to_string_lossy().into_owned()),
        inputs,
        ..Default::default()
    };
    let stage =
        ArtifactProviderStage::prepare(&mut request, &std::env::temp_dir(), &tables).unwrap();
    let provider = request.artifact.as_ref().unwrap();
    let provider_dir = request.artifact_provider_directory().unwrap();
    assert!(request.artifact_draft_receipt);
    assert_ne!(provider.staging_output, owner.staging_output);
    let table: PathBuf = request.inputs.wired[0]
        .text
        .strip_prefix("Frozen table data for this generation: ")
        .unwrap()
        .into();
    assert!(table.starts_with(provider_dir));
    assert_eq!(fs::read(table).unwrap(), b"n,value\nA,5");
    let original = owner.staging_output.join(ARTIFACT_ENTRY);
    let prior = b"<html><body>prior</body></html>";
    let actual = b"<html><body>new</body></html>";
    fs::write(&original, prior).unwrap();
    let previous_receipt = b"old receipt";
    fs::write(base.join("return.json"), previous_receipt).unwrap();
    let live = provider.staging_output.join(ARTIFACT_ENTRY);
    fs::write(&live, actual).unwrap();
    let manifest = |id: &str, path: &Path| {
        serde_json::to_vec(&serde_json::json!({"id":id,"items":[{"as":"graphic","path":path}]}))
            .unwrap()
    };
    let returned = provider_dir.join("return.json");
    fs::write(&returned, manifest("sibling", &live)).unwrap();
    assert!(stage.import(&base).is_err());
    fs::write(&returned, manifest("first", &original)).unwrap();
    assert!(stage.import(&base).is_err());
    fs::write(&returned, manifest("first", &live)).unwrap();
    let extra = provider.staging_output.join("extra.txt");
    fs::write(&extra, b"no other files").unwrap();
    assert!(stage.import(&base).is_err());
    assert_eq!(fs::read(&original).unwrap(), prior);
    assert_eq!(
        fs::read(base.join("return.json")).unwrap(),
        previous_receipt
    );
    fs::remove_file(extra).unwrap();
    stage.import(&base).unwrap();
    validate_artifact_return(&owner, &base).unwrap();
    assert_eq!(fs::read(&original).unwrap(), actual);
    assert_eq!(fs::read(&source).unwrap(), b"later edit");
    fs::remove_dir_all(&base).unwrap();
}

#[cfg(windows)]
#[test]
fn long_windows_artifact_work_starts_the_provider_and_returns_to_its_original_owner() {
    let base = std::env::temp_dir().join(atlas_agent::request_id());
    let mut deep = base.clone();
    // Eight-character names preserve the test even when short DOS names are enabled.
    while deep.as_os_str().len() < 330 {
        deep.push("abcdefgh");
    }
    fs::create_dir_all(&deep).unwrap();
    let owner = prepare_artifact_run(&deep, "family", "projected", None).unwrap();
    let inputs = freeze_artifact_inputs(&owner, &Default::default(), &HashMap::new())
        .unwrap()
        .0;
    let mut request = AgentRequest {
        id: owner.run_id.clone(),
        artifact: Some(owner.clone()),
        output_dir: Some(owner.staging_output.to_string_lossy().into_owned()),
        inputs,
        ..Default::default()
    };
    let cmd = |cwd: &Path| {
        Command::new("cmd.exe")
            .args(["/d", "/c", "exit", "0"])
            .current_dir(cwd)
            .status()
    };
    assert!(matches!(
        cmd(&deep).unwrap_err().raw_os_error(),
        Some(206 | 267)
    ));
    let stage =
        ArtifactProviderStage::prepare_if_needed(&mut request, &deep, &deep, &HashMap::new())
            .unwrap()
            .unwrap();
    let work = request.artifact_provider_directory().unwrap();
    assert!(cmd(work).unwrap().success());
    let output = request
        .artifact
        .as_ref()
        .unwrap()
        .staging_output
        .join(ARTIFACT_ENTRY);
    fs::write(
        &output,
        b"<html><body>authenticated job stand-in</body></html>",
    )
    .unwrap();
    fs::write(
        work.join("return.json"),
        serde_json::to_vec(&serde_json::json!({
       "id":request.id,"items":[{"as":"graphic","path":output}]}))
        .unwrap(),
    )
    .unwrap();
    stage.import(&deep).unwrap();
    validate_artifact_return(&owner, &deep).unwrap();
    fs::remove_dir_all(&base).unwrap();
}
