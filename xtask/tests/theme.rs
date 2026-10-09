//! Hardcoded board UI colours regress to literals without an allowlist row.

use std::path::{Path, PathBuf};

use xtask::{audit_theme, render_theme_audit};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ sits directly under the workspace root")
        .to_path_buf()
}

#[test]
fn board_ui_has_no_unallowlisted_color_literals() {
    let audit = audit_theme(&workspace_root()).expect("scoped app files are readable");
    assert!(
        audit.findings.is_empty(),
        "theme lint findings:\n{}",
        render_theme_audit(&audit)
    );
}
