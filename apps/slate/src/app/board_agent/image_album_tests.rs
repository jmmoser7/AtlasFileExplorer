//! Generated albums retain their resting pick and include arriving results.
use super::*;
use crate::app::tests::Harness;

pub(super) fn append_result(h: &mut Harness, id: NodeId, name: &str) {
    // Runtime output belongs to the current binding; a provider change must
    // still discard output from its previous session in the real frame pump.
    let binding = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .session
        .clone();
    h.app.agents.bindings.insert(id, binding);
    let folder = if h.app.tab().path.is_some() {
        h.base.join("assets/album")
    } else {
        h.base.clone()
    };
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(format!("{name}.png"));
    image::RgbImage::new(20, 10).save(&path).unwrap();
    let mut session = h
        .app
        .agents
        .sessions
        .get(&id)
        .map(|s| (**s).clone())
        .unwrap_or_else(|| atlas_agent::AgentSession {
            activity: Vec::new(),
            approval: None,
            conversation: String::new(),
            artifacts: vec![],
            status: AgentStatus::Idle,
            provider: "comfy".into(),
            turns: vec![],
            updated_at: 0,
            bundle: Default::default(),
            request: String::new(),
            usage: None,
        });
    session.bundle.images.push(atlas_agent::ImageOutput {
        id: name.into(),
        source: path.to_string_lossy().into_owned(),
        request: name.into(),
        ..Default::default()
    });
    h.app
        .agents
        .sessions
        .insert(id, std::sync::Arc::new(session));
    h.app.agents.output_epoch += 1;
}

fn frame(h: &mut Harness, id: NodeId, wheel: bool) {
    let node = h.app.doc().scene.node(id).unwrap();
    let pointer = h.app.board_xf().rect_w2s(node.rect).center();
    let time = h.ctx.input(|i| i.time) + 1.0 / 60.0;
    h.frame_with(|input| {
        input.time = Some(time);
        input.events.push(egui::Event::PointerMoved(pointer));
        if wheel {
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -3.0),
                modifiers: egui::Modifiers::NONE,
            });
        }
    });
}

fn settle(h: &mut Harness, id: NodeId) {
    for _ in 0..180 {
        frame(h, id, false);
    }
    assert!(atlas_shell::home::album_browsing(&h.ctx, Id::new(("agent-picture", id.0))) < 0.001);
}

#[test]
fn generated_album_resumes_at_its_resting_pick_and_reaches_new_results() {
    let (mut h, id) = generator_with_output("album-rest-and-arrival");
    append_result(&mut h, id, "second");
    append_result(&mut h, id, "third");
    h.app.pick_agent_result(id, 1);
    h.app.board_sel = std::iter::once(id).collect();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    h.app.tab_mut().cam.z = 1.0;
    settle(&mut h, id);
    let zoom = h.app.tab().cam.z;
    for expected in [2, 0, 1, 2] {
        frame(&mut h, id, true);
        settle(&mut h, id);
        assert_eq!(h.app.agent_shown_index(id), Some(expected));
        assert_eq!(h.app.tab().cam.z, zoom, "album owns the wheel");
    }
    append_result(&mut h, id, "fourth");
    frame(&mut h, id, true);
    settle(&mut h, id);
    assert_eq!(h.app.agent_active_output(id).as_deref(), Some("fourth"));
    frame(&mut h, id, true);
    settle(&mut h, id);
    assert_eq!(h.app.agent_shown_index(id), Some(0));
}

#[test]
fn a_picked_result_keeps_its_relative_workbook_locator_and_its_album_index() {
    let (mut h, id) = generator_with_output("album-relative-pick");
    h.app.tab_mut().path = Some(h.base.join("Book.slate"));
    append_result(&mut h, id, "second");
    append_result(&mut h, id, "third");
    h.app.pick_agent_result(id, 1);
    let NodeKind::Image(image) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    let path = &h.app.doc().item(image.item).unwrap().path;
    assert!(path.is_relative(), "workbook assets keep relative links");
    assert_eq!(h.app.agent_shown_index(id), Some(1));
    assert_eq!(h.app.agent_active_output(id).as_deref(), Some("second"));
    assert!(h.app.agent_shown_path(id).unwrap().is_file());
}

#[test]
fn a_closed_image_blister_keeps_all_result_thumbnail_squares_visible() {
    let (mut h, id) = generator_with_output("album-closed-blister-thumbnails");
    append_result(&mut h, id, "second");
    append_result(&mut h, id, "third");
    h.app.board_sel = std::iter::once(id).collect();
    let node = h.app.doc().scene.node(id).unwrap().clone();
    h.app.zoom_to_rect(node.rect);
    assert!(h.app.image_palette_eligible(id));
    assert!(h.app.shape_properties.panel.is_none());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let textures = loop {
        h.frame();
        let images = h.app.agent_images(id);
        let ready: Option<Vec<_>> = images
            .iter()
            .map(|image| {
                h.app
                    .linked_image_texture(
                        slate_doc::scene::resolve_source(
                            h.app.tab().path.as_deref(),
                            &image.source,
                        ),
                        &image.id,
                        44.0 * h.app.board_xf().z,
                    )
                    .map(|texture| texture.id())
            })
            .collect();
        if let Some(textures) = ready {
            break textures;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "local thumbnails become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    for _ in 0..30 {
        frame(&mut h, id, false);
    }
    let output = h.frame_output(|_| {});
    let bottom = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect)
        .bottom();
    fn under(shape: &egui::Shape, clip: Rect, texture: egui::TextureId, bottom: f32) -> bool {
        match shape {
            egui::Shape::Rect(rect) => {
                rect.brush
                    .as_ref()
                    .is_some_and(|brush| brush.fill_texture_id == texture)
                    && rect.rect.is_positive()
                    && rect.rect.top() > bottom
                    && clip.contains_rect(rect.rect)
            }
            egui::Shape::Mesh(mesh) => {
                mesh.texture_id == texture
                    && !mesh.vertices.is_empty()
                    && mesh
                        .vertices
                        .iter()
                        .all(|vertex| vertex.pos.y > bottom && clip.contains(vertex.pos))
            }
            egui::Shape::Vec(shapes) => shapes
                .iter()
                .any(|shape| under(shape, clip, texture, bottom)),
            _ => false,
        }
    }
    for texture in textures {
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| under(&shape.shape, shape.clip_rect, texture, bottom)),
            "every generated result has a visible thumbnail below the selected image"
        );
    }
}
