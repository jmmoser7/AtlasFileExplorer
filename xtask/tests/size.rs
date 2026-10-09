//! Rust sources must stay under 500 physical lines unless allowlisted.

use std::path::{Path, PathBuf};

use xtask::{audit_size, render_size_audit};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ sits directly under the workspace root")
        .to_path_buf()
}

#[test]
fn rust_sources_respect_size_cap_or_allowlist() {
    let audit = audit_size(&workspace_root()).expect("scoped sources are readable");
    assert!(
        audit.findings.is_empty(),
        "size lint findings:\n{}",
        render_size_audit(&audit)
    );
}
