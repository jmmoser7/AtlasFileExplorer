//! Reproducible local-only workbook for the media refinement native audit.
use super::*;
use slate_doc::scene::{AgentPortalRef, ImageNode, PortalNode, SourceUri};

#[test]
#[ignore = "native review: creates target/review/media-refinement/media-refinement.slate"]
fn media_refinement_native_fixture() {
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
    let mut hosts = Vec::new();
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
        hosts.push(node.id);
        h.app.add_nodes(vec![node]);
    }
    let wire = h.app.build_connector(
        slate_doc::scene::ConnectorEnd::Anchored {
            node: hosts[0],
            side: slate_doc::scene::Side::Right,
            t: slate_doc::agent_inputs::OUTPUT_T,
        },
        slate_doc::scene::ConnectorEnd::Anchored {
            node: hosts[1],
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
    h.app.add_nodes(vec![text_node]);
    let obj_path = dir.join("pyramid.obj");
    std::fs::write(&obj_path, "v -1 0 -1\nv 1 0 -1\nv 1 0 1\nv -1 0 1\nv 0 2 0\nf 1 2 5\nf 2 3 5\nf 3 4 5\nf 4 1 5\nf 4 3 2 1\n").unwrap();
    let item = ready_item(&mut h.app, obj_path);
    let model = h.app.doc_mut().scene.build_node(
        WorldRect::new(-230.0, 250.0, 420.0, 260.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    h.app.add_nodes(vec![model]);
    let html_path = dir.join("dashboard.html");
    std::fs::write(&html_path, "<!doctype html><html><body style='margin:0;background:#204b59;color:white;font:32px sans-serif;padding:24px'><h1>Local media audit</h1><p>Portal silhouette fixture</p></body></html>").unwrap();
    let portal = PortalNode::bound_web("Web portal", html_path.to_string_lossy());
    let portal_node = h.app.doc_mut().scene.build_node(
        WorldRect::new(300.0, 250.0, 420.0, 260.0),
        NodeKind::Portal(portal),
    );
    h.app.add_nodes(vec![portal_node]);
    let mut artifact = PortalNode::bound_web("Artifact generator", html_path.to_string_lossy());
    artifact.agent = Some(Box::new(make_agent("text")));
    artifact.web.as_mut().unwrap().artifact = Some(Default::default());
    let artifact_node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-760.0, 650.0, 420.0, 260.0),
        NodeKind::Portal(artifact),
    );
    h.app.add_nodes(vec![artifact_node]);
    let mut atlas = PortalNode::unbound_file_atlas("File Atlas portal");
    atlas.source = Some(SourceUri {
        locator: dir.to_string_lossy().into_owned(),
    });
    let atlas_node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-230.0, 650.0, 420.0, 260.0),
        NodeKind::Portal(atlas),
    );
    h.app.add_nodes(vec![atlas_node]);
    let video_item = ready_item(&mut h.app, video_path);
    let video = h.app.doc_mut().scene.build_node(
        WorldRect::new(300.0, 650.0, 420.0, 260.0),
        NodeKind::Image(ImageNode::new(video_item)),
    );
    h.app.add_nodes(vec![video]);
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
    h.app.add_nodes(vec![pdf]);
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
    h.app.add_nodes(vec![legacy_text]);
    let doc = &mut h.app.tab_mut().doc;
    doc.view.active_view = ViewKind::Board;
    doc.view.zoom = 1.0;
    doc.view.cam_x = -120.0;
    doc.view.cam_y = -30.0;
    doc.save_to(&dir.join("media-refinement.slate")).unwrap();
    println!("Native media fixture: {}", dir.display());
}

fn ready_item(app: &mut SlateApp, path: std::path::PathBuf) -> slate_doc::ItemId {
    let ready = crate::app::items::ReadyFile::read(path).unwrap();
    assert!(ready.size > 0 && ready.mtime > 0);
    app.item_for_ready_path(&ready.path, ready.size, ready.mtime)
}
