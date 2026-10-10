//! Review sheets for image highlight, tag, layer, and cut-out (ledger SG1–SG5).
//! Masks are the deterministic stub; no model runs.

use crate::app::board::BoardTool;
use crate::app::tests::support::{capture_frame, review_shot, FrameRaster, Harness};
use eframe::egui::{self, Pos2};
use slate_doc::scene::{ImageNode, NodeKind, WorldRect};
use slate_doc::{NodeId, ViewKind};

fn photo_board(tag: &str, dark: bool, zoom: f32, raster: &mut FrameRaster) -> (Harness, NodeId) {
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
    image::RgbaImage::from_fn(64, 48, |x, y| {
        let warm = (x as f32 / 63.0 * 120.0) as u8;
        image::Rgba([120 + warm, 70 + (y as u8 * 2), 60, 255])
    })
    .save(&path)
    .unwrap();
    let item = h.app.add_paths(&[path])[0];
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-200.0, -160.0, 400.0, 320.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    for _ in 0..40 {
        capture_frame(&mut h, raster, |_| {});
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    (h, id)
}

fn pointer(
    h: &mut Harness,
    raster: &mut FrameRaster,
    p: Pos2,
    press: Option<bool>,
) -> egui::FullOutput {
    capture_frame(h, raster, |i| {
        i.events.push(egui::Event::PointerMoved(p));
        if let Some(pressed) = press {
            i.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
    })
}

fn click_at(h: &mut Harness, raster: &mut FrameRaster, p: Pos2) {
    pointer(h, raster, p, None);
    pointer(h, raster, p, Some(true));
    pointer(h, raster, p, Some(false));
}

fn settle(h: &mut Harness, raster: &mut FrameRaster, p: Pos2) -> egui::FullOutput {
    pointer(h, raster, p, None);
    pointer(h, raster, p, None)
}

fn zoom_name(zoom: f32) -> String {
    format!("z{}", zoom.to_string().replace('.', "_"))
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn segment_highlight_sheets() {
    let mut raster = FrameRaster::new(1440, 900);
    for dark in [true, false] {
        let theme = if dark { "dark" } else { "light" };
        let (mut h, id) = photo_board(&format!("sg1_{theme}"), dark, 1.0, &mut raster);
        let p = h.app.install_stub_highlight(id);
        let out = settle(&mut h, &mut raster, p);
        review_shot(&mut h, &mut raster, out, "SG1", &format!("hover_{theme}"));
        click_at(&mut h, &mut raster, p);
        let out = settle(&mut h, &mut raster, p);
        review_shot(
            &mut h,
            &mut raster,
            out,
            "SG1",
            &format!("tag_open_{theme}"),
        );
    }
    for zoom in [0.5_f32, 2.0] {
        let (mut h, id) = photo_board(&format!("sg1_{}", zoom_name(zoom)), true, zoom, &mut raster);
        let p = h.app.install_stub_highlight(id);
        settle(&mut h, &mut raster, p);
        click_at(&mut h, &mut raster, p);
        let out = settle(&mut h, &mut raster, p);
        let name = format!("tag_open_{}", zoom_name(zoom));
        review_shot(&mut h, &mut raster, out, "SG1", &name);
    }

    for dark in [true, false] {
        let theme = if dark { "dark" } else { "light" };
        let (mut h, id) = photo_board(&format!("sg2_{theme}"), dark, 1.0, &mut raster);
        let p = h.app.install_stub_highlight(id);
        settle(&mut h, &mut raster, p);
        click_at(&mut h, &mut raster, p);
        let tag = h.app.image_segments.tag_rects[0].center();
        click_at(&mut h, &mut raster, tag);
        let out = settle(&mut h, &mut raster, tag);
        review_shot(
            &mut h,
            &mut raster,
            out,
            "SG2",
            &format!("layer_menu_open_{theme}"),
        );
    }

    let (mut h, id) = photo_board("sg3", true, 1.0, &mut raster);
    let p = h.app.install_stub_highlight(id);
    settle(&mut h, &mut raster, p);
    let outside = h.app.board_xf().w2s(Pos2::new(420.0, 40.0));
    pointer(&mut h, &mut raster, p, Some(true));
    for t in [0.2, 0.4, 0.6, 0.8, 1.0] {
        pointer(&mut h, &mut raster, p + (outside - p) * t, None);
    }
    let out = pointer(&mut h, &mut raster, outside, None);
    review_shot(&mut h, &mut raster, out, "SG3", "mid_drag");
    pointer(&mut h, &mut raster, outside, Some(false));
    let away = Pos2::new(1300.0, 820.0);
    let out = settle(&mut h, &mut raster, away);
    review_shot(&mut h, &mut raster, out, "SG3", "placed_sticker");

    for dark in [true, false] {
        let theme = if dark { "dark" } else { "light" };
        let (mut h, id) = photo_board(&format!("sg4_{theme}"), dark, 1.0, &mut raster);
        let rect = h.app.doc().scene.node(id).unwrap().rect;
        let p = h.app.board_xf().rect_w2s(rect).center();
        h.app.board_menu = Some((id, p));
        settle(&mut h, &mut raster, p);
        let out = settle(&mut h, &mut raster, p);
        review_shot(
            &mut h,
            &mut raster,
            out,
            "SG4",
            &format!("image_menu_open_{theme}"),
        );
    }

    let (mut h, id) = photo_board("sg5", true, 1.0, &mut raster);
    let p = h.app.install_stub_highlight(id);
    h.app.image_segments.grow_debug = true;
    let out = settle(&mut h, &mut raster, p);
    review_shot(&mut h, &mut raster, out, "SG5", "prototype_start");
    let mut q = p;
    for _ in 0..12 {
        q += egui::vec2(1.5, 0.5);
        pointer(&mut h, &mut raster, q, None);
    }
    let out = pointer(&mut h, &mut raster, q, None);
    review_shot(&mut h, &mut raster, out, "SG5", "prototype_grown");
    let out = pointer(&mut h, &mut raster, q + egui::vec2(-30.0, 0.0), None);
    review_shot(&mut h, &mut raster, out, "SG5", "prototype_retreat");
}
