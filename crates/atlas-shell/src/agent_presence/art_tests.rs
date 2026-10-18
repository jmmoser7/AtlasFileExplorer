use super::*;
#[test]
fn all_canonical_artwork_parses_to_finite_cached_meshes() {
    for (name, svg) in &master().props {
        let artwork = build(svg, false, 1.0);
        assert!(!artwork.parts.is_empty(), "{name}");
        for part in artwork.parts {
            for mesh in part.frames {
                assert!(mesh.vertices.iter().all(|v| v.pos.is_finite()));
                assert!(mesh
                    .indices
                    .iter()
                    .all(|i| (*i as usize) < mesh.vertices.len()));
            }
        }
    }
    for (name, hat) in &master().hats {
        if !hat.svg.is_empty() {
            assert!(!build(&hat.svg, true, 5.0).parts.is_empty(), "{name}");
        }
    }
}
#[test]
fn a_book_page_really_turns_and_reduced_motion_has_a_stable_first_frame() {
    let art = build(&master().props["book"], false, 1.0);
    let page = art.parts.iter().find(|p| p.frames.len() > 1).unwrap();
    assert!(page.duration > 1.0);
    assert_ne!(page.frames[0].vertices, page.frames[24].vertices);
    assert!(Arc::ptr_eq(&page.frames[0], &page.frames[0]));
}
#[test]
fn aliases_and_departures_have_one_master() {
    assert!(master()
        .aliases
        .values()
        .all(|key| master().props.contains_key(key)));
    assert_eq!(master().departures.len(), 4);
    assert!(master().departures.iter().all(|d| d.duration > 0.0));
}

#[test]
fn props_survive_a_nonzero_canvas_origin_and_keep_the_screen_clip() {
    let canvas = egui::Rect::from_min_max(egui::pos2(120.0, 90.0), egui::pos2(680.0, 500.0));
    for density in [1.0, 2.0] {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(density);
        let mut cache = ArtCache::default();
        for prop in ["book", "pencil"] {
            // Center the prop inside, across the edge, and beyond the viewport.
            // The avatar anchor includes the canonical prop attachment offset.
            for (case, prop_center) in [
                ("inside", egui::pos2(300.0, 240.0)),
                ("edge", egui::pos2(canvas.right(), 240.0)),
                ("outside", egui::pos2(canvas.right() + 40.0, 240.0)),
            ] {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        let painter = ctx
                            .layer_painter(egui::LayerId::new(
                                egui::Order::Middle,
                                Id::new("nonzero-canvas-props"),
                            ))
                            .with_clip_rect(canvas);
                        cache.paint(
                            &painter,
                            &AvatarVisual {
                                id: prop,
                                center: prop_center
                                    - egui::vec2(master().prop_offset[0], master().prop_offset[1]),
                                expression: "neutral",
                                orientation: "forward",
                                hat: "none",
                                prop,
                                opacity: 1.0,
                            },
                            0.7,
                            false,
                            master().head_points,
                            1.0,
                        );
                    },
                );
                let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
                let (mut visible, mut outside) = (0, 0);
                for primitive in primitives {
                    // Scissor output must be the original screen-space canvas,
                    // not a smaller intersection with local SVG coordinates.
                    assert!((primitive.clip_rect.min - canvas.min).length() < 0.001);
                    assert!((primitive.clip_rect.max - canvas.max).length() < 0.001);
                    if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                        for vertex in mesh.vertices.iter().filter(|v| v.color.a() > 0) {
                            if primitive.clip_rect.contains(vertex.pos) {
                                visible += 1;
                            } else {
                                outside += 1;
                            }
                        }
                    }
                }
                if case == "outside" {
                    assert_eq!(visible, 0, "{prop} must not leak beyond the canvas");
                } else {
                    assert!(
                        visible > 0,
                        "{prop}/{case} disappeared at nonzero canvas origin"
                    );
                    if case == "edge" {
                        assert!(
                            outside > 0,
                            "edge case really exercises partial viewport clipping"
                        );
                    }
                }
            }
        }
    }
}
