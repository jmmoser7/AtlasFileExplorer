//! Opt-in visual review of the actual Slate/shared-shell image palettes.
//! Install only in app/tests/review; this adds no production dependency.
use super::*;
use crate::app::board_properties::Panel;
use slate_doc::image_paint::{ensure_result_layers_mut, PaintLayer, PaintLayerId};
use slate_doc::scene::{AgentPortalRef, ImageNode};

#[derive(Clone, Copy, PartialEq)]
enum State {
    Idle,
    Collapsed,
    Generator,
    Fill,
    Stroke,
    Corners,
    Filters,
    Brush,
    Layers,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Collapsed => "selected_collapsed",
            Self::Generator => "generator_history",
            Self::Fill => "fill",
            Self::Stroke => "stroke",
            Self::Corners => "corners_crop",
            Self::Filters => "filters",
            Self::Brush => "brush",
            Self::Layers => "layers",
        }
    }
    fn panel(self) -> Option<Panel> {
        match self {
            Self::Generator => Some(Panel::Agent),
            Self::Fill => Some(Panel::Fill),
            Self::Stroke => Some(Panel::Stroke),
            Self::Corners => Some(Panel::Corners),
            Self::Filters => Some(Panel::Filter),
            Self::Brush => Some(Panel::Brush),
            Self::Layers => Some(Panel::Layers),
            _ => None,
        }
    }
}

fn photograph(path: &std::path::Path, variant: u32) {
    // Extract only photographic pixels from the recovered approved reference.
    // Actual production widgets paint every control; no reference UI is copied.
    let reference = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../design/image-workflow-2026-10-11/reference/fill-wide.png");
    image::open(reference)
        .expect("approved reference photograph")
        .crop_imm(95 + variant % 8 * 8, 84, 1690, 320)
        .resize(1000, 1000, image::imageops::FilterType::Triangle)
        .save(path)
        .expect("write local review photograph");
}

fn layer_source(path: &std::path::Path, index: usize) {
    let colors = [
        [45, 212, 191],
        [245, 92, 105],
        [230, 180, 53],
        [152, 99, 235],
        [112, 158, 200],
        [77, 85, 94],
    ];
    let [r, g, b] = colors[index];
    image::RgbaImage::from_fn(96, 96, |x, y| {
        let inside = (x as f32 - 48.0).powi(2) / 900.0
            + (y as f32 - 48.0).powi(2) / (400.0 + index as f32 * 80.0)
            < 1.0;
        image::Rgba([r, g, b, if inside { 255 } else { 0 }])
    })
    .save(path)
    .expect("transparent isolated layer fixture");
}

fn native_smoke_workbook(h: &Harness, item: slate_doc::ItemId) {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/review/image-workflow");
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("native-source.png");
    photograph(&source, 0);
    let mut doc = h.app.doc().clone();
    doc.items
        .iter_mut()
        .find(|entry| entry.id == item)
        .unwrap()
        .path = source.canonicalize().unwrap();
    doc.save_to(&dir.join("native-smoke.slate")).unwrap();
}

fn fixture(
    state: State,
    width: f32,
    height: f32,
) -> (Harness, NodeId, slate_doc::ItemId, egui::TexturesDelta) {
    let mut h = Harness::new("image_palette_review");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.settings.preview = settings::PreviewSettings::default();
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.app.set_board_tool(crate::app::board::BoardTool::Select);
    h.app.doc_mut().view.recent_colors = Some(vec![
        [45, 212, 191],
        [245, 92, 105],
        [230, 180, 53],
        [152, 99, 235],
        [112, 158, 200],
        [77, 85, 94],
    ]);
    h.app.board_colors.fg = slate_doc::scene::Rgba([45, 212, 191, 204]);
    h.app.brush_width = 24.0;
    h.app.brush_softness = 0.35;
    h.app.brush_opacity = 0.8;
    let source = h.base.join("review.png");
    photograph(&source, 0);
    let item = h.app.add_paths(std::slice::from_ref(&source))[0];
    let mut image = ImageNode::new(item);
    if state == State::Generator {
        image.agent = Some(Box::new(AgentPortalRef {
            provider: "comfy".into(),
            session: String::new(),
            context: Default::default(),
            channel: None,
            model: None,
            bundle: None,
            seed: Some(atlas_ai::agent::ImageOutput {
                id: "review-seed".into(),
                source: source.to_string_lossy().into_owned(),
                prompt: "Mountain lake at dusk".into(),
                ..Default::default()
            }),
            view: atlas_agent::PortalView::Images,
            chat: Default::default(),
            live: false,
            instruction: "Mountain lake at dusk, warm reflected light".into(),
            image: Default::default(),
        }));
    }
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-width / 2.0, -290.0, width, height),
        NodeKind::Image(image),
    );
    let host = node.id;
    h.app.add_nodes(vec![node]);
    let mut bootstrap_textures = egui::TexturesDelta::default();
    if state == State::Generator {
        let outputs = (1..=7)
            .map(|i| {
                let path = h.base.join(format!("history-{i}.png"));
                photograph(&path, i);
                atlas_ai::agent::ImageOutput {
                    id: format!("review-{i}"),
                    source: path.to_string_lossy().into_owned(),
                    request: format!("review-run-{i}"),
                    prompt: "Mountain lake at dusk".into(),
                    ..Default::default()
                }
            })
            .collect();
        // Let the real lifecycle establish this card's session binding first.
        bootstrap_textures = h.frame_output(|_| {}).textures_delta;
        h.app.agents.sessions.insert(
            host,
            std::sync::Arc::new(atlas_ai::agent::AgentSession {
                usage: None,
                approval: None,
                conversation: String::new(),
                artifacts: vec![],
                status: atlas_ai::agent::AgentStatus::Idle,
                provider: "comfy".into(),
                turns: vec![],
                updated_at: 0,
                bundle: atlas_ai::agent::ImageBundle { images: outputs },
                request: String::new(),
            }),
        );
        h.app.note_scene_change();
        assert_eq!(h.app.agent_images(host).len(), 8, "real history fixture");
    }
    if state == State::Layers {
        let result = h.app.image_result_id(host).expect("result identity");
        let children: Vec<_> = (0..6)
            .map(|index| {
                let path = h.base.join(format!("layer-{index}.png"));
                layer_source(&path, index);
                let layer_item = h.app.add_paths(&[path])[0];
                h.app.doc_mut().scene.build_node(
                    WorldRect::new(0.1, 0.1, 0.8, 0.8),
                    NodeKind::Image(ImageNode::new(layer_item)),
                )
            })
            .collect();
        h.app.patch_nodes(&[host], |node| {
            let NodeKind::Image(image) = &mut node.kind else {
                unreachable!()
            };
            let stack = ensure_result_layers_mut(image, result.clone());
            stack.layers = children
                .iter()
                .enumerate()
                .map(|(i, child)| {
                    let mut layer = PaintLayer::new(PaintLayerId(i as u64 + 1));
                    layer.opacity = if i == 2 { 0.8 } else { 1.0 };
                    layer.nodes.push(child.clone());
                    layer
                })
                .collect();
            stack.normalize_metadata();
            for (meta, name) in stack.metadata.iter_mut().zip([
                "Leaves",
                "Planting",
                "Tree",
                "Architecture",
                "Light",
                "Base",
            ]) {
                meta.name = name.into();
            }
        });
    }
    if state != State::Idle {
        h.app.board_sel.insert(host);
    }
    h.app.sync_shape_properties();
    h.app.shape_properties.panel = state.panel();
    (h, host, item, bootstrap_textures)
}

#[test]
#[ignore = "visual review: writes 27 real palette PNGs to target/review/image-workflow"]
fn capture_image_palettes() {
    for (size, width, height) in [
        ("narrow", 220.0, 260.0),
        ("square", 360.0, 360.0),
        ("wide", 680.0, 300.0),
    ] {
        for state in [
            State::Idle,
            State::Collapsed,
            State::Generator,
            State::Fill,
            State::Stroke,
            State::Corners,
            State::Filters,
            State::Brush,
            State::Layers,
        ] {
            let (mut h, host, item, bootstrap_textures) = fixture(state, width, height);
            let mut raster = FrameRaster::new(1440, 900);
            raster.apply(&bootstrap_textures);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut frame = 0;
            let out = loop {
                frame += 1;
                let out = capture_frame(&mut h, &mut raster, |input| {
                    input.time = Some(frame as f64 / 60.0);
                    input
                        .events
                        .push(egui::Event::PointerMoved(Pos2::new(20.0, 750.0)));
                });
                let loaded = h.app.item_texture(item, width).is_some();
                if state == State::Layers && frame == 8 {
                    if let Some(session) = h.app.image_paint.as_mut() {
                        session.layer_index = 2;
                        session.focus = crate::app::board_image_layers::ImageStripFocus::Layer(2);
                    }
                }
                if frame >= 36 && loaded {
                    break out;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "review texture did not load"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            assert!(h.app.doc().scene.node(host).is_some());
            if state == State::Generator {
                assert_eq!(
                    h.app.agent_images(host).len(),
                    8,
                    "history survived session binding"
                );
            }
            assert_eq!(
                h.app.shape_properties.panel,
                state.panel(),
                "fixture panel remained open"
            );
            if state != State::Idle {
                assert!(
                    !h.app.shape_properties.chrome_hits.is_empty(),
                    "actual palette produced hit regions"
                );
            }
            let name = format!("{size}_{}", state.name());
            review_shot(&mut h, &mut raster, out, "image-workflow", &name);
            if size == "wide" && state == State::Idle {
                native_smoke_workbook(&h, item);
            }
            eprintln!("captured {name}.png");
        }
    }
}

#[test]
#[ignore = "visual review: writes six production-widget light/dark interaction PNGs"]
fn capture_image_interaction_chrome() {
    for dark_mode in [false, true] {
        for state in [State::Collapsed, State::Fill, State::Brush] {
            let (mut h, host, item, textures) = fixture(state, 680.0, 300.0);
            h.app.dark_mode = dark_mode;
            h.app.patch_nodes(&[host], |node| {
                if let NodeKind::Image(image) = &mut node.kind {
                    image.corner = slate_doc::scene::Corner::Rounded { radius: 48.0 };
                }
            });
            if state == State::Brush {
                h.app.arm_image_brush(host);
            }
            let mut raster = FrameRaster::new(1440, 900);
            raster.apply(&textures);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            let mut frame = 0;
            let out = loop {
                frame += 1;
                let out = capture_frame(&mut h, &mut raster, |input| {
                    input.time = Some(frame as f64 / 60.0);
                    input
                        .events
                        .push(egui::Event::PointerMoved(Pos2::new(20.0, 750.0)));
                });
                if frame >= 36 && h.app.item_texture(item, 680.0).is_some() {
                    break out;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "review texture did not load"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            let mode = if dark_mode { "dark" } else { "light" };
            let name = format!("interaction_{mode}_{}", state.name());
            review_shot(&mut h, &mut raster, out, "image-workflow", &name);
            eprintln!("captured {name}.png");
        }
    }
}

fn text_center(shape: &egui::Shape, label: &str) -> Option<Pos2> {
    match shape {
        egui::Shape::Text(text) if text.galley.text() == label => {
            Some(text.pos + text.galley.rect.center().to_vec2())
        }
        egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_center(shape, label)),
        _ => None,
    }
}

#[test]
#[ignore = "visual review: captures the real generator dropdowns and Play hover"]
fn capture_generator_dropdowns() {
    for (name, label, expected) in [
        ("model", "ComfyUI · Auto", "ComfyUI · Auto"),
        ("aspect", "Source", "16:9"),
        ("mode", "Single", "Live"),
        ("settings", "Settings", "Seed locked"),
        ("play_hover", "", "1"),
    ] {
        let (mut h, host, _, bootstrap_textures) = fixture(State::Generator, 680.0, 300.0);
        let mut raster = FrameRaster::new(1440, 900);
        raster.apply(&bootstrap_textures);
        let mut frame = 0;
        let mut out = capture_frame(&mut h, &mut raster, |_| {});
        for _ in 0..48 {
            frame += 1;
            out = capture_frame(&mut h, &mut raster, |input| {
                input.time = Some(frame as f64 / 60.0);
                input
                    .events
                    .push(egui::Event::PointerMoved(Pos2::new(20.0, 750.0)));
            });
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let point = if label.is_empty() {
            let node = h.app.doc().scene.node(host).unwrap();
            let layout = atlas_shell::selection_tools::image_presentation_layout(
                h.app.board_xf().rect_w2s(node.rect),
                1.0,
                atlas_shell::selection_tools::ImagePanelKind::Custom {
                    main_height: h.app.image_agent_body_height(
                        &h.ctx.layer_painter(egui::LayerId::background()),
                        host,
                    ),
                },
            );
            Pos2::new(layout.main.right() - 22.0, layout.main.top() + 24.0)
        } else {
            out.shapes
                .iter()
                .find_map(|shape| text_center(&shape.shape, label))
                .unwrap_or_else(|| panic!("missing generator field {label}"))
        };
        for pressed in [true, false] {
            if label.is_empty() {
                break;
            }
            frame += 1;
            out = capture_frame(&mut h, &mut raster, |input| {
                input.time = Some(frame as f64 / 60.0);
                input.events.push(egui::Event::PointerMoved(point));
                input.events.push(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                });
            });
        }
        for _ in 0..8 {
            frame += 1;
            out = capture_frame(&mut h, &mut raster, |input| {
                input.time = Some(frame as f64 / 60.0);
                input.events.push(egui::Event::PointerMoved(point));
            });
        }
        assert!(
            out.shapes
                .iter()
                .any(|shape| text_center(&shape.shape, expected).is_some()),
            "{name} did not display expected option {expected}"
        );
        review_shot(
            &mut h,
            &mut raster,
            out,
            "image-workflow",
            &format!("wide_generator_{name}"),
        );
        eprintln!("captured wide_generator_{name}.png");
    }
}
