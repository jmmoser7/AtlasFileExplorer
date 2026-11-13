use super::*;
#[test]
fn text_paste_has_one_owner_even_when_the_platform_event_arrives_later() {
    let mut down = false;
    // Native polling sees the press before egui delivers the text event.
    assert_eq!(
        paste_fallback_edge(&mut down, Some(false), true, true),
        None
    );
    assert_eq!(
        paste_fallback_edge(&mut down, Some(false), true, true),
        None
    );
    // Releasing before the queued Paste event does not create a fallback.
    assert_eq!(paste_fallback_edge(&mut down, None, true, true), None);
    // A second real text paste still belongs to egui.
    assert_eq!(paste_fallback_edge(&mut down, Some(true), true, true), None);
}

#[test]
fn bitmap_paste_fires_once_per_press_and_never_on_focus_regain() {
    let mut down = false;
    assert_eq!(
        paste_fallback_edge(&mut down, Some(false), true, false),
        Some(false)
    );
    assert_eq!(
        paste_fallback_edge(&mut down, Some(false), true, false),
        None
    );
    assert_eq!(paste_fallback_edge(&mut down, None, true, false), None);
    assert_eq!(
        paste_fallback_edge(&mut down, Some(true), true, false),
        Some(true)
    );
    assert_eq!(paste_fallback_edge(&mut down, None, false, false), None);
    assert_eq!(
        paste_fallback_edge(&mut down, Some(false), false, false),
        None
    );
    assert_eq!(
        paste_fallback_edge(&mut down, Some(false), true, false),
        None
    );
}
use slate_doc::scene::{ConnectorNode, Side, Stroke, TextNode, WireDisplay, WorldRect};

fn text_node(scene: &mut Scene, x: f32, y: f32) -> Node {
    scene.build_node(
        WorldRect::new(x, y, 100.0, 50.0),
        NodeKind::Text(TextNode {
            answers: Default::default(),
            text: "t".into(),
            style: Default::default(),
            family: Default::default(),
            size: 12.0,
            color: slate_doc::scene::Rgba::opaque(0, 0, 0),
            align: Default::default(),
            fill: None,
            stroke: Default::default(),
            agent: None,
        }),
    )
}

fn connector(scene: &mut Scene, a: NodeId, b: NodeId) -> Node {
    scene.build_node(
        WorldRect::new(0.0, 0.0, 1.0, 1.0),
        NodeKind::Connector(ConnectorNode {
            crosstalk: None,
            grasshopper: None,
            routing: None,
            binding: None,
            a: ConnectorEnd::Anchored {
                node: a,
                side: Side::Right,
                t: 0.5,
            },
            b: ConnectorEnd::Anchored {
                node: b,
                side: Side::Left,
                t: 0.5,
            },
            stroke: Stroke::none(),
            color: None,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: WireDisplay::Default,
        }),
    )
}

/// Copy 2 nodes + their connector → paste → fresh ids, same relative
/// geometry; a connector end anchored outside the set became Free.
#[test]
fn payload_includes_bridging_connector_and_degrades_outside_anchor() {
    let mut scene = Scene::default();
    let a = text_node(&mut scene, 0.0, 0.0);
    let b = text_node(&mut scene, 300.0, 0.0);
    let c = text_node(&mut scene, 600.0, 0.0);
    let (ida, idb, idc) = (a.id, b.id, c.id);
    scene.nodes.extend([a, b, c]);
    let wire_ab = connector(&mut scene, ida, idb);
    let wire_bc = connector(&mut scene, idb, idc);
    let (wab, wbc) = (wire_ab.id, wire_bc.id);
    scene.nodes.extend([wire_ab, wire_bc]);

    // Select a, b, and the b→c wire (c itself stays out).
    let selected: HashSet<NodeId> = [ida, idb, wbc].into_iter().collect();
    let payload = clipboard_payload(&scene, &selected);

    // a, b, the auto-included a→b wire, and the selected b→c wire.
    let ids: HashSet<NodeId> = payload.iter().map(|n| n.id).collect();
    assert_eq!(ids, [ida, idb, wab, wbc].into_iter().collect());

    // The b→c wire's far end degraded to Free at c's left-mid anchor.
    let bc = payload.iter().find(|n| n.id == wbc).unwrap();
    let NodeKind::Connector(cn) = &bc.kind else {
        panic!("connector expected")
    };
    assert!(matches!(cn.a, ConnectorEnd::Anchored { node, .. } if node == idb));
    match cn.b {
        ConnectorEnd::Free { point } => {
            assert_eq!(point, [600.0, 25.0]); // left side midpoint of c
        }
        _ => panic!("outside anchor must degrade to Free"),
    }

    // The a→b wire stays fully anchored.
    let ab = payload.iter().find(|n| n.id == wab).unwrap();
    let NodeKind::Connector(cn) = &ab.kind else {
        panic!("connector expected")
    };
    assert!(matches!(cn.a, ConnectorEnd::Anchored { .. }));
    assert!(matches!(cn.b, ConnectorEnd::Anchored { .. }));
}

#[test]
fn remap_gives_fresh_ids_and_preserves_relative_geometry() {
    let mut scene = Scene::default();
    let mut a = text_node(&mut scene, 10.0, 20.0);
    let b = text_node(&mut scene, 310.0, 20.0);
    let g = scene.alloc_group_key();
    a.group = Some(g);
    let (ida, idb) = (a.id, b.id);
    scene.nodes.extend([a, b]);
    let wire = connector(&mut scene, ida, idb);
    let wid = wire.id;
    scene.nodes.push(wire);

    let selected: HashSet<NodeId> = [ida, idb, wid].into_iter().collect();
    let payload = clipboard_payload(&scene, &selected);

    let mut next = 1000u64;
    let mut next_group = 5000u64;
    let out = remap_for_paste(
        &payload,
        || {
            next += 1;
            NodeId(next)
        },
        || {
            next_group += 1;
            GroupKey(next_group)
        },
        24.0,
        24.0,
    );

    // Every id is fresh and distinct.
    let old: HashSet<NodeId> = payload.iter().map(|n| n.id).collect();
    let new: HashSet<NodeId> = out.iter().map(|n| n.id).collect();
    assert_eq!(new.len(), out.len());
    assert!(old.is_disjoint(&new));

    // Relative geometry survives the offset.
    let ra = out[0].rect;
    let rb = out[1].rect;
    assert_eq!(rb.x - ra.x, 300.0);
    assert_eq!(ra.x, 34.0);
    assert_eq!(ra.y, 44.0);

    // Group keys are freshly allocated.
    assert_eq!(out[0].group, Some(GroupKey(5001)));

    // The connector follows the remapped ids.
    let NodeKind::Connector(cn) = &out[2].kind else {
        panic!("connector expected")
    };
    assert!(matches!(cn.a, ConnectorEnd::Anchored { node, .. } if node == out[0].id));
    assert!(matches!(cn.b, ConnectorEnd::Anchored { node, .. } if node == out[1].id));
}

#[test]
fn remap_offsets_free_points_and_shares_group_keys() {
    let mut scene = Scene::default();
    let mut a = text_node(&mut scene, 0.0, 0.0);
    let mut b = text_node(&mut scene, 200.0, 0.0);
    let g = scene.alloc_group_key();
    a.group = Some(g);
    b.group = Some(g);
    let free_wire = scene.build_node(
        WorldRect::new(0.0, 0.0, 1.0, 1.0),
        NodeKind::Connector(ConnectorNode {
            crosstalk: None,
            grasshopper: None,
            routing: None,
            binding: None,
            a: ConnectorEnd::Free { point: [5.0, 6.0] },
            b: ConnectorEnd::Free { point: [7.0, 8.0] },
            stroke: Stroke::none(),
            color: None,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: WireDisplay::Default,
        }),
    );
    let payload = vec![a, b, free_wire];

    let mut next = 0u64;
    let mut groups = 0u64;
    let out = remap_for_paste(
        &payload,
        || {
            next += 1;
            NodeId(next)
        },
        || {
            groups += 1;
            GroupKey(100 + groups)
        },
        10.0,
        -10.0,
    );

    // One shared source key → one shared fresh key.
    assert_eq!(out[0].group, out[1].group);
    assert_eq!(out[0].group, Some(GroupKey(101)));
    assert_eq!(groups, 1);

    let NodeKind::Connector(cn) = &out[2].kind else {
        panic!("connector expected")
    };
    assert_eq!(
        (cn.a, cn.b),
        (
            ConnectorEnd::Free {
                point: [15.0, -4.0]
            },
            ConnectorEnd::Free {
                point: [17.0, -2.0]
            }
        )
    );
}

fn dib_header(width: i32, height: i32, bit_count: u16) -> Vec<u8> {
    let mut h = vec![0u8; 40];
    h[0..4].copy_from_slice(&40u32.to_le_bytes());
    h[4..8].copy_from_slice(&width.to_le_bytes());
    h[8..12].copy_from_slice(&height.to_le_bytes());
    h[12..14].copy_from_slice(&1u16.to_le_bytes());
    h[14..16].copy_from_slice(&bit_count.to_le_bytes());
    h
}

fn png_pixel(png: &[u8]) -> [u8; 4] {
    image::load_from_memory(png)
        .unwrap()
        .to_rgba8()
        .get_pixel(0, 0)
        .0
}

#[test]
fn dib_32_zero_alpha_pastes_as_opaque_color() {
    let mut dib = dib_header(1, 1, 32);
    dib.extend_from_slice(&[0, 0, 255, 0]); // B, G, R, unused alpha → red
    let png = atlas_core::clipboard_image::decode_clipboard_bitmap(&dib).unwrap();
    assert_eq!(png_pixel(&png), [255, 0, 0, 255]);
}

#[test]
fn dib_24_pads_rows_and_keeps_color() {
    let mut dib = dib_header(1, 1, 24);
    dib.extend_from_slice(&[10, 20, 30, 0]); // BGR + pad
    let png = atlas_core::clipboard_image::decode_clipboard_bitmap(&dib).unwrap();
    assert_eq!(png_pixel(&png), [30, 20, 10, 255]);
}

#[test]
fn top_down_dib_keeps_the_first_row_on_top() {
    let mut dib = dib_header(1, -2, 32);
    dib.extend_from_slice(&[0, 0, 255, 0]); // top: red
    dib.extend_from_slice(&[0, 255, 0, 0]); // bottom: green
    let png = atlas_core::clipboard_image::decode_clipboard_bitmap(&dib).unwrap();
    let img = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert_eq!(img.get_pixel(0, 1).0, [0, 255, 0, 255]);
}

#[test]
fn bmp_file_header_points_at_the_pixels() {
    let mut dib = dib_header(1, 1, 32);
    dib.extend_from_slice(&[255, 0, 0, 0]); // blue channel in BGRA, zero alpha
    let mut bmp = vec![0u8; 14];
    bmp[0..2].copy_from_slice(b"BM");
    let off = (14 + dib.len() - 4) as u32;
    bmp[10..14].copy_from_slice(&off.to_le_bytes());
    bmp.extend_from_slice(&dib);
    let png = atlas_core::clipboard_image::decode_clipboard_bitmap(&bmp).unwrap();
    assert_eq!(png_pixel(&png), [0, 0, 255, 255]);
}

#[test]
fn dibv5_round_trips_through_the_paste_decoder() {
    let img = image::RgbaImage::from_fn(3, 2, |x, y| {
        image::Rgba([x as u8 * 80, y as u8 * 120, 200, 255 - x as u8 * 100])
    });
    let dib = encode_dibv5(&img);
    assert_eq!(u32::from_le_bytes(dib[0..4].try_into().unwrap()), 124);
    assert_eq!(i32::from_le_bytes(dib[4..8].try_into().unwrap()), 3);
    // Bottom-up rows: Word refuses a negative height.
    assert_eq!(i32::from_le_bytes(dib[8..12].try_into().unwrap()), 2);
    assert_eq!(dib.len(), 124 + 3 * 2 * 4);
    let png = atlas_core::clipboard_image::decode_clipboard_bitmap(&dib).unwrap();
    let back = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(back, img);
}

#[test]
fn the_text_fallback_is_never_the_node_json() {
    let mut scene = Scene::default();
    let words = text_node(&mut scene, 0.0, 0.0);
    let rect = scene.build_node(
        WorldRect::new(0.0, 0.0, 10.0, 10.0),
        NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Rect,
            fill: None,
            stroke: Stroke::none(),
            corner: Default::default(),
            sides: 6,
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let no_pictures = |_: &Node| false;
    assert_eq!(
        clipboard_text_fallback(&[words.clone(), rect.clone()], no_pictures).as_deref(),
        Some("t")
    );
    let shapes = clipboard_text_fallback(&[rect.clone(), rect.clone()], no_pictures).unwrap();
    assert_eq!(shapes, "Slate selection: 2 shapes");
    let json = serde_json::to_string(std::slice::from_ref(&rect)).unwrap();
    assert_ne!(shapes, json);
    assert!(!shapes.trim_start().starts_with('['));
    let all_pictures = |_: &Node| true;
    assert_eq!(clipboard_text_fallback(&[rect], all_pictures), None);
}

#[test]
fn clipboard_text_path_accepts_a_file_url_and_rejects_a_page() {
    let (input, expected) = if cfg!(windows) {
        ("file:///C:/shots/a%20b.png", "C:/shots/a b.png")
    } else {
        ("file:///tmp/a%20b.png", "/tmp/a b.png")
    };
    assert_eq!(
        clipboard_text_path(input).as_deref(),
        Some(std::path::Path::new(expected))
    );
    assert!(clipboard_text_path("https://example.com/a.png").is_none());
    assert!(clipboard_text_path("[{\"id\":1}]").is_none());
}
