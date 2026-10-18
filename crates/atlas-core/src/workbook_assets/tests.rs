use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(prefix: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}-{n}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn layout_paths_sit_beside_the_workbook() {
    let workbook = Path::new("/boards/deck/Board.slate");
    assert_eq!(assets_root(workbook), PathBuf::from("/boards/deck/assets"));
    assert_eq!(
        pasted_dir(workbook),
        PathBuf::from("/boards/deck/assets/pasted")
    );
    assert_eq!(
        generated_dir(workbook, "openai-image", "2026-10"),
        PathBuf::from("/boards/deck/assets/generated/openai-image/2026-10")
    );
    assert_eq!(
        generated_dir(workbook, "ollama/llava", "2026-10"),
        PathBuf::from("/boards/deck/assets/generated/ollama-llava/2026-10")
    );
    assert_eq!(
        web_package_dir(workbook, "Dash"),
        PathBuf::from("/boards/deck/assets/web/Dash")
    );
    assert_eq!(
        documents_dir(workbook),
        PathBuf::from("/boards/deck/assets/documents")
    );
    assert_eq!(
        agent_dir(workbook),
        PathBuf::from("/boards/deck/assets/agent")
    );
    let data = Path::new("/app-data");
    assert_eq!(paste_dir(None, data), PathBuf::from("/app-data/pasted"));
    assert_eq!(paste_dir(Some(workbook), data), pasted_dir(workbook));
    assert_eq!(
        generated_output_dir(None, data, "comfy", "session", "2026-10"),
        PathBuf::from("/app-data/comfy/session")
    );
    assert_eq!(
        unsaved_agent_dir(data),
        PathBuf::from("/app-data/assets/agent")
    );
    let pages = document_pages_dir(Some(workbook), data, "Essay", Path::new("/docs/Essay.docx"));
    let name = pages.file_name().unwrap().to_string_lossy();
    assert!(name.starts_with("Essay-") && name.len() == "Essay-".len() + 8);
    assert!(pages.starts_with(documents_dir(workbook)));
    let unsaved = document_pages_dir(None, data, "Essay", Path::new("/docs/Essay.docx"));
    assert!(unsaved.starts_with(data.join("document-pages")));
    assert_eq!(pages.file_name(), unsaved.file_name());
}

#[test]
fn year_month_is_utc() {
    assert_eq!(year_month(0), "1970-01");
    assert_eq!(year_month(86_400), "1970-01");
    assert_eq!(year_month(31 * 86_400), "1970-02");
}

#[test]
fn sidecar_has_no_secret_slot_and_scrub_drops_keys() {
    let sidecar = GeneratedSidecar {
        prompt: "a red barn".into(),
        model: "gpt-image".into(),
        seed: Some(7),
        params: BTreeMap::from([("aspect".into(), "3:2".into())]),
        created: 1,
        source_run: "req-1".into(),
    };
    let json = serde_json::to_string(&sidecar).unwrap();
    assert!(json.contains("a red barn"));
    assert!(!json.contains("api_key"));
    let mut dirty: serde_json::Value = serde_json::from_str(
        r#"{"prompt":"hi","api_key":"sk-secret","params":{"access_token":"no","seed":"1"}}"#,
    )
    .unwrap();
    scrub_sidecar(&mut dirty);
    let text = dirty.to_string();
    assert!(text.contains("hi"));
    assert!(!text.contains("sk-secret"));
    assert!(!text.contains("access_token"));
    assert!(text.contains("seed"));
}

#[test]
fn collect_copies_data_dir_images_and_refuses_the_rest() {
    let root = temp_dir("wb-assets");
    let data = root.join("NativeFileAtlas");
    let workbook = root.join("boards").join("Board.slate");
    std::fs::create_dir_all(data.join("pasted")).unwrap();
    std::fs::create_dir_all(data.join("openai-image").join("sess")).unwrap();
    std::fs::create_dir_all(data.join("thumbs")).unwrap();
    std::fs::create_dir_all(data.join("web-stills")).unwrap();
    let pasted = data.join("pasted").join("paste-9.png");
    let generated = data.join("openai-image").join("sess").join("req-1.png");
    let thumb = data.join("thumbs").join("a.png");
    let still = data.join("web-stills").join("shot.png");
    let user = root.join("desktop").join("holiday.png");
    std::fs::create_dir_all(user.parent().unwrap()).unwrap();
    std::fs::write(&pasted, b"pasted-bytes").unwrap();
    std::fs::write(&generated, b"gen-bytes").unwrap();
    std::fs::write(
        sidecar_path(&generated),
        br#"{"prompt":"barn","api_key":"sk-live","model":"gpt"}"#,
    )
    .unwrap();
    std::fs::write(&thumb, b"thumb").unwrap();
    std::fs::write(&still, b"still").unwrap();
    std::fs::write(&user, b"user").unwrap();
    let old_paste = root.join("boards").join("assets").join("paste-old.png");
    std::fs::create_dir_all(old_paste.parent().unwrap()).unwrap();
    std::fs::write(&old_paste, b"old").unwrap();

    let outcome = collect_data_dir_assets(
        &data,
        &workbook,
        &[
            pasted.clone(),
            generated.clone(),
            thumb.clone(),
            still.clone(),
            user.clone(),
            PathBuf::from("assets/paste-old.png"),
        ],
        1_760_000_000,
    );
    assert_eq!(outcome.collected.len(), 2, "{outcome:?}");
    let pasted_to = pasted_dir(&workbook).join("paste-9.png");
    let gen_to = outcome
        .collected
        .iter()
        .find(|c| c.from == generated)
        .unwrap();
    assert!(gen_to.locator.starts_with("assets/generated/openai-image/"));
    assert!(gen_to.locator.ends_with("/req-1.png"), "{}", gen_to.locator);
    assert_eq!(std::fs::read(&pasted_to).unwrap(), b"pasted-bytes");
    assert_eq!(std::fs::read(&gen_to.to).unwrap(), b"gen-bytes");
    let sidecar: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sidecar_path(&gen_to.to)).unwrap()).unwrap();
    assert_eq!(sidecar["prompt"], "barn");
    assert!(sidecar.get("api_key").is_none());
    assert!(outcome
        .refused
        .iter()
        .any(|r| r.path == user && r.reason == "outside the app data folder"));
    assert!(outcome
        .refused
        .iter()
        .any(|r| r.path == still && r.reason == "machine-private"));
    assert!(outcome
        .refused
        .iter()
        .any(|r| r.path == thumb && r.reason == "not a pasted or generated image"));
    assert!(!outcome
        .refused
        .iter()
        .any(|r| r.path.ends_with("paste-old.png")));
    assert!(old_paste.is_file());
    assert!(!pasted_dir(&workbook).join("paste-old.png").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn save_as_copies_only_assets_the_workbook_references() {
    let root = temp_dir("wb-saveas");
    let from = root.join("a").join("Board.slate");
    let to = root.join("b").join("Board.slate");
    let pasted = root
        .join("a")
        .join("assets")
        .join("pasted")
        .join("paste-1.png");
    let photo = root.join("a").join("photos").join("north.png");
    std::fs::create_dir_all(pasted.parent().unwrap()).unwrap();
    std::fs::create_dir_all(photo.parent().unwrap()).unwrap();
    std::fs::write(&pasted, b"paste").unwrap();
    std::fs::write(&photo, b"photo").unwrap();
    let outcome = copy_referenced_assets(
        &from,
        &to,
        &[PathBuf::from("assets/pasted/paste-1.png"), photo.clone()],
    );
    assert_eq!(outcome.collected.len(), 1, "{outcome:?}");
    assert!(outcome.refused.is_empty(), "{outcome:?}");
    let copied = root
        .join("b")
        .join("assets")
        .join("pasted")
        .join("paste-1.png");
    assert_eq!(std::fs::read(copied).unwrap(), b"paste");
    assert!(!root.join("b").join("photos").join("north.png").exists());
    assert_eq!(std::fs::read(photo).unwrap(), b"photo");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn first_save_collects_owned_portal_mp4_and_stills_without_absorbing_other_videos() {
    let root = temp_dir("wb-portal-captures");
    let data = root.join("NativeFileAtlas");
    let workbook = root.join("boards").join("Board.slate");
    let capture = data.join(CAPTURES).join("capture-1-1760000000-25-1.mp4");
    let still = data.join(CAPTURES).join("capture-1-1760000000-25-2.png");
    let refused = [
        data.join(CAPTURES).join("holiday.mp4"),
        data.join(CAPTURES).join("nested").join("capture-3.mp4"),
        data.join("openai-image").join("capture-4.mp4"),
        data.join("pasted").join("capture-5.mp4"),
        data.join(CAPTURES).join("capture-6.webm"),
        data.join("web-stills").join("capture-7.mp4"),
    ];
    let sources: Vec<_> = [capture.clone(), still.clone()]
        .into_iter()
        .chain(refused.iter().cloned())
        .collect();
    for source in &sources {
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(source, b"owned test bytes").unwrap();
    }
    let outcome = collect_data_dir_assets(&data, &workbook, &sources, 1_760_000_000);
    assert_eq!(outcome.collected.len(), 2, "{outcome:?}");
    for source in [capture, still] {
        let collected = outcome.collected.iter().find(|c| c.from == source).unwrap();
        assert!(collected.locator.starts_with("assets/generated/captures/"));
        assert_eq!(std::fs::read(&collected.to).unwrap(), b"owned test bytes");
        assert!(source.is_file(), "first Save copies the durable original");
    }
    for source in refused {
        assert!(
            outcome.refused.iter().any(|r| r.path == source),
            "{outcome:?}"
        );
    }
    assert!(root.starts_with(std::env::temp_dir()));
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn first_save_does_not_hydrate_a_dehydrated_owned_capture() {
    let root = temp_dir("wb-cloud-capture");
    let data = root.join("NativeFileAtlas");
    let source = data.join(CAPTURES).join("capture-1-cloud.mp4");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, b"owned cloud fixture").unwrap();
    assert!(crate::cloud::mark_offline(&source));
    let outcome = collect_data_dir_assets(
        &data,
        &root.join("Board.slate"),
        std::slice::from_ref(&source),
        0,
    );
    assert!(outcome.collected.is_empty());
    assert!(
        outcome
            .refused
            .iter()
            .any(|r| r.path == source && r.reason == "cloud placeholder"),
        "{outcome:?}"
    );
    assert!(crate::cloud::is_dehydrated(&source));
    crate::cloud::clear_offline(&source);
    assert!(root.starts_with(std::env::temp_dir()));
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn collect_refuses_a_dehydrated_placeholder_without_copying_it() {
    let root = temp_dir("wb-cloud");
    let data = root.join("NativeFileAtlas");
    let workbook = root.join("Board.slate");
    let src = data.join("pasted").join("paste-cloud.png");
    std::fs::create_dir_all(src.parent().unwrap()).unwrap();
    std::fs::write(&src, b"cloud").unwrap();
    assert!(
        crate::cloud::mark_offline(&src),
        "could not mark the fixture offline"
    );
    let outcome = collect_data_dir_assets(&data, &workbook, std::slice::from_ref(&src), 0);
    assert!(
        outcome
            .refused
            .iter()
            .any(|r| r.reason == "cloud placeholder"),
        "{outcome:?}"
    );
    assert!(outcome.collected.is_empty());
    assert!(!pasted_dir(&workbook).join("paste-cloud.png").exists());
    crate::cloud::clear_offline(&src);
    let _ = std::fs::remove_dir_all(root);
}
