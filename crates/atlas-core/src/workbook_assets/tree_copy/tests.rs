use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "atlas-package-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn path(&self, part: &str) -> PathBuf {
        self.0.join(part)
    }
    fn write(&self, part: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(part);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn ordinary_dashboard_copies_to_a_sibling() {
    let f = Fixture::new();
    f.write("source/index.html", b"dashboard");
    f.write("source/data/table.csv", b"x,y\n1,2");
    let destination = f.path("output/page");
    assert_eq!(copy_tree(&f.path("source"), &destination).unwrap(), 2);
    assert_eq!(
        fs::read(destination.join("index.html")).unwrap(),
        b"dashboard"
    );
    assert_eq!(
        fs::read(destination.join("data/table.csv")).unwrap(),
        b"x,y\n1,2"
    );
}

#[test]
#[cfg(windows)]
fn canonical_verbatim_asset_paths_skip_the_incomplete_drive_prefix() {
    let f = Fixture::new();
    let source = f
        .write("source/index.html", b"dashboard")
        .canonicalize()
        .unwrap();
    assert!(source.to_string_lossy().starts_with(r"\\?\"));
    require_local_source(&source).unwrap();
    let destination = f.0.canonicalize().unwrap().join("new/index.html");
    require_copy_destination(&destination).unwrap();
    copy_file(&source, &destination).unwrap();
    assert_eq!(fs::read(destination).unwrap(), b"dashboard");
}

#[test]
fn self_and_descendant_destinations_fail_before_creation() {
    let f = Fixture::new();
    let file = f.write("source/index.html", b"keep");
    let source = f.path("source");
    for destination in [
        source.clone(),
        source.join("assets/web/page"),
        source.join("absent/../output"),
    ] {
        assert_eq!(
            copy_tree(&source, &destination).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert!(!source.join("assets").exists());
    assert!(!source.join("absent").exists());
    assert!(!source.join("output").exists());
    assert!(copy_file(&file, &file).is_err());
    assert_eq!(fs::read(file).unwrap(), b"keep");
}

#[test]
fn private_descendant_refuses_the_whole_tree_before_copying() {
    let f = Fixture::new();
    f.write("source/index.html", b"public");
    f.write(
        "source/NativeFileAtlas/secrets/synthetic",
        b"dummy sentinel",
    );
    let output = f.path("output");
    assert_eq!(
        copy_tree(&f.path("source"), &output).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert!(!output.exists());
}

#[test]
fn private_destination_is_never_created() {
    let f = Fixture::new();
    let source = f.write("source/page.html", b"public");
    let output = f.path("NativeFileAtlas/webview2/copied.html");
    assert!(copy_file(&source, &output).is_err());
    assert!(!output.exists());
}

#[cfg(windows)]
#[test]
fn case_variants_cannot_hide_a_descendant_destination() {
    let f = Fixture::new();
    f.write("source/index.html", b"public");
    let source = f.path("source");
    let output = PathBuf::from(source.to_string_lossy().to_uppercase()).join("output");
    assert!(copy_tree(&source, &output).is_err());
    assert!(!source.join("output").exists());
}

#[cfg(windows)]
#[test]
fn cloud_descendant_is_refused_before_output_or_byte_copy() {
    let f = Fixture::new();
    let source = f.write("source/offline.txt", b"synthetic local test data");
    assert!(crate::cloud::mark_offline(&source));
    let result = copy_tree(&f.path("source"), &f.path("output"));
    let _ = crate::cloud::clear_offline(&source);
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    assert!(!f.path("output").exists());
}

#[cfg(unix)]
#[test]
fn source_and_destination_aliases_are_refused() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.write("source/index.html", b"public");
    fs::create_dir_all(f.path("actual-output")).unwrap();
    symlink(f.path("source"), f.path("source-alias")).unwrap();
    symlink(f.path("actual-output"), f.path("output-alias")).unwrap();
    assert!(copy_tree(&f.path("source-alias"), &f.path("one")).is_err());
    assert!(copy_tree(&f.path("source"), &f.path("output-alias/page")).is_err());
    assert!(!f.path("one").exists());
    assert!(!f.path("actual-output/page").exists());
}

#[cfg(unix)]
#[test]
fn parent_components_do_not_hide_symlink_traversal() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let private = f.write("NativeFileAtlas/secrets/value.txt", b"synthetic secret");
    fs::create_dir_all(f.path("NativeFileAtlas/secrets/child")).unwrap();
    symlink(f.path("NativeFileAtlas/secrets/child"), f.path("alias")).unwrap();
    let public = f.write("value.txt", b"public");
    let output = f.path("copied.txt");
    assert!(copy_file(&f.path("alias/../value.txt"), &output).is_err());
    assert!(copy_file(&public, &f.path("alias/../overwritten.txt")).is_err());
    assert!(!output.exists());
    assert!(!private.parent().unwrap().join("overwritten.txt").exists());
    assert_eq!(fs::read(private).unwrap(), b"synthetic secret");
}

#[cfg(unix)]
#[test]
fn generated_metadata_cannot_follow_an_existing_file_alias() {
    let f = Fixture::new();
    let sentinel = f.write("outside.txt", b"preserve me");
    fs::create_dir_all(f.path("package")).unwrap();
    let provenance = f.path("package/origin.txt");
    std::os::unix::fs::symlink(&sentinel, &provenance).unwrap();
    assert!(require_copy_destination(&provenance).is_err());
    assert!(write_file(&provenance, b"generated").is_err());
    assert_eq!(fs::read(sentinel).unwrap(), b"preserve me");
}

#[test]
fn ancestor_output_cannot_overwrite_source_contents() {
    let f = Fixture::new();
    f.write("source/source/page.html", b"public");
    assert!(copy_tree(&f.path("source"), &f.path("")).is_err());
}
