use super::*;

fn test_path(tag: &str) -> PathBuf {
    let serial = NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "slate-capture-output-test-{}-{serial}-{tag}.png",
        std::process::id()
    ))
}

#[test]
fn color_image_writes_straight_alpha_instead_of_dark_premultiplied_pixels() {
    let path = test_path("alpha");
    let color = egui::Color32::from_rgba_unmultiplied(100, 50, 24, 128);
    let image = egui::ColorImage::new([1, 1], color);
    write_color_png(&path, &image).unwrap();
    let decoded = image::open(&path).unwrap().to_rgba8();
    assert_eq!(decoded.get_pixel(0, 0).0, color.to_srgba_unmultiplied());
    assert!(decoded.get_pixel(0, 0).0[0] > color.r());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn invalid_capture_does_not_truncate_an_existing_destination() {
    let path = test_path("incomplete");
    std::fs::write(&path, b"existing capture").unwrap();
    let frame = CapturedFrame {
        generation: 1,
        w: 20,
        h: 20,
        rgba: vec![0; 4],
        captured_at: std::time::Instant::now(),
        recycle: None,
    };
    assert!(write_still(&path, &frame).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"existing capture");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn owned_draft_can_create_a_missing_output_folder_on_the_worker() {
    let path = test_path("missing-directory")
        .with_extension("")
        .join("nested")
        .join("capture.mp4");
    let directory = path.parent().unwrap().parent().unwrap().to_path_buf();
    let draft = Draft::reserve(path.clone()).unwrap();
    let temporary = draft.path().to_path_buf();
    assert!(temporary.is_file());
    drop(draft);
    assert!(!temporary.exists());
    std::fs::remove_dir(path.parent().unwrap()).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn concurrent_numbered_image_publication_preserves_existing_and_complete_outputs() {
    let destination = test_path("numbered");
    let numbered = |number| {
        destination.with_file_name(format!(
            "{}_{number:03}.png",
            destination.file_stem().unwrap().to_string_lossy()
        ))
    };
    std::fs::write(&destination, b"existing original").unwrap();
    std::fs::write(numbered(1), b"existing numbered").unwrap();
    let first = Draft::reserve(destination.clone()).unwrap();
    let second = Draft::reserve(destination.clone()).unwrap();
    std::fs::write(first.path(), b"first finalized image").unwrap();
    std::fs::write(second.path(), b"second finalized image").unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let jobs: Vec<_> = [first, second]
        .into_iter()
        .map(|draft| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                draft.commit_unique().unwrap()
            })
        })
        .collect();
    let paths: Vec<_> = jobs.into_iter().map(|job| job.join().unwrap()).collect();
    assert_ne!(paths[0], paths[1]);
    assert!(paths.contains(&numbered(2)));
    assert!(paths.contains(&numbered(3)));
    assert_eq!(std::fs::read(&destination).unwrap(), b"existing original");
    assert_eq!(std::fs::read(numbered(1)).unwrap(), b"existing numbered");
    assert_eq!(std::fs::read(&paths[0]).unwrap(), b"first finalized image");
    assert_eq!(std::fs::read(&paths[1]).unwrap(), b"second finalized image");
    for path in [destination.clone(), numbered(1), numbered(2), numbered(3)] {
        std::fs::remove_file(path).unwrap();
    }
}
