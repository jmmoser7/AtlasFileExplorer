//! Review sheets for image highlight, tag, layer, and cut-out (ledger SG1–SG5).

use crate::app::board::BoardTool;
use crate::app::tests::support::{capture_frame, review_shot, FrameRaster, Harness};
use eframe::egui::{self, Pos2};
use slate_doc::scene::{ImageNode, NodeKind, WorldRect};
use slate_doc::{NodeId, ViewKind};

fn photo_board(tag: &str, dark: bool, zoom: f32) -> (Harness, NodeId) {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.tab_mut().cam.z = zoom;
    h.app.dark_mode = dark;
    h.ctx.set_visuals(if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    });
    let path = h.base.join("photo.png");
    image::RgbaImage::from_pixel(64, 48, image::Rgba([180, 90, 40, 255]))
        .save(&path)
        .unwrap();
    let item = h.app.add_paths(&[path])[0];
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-200.0, -160.0, 400.0, 320.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    h.frame();
    (h, id)
}

fn click_at(h: &mut Harness, raster: &mut FrameRaster, p: Pos2) {
    for pressed in [true, false] {
        let _ = capture_frame(h, raster, |i| {
            i.events.push(egui::Event::PointerMoved(p));
            i.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        });
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn segment_highlight_sheets() {
    let mut raster = FrameRaster::new(1440, 900);
    for dark in [true, false] {
        let theme = if dark { "dark" } else { "light" };
        let (mut h, id) = photo_board(&format!("sg1_{theme}"), dark, 1.0);
        capture_frame(&mut h, &mut raster, |_| {});
        let p = h.app.install_stub_highlight(id);
        let out = capture_frame(&mut h, &mut raster, |i| {
            i.events.push(egui::Event::PointerMoved(p));
        });
        review_shot(&mut h, &mut raster, out, "SG1", &format!("hover_{theme}"));
        click_at(&mut h, &mut raster, p);
        let out = capture_frame(&mut h, &mut raster, |_| {});
        review_shot(
            &mut h,
            &mut raster,
            out,
            "SG1",
            &format!("tag_open_{theme}"),
        );
    }
    for zoom in [0.5_f32, 1.0, 2.0] {
        let (mut h, id) = photo_board(&format!("sg1_z{zoom}"), true, zoom);
        capture_frame(&mut h, &mut raster, |_| {});
        let p = h.app.install_stub_highlight(id);
        click_at(&mut h, &mut raster, p);
        let out = capture_frame(&mut h, &mut raster, |_| {});
        let name = format!("tag_zoom_{}", zoom.to_string().replace('.', "_"));
        review_shot(&mut h, &mut raster, out, "SG1", &name);
    }

    let (mut h, id) = photo_board("sg2", true, 1.0);
    capture_frame(&mut h, &mut raster, |_| {});
    let p = h.app.install_stub_highlight(id);
    click_at(&mut h, &mut raster, p);
    let tag = h.app.image_segments.tag_rects[0].center();
    click_at(&mut h, &mut raster, tag);
    let out = capture_frame(&mut h, &mut raster, |_| {});
    review_shot(&mut h, &mut raster, out, "SG2", "layer_menu_open");

    let (mut h, id) = photo_board("sg3", true, 1.0);
    capture_frame(&mut h, &mut raster, |_| {});
    let p = h.app.install_stub_highlight(id);
    let outside = h.app.board_xf().w2s(Pos2::new(360.0, 0.0));
    let _ = capture_frame(&mut h, &mut raster, |i| {
        i.events.push(egui::Event::PointerMoved(p));
        i.events.push(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
    });
    let out = capture_frame(&mut h, &mut raster, |i| {
        i.events.push(egui::Event::PointerMoved(outside));
    });
    review_shot(&mut h, &mut raster, out, "SG3", "mid_drag");
    let out = capture_frame(&mut h, &mut raster, |i| {
        i.events.push(egui::Event::PointerMoved(outside));
        i.events.push(egui::Event::PointerButton {
            pos: outside,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
    });
    review_shot(&mut h, &mut raster, out, "SG3", "placed_sticker");

    let (mut h, id) = photo_board("sg4", true, 1.0);
    capture_frame(&mut h, &mut raster, |_| {});
    let p = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect)
        .center();
    h.app.board_menu = Some((id, p));
    let out = capture_frame(&mut h, &mut raster, |_| {});
    review_shot(&mut h, &mut raster, out, "SG4", "image_menu_open");

    let (mut h, id) = photo_board("sg5", true, 1.0);
    capture_frame(&mut h, &mut raster, |_| {});
    let p = h.app.install_stub_highlight(id);
    h.app.image_segments.grow_debug = true;
    let out = capture_frame(&mut h, &mut raster, |i| {
        i.events.push(egui::Event::PointerMoved(p));
    });
    let _ = out;
    let out = capture_frame(&mut h, &mut raster, |i| {
        i.events
            .push(egui::Event::PointerMoved(p + egui::vec2(1.2, 0.4)));
    });
    review_shot(&mut h, &mut raster, out, "SG5", "prototype_drift");
}
