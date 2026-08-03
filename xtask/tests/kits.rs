//! The guarantee the kit audit has to keep: every `.slatekit` file committed to
//! this repository parses and resolves, in the tree as it is committed.
//!
//! Kits are data the program reads at startup, so a malformed one is a runtime
//! surprise rather than a compile error. This is the check that turns it back
//! into a build failure — including for the built-in kit, which is where the
//! board's own tool results now live.

use std::path::{Path, PathBuf};

use xtask::kits::BUILTIN_KIT;
use xtask::{audit_kits, render_kit_audit};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ sits directly under the workspace root")
        .to_path_buf()
}

#[test]
fn every_committed_kit_parses_and_resolves() {
    let audit = audit_kits(&workspace_root()).expect("the kit files are readable");
    assert!(
        audit.findings.is_empty(),
        "kit audit findings:\n{}",
        render_kit_audit(&audit)
    );
}

#[test]
fn the_builtin_kit_is_present_and_carries_tools() {
    let root = workspace_root();
    let audit = audit_kits(&root).expect("the kit files are readable");
    assert!(
        !audit.files.is_empty(),
        "no kit files found — the audit would pass vacuously"
    );
    let builtin = audit
        .files
        .iter()
        .find(|f| f.path == root.join(BUILTIN_KIT))
        .expect("the compiled-in kit is committed at BUILTIN_KIT");
    assert_eq!(builtin.id, "core");
    assert!(
        builtin.tools >= 4,
        "the built-in kit lost tools: {} left",
        builtin.tools
    );
}

#[test]
fn the_audit_reports_a_broken_kit_rather_than_erroring_out() {
    // The audit's job is to collect findings, not to stop at the first one, so
    // one bad file cannot hide the rest.
    let dir = std::env::temp_dir().join(format!("xtask-kits-broken-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a-bad.slatekit"), "format_version = ").unwrap();
    std::fs::write(
        dir.join("b-good.slatekit"),
        "format_version = 1\nid = \"k\"\nname = \"K\"\n",
    )
    .unwrap();

    let audit = audit_kits(&dir).expect("a malformed kit is a finding, not an error");
    assert_eq!(audit.findings.len(), 1);
    assert_eq!(audit.files.len(), 1, "the healthy kit is still listed");

    std::fs::remove_dir_all(&dir).unwrap();
}
