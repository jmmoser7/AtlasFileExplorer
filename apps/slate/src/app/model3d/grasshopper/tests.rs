use super::*;
use crate::app::tests::Harness;

#[test]
fn solving_a_locked_model_queues_its_frozen_poster_while_the_html_keeps_focus() {
    let mut h = Harness::new("grasshopper-locked-poster");
    let definition = h.base.join("model.ghx");
    std::fs::write(&definition, "fixture").unwrap();
    let item = h.app.add_paths(&[definition])[0];
    h.app.place_items_on_board(&[item], egui::Pos2::ZERO);
    let model = *h.app.board_sel.iter().next().unwrap();
    let doc = h.app.tab().id;
    h.app.request_grasshopper(model, &h.ctx);
    let page = h.app.add_web_portal(
        slate_doc::WorldRect::new(0., 0., 400., 300.),
        Some("graph.html".into()),
        "controller",
    );
    h.app.web_enter_contents(page);
    assert!(!h.app.model3d.live.contains_key(&model));

    let texture = |shade: u8| {
        h.ctx.load_texture(
            format!("poster-{shade}"),
            egui::ColorImage::from_rgba_unmultiplied([1, 1], &[shade, shade, shade, 255]),
            egui::TextureOptions::default(),
        )
    };
    let old = texture(50);
    assert_eq!(
        h.app.grasshopper_displayed_poster(model, Some(old.clone())),
        Some(old.id())
    );
    let mesh = h.base.join("new-solve.obj");
    std::fs::write(&mesh, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let (scene, hash) = parse_grasshopper_preview(&mesh).unwrap();
    h.app
        .install_grasshopper_preview(doc, model, format!("grasshopper-new-{hash}"), scene, hash);
    assert_eq!(
        h.app.model3d.want_poster.get(&(doc, model)),
        Some(&h.app.frame_no)
    );
    assert_eq!(
        h.app.grasshopper_displayed_poster(model, None),
        Some(old.id())
    );
    let next = texture(200);
    assert_eq!(
        h.app
            .grasshopper_displayed_poster(model, Some(next.clone())),
        Some(next.id())
    );
    assert_eq!(h.app.web.focused, Some(page));
    assert!(!h.app.model3d.live.contains_key(&model));
}
