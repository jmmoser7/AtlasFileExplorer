use super::*;
use crate::SlateLoadError;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn link_revision_tracks_sources_but_not_authored_equality_or_scene_edits() {
    let mut doc = SlateDoc::new("links");
    let initial = doc.item_paths_revision();
    let id = doc.add_item("old.txt".into(), "old.txt", 0, 0, "");
    assert_ne!(doc.item_paths_revision(), initial);
    let added = doc.item_paths_revision();
    assert_eq!(doc.add_item("old.txt".into(), "old.txt", 0, 0, ""), id);
    doc.scene.build_node(
        crate::scene::WorldRect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        },
        crate::scene::NodeKind::Shape(crate::scene::ShapeNode {
            shape: crate::scene::ShapeKind::Rect,
            fill: None,
            stroke: Default::default(),
            corner: Default::default(),
            sides: crate::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    assert_eq!(doc.item_paths_revision(), added);
    assert!(doc.relink(id, "new.txt".into()));
    assert_ne!(doc.item_paths_revision(), added);
    let json = serde_json::to_string(&doc).unwrap();
    assert!(!json.contains("item_paths_revision"));
    let loaded: SlateDoc = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded, doc);
    let relinked = doc.item_paths_revision();
    assert!(doc.remove_item(id));
    assert_ne!(doc.item_paths_revision(), relinked);
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}-{n}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn assign_enforces_mutual_exclusion_within_group() {
    let mut doc = SlateDoc::new("test");
    let size = doc.add_group("Size");
    let big = doc.add_tag(size, "Big", [200, 80, 80]).unwrap();
    let small = doc.add_tag(size, "Small", [80, 120, 200]).unwrap();
    let color = doc.add_group("Color");
    let red = doc.add_tag(color, "Red", [220, 40, 40]).unwrap();

    let item = doc.add_item(
        PathBuf::from("/tmp/photo.jpg"),
        "photo.jpg",
        1024,
        1,
        "key1",
    );

    assert!(doc.assign(item, big));
    assert_eq!(doc.item(item).unwrap().assignments.get(&size), Some(&big));
    assert!(doc.assign(item, red));
    assert_eq!(doc.item(item).unwrap().assignments.len(), 2);

    assert!(doc.assign(item, small));
    let assignments = &doc.item(item).unwrap().assignments;
    assert_eq!(assignments.get(&size), Some(&small));
    assert_eq!(assignments.get(&color), Some(&red));
    assert_ne!(assignments.get(&size), Some(&big));
}

#[test]
fn remove_group_strips_assignments() {
    let mut doc = SlateDoc::new("test");
    let g1 = doc.add_group("A");
    let g2 = doc.add_group("B");
    let t1 = doc.add_tag(g1, "a1", [1, 2, 3]).unwrap();
    let t2 = doc.add_tag(g2, "b1", [4, 5, 6]).unwrap();
    let item = doc.add_item(PathBuf::from("/x"), "x", 0, 0, "");
    doc.assign(item, t1);
    doc.assign(item, t2);

    assert!(doc.remove_group(g1));
    let assignments = &doc.item(item).unwrap().assignments;
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments.get(&g2), Some(&t2));
    assert!(!assignments.contains_key(&g1));

    assert!(doc.remove_tag(t2));
    assert!(doc.item(item).unwrap().assignments.is_empty());
}

#[test]
fn add_item_dedupes_by_absolute_path() {
    let mut doc = SlateDoc::new("test");
    let path = PathBuf::from("/unique/file.png");
    let a = doc.add_item(path.clone(), "file.png", 10, 20, "k");
    let b = doc.add_item(path, "other-name.png", 99, 99, "other");
    assert_eq!(a, b);
    assert_eq!(doc.items.len(), 1);
    assert_eq!(doc.item(a).unwrap().file_name, "file.png");
}

#[test]
fn add_item_page_allows_same_pdf_path_at_different_pages() {
    let mut doc = SlateDoc::new("test");
    let path = PathBuf::from("/docs/report.pdf");
    let p0 = doc.add_item_page(path.clone(), "report.pdf", 100, 1, "k0", 0);
    let p1 = doc.add_item_page(path.clone(), "report.pdf — page 2", 100, 1, "k1", 1);
    let p0_again = doc.add_item_page(path, "dup", 100, 1, "k0", 0);
    assert_eq!(p0, p0_again);
    assert_ne!(p0, p1);
    assert_eq!(doc.items.len(), 2);
    assert_eq!(doc.item(p1).unwrap().pdf_page, 1);
}

#[test]
fn save_load_round_trip() {
    let dir = unique_temp_dir("slate-doc-roundtrip");
    let path = dir.join(format!("workbook.{SLATE_EXTENSION}"));

    let mut doc = SlateDoc::new("Round Trip");
    let group = doc.add_group("Priority");
    let tag = doc.add_tag(group, "High", [255, 0, 0]).unwrap();
    let item = doc.add_item(PathBuf::from("/data/doc.pdf"), "doc.pdf", 500, 123, "cache");
    doc.assign(item, tag);
    doc.view.active_view = crate::view::ViewKind::Venn;
    doc.view.cam_x = 12.5;
    doc.view.zoom = 2.0;

    doc.save_to(&path).expect("save");

    let loaded = SlateDoc::load_from(&path).expect("load");
    assert_eq!(loaded.name, "Round Trip");
    assert_eq!(loaded.format_version, SlateDoc::CURRENT);
    assert_eq!(loaded.groups, doc.groups);
    assert_eq!(loaded.items, doc.items);
    assert_eq!(loaded.view.active_view, crate::view::ViewKind::Board);
    assert!((loaded.view.cam_x - 12.5).abs() < f32::EPSILON);
    assert!((loaded.view.zoom - 2.0).abs() < f32::EPSILON);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_rejects_unsupported_version() {
    let dir = unique_temp_dir("slate-doc-version");
    let path = dir.join("future.slate");
    let json = format!(
        r#"{{"format_version":{},"name":"x","groups":[],"items":[],"view":{{"active_view":"grid","cam_x":0.0,"cam_y":0.0,"zoom":1.0}},"next_group_id":1,"next_tag_id":1,"next_item_id":1}}"#,
        SlateDoc::CURRENT + 1
    );
    fs::write(&path, json).expect("write");

    let err = SlateDoc::load_from(&path).unwrap_err();
    assert_eq!(
        err,
        SlateLoadError::UnsupportedVersion {
            found: SlateDoc::CURRENT + 1
        }
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn combination_buckets_three_tag_venn() {
    let mut doc = SlateDoc::new("venn");
    let g_a = doc.add_group("A");
    let g_b = doc.add_group("B");
    let g_c = doc.add_group("C");
    let tag_a = doc.add_tag(g_a, "A", [255, 0, 0]).unwrap();
    let tag_b = doc.add_tag(g_b, "B", [0, 255, 0]).unwrap();
    let tag_c = doc.add_tag(g_c, "C", [0, 0, 255]).unwrap();

    let i_ab = doc.add_item(PathBuf::from("/ab"), "ab", 0, 0, "");
    let i_a = doc.add_item(PathBuf::from("/a"), "a", 0, 0, "");
    let i_none = doc.add_item(PathBuf::from("/none"), "none", 0, 0, "");
    let i_abc = doc.add_item(PathBuf::from("/abc"), "abc", 0, 0, "");

    doc.assign(i_ab, tag_a);
    doc.assign(i_ab, tag_b);
    doc.assign(i_a, tag_a);
    doc.assign(i_abc, tag_a);
    doc.assign(i_abc, tag_b);
    doc.assign(i_abc, tag_c);

    let active = [tag_a, tag_b, tag_c];
    let buckets = doc.combination_buckets(&active);

    assert_eq!(buckets.get(&vec![tag_a]), Some(&vec![i_a]));
    assert_eq!(buckets.get(&vec![tag_a, tag_b]), Some(&vec![i_ab]));
    assert_eq!(buckets.get(&vec![tag_a, tag_b, tag_c]), Some(&vec![i_abc]));
    assert!(!buckets.values().any(|ids| ids.contains(&i_none)));
    assert_eq!(buckets.len(), 3);
}

#[test]
fn uncategorized_items_and_clear_assignments() {
    let mut doc = SlateDoc::new("test");
    let g = doc.add_group("G");
    let t = doc.add_tag(g, "t", [0, 0, 0]).unwrap();
    let cat = doc.add_item(PathBuf::from("/cat"), "cat", 0, 0, "");
    let uncat = doc.add_item(PathBuf::from("/uncat"), "uncat", 0, 0, "");
    doc.assign(cat, t);

    assert_eq!(doc.uncategorized_items(), vec![uncat]);
    assert_eq!(doc.items_with_tag(t), vec![cat]);

    assert!(doc.clear_assignments(cat));
    assert_eq!(doc.uncategorized_items().len(), 2);
}

#[test]
fn view_kind_unknown_deserializes_as_grid_after_load() {
    let dir = unique_temp_dir("slate-doc-unknown-view");
    let path = dir.join("unknown.slate");
    let json = r#"{
        "format_version": 1,
        "name": "x",
        "groups": [],
        "items": [],
        "view": { "active_view": "future_mode", "cam_x": 0.0, "cam_y": 0.0, "zoom": 1.0 },
        "next_group_id": 1,
        "next_tag_id": 1,
        "next_item_id": 1
    }"#;
    fs::write(&path, json).expect("write");
    let doc = SlateDoc::load_from(&path).expect("load");
    assert_eq!(doc.view.active_view, crate::view::ViewKind::Board);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_drops_retired_portals_and_keeps_the_rest_of_the_board() {
    let dir = unique_temp_dir("slate-doc-retired-portals");
    let path = dir.join("mixed.slate");
    let json = r#"{
        "format_version": 2,
        "name": "mixed",
        "groups": [],
        "items": [],
        "view": { "active_view": "board", "cam_x": 0.0, "cam_y": 0.0, "zoom": 1.0 },
        "scene": { "nodes": [
            {
                "id": 1,
                "rect": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 },
                "kind": { "text": { "text": "kept", "family": "sans", "size": 16.0, "color": [0, 0, 0, 255], "align": "left" } }
            },
            {
                "id": 2,
                "rect": { "x": 0.0, "y": 0.0, "w": 960.0, "h": 540.0 },
                "kind": { "portal": { "class": "generated", "kind": "repo_lens", "title": "Repository Lens", "fill": [18, 20, 24, 255] } }
            },
            {
                "id": 3,
                "rect": { "x": 0.0, "y": 0.0, "w": 960.0, "h": 720.0 },
                "kind": { "portal": { "class": "generated", "kind": "status_board", "title": "Status Board", "fill": [14, 17, 20, 255] } }
            }
        ], "next_node_id": 4 },
        "next_group_id": 1,
        "next_tag_id": 1,
        "next_item_id": 1
    }"#;
    fs::write(&path, json).expect("write");
    let doc = SlateDoc::load_from(&path).expect("load");
    assert_eq!(doc.scene.nodes.len(), 1);
    match &doc.scene.nodes[0].kind {
        crate::scene::NodeKind::Text(text) => assert_eq!(text.text, "kept"),
        other => panic!("expected the text node, got {other:?}"),
    }
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_without_lens_root_defaults_none() {
    let dir = unique_temp_dir("slate-doc-lens-root-backcompat");
    let path = dir.join("no-lens.slate");
    let json = r#"{
        "format_version": 2,
        "name": "legacy",
        "groups": [],
        "items": [],
        "view": { "active_view": "grid", "cam_x": 0.0, "cam_y": 0.0, "zoom": 1.0 },
        "next_group_id": 1,
        "next_tag_id": 1,
        "next_item_id": 1
    }"#;
    fs::write(&path, json).expect("write");
    let doc = SlateDoc::load_from(&path).expect("load");
    assert_eq!(doc.lens_root, None);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn lens_root_round_trip() {
    let dir = unique_temp_dir("slate-doc-lens-root-roundtrip");
    let path = dir.join(format!("lens.{SLATE_EXTENSION}"));

    let mut doc = SlateDoc::new("Lens root");
    doc.lens_root = Some(PathBuf::from("/workspace"));
    doc.view.active_view = crate::view::ViewKind::Lens;

    doc.save_to(&path).expect("save");
    let loaded = SlateDoc::load_from(&path).expect("load");
    assert_eq!(loaded.lens_root, Some(PathBuf::from("/workspace")));
    assert_eq!(loaded.view.active_view, crate::view::ViewKind::Board);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn relink_updates_path_and_file_name() {
    let mut doc = SlateDoc::new("test");
    let item = doc.add_item(PathBuf::from("/old/name.txt"), "name.txt", 1, 2, "");
    assert!(doc.relink(item, PathBuf::from("/new/other.txt")));
    let updated = doc.item(item).unwrap();
    assert_eq!(updated.path, PathBuf::from("/new/other.txt"));
    assert_eq!(updated.file_name, "other.txt");
}

#[test]
fn link_status_reflects_filesystem() {
    let dir = unique_temp_dir("slate-doc-link");
    let existing = dir.join("exists.txt");
    fs::write(&existing, "hi").expect("write");
    let missing = dir.join("missing.txt");

    let item_ok = SlateItem {
        id: ItemId(1),
        path: existing,
        file_name: "exists.txt".into(),
        size: 2,
        mtime: 0,
        cache_key: String::new(),
        pdf_page: 0,
        assignments: BTreeMap::new(),
    };
    let item_missing = SlateItem {
        id: ItemId(2),
        path: missing,
        file_name: "missing.txt".into(),
        size: 0,
        mtime: 0,
        cache_key: String::new(),
        pdf_page: 0,
        assignments: BTreeMap::new(),
    };

    let doc = SlateDoc::default();
    assert_eq!(doc.link_status(&item_ok), LinkStatus::Ok);
    assert_eq!(doc.link_status(&item_missing), LinkStatus::Missing);

    let _ = fs::remove_dir_all(dir);
}
