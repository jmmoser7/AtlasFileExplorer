use super::*;

#[test]
fn opening_an_object_panel_retires_the_previous_inline_number_editor() {
    use crate::app::tests::Harness;
    let mut h = Harness::new("capture-retires-number-editor");
    h.app.ensure_work_tab();
    h.app.leave_home();
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 320.0, 200.0),
        NodeKind::Portal(scene::PortalNode::unbound_web("Web")),
    );
    let source = h.app.add_nodes(vec![node])[0];
    h.app.board_sel = std::iter::once(source).collect();
    h.app.sync_shape_properties();
    h.app.shape_properties.number = Some(NumberEdit {
        kind: DimensionKind::Width,
        input: Some(chrome::NumberEdit::new(320.0)),
    });
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(h.app.open_object_panel(Panel::Capture));
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Capture));
    assert!(!h.app.property_inline_editor_open());
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn click_away_recovery_expires_after_one_and_a_half_seconds() {
    let at = Instant::now();
    for (millis, expected) in [
        (1499, Some(Panel::Layers)),
        (1500, Some(Panel::Layers)),
        (1501, None),
    ] {
        let mut memory = ObjectPanelMemory::default();
        memory.dismiss(1, NodeId(7), Panel::Layers, None, at);
        assert_eq!(
            memory
                .restore(1, NodeId(7), at + Duration::from_millis(millis))
                .map(|entry| entry.panel),
            expected
        );
    }
}

#[test]
fn editing_and_tool_resets_do_not_spend_the_click_away_window() {
    let at = Instant::now();
    let mut memory = ObjectPanelMemory::default();
    memory.open(1, NodeId(7), Panel::Brush, None);
    let dismissed = at + Duration::from_secs(600);
    assert_eq!(
        memory
            .restore(1, NodeId(7), dismissed)
            .map(|entry| entry.panel),
        Some(Panel::Brush)
    );
    memory.selection_changed(1, &[], dismissed);
    memory.selection_changed(1, &[], dismissed + Duration::from_secs(1));
    assert_eq!(
        memory
            .restore(1, NodeId(7), dismissed + Duration::from_millis(1501))
            .map(|entry| entry.panel),
        None,
        "later selection sync must not restart the deadline"
    );
}

#[test]
fn palette_recovery_is_image_and_workbook_specific_and_explicit_close_forgets_it() {
    let at = Instant::now();
    let mut memory = ObjectPanelMemory::default();
    memory.dismiss(1, NodeId(7), Panel::Filter, None, at);
    assert_eq!(
        memory.restore(2, NodeId(7), at).map(|entry| entry.panel),
        None
    );
    assert_eq!(
        memory.restore(1, NodeId(8), at).map(|entry| entry.panel),
        None
    );
    memory.forget(1, NodeId(7));
    assert_eq!(
        memory.restore(1, NodeId(7), at).map(|entry| entry.panel),
        None
    );
}

#[test]
fn quick_image_reselection_restores_the_panel_but_a_later_return_is_collapsed() {
    use crate::app::board_flow::tests::{generator_with_output, place};
    let (mut h, _) = generator_with_output("image-palette-return-timeout");
    let image = place(&mut h, "photo.png", 0.0);
    h.app.set_board_tool(BoardTool::Select);
    h.app.board_sel = std::iter::once(image).collect();
    let at = Instant::now();
    h.app.sync_shape_properties_at(at);
    h.app.shape_properties.panel = Some(Panel::Brush);
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    // A long editing session still leaves the full return window.
    let dismissed = at + Duration::from_secs(600);
    h.app.board_sel.clear();
    h.app.sync_shape_properties_at(dismissed);
    h.app.board_sel.insert(image);
    h.app
        .sync_shape_properties_at(dismissed + Duration::from_millis(1499));
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    assert_eq!(
        h.app.board_tool,
        BoardTool::Select,
        "recovery does not rearm Brush"
    );
    let dismissed_again = dismissed + Duration::from_secs(2);
    h.app.board_sel.clear();
    h.app.sync_shape_properties_at(dismissed_again);
    h.app.board_sel.insert(image);
    h.app
        .sync_shape_properties_at(dismissed_again + Duration::from_millis(1501));
    assert_eq!(h.app.shape_properties.panel, None);
    assert_eq!(h.app.board_sel, std::iter::once(image).collect());
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

fn artifact_selection(tag: &str) -> (crate::app::tests::Harness, NodeId) {
    let mut h = crate::app::tests::Harness::new(tag);
    h.app.ensure_work_tab();
    h.app.leave_home();
    let mut portal = scene::PortalNode::unbound_web("Dashboard");
    portal.web.as_mut().unwrap().artifact = Some(Box::new(scene::ArtifactGeneratorRef::default()));
    portal.agent = scene::PortalNode::unbound_agent("Dashboard", "codex").agent;
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 640.0, 400.0),
        NodeKind::Portal(portal),
    );
    let id = h.app.add_nodes(vec![node])[0];
    h.app.board_sel = [id].into_iter().collect();
    h.app.sync_shape_properties();
    (h, id)
}

fn open_generator_menu(
    h: &mut crate::app::tests::Harness,
    id: NodeId,
) -> chrome::generator::GeneratorPresentation {
    let presentation = chrome::generator::GeneratorPresentation {
        model_open: true,
        picker_page: chrome::generator::PickerPage::Parameter(egui::Id::new("thought-level")),
        activity: Some(chrome::generator::activity::ActivityState {
            fold: chrome::generator::activity::CardFold::Open,
            scroll: chrome::generator::activity::FollowScroll {
                follow: false,
                offset: 42.0,
            },
            content_height: 300.0,
        }),
        ..Default::default()
    };
    h.app.shape_properties.panel = Some(Panel::Agent);
    h.app.restore_generator_presentation(id, presentation);
    h.app.remember_object_panel_open(id, Panel::Agent);
    presentation
}

#[test]
fn artifact_quick_reselection_restores_the_same_editor_dropdown_and_activity() {
    let (mut h, id) = artifact_selection("artifact-generator-selection-memory");
    let expected = open_generator_menu(&mut h, id);
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    let dismissed = Instant::now();
    h.app.board_sel.clear();
    h.app.sync_shape_properties_at(dismissed);
    assert_eq!(
        h.app.generator_presentation(id),
        Default::default(),
        "an unselected editor has no live popup"
    );
    h.app.board_sel.insert(id);
    h.app
        .sync_shape_properties_at(dismissed + Duration::from_millis(1500));
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Agent));
    assert_eq!(h.app.generator_presentation(id), expected);
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn expired_artifact_reselection_forgets_the_dropdown_and_expansion() {
    let (mut h, id) = artifact_selection("artifact-generator-selection-expired");
    open_generator_menu(&mut h, id);
    let dismissed = Instant::now();
    h.app.board_sel.clear();
    h.app.sync_shape_properties_at(dismissed);
    h.app.board_sel.insert(id);
    h.app
        .sync_shape_properties_at(dismissed + Duration::from_millis(1501));
    assert_eq!(h.app.shape_properties.panel, None);
    assert_eq!(h.app.generator_presentation(id), Default::default());
}

#[test]
fn explicit_close_and_escape_forget_artifact_generator_memory() {
    for escape in [false, true] {
        let (mut h, id) = artifact_selection(if escape {
            "artifact-generator-escape"
        } else {
            "artifact-generator-close"
        });
        open_generator_menu(&mut h, id);
        if escape {
            h.frame_with(|input| {
                input.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
            });
        } else {
            h.app.forget_object_panel(id);
            h.app.shape_properties.panel = None;
        }
        let dismissed = Instant::now();
        h.app.board_sel.clear();
        h.app.sync_shape_properties_at(dismissed);
        h.app.board_sel.insert(id);
        h.app
            .sync_shape_properties_at(dismissed + Duration::from_millis(10));
        assert_eq!(h.app.shape_properties.panel, None, "escape={escape}");
        assert_eq!(
            h.app.generator_presentation(id),
            Default::default(),
            "escape={escape}"
        );
    }
}
