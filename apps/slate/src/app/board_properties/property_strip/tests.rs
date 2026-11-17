use super::*;
use crate::app::tests::Harness;

fn image(h: &mut Harness, path: &str, width: f32, height: f32) -> Node {
    let item = h.app.doc_mut().add_item(path.into(), path, 0, 0, path);
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, width, height),
        NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    h.app.doc().scene.node(id).unwrap().clone()
}

#[test]
fn image_owner_rejection_cannot_fall_back_to_a_smaller_generic_blister() {
    let mut h = Harness::new("image-blister-one-owner");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let rejected = image(&mut h, "picture.png", 220.0, 260.0);
    assert!(h.app.image_palette_eligible(rejected.id));
    assert!(!h.app.image_blister_eligible(&rejected));
    assert!(
        chrome::blister_fits(
            Rect::from_min_size(Pos2::ZERO, egui::vec2(220.0, 260.0)),
            slate_doc::scene::resolved_corner_radius(&rejected, None),
            1.0,
            1
        ),
        "the smaller generic count would have re-admitted this image"
    );
    assert!(!rectangular_blister(&h.app, &[rejected], 1));
    let accepted = image(&mut h, "large.png", 900.0, 600.0);
    assert!(h.app.image_blister_eligible(&accepted));
    assert!(
        !rectangular_blister(&h.app, &[accepted], 1),
        "accepted images also remain with the image owner"
    );
}

#[test]
fn image_backed_nonimage_media_keep_the_generic_portal_chrome() {
    let mut h = Harness::new("nonimage-blister-categories");
    h.app.leave_home();
    h.app.ensure_work_tab();
    for path in ["model.obj", "document.pdf", "clip.mp4", "snippet.txt"] {
        let node = image(&mut h, path, 900.0, 600.0);
        assert!(!h.app.image_palette_eligible(node.id), "{path}");
        assert!(rectangular_blister(&h.app, &[node], 1), "{path}");
    }
}
