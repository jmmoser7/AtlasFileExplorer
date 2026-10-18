//! Reproducible local-only workbook for the media refinement native audit.
use super::*;
use crate::app::board_properties::Panel;
use slate_doc::scene::{AgentPortalRef, ImageNode, NodeId, PortalNode, SourceUri};

#[test]
#[ignore = "native review: creates target/review/media-refinement/media-refinement.slate"]
fn media_refinement_native_fixture() {
    let fixture = media_refinement_fixture(true);
    println!("Native media fixture: {}", fixture.dir.display());
}

#[derive(Clone, Copy)]
struct MediaFixtureHosts {
    generated_text: NodeId,
    model: NodeId,
    web: NodeId,
    artifact: NodeId,
    atlas: NodeId,
    video: NodeId,
    pdf: NodeId,
    legacy_text: NodeId,
}

struct MediaRefinementFixture {
    h: Harness,
    dir: std::path::PathBuf,
    hosts: MediaFixtureHosts,
    video_item: slate_doc::ItemId,
    pdf_item: slate_doc::ItemId,
}

fn media_refinement_fixture(save_workbook: bool) -> MediaRefinementFixture {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let canonical_dir = root.join("target/review/media-refinement");
    let dir = std::env::var_os("SLATE_MEDIA_REVIEW_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| canonical_dir.clone());
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let video_path = dir.join("sample.mp4");
    if !video_path.exists() && dir != canonical_dir.canonicalize().unwrap() {
        std::fs::copy(canonical_dir.join("sample.mp4"), &video_path).unwrap();
    }
    assert!(
        video_path.is_file(),
        "Run media_refinement_video_fixture first"
    );
    let reference =
        image::open(root.join("design/media-ui-2026-10-17/reference/media-overview.png"))
            .unwrap()
            .crop_imm(335, 135, 1390, 755)
            .to_rgba8();
    let mut h = Harness::new("media_refinement_native");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.tab_mut().doc = slate_doc::SlateDoc::new("Media refinement audit");
    let mut outputs = Vec::new();
    let mut items = Vec::new();
    for index in 0..5 {
        let path = dir.join(format!("render-scene-{:02}.png", index + 1));
        let mut pixels = reference.clone();
        for pixel in pixels.pixels_mut() {
            pixel.0[index % 3] = pixel.0[index % 3].saturating_add(index as u8 * 9);
        }
        pixels.save(&path).unwrap();
        items.push(ready_item(&mut h.app, path.clone()));
        outputs.push(atlas_agent::ImageOutput {
            id: format!("review-result-{index}"),
            source: path.to_string_lossy().into_owned(),
            request: if index < 3 {
                "review-run-a"
            } else {
                "review-run-b"
            }
            .into(),
            prompt: format!("Local fixture result {}", index + 1),
            ..Default::default()
        });
    }
    let session = atlas_agent::AgentSession {
        provider: "comfy".into(),
        bundle: atlas_agent::ImageBundle {
            images: outputs.clone(),
        },
        ..Default::default()
    };
    let session_path = dir.join("session.json");
    std::fs::write(&session_path, serde_json::to_vec_pretty(&session).unwrap()).unwrap();
    let session_id = format!("{}-review", dir.file_name().unwrap().to_string_lossy());
    let make_agent = |view: &str| -> AgentPortalRef {
        serde_json::from_value(serde_json::json!({
            "provider":"comfy", "session":session_id, "view":view,
            "bundle": SourceUri { locator: session_path.to_string_lossy().into_owned() },
            "seed": outputs[0], "instruction":"Render scene with warm reflected light"
        }))
        .unwrap()
    };
    let mut generated_images = Vec::new();
    for (index, (x, y, w, height)) in [
        (-760.0, -400.0, 680.0, 380.0),
        (100.0, -400.0, 460.0, 520.0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut image = ImageNode::new(items[index]);
        image.corner = slate_doc::scene::Corner::Rounded { radius: 18.0 };
        image.agent = Some(Box::new(make_agent("images")));
        let node = h
            .app
            .doc_mut()
            .scene
            .build_node(WorldRect::new(x, y, w, height), NodeKind::Image(image));
        generated_images.push(node.id);
        h.app.add_nodes(vec![node]);
    }
    let wire = h.app.build_connector(
        slate_doc::scene::ConnectorEnd::Anchored {
            node: generated_images[0],
            side: slate_doc::scene::Side::Right,
            t: slate_doc::agent_inputs::OUTPUT_T,
        },
        slate_doc::scene::ConnectorEnd::Anchored {
            node: generated_images[1],
            side: slate_doc::scene::Side::Left,
            t: 0.25,
        },
    );
    h.app.add_nodes(vec![wire]);
    let text_path = dir.join("generated.txt");
    std::fs::write(
        &text_path,
        "Generated text fixture\n\nA local media audit. No model request is sent.\n",
    )
    .unwrap();
    let text_item = ready_item(&mut h.app, text_path);
    let mut text = ImageNode::new(text_item);
    text.agent = Some(Box::new(make_agent("text")));
    let text_node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-760.0, 250.0, 420.0, 260.0),
        NodeKind::Image(text),
    );
    let generated_text = h.app.add_nodes(vec![text_node])[0];
    let obj_path = dir.join("pyramid.obj");
    std::fs::write(&obj_path, "v -1 0 -1\nv 1 0 -1\nv 1 0 1\nv -1 0 1\nv 0 2 0\nf 1 2 5\nf 2 3 5\nf 3 4 5\nf 4 1 5\nf 4 3 2 1\n").unwrap();
    let item = ready_item(&mut h.app, obj_path);
    let model = h.app.doc_mut().scene.build_node(
        WorldRect::new(-230.0, 250.0, 420.0, 260.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let model = h.app.add_nodes(vec![model])[0];
    let html_path = dir.join("dashboard.html");
    std::fs::write(&html_path, "<!doctype html><html><body style='margin:0;background:#204b59;color:white;font:32px sans-serif;padding:24px'><h1>Local media audit</h1><p>Portal silhouette fixture</p></body></html>").unwrap();
    let portal = PortalNode::bound_web("Web portal", html_path.to_string_lossy());
    let portal_node = h.app.doc_mut().scene.build_node(
        WorldRect::new(300.0, 250.0, 420.0, 260.0),
        NodeKind::Portal(portal),
    );
    let web = h.app.add_nodes(vec![portal_node])[0];
    let mut artifact = PortalNode::bound_web("Artifact generator", html_path.to_string_lossy());
    artifact.agent = Some(Box::new(make_agent("text")));
    artifact.web.as_mut().unwrap().artifact = Some(Default::default());
    let artifact_node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-760.0, 650.0, 420.0, 260.0),
        NodeKind::Portal(artifact),
    );
    let artifact = h.app.add_nodes(vec![artifact_node])[0];
    let mut atlas = PortalNode::unbound_file_atlas("File Atlas portal");
    atlas.source = Some(SourceUri {
        locator: dir.to_string_lossy().into_owned(),
    });
    let atlas_node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-230.0, 650.0, 420.0, 260.0),
        NodeKind::Portal(atlas),
    );
    let atlas = h.app.add_nodes(vec![atlas_node])[0];
    let video_item = ready_item(&mut h.app, video_path);
    let video = h.app.doc_mut().scene.build_node(
        WorldRect::new(300.0, 650.0, 420.0, 260.0),
        NodeKind::Image(ImageNode::new(video_item)),
    );
    let video = h.app.add_nodes(vec![video])[0];
    let ordinary = h.app.doc_mut().scene.build_node(
        WorldRect::new(700.0, -400.0, 680.0, 380.0),
        NodeKind::Image(ImageNode::new(items[0])),
    );
    h.app.add_nodes(vec![ordinary]);
    let pdf_path = dir.join("pages.pdf");
    std::fs::write(&pdf_path, crate::app::tests::fixture_pdf(2)).unwrap();
    let pdf_item = ready_item(&mut h.app, pdf_path);
    let pdf = h.app.doc_mut().scene.build_node(
        WorldRect::new(850.0, 250.0, 420.0, 260.0),
        NodeKind::Image(ImageNode::new(pdf_item)),
    );
    let pdf = h.app.add_nodes(vec![pdf])[0];
    let legacy_text = h.app.doc_mut().scene.build_node(
        WorldRect::new(850.0, 650.0, 420.0, 260.0),
        NodeKind::Text(slate_doc::scene::TextNode {
            text: "Legacy generated TextNode\n\nShared notch, shadow, and Agent controls.".into(),
            style: Default::default(),
            family: Default::default(),
            size: 18.0,
            color: slate_doc::scene::Rgba::BLACK,
            align: Default::default(),
            fill: Some(slate_doc::scene::Rgba::WHITE),
            stroke: slate_doc::scene::Stroke::none(),
            agent: Some(Box::new(make_agent("text"))),
        }),
    );
    let legacy_text = h.app.add_nodes(vec![legacy_text])[0];
    let doc = &mut h.app.tab_mut().doc;
    doc.view.active_view = ViewKind::Board;
    doc.view.zoom = 1.0;
    doc.view.cam_x = -120.0;
    doc.view.cam_y = -30.0;
    if save_workbook {
        doc.save_to(&dir.join("media-refinement.slate")).unwrap();
    }
    MediaRefinementFixture {
        h,
        dir,
        hosts: MediaFixtureHosts {
            generated_text,
            model,
            web,
            artifact,
            atlas,
            video,
            pdf,
            legacy_text,
        },
        video_item,
        pdf_item,
    }
}

fn ready_item(app: &mut SlateApp, path: std::path::PathBuf) -> slate_doc::ItemId {
    let ready = crate::app::items::ReadyFile::read(path).unwrap();
    assert!(ready.size > 0 && ready.mtime > 0);
    app.item_for_ready_path(&ready.path, ready.size, ready.mtime)
}

#[test]
#[ignore = "visual review: Slate-owned media panels; native WebView2/GPU payload needs native capture"]
fn capture_media_refinement_panels() {
    let mut fixture = media_refinement_fixture(false);
    let mut raster = FrameRaster::new(1440, 900);
    let mut frame = 0;
    let hosts = fixture.hosts;
    for (id, name, panel, texture) in [
        (hosts.generated_text, "generated_text", Panel::Agent, None),
        (hosts.legacy_text, "legacy_text", Panel::Agent, None),
        (hosts.artifact, "artifact", Panel::Agent, None),
        (hosts.model, "model", Panel::ModelDisplay, None),
        (hosts.web, "web", Panel::Fill, None),
        (hosts.atlas, "atlas", Panel::AtlasFormat, None),
        (
            hosts.video,
            "video",
            Panel::Filter,
            Some(fixture.video_item),
        ),
        (hosts.pdf, "pdf", Panel::Pages, Some(fixture.pdf_item)),
    ] {
        capture_media_class(
            &mut fixture,
            &mut raster,
            &mut frame,
            id,
            name,
            panel,
            texture,
        );
    }
}

fn capture_media_class(
    fixture: &mut MediaRefinementFixture,
    raster: &mut FrameRaster,
    frame: &mut u64,
    id: NodeId,
    name: &str,
    panel: Panel,
    texture: Option<slate_doc::ItemId>,
) {
    fixture.h.app.shape_properties.panel = None;
    fixture.h.app.board_sel.clear();
    fixture.h.app.board_sel.insert(id);
    fixture.h.app.sync_shape_properties();
    *frame += 1;
    let frame_number = *frame;
    let _ = capture_frame(&mut fixture.h, raster, |input| {
        input.time = Some(frame_number as f64 / 60.0);
    });
    let rect = fixture.h.app.doc().scene.node(id).unwrap().rect;
    let canvas = fixture.h.app.canvas_rect;
    let (center_x, _) = rect.center();
    let host_top = canvas.min.y + canvas.height() * 0.14;
    let cam = &mut fixture.h.app.tab_mut().cam;
    cam.z = 1.0;
    cam.offset = egui::Vec2::new(center_x, rect.y + canvas.center().y - host_top);
    if panel == Panel::ModelDisplay {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !fixture.h.app.model_has_viewport(id) {
            *frame += 1;
            let frame_number = *frame;
            let _ = capture_frame(&mut fixture.h, raster, |input| {
                input.time = Some(frame_number as f64 / 60.0);
            });
            assert!(
                std::time::Instant::now() < deadline,
                "model fixture did not produce the registered viewport panel"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    assert!(fixture.h.app.board_sel.contains(&id), "{name} is selected");
    assert_eq!(
        fixture.h.app.shape_properties.panel, None,
        "{name} starts collapsed"
    );
    let collapsed = capture_media_frame(fixture, raster, frame, texture);
    assert!(
        !fixture.h.app.shape_properties.chrome_hits.is_empty(),
        "{name} collapsed chrome has hit regions"
    );
    assert_visible_media_chrome(&fixture.h.app, id, name);
    review_shot(
        &mut fixture.h,
        raster,
        collapsed,
        "media-refinement",
        &format!("{name}_collapsed"),
    );
    eprintln!("captured {name}_collapsed.png");

    assert!(
        fixture.h.app.open_object_panel(panel),
        "{name} registers {panel:?}"
    );
    assert_eq!(fixture.h.app.shape_properties.panel, Some(panel));
    let opened = capture_media_frame(fixture, raster, frame, texture);
    assert_eq!(fixture.h.app.shape_properties.panel, Some(panel));
    assert!(
        !fixture.h.app.shape_properties.chrome_hits.is_empty(),
        "{name} {panel:?} chrome has hit regions"
    );
    assert_visible_media_chrome(&fixture.h.app, id, name);
    review_shot(
        &mut fixture.h,
        raster,
        opened,
        "media-refinement",
        &format!("{name}_{}", panel_state(panel)),
    );
    eprintln!("captured {name}_{}.png", panel_state(panel));
}

fn assert_visible_media_chrome(app: &SlateApp, id: NodeId, name: &str) {
    let canvas = app.canvas_rect;
    let node = app.doc().scene.node(id).expect("captured host");
    let host = app
        .board_xf()
        .rect_w2s(node.rect.rotated_bounds(node.rotation_deg));
    assert!(
        canvas.contains_rect(host),
        "{name} host is fully visible on canvas"
    );
    assert!(
        app.shape_properties
            .chrome_hits
            .iter()
            .all(|hit| canvas.contains_rect(*hit)),
        "all {name} chrome hit regions are fully visible on canvas"
    );
}

fn panel_state(panel: Panel) -> &'static str {
    match panel {
        Panel::Agent => "agent",
        Panel::ModelDisplay => "model_display",
        Panel::Fill => "fill",
        Panel::AtlasFormat => "atlas_format",
        Panel::Filter => "filter",
        Panel::Pages => "pages",
        _ => unreachable!("review captures name their expected panel"),
    }
}

fn capture_media_frame(
    fixture: &mut MediaRefinementFixture,
    raster: &mut FrameRaster,
    frame: &mut u64,
    texture: Option<slate_doc::ItemId>,
) -> egui::FullOutput {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut out = None;
    for _ in 0..36 {
        *frame += 1;
        let frame_number = *frame;
        out = Some(capture_frame(&mut fixture.h, raster, |input| {
            input.time = Some(frame_number as f64 / 60.0);
            input
                .events
                .push(egui::Event::PointerMoved(Pos2::new(20.0, 850.0)));
        }));
    }
    if let Some(item) = texture {
        while fixture.h.app.item_texture(item, 420.0).is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "required media texture did not load"
            );
            *frame += 1;
            let frame_number = *frame;
            let _ = capture_frame(&mut fixture.h, raster, |input| {
                input.time = Some(frame_number as f64 / 60.0);
                input
                    .events
                    .push(egui::Event::PointerMoved(Pos2::new(20.0, 850.0)));
            });
            if fixture.h.app.item_texture(item, 420.0).is_none() {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        *frame += 1;
        let frame_number = *frame;
        out = Some(capture_frame(&mut fixture.h, raster, |input| {
            input.time = Some(frame_number as f64 / 60.0);
            input
                .events
                .push(egui::Event::PointerMoved(Pos2::new(20.0, 850.0)));
        }));
        assert!(
            fixture.h.app.item_texture(item, 420.0).is_some(),
            "required media texture remains loaded in the captured frame"
        );
    }
    out.expect("review frame captured")
}
