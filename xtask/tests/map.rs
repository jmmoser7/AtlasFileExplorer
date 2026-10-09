//! Committed code map and symbol index must match `cargo xtask map`.

use std::path::{Path, PathBuf};

use xtask::map;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ sits directly under the workspace root")
        .to_path_buf()
}

#[test]
fn committed_code_map_is_fresh() {
    map::check(&workspace_root()).expect("code map matches collector");
}
