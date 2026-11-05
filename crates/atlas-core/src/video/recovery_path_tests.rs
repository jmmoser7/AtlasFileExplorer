//! Real packaged-worker regression for paths produced by Windows canonicalize.
use super::*;

#[test]
fn canonical_extended_unicode_path_decodes_the_same_real_video_frames() {
    let directory = TestDirectory::new();
    let original =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-motion.mp4");
    let ordinary = directory.path().join("video-\u{03bb}-#100%.mp4");
    std::fs::copy(original, &ordinary).unwrap();
    let canonical = std::fs::canonicalize(&ordinary).unwrap();
    assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
    let pool = VideoPool::spawn();
    let normal = play_to_end(&pool, ordinary, 1500, 0.0);
    let extended = play_to_end(&pool, canonical, 1501, 0.0);
    assert_eq!(normal.0, 0.0);
    assert_eq!(extended.0, normal.0);
    assert_eq!(extended.1, normal.1);
    assert_eq!(pool.health().failures, 0);
}

#[test]
fn relative_fixture_path_decodes_from_the_inherited_working_directory() {
    let directory = std::env::current_dir().unwrap();
    let absolute =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-motion.mp4");
    let relative = absolute
        .strip_prefix(&directory)
        .expect("Cargo executes tests within the package/workspace")
        .to_path_buf();
    assert!(!relative.is_absolute());
    let pool = VideoPool::spawn();
    let frame = play_to_end(&pool, relative, 1502, 0.0);
    assert_eq!(frame.0, 0.0);
    assert!(!frame.1.is_empty());
    assert_eq!(pool.health().failures, 0);
}

#[test]
fn long_canonical_filesystem_path_decodes_without_url_or_max_path_truncation() {
    let directory = TestDirectory::new();
    let mut deep = directory.path().to_path_buf();
    while deep.as_os_str().len() < 280 {
        deep.push("long-video-directory");
    }
    std::fs::create_dir_all(&deep).unwrap();
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-motion.mp4");
    let destination = deep.join("video-\u{03bb}-#100%.mp4");
    std::fs::copy(source, &destination).unwrap();
    let canonical = std::fs::canonicalize(destination).unwrap();
    assert!(canonical.as_os_str().len() > 260);
    assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
    let pool = VideoPool::spawn();
    let frame = play_to_end(&pool, canonical, 1503, 0.0);
    assert_eq!(frame.0, 0.0);
    assert!(!frame.1.is_empty());
    assert_eq!(pool.health().failures, 0);
}
