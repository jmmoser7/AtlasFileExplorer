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
    use crate::app::preview::{linked_image_key, PreviewEntry, PX_EXACT};
    use std::sync::Arc;

    let (mut h, id) = generator_with_output("album-closed-blister-thumbnails");
    append_result(&mut h, id, "second");
    append_result(&mut h, id, "third");
    h.app.board_sel = std::iter::once(id).collect();
    let node = h.app.doc().scene.node(id).unwrap().clone();
    h.app.zoom_to_rect(node.rect);
    let z = h.app.tab().cam.z * 0.42;
    let canvas_height = h.app.canvas_rect.height();
    h.app.tab_mut().cam.z = z;
    h.app.tab_mut().cam.offset = egui::vec2(
        node.rect.x + node.rect.w * 0.5,
        node.rect.y + node.rect.h * 0.5 + canvas_height * 0.22 / z,
    );
    assert!(h.app.image_palette_eligible(id));
    assert!(
        h.app.image_blister_eligible(&node),
        "the 0.42 zoom fixture still routes to the attached image blister"
    );
    assert!(h.app.shape_properties.panel.is_none());
    let images = h.app.agent_images(id);
    for image in images.iter() {
        let path = slate_doc::scene::resolve_source(h.app.tab().path.as_deref(), &image.source);
        let pixels = egui::ColorImage::new([20, 10], egui::Color32::WHITE);
        let tex = h.ctx.load_texture(
            format!("album-result-{}", image.id),
            pixels.clone(),
            egui::TextureOptions::LINEAR,
        );
        h.app.preview_cache.insert(
            linked_image_key(&path, &image.id),
            PreviewEntry {
                tex,
                bytes: pixels.pixels.len() * 4,
                pixels: Arc::new(pixels),
                px: PX_EXACT,
                last_used: 0,
            },
        );
    }
    let textures: Vec<_> = images
        .iter()
        .map(|image| {
            let path = slate_doc::scene::resolve_source(h.app.tab().path.as_deref(), &image.source);
            h.app
                .linked_image_texture(path, &image.id, 44.0 * z)
                .unwrap()
                .id()
        })
        .collect();
    for _ in 0..30 {
        frame(&mut h, id, false);
    }
    let output = h.frame_output(|_| {});
    let body = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let chrome_bottom = h
        .app
        .shape_properties
        .result_chrome_bottom
        .filter(|(tab, host, _)| *tab == h.app.tab().id && *host == id)
        .map(|(_, _, bottom)| bottom)
        .expect("the selected image publishes its current occupied chrome bottom");
    let old_squares = atlas_shell::home::album_index_rects(
        body,
        &h.app
            .agent_images(id)
            .iter()
            .map(|image| image.request.as_str())
            .collect::<Vec<_>>(),
        textures.len(),
        h.app.agent_shown_index(id).unwrap_or(0),
        h.app.board_xf().z,
    );
    assert!(
        old_squares
            .iter()
            .any(|(_, rect)| rect.top() <= chrome_bottom),
        "the regression fixture places an old thumbnail square overlapping occupied chrome"
    );
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
    for texture in &textures {
        assert!(
            output.shapes.iter().any(|shape| under(
                &shape.shape,
                shape.clip_rect,
                *texture,
                chrome_bottom
            )),
            "every generated result paints below the current occupied image chrome"
        );
    }

    h.app.shape_properties.panel = Some(crate::app::board_properties::Panel::Agent);
    let output = h.frame_output(|_| {});
    let editor_bottom = h
        .app
        .shape_properties
        .result_chrome_bottom
        .filter(|(tab, host, _)| *tab == h.app.tab().id && *host == id)
        .map(|(_, _, bottom)| bottom)
        .expect("the open generator editor publishes its current occupied bottom");
    for texture in &textures {
        assert!(
            output.shapes.iter().any(|shape| under(
                &shape.shape,
                shape.clip_rect,
                *texture,
                editor_bottom
            )),
            "the result thumbnails remain painted below the open generator editor"
        );
    }
}
