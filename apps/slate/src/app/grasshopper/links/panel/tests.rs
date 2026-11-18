use super::*;
use crate::app::tests::Harness;
use serde_json::{json, Value};

fn leaf(kind: &str, value: Value) -> Value {
    json!({"kind":kind,"value":value})
}
fn packet(path: &[i32], items: Vec<Value>) -> Value {
    json!({"branches":[{"path":path,"items":items}]})
}
fn printed(tree: &Value) -> String {
    let mut s = PanelText::default();
    let _ = format_packet(&mut s, tree);
    s.0
}

#[test]
fn projection_preserves_trees_nulls_unicode_and_readable_geometry_with_a_size_bound() {
    assert_eq!(printed(&json!({"branches":[]})), "<empty tree>");
    assert_eq!(printed(&packet(&[0], vec![])), "<empty branch>");
    assert_eq!(printed(&packet(&[0], vec![leaf("text", json!(""))])), "");
    assert_eq!(
        printed(&packet(&[0], vec![leaf("null", Value::Null)])),
        "<null>"
    );
    let tree = json!({"branches":[
        {"path":[0,1],"items":[leaf("number",json!(6.0)),leaf("text",json!("蓝\n绿"))]},
        {"path":[2,4],"items":[leaf("point",json!([1,-2.5,3])),leaf("color",json!([1,2,3,255])),leaf("boolean",json!(true))]}]});
    assert_eq!(
        printed(&tree),
        "{0;1}\n0  6\n1  蓝\n绿\n{2;4}\n0  (1, -2.5, 3)\n1  RGBA(1, 2, 3, 255)\n2  True"
    );
    let geo =
        json!({"kind":"geometry","value":"never display archive bytes","display":"Closed Brep"});
    assert_eq!(printed(&packet(&[0], vec![geo])), "Closed Brep");
    assert_eq!(
        printed(&packet(
            &[0],
            vec![leaf("geometry", json!("legacy archive"))]
        )),
        "Geometry"
    );
    assert_eq!(
        printed(&packet(&[0], vec![leaf("display", json!("XY Plane"))])),
        "XY Plane"
    );
    let clipped = printed(&packet(
        &[0],
        vec![leaf("text", json!("🪨".repeat(MAX_PANEL_BYTES)))],
    ));
    assert!(clipped.len() <= MAX_PANEL_BYTES && clipped.ends_with("… (panel display limit)"));
}

#[test]
fn linked_text_panels_merge_outputs_restore_authored_content_and_repopulate_after_reopen() {
    let mut h = Harness::new("gh-panel-output");
    h.app.leave_home();
    let mut place = |name: &str, bytes: &str| {
        let path = h.base.join(name);
        std::fs::write(&path, bytes).unwrap();
        let item = h.app.add_paths(&[path])[0];
        h.app.place_items_on_board(&[item], egui::Pos2::ZERO);
        *h.app.board_sel.iter().next().unwrap()
    };
    let source = place("source.ghx", "fixture");
    let text = place("original.TXT", "Original disk text");
    let code = place("script.py", "print('source')");
    let raw = serde_json::from_value(
        json!({"text":"Handwritten text","size":18.,"color":[12,23,34,255]}),
    )
    .unwrap();
    let node = h
        .app
        .doc_mut()
        .scene
        .build_node(WorldRect::new(10., 20., 330., 180.), NodeKind::Text(raw));
    let note = node.id;
    h.app.add_nodes(vec![node]);
    let key = (h.app.tab().id, source);
    h.app.request_grasshopper(source, &h.ctx);
    let source_ports: Vec<_> = ["Width", "Profile"]
        .iter()
        .enumerate()
        .map(|(i, name)| GrasshopperPort {
            id: format!("a4d27126-0ebf-444f-b717-69fdbc0f000{i}")
                .parse()
                .unwrap(),
            name: (*name).into(),
            direction: GrasshopperPortDirection::Output,
            access: GrasshopperPortAccess::Tree,
            kind: GrasshopperPortType::Number,
        })
        .collect();
    let width = source_ports[0].id.clone();
    let profile = source_ports[1].id.clone();
    h.app.grasshopper.views.get_mut(&key).unwrap().ports = source_ports.clone();
    assert!(h.app.grasshopper_component_ports(text).is_empty());
    assert!(h.app.grasshopper_component_ports(note).is_empty());
    assert!(h.app.grasshopper_ports(text).contains(input_port()));
    assert!(!h.app.is_grasshopper_text_panel(code));
    let mut missing = source_ports[0].clone();
    missing.id = "00000000-0000-4000-8000-000000000000".parse().unwrap();
    assert!(!h
        .app
        .can_connect_grasshopper_output_to_panel(source, &missing, note));
    assert!(h.app.grasshopper_panel_text(note).is_none());
    let connect = |h: &mut Harness, port: &GrasshopperPortId, target| {
        h.app
            .connect_grasshopper_ports(source, port, target, &input_port().id, 0.5, 0.5)
    };
    assert!(connect(&mut h, &width, code).is_err());
    connect(&mut h, &width, text).unwrap();
    let first = connect(&mut h, &width, note).unwrap();
    assert!(connect(&mut h, &width, note).is_err());
    let second = connect(&mut h, &profile, note).unwrap();
    let update = |h: &mut Harness, width_value| {
        h.app.request_grasshopper_dependencies(&h.ctx);
        let v = h.app.grasshopper.views.get_mut(&key).unwrap();
        v.ports = source_ports.clone();
        v.generation += 1;
        v.outputs = Some(
            [
                (
                    width.clone(),
                    packet(&[0], vec![leaf("number", json!(width_value))]),
                ),
                (
                    profile.clone(),
                    packet(
                        &[0],
                        vec![leaf("integer", json!(6)), leaf("integer", json!(9))],
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        );
        h.app.update_grasshopper_links();
    };
    update(&mut h, 6);
    assert_eq!(h.app.grasshopper_panel_text(text).as_deref(), Some("6"));
    let first_text = h.app.grasshopper_panel_text(note).unwrap();
    assert_eq!(&*first_text, "Width\n6\n\nProfile\n0  6\n1  9");
    h.app.update_grasshopper_links();
    assert!(Arc::ptr_eq(
        &first_text,
        &h.app.grasshopper_panel_text(note).unwrap()
    ));
    update(&mut h, 12);
    assert_eq!(h.app.grasshopper_panel_text(text).as_deref(), Some("12"));
    let path = h.base.join("roundtrip.slate");
    h.app.doc().save_to(&path).unwrap();
    h.app.delete_board_nodes(&[first, second]);
    h.app.request_grasshopper_dependencies(&h.ctx);
    h.app.update_grasshopper_links();
    assert!(h.app.grasshopper_panel_text(note).is_none());
    assert!(
        matches!(&h.app.doc().scene.node(note).unwrap().kind,NodeKind::Text(t) if t.text=="Handwritten text")
    );
    h.app.board_undo();
    update(&mut h, 30);
    assert!(h
        .app
        .grasshopper_panel_text(note)
        .unwrap()
        .starts_with("Width\n30"));
    *h.app.doc_mut() = slate_doc::SlateDoc::load_from(&path).unwrap();
    h.app.grasshopper = Default::default();
    h.app.request_grasshopper_dependencies(&h.ctx);
    assert!(!h.app.grasshopper.views.contains_key(&key));
    h.app.request_visible_grasshopper_panel(text, &h.ctx);
    assert!(h.app.grasshopper.views.contains_key(&key));
    update(&mut h, 21);
    assert_eq!(h.app.grasshopper_panel_text(text).as_deref(), Some("21"));
    assert_eq!(
        std::fs::read_to_string(h.base.join("original.TXT")).unwrap(),
        "Original disk text"
    );
    assert!(
        matches!(&h.app.doc().scene.node(text).unwrap().kind,NodeKind::Image(i) if i.text_override.is_none())
    );
    let ordinary_kind = h.app.doc().scene.node(note).unwrap().kind.clone();
    h.app.new_tab();
    let ordinary = h
        .app
        .doc_mut()
        .scene
        .build_node(WorldRect::new(0., 0., 120., 40.), ordinary_kind);
    let ordinary_id = ordinary.id;
    h.app.add_nodes(vec![ordinary]);
    assert!(h.app.grasshopper_ports(ordinary_id).is_empty());
}
