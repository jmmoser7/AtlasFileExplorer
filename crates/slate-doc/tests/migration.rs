//! One verbatim workbook per historical on-disk format version, and the
//! harness that proves each one still opens.
//!
//! Convention: a card that bumps `SlateDoc::CURRENT` adds a fixture for the
//! version it supersedes plus a `vN_fixture_upgrades_to_current` test; it never
//! edits an existing fixture. The committed JSON is the source of truth —
//! [`generate_fixtures`] is ignored and exists only to make the next fixture
//! cheap to author.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use slate_doc::scene::*;
use slate_doc::{GroupId, ItemId, SlateDoc, SlateLoadError, TagId, ViewKind};

const V1_FIXTURE: &str = "v1-tags-items.slate.json";
const V2_FIXTURE: &str = "v2-board.slate.json";
/// Fixtures the loader must *refuse* live here, so the round-trip harness —
/// which walks the top level of the fixtures directory — never sees them.
const UNSUPPORTED_DIR: &str = "unsupported";
const V99_FIXTURE: &str = "v99-future.slate.json";

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Every historical fixture, sorted by name. All of these must load.
fn loadable_fixtures() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .expect("read fixtures dir")
        .map(|entry| entry.expect("fixture dir entry"))
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".slate.json"))
        .collect();
    names.sort();
    names
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}-{n}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Loads `fixture`, asserts it upgrades to [`SlateDoc::CURRENT`], saves it,
/// reloads it, and asserts the two in-memory documents are equal. Returns the
/// loaded document so callers can assert on its content.
fn round_trip(fixture: &str) -> SlateDoc {
    let path = fixture_dir().join(fixture);
    let doc = SlateDoc::load_from(&path).unwrap_or_else(|err| panic!("load {fixture}: {err}"));
    assert_eq!(
        doc.format_version,
        SlateDoc::CURRENT,
        "{fixture} upgrades on load"
    );

    let dir = unique_temp_dir("slate-doc-migration");
    let saved = dir.join(fixture.trim_end_matches(".json"));
    doc.save_to(&saved)
        .unwrap_or_else(|err| panic!("save {fixture}: {err}"));
    let reloaded =
        SlateDoc::load_from(&saved).unwrap_or_else(|err| panic!("reload {fixture}: {err}"));
    assert_eq!(reloaded, doc, "{fixture} survives save and reload");

    let _ = std::fs::remove_dir_all(dir);
    doc
}

#[test]
fn v1_fixture_upgrades_to_current() {
    let doc = round_trip(V1_FIXTURE);

    assert_eq!(doc.name, "Field Survey");
    // Pre-board: the scene defaults empty and there is no Lens root.
    assert!(doc.scene.is_empty());
    assert_eq!(doc.lens_root, None);

    assert_eq!(doc.groups.len(), 2);
    let discipline = &doc.groups[0];
    assert_eq!(discipline.id, GroupId(1));
    assert_eq!(discipline.name, "Discipline");
    assert_eq!(discipline.tags.len(), 2);
    assert_eq!(discipline.tags[0].id, TagId(1));
    assert_eq!(discipline.tags[0].name, "Architecture");
    assert_eq!(discipline.tags[0].color, [90, 140, 220]);
    assert_eq!(discipline.tags[1].id, TagId(2));
    assert_eq!(discipline.tags[1].name, "Structure");
    assert_eq!(discipline.tags[1].color, [220, 150, 70]);

    let status = &doc.groups[1];
    assert_eq!(status.id, GroupId(2));
    assert_eq!(status.name, "Status");
    assert_eq!(status.tags.len(), 2);
    assert_eq!(status.tags[0].id, TagId(3));
    assert_eq!(status.tags[0].name, "Draft");
    assert_eq!(status.tags[0].color, [150, 150, 150]);
    assert_eq!(status.tags[1].id, TagId(4));
    assert_eq!(status.tags[1].name, "Issued");
    assert_eq!(status.tags[1].color, [80, 180, 120]);

    assert_eq!(doc.items.len(), 3);
    let plan = &doc.items[0];
    assert_eq!(plan.id, ItemId(1));
    assert_eq!(plan.path, PathBuf::from("/projects/harbour/site-plan.pdf"));
    assert_eq!(plan.file_name, "site-plan.pdf");
    assert_eq!(plan.size, 482_133);
    assert_eq!(plan.mtime, 1_752_000_000);
    assert_eq!(plan.cache_key, "site-plan-1");
    // v1 predates `pdf_page`; the field is absent and defaults to the poster page.
    assert_eq!(plan.pdf_page, 0);
    assert_eq!(
        plan.assignments,
        BTreeMap::from([(GroupId(1), TagId(1)), (GroupId(2), TagId(4))])
    );

    let elevation = &doc.items[1];
    assert_eq!(elevation.id, ItemId(2));
    assert_eq!(
        elevation.path,
        PathBuf::from("/projects/harbour/elevation-north.png")
    );
    assert_eq!(elevation.file_name, "elevation-north.png");
    assert_eq!(elevation.size, 2_310_442);
    assert_eq!(elevation.mtime, 1_752_010_000);
    assert_eq!(elevation.cache_key, "elevation-north-1");
    assert_eq!(elevation.pdf_page, 0);
    assert_eq!(
        elevation.assignments,
        BTreeMap::from([(GroupId(1), TagId(1)), (GroupId(2), TagId(3))])
    );

    let schedule = &doc.items[2];
    assert_eq!(schedule.id, ItemId(3));
    assert_eq!(
        schedule.path,
        PathBuf::from("/projects/harbour/beam-schedule.xlsx")
    );
    assert_eq!(schedule.file_name, "beam-schedule.xlsx");
    assert_eq!(schedule.size, 64_120);
    assert_eq!(schedule.mtime, 1_752_020_000);
    assert_eq!(schedule.cache_key, "beam-schedule-1");
    assert_eq!(schedule.pdf_page, 0);
    assert_eq!(
        schedule.assignments,
        BTreeMap::from([(GroupId(1), TagId(2))])
    );

    assert_eq!(doc.view.active_view, ViewKind::Venn);
    assert_eq!(doc.view.cam_x, -120.0);
    assert_eq!(doc.view.cam_y, 48.0);
    assert_eq!(doc.view.zoom, 1.5);
}

#[test]
fn v2_fixture_upgrades_to_current() {
    let doc = round_trip(V2_FIXTURE);

    assert_eq!(doc.name, "Harbour Deck");
    assert_eq!(doc.lens_root, Some(PathBuf::from("/repos/atlas")));

    assert_eq!(doc.groups.len(), 1);
    let chapter = &doc.groups[0];
    assert_eq!(chapter.id, GroupId(1));
    assert_eq!(chapter.name, "Chapter");
    assert_eq!(chapter.tags.len(), 2);
    assert_eq!(chapter.tags[0].id, TagId(1));
    assert_eq!(chapter.tags[0].name, "Intro");
    assert_eq!(chapter.tags[0].color, [90, 140, 220]);
    assert_eq!(chapter.tags[1].id, TagId(2));
    assert_eq!(chapter.tags[1].name, "Detail");
    assert_eq!(chapter.tags[1].color, [220, 150, 70]);

    assert_eq!(doc.items.len(), 2);
    let hero = &doc.items[0];
    assert_eq!(hero.id, ItemId(1));
    assert_eq!(hero.path, PathBuf::from("/decks/harbour/hero.jpg"));
    assert_eq!(hero.file_name, "hero.jpg");
    assert_eq!(hero.size, 1_204_880);
    assert_eq!(hero.mtime, 1_752_100_000);
    assert_eq!(hero.cache_key, "hero-1");
    assert_eq!(hero.pdf_page, 0);
    assert!(hero.assignments.is_empty());

    let report = &doc.items[1];
    assert_eq!(report.id, ItemId(2));
    assert_eq!(report.path, PathBuf::from("/decks/harbour/report.pdf"));
    assert_eq!(report.file_name, "report.pdf — page 4");
    assert_eq!(report.size, 982_004);
    assert_eq!(report.mtime, 1_752_200_000);
    assert_eq!(report.cache_key, "report-3");
    assert_eq!(report.pdf_page, 3);
    assert_eq!(report.assignments, BTreeMap::from([(GroupId(1), TagId(2))]));

    assert_eq!(doc.view.active_view, ViewKind::Board);
    assert_eq!(doc.view.cam_x, 240.0);
    assert_eq!(doc.view.cam_y, -80.0);
    assert_eq!(doc.view.zoom, 0.75);

    // Node-level flags every later version must keep carrying.
    let image = doc.scene.node(NodeId(2)).expect("image node");
    assert_eq!(image.rect, WorldRect::new(80.0, 120.0, 480.0, 320.0));
    assert_eq!(image.opacity, 0.8);
    assert!(image.locked);
    assert!(!image.hidden);
    let text = doc.scene.node(NodeId(4)).expect("text node");
    assert_eq!(text.rect, WorldRect::new(640.0, 460.0, 420.0, 120.0));
    assert_eq!(text.rotation_deg, -6.0);
    assert!(text.hidden);
    assert!(!text.locked);
}

#[test]
fn every_fixture_round_trips() {
    let names = loadable_fixtures();
    assert!(
        names.contains(&V1_FIXTURE.to_string()) && names.contains(&V2_FIXTURE.to_string()),
        "expected at least the v1 and v2 fixtures, found {names:?}"
    );
    for name in names {
        round_trip(&name);
    }
}

#[test]
fn v2_fixture_preserves_scene_shape() {
    let doc = round_trip(V2_FIXTURE);
    let scene = &doc.scene;

    assert_eq!(scene.nodes.len(), 5);
    let kinds: Vec<&str> = scene.nodes.iter().map(|n| n.kind.kind_name()).collect();
    assert_eq!(kinds, ["frame", "image", "shape", "text", "connector"]);
    let ids: Vec<NodeId> = scene.nodes.iter().map(|n| n.id).collect();
    assert_eq!(
        ids,
        vec![NodeId(1), NodeId(2), NodeId(3), NodeId(4), NodeId(5)]
    );

    let frames = scene.frames_in_order();
    assert_eq!(frames.len(), 1);
    let NodeKind::Frame(frame) = &frames[0].kind else {
        panic!("frames_in_order returned a non-frame");
    };
    assert_eq!(frame.title, "Cover");
    assert_eq!(frame.order, 2);
    assert_eq!(frame.fill, Rgba::opaque(246, 246, 244));
    assert_eq!(frame.assignments, BTreeMap::from([(GroupId(1), TagId(1))]));
    assert_eq!(scene.next_frame_order(), 3);

    // Every content node sits inside the single frame — one slide, four members.
    assert_eq!(
        scene.members_of(NodeId(1)),
        vec![NodeId(2), NodeId(3), NodeId(4), NodeId(5)]
    );

    let NodeKind::Shape(shape) = &scene.nodes[2].kind else {
        panic!("node 3 is not a shape");
    };
    assert_eq!(shape.shape, ShapeKind::Path);
    assert_eq!(shape.fill, None);
    let path = shape.path.as_ref().expect("shape path");
    assert_eq!(path.start, [0.0, 0.0]);
    assert_eq!(
        path.segs,
        vec![
            PathSeg::Line { to: [1.0, 0.4] },
            PathSeg::Cubic {
                c1: [0.75, 0.7],
                c2: [0.25, 0.9],
                to: [0.0, 1.0],
            },
        ]
    );
    assert!(!path.closed);

    // One group, holding the shape and the text.
    let groups: Vec<Option<GroupKey>> = scene.nodes.iter().map(|n| n.group).collect();
    assert_eq!(
        groups,
        vec![None, None, Some(GroupKey(1)), Some(GroupKey(1)), None]
    );

    let NodeKind::Connector(connector) = &scene.nodes[4].kind else {
        panic!("node 5 is not a connector");
    };
    assert_eq!(
        connector.a,
        ConnectorEnd::Anchored {
            node: NodeId(2),
            side: Side::Right,
            t: 0.5,
        }
    );
    assert_eq!(
        connector.b,
        ConnectorEnd::Anchored {
            node: NodeId(3),
            side: Side::Left,
            t: 0.25,
        }
    );
    assert!(!connector.arrow_a);
    assert!(connector.arrow_b);
    assert_eq!(connector.label.as_deref(), Some("informs"));
    assert_eq!(connector.display, WireDisplay::Faint);

    // The stored rect still matches the curve derived from the live endpoints.
    let derived = connector_aabb(connector, |id| scene.node(id).map(|n| n.rect))
        .expect("connector endpoints resolve");
    let stored = scene.nodes[4].rect;
    for (a, b) in [
        (derived.x, stored.x),
        (derived.y, stored.y),
        (derived.w, stored.w),
        (derived.h, stored.h),
    ] {
        assert!(
            (a - b).abs() < 0.01,
            "connector rect {stored:?} vs {derived:?}"
        );
    }
}

#[test]
fn future_version_is_rejected() {
    let path = fixture_dir().join(UNSUPPORTED_DIR).join(V99_FIXTURE);
    let err = SlateDoc::load_from(&path).expect_err("a v99 workbook must not load");
    assert_eq!(err, SlateLoadError::UnsupportedVersion { found: 99 });
}

// ---------- fixture generation ----------

/// Rewrites every fixture from the public API. Ignored: the committed JSON is
/// the truth, and regenerating it silently would defeat the point of a
/// migration fixture. Run it deliberately when a new version needs a fixture,
/// then hand-check the diff:
///
/// ```powershell
/// cargo test -p slate-doc --test migration -- --ignored generate_fixtures
/// ```
#[test]
#[ignore = "overwrites the committed fixtures; run deliberately and hand-check the diff"]
fn generate_fixtures() {
    let dir = fixture_dir();
    std::fs::create_dir_all(dir.join(UNSUPPORTED_DIR)).expect("create fixtures dir");

    // v1 predates the board, so today's writer cannot emit it: the document is
    // authored through the API and then stripped of every field v1 never had.
    let mut v1 = serde_json::to_value(v1_document()).expect("serialize v1");
    let obj = v1.as_object_mut().expect("v1 object");
    obj.insert("format_version".into(), 1.into());
    obj.remove("scene");
    obj.remove("lens_root");
    for item in obj["items"].as_array_mut().expect("v1 items") {
        item.as_object_mut().expect("v1 item").remove("pdf_page");
    }
    write_json(&dir.join(V1_FIXTURE), &v1);

    // v2 is the current format, so the writer under test is the generator.
    v2_document()
        .save_to(&dir.join(V2_FIXTURE))
        .expect("write v2 fixture");

    // A workbook from a version this build has never heard of.
    let mut future = serde_json::to_value(SlateDoc::new("From The Future")).expect("serialize v99");
    future["format_version"] = 99.into();
    write_json(&dir.join(UNSUPPORTED_DIR).join(V99_FIXTURE), &future);
}

fn write_json(path: &Path, value: &serde_json::Value) {
    let json = serde_json::to_string_pretty(value).expect("serialize fixture");
    std::fs::write(path, json).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
}

/// Pre-board workbook: two facets, three tagged items, a Venn camera.
fn v1_document() -> SlateDoc {
    let mut doc = SlateDoc::new("Field Survey");
    let discipline = doc.add_group("Discipline");
    let architecture = doc
        .add_tag(discipline, "Architecture", [90, 140, 220])
        .expect("Architecture");
    let structure = doc
        .add_tag(discipline, "Structure", [220, 150, 70])
        .expect("Structure");
    let status = doc.add_group("Status");
    let draft = doc
        .add_tag(status, "Draft", [150, 150, 150])
        .expect("Draft");
    let issued = doc
        .add_tag(status, "Issued", [80, 180, 120])
        .expect("Issued");

    let plan = doc.add_item(
        PathBuf::from("/projects/harbour/site-plan.pdf"),
        "site-plan.pdf",
        482_133,
        1_752_000_000,
        "site-plan-1",
    );
    let elevation = doc.add_item(
        PathBuf::from("/projects/harbour/elevation-north.png"),
        "elevation-north.png",
        2_310_442,
        1_752_010_000,
        "elevation-north-1",
    );
    let schedule = doc.add_item(
        PathBuf::from("/projects/harbour/beam-schedule.xlsx"),
        "beam-schedule.xlsx",
        64_120,
        1_752_020_000,
        "beam-schedule-1",
    );
    doc.assign(plan, architecture);
    doc.assign(plan, issued);
    doc.assign(elevation, architecture);
    doc.assign(elevation, draft);
    doc.assign(schedule, structure);

    doc.view.active_view = ViewKind::Venn;
    doc.view.cam_x = -120.0;
    doc.view.cam_y = 48.0;
    doc.view.zoom = 1.5;
    doc
}

/// Board workbook: one ordered frame, one locked image, a path shape and a
/// hidden text sharing a group, and a connector anchored at both ends.
fn v2_document() -> SlateDoc {
    let mut doc = SlateDoc::new("Harbour Deck");
    let chapter = doc.add_group("Chapter");
    let intro = doc
        .add_tag(chapter, "Intro", [90, 140, 220])
        .expect("Intro");
    let detail = doc
        .add_tag(chapter, "Detail", [220, 150, 70])
        .expect("Detail");

    let hero = doc.add_item(
        PathBuf::from("/decks/harbour/hero.jpg"),
        "hero.jpg",
        1_204_880,
        1_752_100_000,
        "hero-1",
    );
    let report = doc.add_item_page(
        PathBuf::from("/decks/harbour/report.pdf"),
        "report.pdf — page 4",
        982_004,
        1_752_200_000,
        "report-3",
        3,
    );
    doc.assign(report, detail);
    doc.lens_root = Some(PathBuf::from("/repos/atlas"));
    doc.view.active_view = ViewKind::Board;
    doc.view.cam_x = 240.0;
    doc.view.cam_y = -80.0;
    doc.view.zoom = 0.75;

    let mut journal = SceneJournal::default();
    let scene = &mut doc.scene;
    let group = scene.alloc_group_key();

    let frame = scene.build_node(
        WorldRect::new(0.0, 0.0, 1280.0, 720.0),
        NodeKind::Frame(FrameNode {
            title: "Cover".into(),
            order: 2,
            fill: Rgba::opaque(246, 246, 244),
            assignments: BTreeMap::from([(chapter, intro)]),
        }),
    );

    let mut image = scene.build_node(
        WorldRect::new(80.0, 120.0, 480.0, 320.0),
        NodeKind::Image(ImageNode {
            corner: Corner::Rounded { radius: 12.0 },
            ..ImageNode::new(hero)
        }),
    );
    image.opacity = 0.8;
    image.locked = true;
    let image_id = image.id;

    let mut shape = scene.build_node(
        WorldRect::new(640.0, 120.0, 420.0, 300.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: Stroke {
                width: 3.0,
                color: Rgba::opaque(30, 30, 30),
                cap: StrokeCap::Round,
                join: StrokeJoin::Round,
                ..Default::default()
            },
            corner: Corner::Square,
            flip: false,
            path: Some(PathData {
                start: [0.0, 0.0],
                segs: vec![
                    PathSeg::Line { to: [1.0, 0.4] },
                    PathSeg::Cubic {
                        c1: [0.75, 0.7],
                        c2: [0.25, 0.9],
                        to: [0.0, 1.0],
                    },
                ],
                closed: false,
            }),
        }),
    );
    shape.group = Some(group);
    let shape_id = shape.id;

    let mut text = scene.build_node(
        WorldRect::new(640.0, 460.0, 420.0, 120.0),
        NodeKind::Text(TextNode {
            text: "Harbour frontage — draft".into(),
            family: FontChoice::Serif,
            size: 34.0,
            color: Rgba::opaque(20, 20, 40),
            align: TextAlign::Center,
            fill: Some(Rgba([255, 244, 200, 255])),
        }),
    );
    text.rotation_deg = -6.0;
    text.hidden = true;
    text.group = Some(group);

    for node in [frame, image, shape, text] {
        let index = scene.nodes.len();
        assert!(journal.commit(scene, vec![SceneCmd::Add { index, node }]));
    }

    let connector = ConnectorNode {
        a: ConnectorEnd::Anchored {
            node: image_id,
            side: Side::Right,
            t: 0.5,
        },
        b: ConnectorEnd::Anchored {
            node: shape_id,
            side: Side::Left,
            t: 0.25,
        },
        stroke: Stroke {
            width: 2.0,
            color: Rgba::opaque(120, 120, 130),
            dash: Dash::Dashed,
            ..Default::default()
        },
        arrow_a: false,
        arrow_b: true,
        label: Some("informs".into()),
        display: WireDisplay::Faint,
    };
    // The board keeps a connector's rect equal to its derived curve bounds.
    let rect = connector_aabb(&connector, |id| scene.node(id).map(|n| n.rect))
        .expect("connector endpoints resolve");
    let node = scene.build_node(rect, NodeKind::Connector(connector));
    let index = scene.nodes.len();
    assert!(journal.commit(scene, vec![SceneCmd::Add { index, node }]));

    doc
}
