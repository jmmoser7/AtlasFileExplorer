//! Mixed-canvas regressions for the native image output and Grasshopper file binding.
use super::*;
use crate::app::board_wire::{WireDrag, WireMode};
use crate::app::tests::painted;
use egui::Pos2;
use slate_doc::agent_inputs::OUTPUT_T;
use slate_doc::wire_host::{grasshopper_port_row, grasshopper_port_t};
use slate_doc::WorldRect;

struct Board {
    h: Harness,
    image: NodeId,
    text: NodeId,
    model: NodeId,
}

impl Board {
    fn new(tag: &str) -> Self {
        let mut h = Harness::new(tag);
        h.app.home_new_workspace();
        let path = h.base.join("reference.png");
        ::image::RgbaImage::from_pixel(16, 16, ::image::Rgba([210, 40, 23, 255]))
            .save(&path)
            .unwrap();
        let mut place = |path, rect| {
            let item = h.app.add_paths(&[path])[0];
            h.app.place_items_on_board(&[item], Pos2::ZERO);
            let id = *h.app.board_sel.iter().next().unwrap();
            h.app.patch_nodes(&[id], |n| n.rect = rect);
            id
        };
        let image = place(path, WorldRect::new(-430., -140., 240., 194.));
        let txt = h.base.join("note.txt");
        std::fs::write(&txt, "ordinary note").unwrap();
        let text = place(txt, WorldRect::new(-430., 138., 240., 80.));
        let gh = h.base.join("relief.ghx");
        std::fs::write(&gh, "test schema supplied below").unwrap();
        let model = place(gh, WorldRect::new(60., -140., 250., 225.));
        h.app.tab_mut().cam.z = 1.0;
        h.app.tab_mut().cam.offset = egui::Vec2::ZERO;
        h.app.board_sel.clear();
        h.app.request_grasshopper(model, &h.ctx);
        let key = (h.app.tab().id, model);
        let view = h.app.grasshopper.views.get_mut(&key).unwrap();
        view.due = None;
        view.ports = vec![GrasshopperPort {
            id: port(7),
            name: "Relief map".into(),
            direction: GrasshopperPortDirection::Input,
            kind: GrasshopperPortType::Text,
            access: GrasshopperPortAccess::Item,
        }];
        Self {
            h,
            image,
            text,
            model,
        }
    }

    fn point(&self, id: NodeId, side: Side, t: f32) -> Pos2 {
        let n = self.h.app.doc().scene.node(id).unwrap();
        self.h.app.wire_port_point(n, side, t)
    }

    fn center(&self, id: NodeId) -> Pos2 {
        Pos2::from(self.h.app.doc().scene.node(id).unwrap().rect.center())
    }
}

#[test]
fn images_and_text_keep_their_own_handles_when_a_grasshopper_model_is_present() {
    let mut b = Board::new("mixed-board-handle-ownership");
    for id in [b.image, b.text] {
        for selected in [false, true] {
            b.h.app.board_sel.clear();
            if selected {
                b.h.app.board_sel.insert(id);
            }
            let pointer = b.h.app.world_to_screen(b.center(id));
            let labels = painted(&mut b.h, pointer);
            assert!(
                !labels.iter().any(|s| s == "Image file" || s == "Panel"),
                "{labels:?}"
            );
            let node = b.h.app.doc().scene.node(id).unwrap();
            assert!(b
                .h
                .app
                .presented_wire_ports(node)
                .iter()
                .all(|p| grasshopper_port_row(p.t).is_none()));
        }
    }
    let image_rect = b.h.app.doc().scene.node(b.image).unwrap().rect;
    assert_eq!(
        b.point(b.image, Side::Right, OUTPUT_T),
        Pos2::new(
            image_rect.x + image_rect.w,
            image_rect.y + image_rect.h * 0.5
        )
    );
    let target = b.h.app.world_to_screen(b.center(b.model));
    assert!(painted(&mut b.h, target).iter().any(|s| s == "Relief map"));
}

#[test]
fn only_explicit_native_grasshopper_inputs_convert_an_image_wire_to_a_file_path() {
    for backward in [false, true] {
        let mut b = Board::new(if backward {
            "native-file-wire-backwards"
        } else {
            "native-file-wire-forwards"
        });
        let image = (b.image, Side::Right, OUTPUT_T);
        let input = (b.model, Side::Left, grasshopper_port_t(0));
        let (from, expected, end) = if backward {
            (input, image, b.center(b.image))
        } else {
            (image, input, b.point(input.0, input.1, input.2))
        };
        let start = b.point(from.0, from.1, from.2);
        let screen = b.h.app.world_to_screen(start);
        let mut drag =
            b.h.app
                .try_begin_wire_drag(screen, start, Default::default())
                .expect("real native grip");
        assert!(matches!(drag.mode, WireMode::Add {from: actual} if actual == from));
        b.h.app.wire_drag_update(&mut drag, end, false);
        assert_eq!(drag.snap, Some(expected));
        b.h.app.finish_wire_drag(drag);
        let conn =
            b.h.app
                .doc()
                .scene
                .nodes
                .iter()
                .find_map(|n| match &n.kind {
                    NodeKind::Connector(c) => Some(c),
                    _ => None,
                })
                .unwrap();
        let link = grasshopper_wire(conn).unwrap();
        assert_eq!((link.source, link.target), (b.image, b.model));
        assert_eq!(link.source_port, image::file_port().id);
        assert!(matches!(conn.a, ConnectorEnd::Anchored {side: Side::Right,t,..} if t == OUTPUT_T));
        b.h.app.board_undo();
        let to = (b.text, Side::Left, 0.5);
        b.h.app.finish_wire_drag(WireDrag {
            mode: WireMode::Add { from: image },
            cursor: b.center(b.text),
            snap: Some(to),
        });
        let ordinary =
            b.h.app
                .doc()
                .scene
                .nodes
                .iter()
                .find_map(|n| {
                    if let NodeKind::Connector(c) = &n.kind {
                        Some(c)
                    } else {
                        None
                    }
                })
                .unwrap();
        assert!(ordinary.grasshopper.is_none());
    }
}
