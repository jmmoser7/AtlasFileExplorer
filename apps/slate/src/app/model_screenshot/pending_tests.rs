//! A loading model capture stays bound to its original editable source tab.
use super::super::tests::Harness;
use super::*;

fn queued(tag: &str, file: bool) -> (Harness, NodeId) {
    let mut h = Harness::new(tag);
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.tab_mut().path = Some(h.base.join("book.slate"));
    let source = h.base.join("loading.obj");
    std::fs::write(&source, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let item = h.app.add_paths(&[source])[0];
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(20.0, 30.0, 320.0, 160.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let source = h.app.add_nodes(vec![node])[0];
    let info = h.app.model_node_info(source).unwrap();
    h.app.model3d.request_model(&info.cache_key, &info.path);
    assert!(h.app.model3d.parse_outcome(&info.cache_key).is_none());
    let destination = file.then(|| h.base.join("capture.png"));
    h.app.queue_model_screenshot(source, destination);
    assert!(h.app.model_shot_pending.is_some());
    (h, source)
}

#[test]
fn a_loading_capture_cannot_resume_after_its_model_is_rebound() {
    for same_key in [false, true] {
        let (mut h, source) = queued(
            if same_key {
                "model_shot_path"
            } else {
                "model_shot_key"
            },
            false,
        );
        let original = h.app.model_node_info(source).unwrap();
        std::fs::write(h.base.join("replacement.obj"), "v 0 0 0\n").unwrap();
        let key = if same_key {
            original.cache_key.as_str()
        } else {
            "replacement-key"
        };
        let item =
            h.app
                .doc_mut()
                .add_item(h.base.join("replacement.obj"), "replacement.obj", 1, 0, key);
        h.app.patch_nodes(&[source], |n| {
            if let NodeKind::Image(image) = &mut n.kind {
                image.item = item;
            }
        });
        h.app.maintain_model_shot_pending();
        assert!(h.app.model_shot_pending.is_none());
        h.app.board_undo();
        h.app.maintain_model_shot_pending();
        assert!(
            h.app.model_shot_pending.is_none(),
            "undoing the rebind cannot resurrect a capture"
        );
        assert_eq!(h.app.doc().scene.nodes.len(), 1);
        assert!(!h.base.join("assets").exists());
    }
}

#[test]
fn a_loading_canvas_capture_cannot_resume_after_read_only_is_removed() {
    let (mut h, _) = queued("model_shot_read_only", false);
    h.app.tab_mut().read_only = true;
    h.app.maintain_model_shot_pending();
    assert!(h.app.model_shot_pending.is_none());
    h.app.tab_mut().read_only = false;
    h.app.maintain_model_shot_pending();
    assert!(h.app.model_shot_pending.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert!(!h.base.join("assets").exists());
}

#[test]
fn a_loading_capture_cannot_resume_after_returning_to_its_tab_or_leaving_home() {
    for file in [false, true] {
        for home in [false, true] {
            let (mut h, _) = queued(&format!("model_shot_away_{file}_{home}"), file);
            let origin = h.app.active_tab;
            if home {
                h.app.go_home();
            } else {
                h.app.new_tab();
            }
            h.app.maintain_model_shot_pending();
            assert!(h.app.model_shot_pending.is_none());
            if home {
                h.app.leave_home();
            } else {
                h.app.switch_tab(origin);
            }
            h.app.maintain_model_shot_pending();
            assert!(h.app.model_shot_pending.is_none());
            assert_eq!(h.app.doc().scene.nodes.len(), 1);
            assert!(!h.base.join("assets").exists());
            assert!(!h.base.join("capture.png").exists());
        }
    }
}

#[test]
fn deleting_then_undoing_a_loading_model_does_not_resume_its_capture() {
    let (mut h, source) = queued("model_shot_deleted", false);
    h.app.delete_board_nodes(&[source]);
    h.app.maintain_model_shot_pending();
    assert!(h.app.model_shot_pending.is_none());
    h.app.board_undo();
    h.app.maintain_model_shot_pending();
    assert!(h.app.model_shot_pending.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert!(!h.base.join("assets").exists());
}

#[test]
fn canvas_screenshot_rechecks_read_only_before_writing_or_linking() {
    let (mut h, source) = queued("model_shot_save_guard", false);
    let info = h.app.model_node_info(source).unwrap();
    let shot = ModelShot {
        rgba: vec![255; 4 * 4 * 4],
        w: 4,
        h: 4,
        meta: h
            .app
            .view_meta_input(source, &info, info.cam, 4, 4, &ImageAdjust::default())
            .unwrap(),
    };
    let items = h.app.doc().items.len();
    let undo = h.app.tab().journal.undo_depth();
    h.app.tab_mut().read_only = true;
    h.app.save_model_screenshot(source, shot, None);
    assert_eq!(h.app.doc().items.len(), items);
    assert_eq!(h.app.tab().journal.undo_depth(), undo);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert!(!h.base.join("assets").exists());
}
