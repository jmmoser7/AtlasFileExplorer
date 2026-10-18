//! Shared image-album renderer: full-bleed hosts and compact persistent pickers.
use super::*;

pub fn image_album(
    ui: &egui::Ui,
    id: Id,
    rect: Rect,
    images: &[AlbumImage],
    focus: usize,
    input: AlbumInput,
) -> usize {
    render_album(ui, id, rect, images, focus, input, None)
}

/// Keeps neighboring cards visible at rest, with the same wheel, drag, spring and projection owner.
pub fn image_album_picker(
    ui: &egui::Ui,
    id: Id,
    rect: Rect,
    images: &[AlbumImage],
    focus: usize,
    cover: Vec2,
) -> usize {
    render_album(
        ui,
        id,
        rect,
        images,
        focus,
        AlbumInput {
            drag: true,
            wheel: true,
            host_paints_rest: false,
        },
        Some(cover),
    )
}

/// Image-only presentation of the shared album motion. At rest the active image
/// fills its portal; browsing reveals neighbors and then settles back to full bleed.
fn render_album(
    ui: &egui::Ui,
    id: Id,
    rect: Rect,
    images: &[AlbumImage],
    focus: usize,
    input: AlbumInput,
    picker: Option<Vec2>,
) -> usize {
    if images.is_empty() {
        return 0;
    }
    let several = images.len() > 1;
    let response = ui.interact(
        rect,
        id.with("album-hit"),
        if input.drag && several {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    let state_id = id.with("album-motion");
    let mut flow = ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_insert_with(state_id, CoverFlowState::default)
            .clone()
    });
    let tokens = tokens::current().home.clone();
    let tuning = CoverFlowTuning::from_tokens(
        &tokens,
        picker.map_or(rect.width() * ALBUM_BROWSE_SCALE, |size| size.x),
    );
    let (mut focus, _, _) = advance_flow(
        ui,
        &response,
        &mut flow,
        images.len(),
        focus,
        FlowInput {
            drag: input.drag && several,
            wheel: input.wheel && several,
            discrete_album_wheel: true,
            keyboard: picker.is_none() || response.has_focus(),
        },
        &tuning,
    );
    if picker.is_some() && response.clicked() {
        response.request_focus();
    }
    let resting = picker.is_none() && flow.phase == InteractionPhase::Idle;
    let expansion =
        ui.ctx()
            .animate_bool_with_time(id.with("album-settle"), resting, ALBUM_SETTLE_SECONDS);
    ui.ctx()
        .data_mut(|d| d.insert_temp(id.with("album-expansion"), expansion));
    let painter = ui.painter_at(rect);
    if resting && expansion > 0.999 {
        if input.host_paints_rest {
            ui.ctx().data_mut(|d| d.insert_temp(state_id, flow));
            return focus;
        }
        if let Some(texture) = images[focus].texture {
            painter.image(
                texture,
                rect,
                album_uv(images[focus].size, rect.size()),
                Color32::WHITE,
            );
        }
    } else {
        let scale = ALBUM_BROWSE_SCALE + (1.0 - ALBUM_BROWSE_SCALE) * expansion;
        for (slot, logical) in visible_slots(flow.position, images.len(), 3) {
            let off = slot_visual_offset(flow.position, slot, images.len());
            let image = images[logical];
            if let Some(texture) = image.texture {
                let size = picker.unwrap_or(rect.size() * scale);
                let uv = album_uv(image.size, size);
                if picker.is_some() && response.clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let q = project_cover(rect.center(), size.x, size.y, off, &tuning);
                        if point_in_quad(pos, &q) {
                            flow.target = slot as f32;
                            flow.phase = InteractionPhase::Snapping;
                            focus = logical;
                            ui.ctx().request_repaint();
                        }
                    }
                }
                let key = Id::new((
                    rect.min.x.to_bits(),
                    rect.min.y.to_bits(),
                    size.x.to_bits(),
                    size.y.to_bits(),
                    off.to_bits(),
                    texture,
                ));
                let cache_id = id.with(("album-art", slot));
                let cached = painter
                    .ctx()
                    .data(|d| d.get_temp::<CachedCoverPaint>(cache_id));
                if let Some(cached) = cached.filter(|c| c.key == key) {
                    painter.extend(cached.shapes.iter().cloned());
                } else {
                    let mut mesh = artwork_mesh(
                        rect.center(),
                        size.x,
                        size.y,
                        off,
                        &tuning,
                        Some(texture),
                        Color32::WHITE,
                        FacePass {
                            reflected: false,
                            height: 1.0,
                            opacity: 1.0,
                        },
                        0.0,
                    );
                    for v in &mut mesh.vertices {
                        v.uv = uv.min + v.uv.to_vec2() * uv.size();
                    }
                    let shapes: Arc<[egui::Shape]> = vec![egui::Shape::mesh(mesh)].into();
                    painter.extend(shapes.iter().cloned());
                    painter
                        .ctx()
                        .data_mut(|d| d.insert_temp(cache_id, CachedCoverPaint { key, shapes }));
                }
            }
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(state_id, flow));
    focus
}
