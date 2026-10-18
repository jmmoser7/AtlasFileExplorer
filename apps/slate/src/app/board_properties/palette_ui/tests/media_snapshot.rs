use super::*;

fn selected_media(tag: &str, name: &str, rect: WorldRect) -> (Harness, slate_doc::NodeId) {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    let id = crate::app::board_flow::tests::place(&mut h, name, rect.x);
    h.app.patch_nodes(&[id], |node| node.rect = rect);
    h.app.tab_mut().cam.offset = egui::Vec2::ZERO;
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_sel = [id].into_iter().collect();
    (h, id)
}

fn has_blister_notch(h: &Harness, id: slate_doc::NodeId) -> bool {
    let xf = h.app.board_xf();
    let host = h.app.doc().scene.node(id).unwrap();
    let bounds = xf.rect_w2s(host.rect.rotated_bounds(host.rotation_deg));
    h.app
        .media_host_surface(&h.ctx, &xf, host)
        .is_some_and(|surface| {
            surface.cutouts.iter().any(|cutout| {
                let cutout = Rect::from_points(cutout);
                cutout.width() > 100.0
                    && (cutout.center().x - bounds.center().x).abs() < 1.0
                    && (cutout.center().y - (bounds.bottom() - 10.0)).abs() < 8.0
            })
        })
}

#[test]
fn prepared_photo_notch_stays_with_its_host_until_the_next_palette_frame() {
    let photo_rect = WorldRect::new(-340.0, -190.0, 680.0, 380.0);
    let (mut h, photo) = selected_media("media-snapshot-photo", "photo.png", photo_rect);
    let generic_rect = WorldRect::new(20.0, -160.0, 640.0, 320.0);
    let generic = crate::app::board_flow::tests::place(&mut h, "reference.pdf", generic_rect.x);
    h.app
        .patch_nodes(&[generic], |node| node.rect = generic_rect);
    h.app.board_sel = [photo].into_iter().collect();
    assert_eq!(h.app.board_sel, [photo].into_iter().collect());
    let mut time = h.ctx.input(|i| i.time);
    for _ in 0..12 {
        frame(&mut h, &mut time);
    }

    let prepared = h
        .app
        .shape_properties
        .last_chrome
        .as_ref()
        .and_then(|chrome| chrome.media)
        .expect("the selected photo's collapsed blister is prepared");
    assert_eq!(prepared.host, photo);
    assert_eq!(prepared.count, 7);
    assert_eq!(
        prepared.presentation,
        chrome::BlisterPresentation::Collapsed
    );
    let xf = h.app.board_xf();
    let host = h.app.doc().scene.node(photo).unwrap();
    let bounds = xf.rect_w2s(host.rect);
    let blister = chrome::blister_layout(bounds, xf.z, prepared.count, prepared.presentation);
    assert_eq!(blister.band.height(), 40.0);
    assert_eq!(blister.band.center().y, bounds.bottom());
    assert_eq!(blister.cutout.unwrap().top(), bounds.bottom() - 20.0);
    assert_eq!(blister.cutout.unwrap().bottom(), bounds.bottom());
    assert!(has_blister_notch(&h, photo));

    h.app.board_sel = [generic].into_iter().collect();

    assert!(
        has_blister_notch(&h, photo),
        "the already prepared photo keeps its same-frame notch after selection changes"
    );
    assert!(
        !has_blister_notch(&h, generic),
        "the newly selected PDF must not inherit the previous host's notch"
    );

    frame(&mut h, &mut time);
    let prepared = h
        .app
        .shape_properties
        .last_chrome
        .as_ref()
        .and_then(|chrome| chrome.media)
        .expect("the next frame prepares the PDF's own blister");
    assert_eq!(prepared.host, generic);
    assert!(!has_blister_notch(&h, photo));
    assert!(has_blister_notch(&h, generic));
}

#[test]
fn expanded_generic_media_blister_is_an_overlay_without_a_host_cutout() {
    for (tag, name) in [
        ("media-snapshot-expanded-photo", "photo.png"),
        ("media-snapshot-expanded-pdf", "slides.pdf"),
    ] {
        let rect = WorldRect::new(-320.0, -160.0, 640.0, 320.0);
        let (mut h, host_id) = selected_media(tag, name, rect);
        let mut time = h.ctx.input(|i| i.time);
        frame(&mut h, &mut time);
        h.app.shape_properties.panel = Some(Panel::Fill);
        for _ in 0..12 {
            frame(&mut h, &mut time);
        }

        let prepared = h
            .app
            .shape_properties
            .last_chrome
            .as_ref()
            .and_then(|chrome| chrome.media)
            .expect("the expanded media blister is prepared");
        assert_eq!(prepared.host, host_id);
        assert_eq!(prepared.presentation, chrome::BlisterPresentation::Overlay);

        let xf = h.app.board_xf();
        let host = h.app.doc().scene.node(host_id).unwrap();
        let bounds = xf.rect_w2s(host.rect);
        let layout = chrome::blister_layout(
            bounds,
            xf.z,
            prepared.count,
            chrome::BlisterPresentation::Overlay,
        );
        assert!(bounds.contains_rect(layout.band));
        assert!(layout.cutout.is_none());
        assert!(!has_blister_notch(&h, host_id));
    }
}
