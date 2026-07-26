//! The two guarantees the metrics tool has to keep: it measures the same tree
//! the same way twice, and it agrees with the ledger it reports on.

use std::path::{Path, PathBuf};

use xtask::{collect, parse_deviations, render_readme, snapshot_json, verify_workspace_root};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ sits directly under the workspace root")
        .to_path_buf()
}

#[test]
fn metrics_snapshot_is_deterministic() {
    let root = workspace_root();
    let first = collect(&root).expect("first collection");
    let second = collect(&root).expect("second collection");

    assert_eq!(
        snapshot_json(&first),
        snapshot_json(&second),
        "two runs on an unchanged tree must produce byte-identical JSON"
    );
    assert_eq!(
        render_readme(std::slice::from_ref(&first)),
        render_readme(std::slice::from_ref(&second)),
        "two runs on an unchanged tree must render an identical README"
    );
}

#[test]
fn deviation_counts_match_ledger() {
    let root = workspace_root();
    let path = root.join("docs").join("audit").join("deviations.md");
    let counts = parse_deviations(&path).expect("the ledger parses");

    // Deliberately not a hardcoded total: every card that closes a row would
    // break that, which is a test failing on success. What must hold is that
    // the parser sees the whole ledger and classifies every row.
    let rows = std::fs::read_to_string(&path)
        .expect("the ledger reads")
        .lines()
        .filter(|line| line.trim_start().starts_with("| DV-"))
        .count() as u32;

    assert_eq!(
        counts.open + counts.accepted + counts.closed,
        rows,
        "every DV row must be classified as open, accepted, or closed"
    );
    assert!(rows >= 11, "the seeded ledger's eleven rows are never deleted");
}

#[test]
fn metrics_refuses_to_run_outside_the_workspace_root() {
    let root = workspace_root();
    verify_workspace_root(&root).expect("the workspace root is accepted");

    let subdir = root.join("crates").join("slate-doc");
    let err = verify_workspace_root(&subdir).expect_err("a member crate is not the root");
    assert!(
        err.to_string().contains("workspace root"),
        "the message must say where to run the tool: {err}"
    );
}

#[test]
fn snapshot_reports_the_model_and_command_surface() {
    let root = workspace_root();
    let snapshot = collect(&root).expect("collection");

    assert_eq!(
        snapshot.model.node_kinds as usize,
        snapshot.model.node_kind_names.len()
    );
    assert_eq!(
        snapshot.model.edge_roles as usize,
        snapshot.model.edge_role_names.len()
    );
    assert!(snapshot.commands.slate > 0 && snapshot.commands.file_atlas > 0);
    assert!(snapshot.crates.windows(2).all(|w| w[0].name < w[1].name));
    assert_eq!(
        snapshot.totals.pure_lines_code + snapshot.totals.renderer_lines_code,
        snapshot.totals.lines_code
    );
}
