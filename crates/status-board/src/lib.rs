//! Status Board — extract a project-status snapshot and lay it out.
//!
//! Pure (Art. I): no renderer, no app types. The board painter and the
//! artifact writer both call [`layout_status`] so they cannot drift (Art. IV).

mod layout;
mod load;
mod model;

pub use layout::{layout_status, Align, Prim, Rect, Size, StatusLayout};
pub use load::{load_snapshot, resolve_snapshot_path, StatusError};
pub use model::{
    DeviationRow, Deviations, Inventory, Meta, NextAction, Phase, ProgressScores, Snapshot,
    StatusQuery, Thesis, Wave, Workplan,
};

/// Byte-identical layout for identical inputs (Art. IV.2 / D28).
pub fn fingerprint(snap: &Snapshot, query: &StatusQuery, frame: Size) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    // Quantize the frame so sub-pixel resize does not thrash the cache.
    ((frame.w.max(1.0)) as u32).hash(&mut h);
    ((frame.h.max(1.0)) as u32).hash(&mut h);
    query.hash(&mut h);
    snap.fingerprint_key().hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project-state.json")
    }

    #[test]
    fn fixture_loads() {
        let snap = load_snapshot(&fixture()).expect("fixture");
        assert_eq!(snap.meta.head_commit, "664e2e1");
        assert_eq!(snap.deviations.open, 8);
        assert!((snap.progress_scores.overall_roadmap - 0.28).abs() < 1e-6);
    }

    #[test]
    fn directory_resolves_project_state_json() {
        let dir = fixture().parent().unwrap().to_path_buf();
        let resolved = resolve_snapshot_path(&dir).expect("dir");
        assert_eq!(resolved, fixture());
    }

    #[test]
    fn layout_is_deterministic() {
        let snap = load_snapshot(&fixture()).unwrap();
        let q = StatusQuery::default();
        let frame = Size { w: 960.0, h: 720.0 };
        let a = layout_status(&snap, &q, frame);
        let b = layout_status(&snap, &q, frame);
        assert_eq!(a, b);
        assert_eq!(fingerprint(&snap, &q, frame), fingerprint(&snap, &q, frame));
        assert!(!a.prims.is_empty());
        assert!(a.caption.contains("664e2e1"));
    }

    #[test]
    fn hiding_sections_shrinks_the_layout() {
        let snap = load_snapshot(&fixture()).unwrap();
        let full = layout_status(&snap, &StatusQuery::default(), Size { w: 960.0, h: 720.0 });
        let compact = layout_status(
            &snap,
            &StatusQuery {
                show_overview: true,
                show_phases: false,
                show_waves: false,
                show_deviations: false,
                show_next: false,
            },
            Size { w: 960.0, h: 720.0 },
        );
        assert!(compact.prims.len() < full.prims.len());
    }

    #[test]
    fn missing_file_is_an_error() {
        let err = load_snapshot(std::path::Path::new("/definitely/not/a/status.json"));
        assert!(matches!(err, Err(StatusError::Missing { .. })));
    }

    #[test]
    fn not_json_is_not_a_snapshot() {
        let dir = std::env::temp_dir().join(format!("status-board-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("notes.txt");
        std::fs::write(&path, "hello").unwrap();
        let err = load_snapshot(&path);
        assert!(matches!(err, Err(StatusError::NotASnapshot { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
