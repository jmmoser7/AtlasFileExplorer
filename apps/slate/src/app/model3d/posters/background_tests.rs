use super::*;

/// Repeat the native auto-lock path against the packaged software provider.
/// The small Rhino fixture is repeated to cross the large-model threshold.
#[test]
#[ignore = "requires the packaged llvmpipe provider and a Windows graphics context"]
fn auto_lock_holds_the_display_and_publishes_only_the_finished_poster() {
    struct Close;
    impl eframe::App for Close {
        fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default().with_visible(false),
        event_loop_builder: Some(Box::new(|builder| {
            #[cfg(windows)]
            {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                builder.with_any_thread(true);
            }
            #[cfg(not(windows))]
            let _ = builder;
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Slate poster refinement",
        options,
        Box::new(|cc| {
            check_auto_lock(cc.gl.clone().expect("GL context"));
            Ok(Box::new(Close))
        }),
    )
    .unwrap();
}

fn check_auto_lock(gl: Arc<glow::Context>) {
    let (mut h, id) = crate::app::model3d::tests::live_model("software_auto_lock");
    h.app.gl = Some(gl.clone());
    let info = h.app.model_node_info(id).unwrap();
    let mut scene =
        model_preview::load_preview(&info.path, &std::fs::read(&info.path).unwrap()).unwrap();
    let indices = &mut scene.meshes[0].indices;
    *indices = indices.repeat(75003usize.div_ceil(indices.len()));
    h.app.model3d.models.insert(
        info.cache_key.clone(),
        CpuEntry {
            state: ModelState::Ready(Arc::new(scene)),
            last_used: Instant::now(),
            progress: Arc::new(ParseProgress::new()),
        },
    );
    let doc = h.app.tab().id;
    let vp = h.app.model3d.live.get_mut(&id).unwrap();
    vp.tex = None;
    vp.rendered = None;
    vp.cam.yaw += 0.19;
    let cam = vp.cam;
    let adjust = ImageAdjust::default();
    let aspect = info.rect.w / info.rect.h.max(1.0);
    for _ in 0..10000 {
        h.app.frame_no += 1;
        h.app
            .model_live_texture(&h.ctx, id, 192.0 * aspect, 192.0, &adjust);
        if !h.app.model3d.live[&id].navigating_frame {
            break;
        }
    }
    assert!(
        h.app.model3d.software_refines(&info.cache_key),
        "use the packaged Mesa provider"
    );
    assert!(!h.app.model3d.live[&id].navigating_frame);
    let held = egui::TextureId::User(7);
    h.app.model3d.live_slots[0].target.egui_id = Some(held);
    let displayed = h.app.model3d.live_frame_pixels(id).unwrap();
    h.app.model3d.live.get_mut(&id).unwrap().last_interact =
        Instant::now() - AUTO_LOCK - Duration::from_secs(1);
    h.app.frame_no += 1;
    let reads = h.app.model3d.gl_counts().1;
    h.app.model3d_frame(&h.ctx);
    let info = h.app.model_node_info(id).unwrap();
    assert_eq!(info.cam, cam);
    assert!(!h.app.model3d.live.contains_key(&id));
    assert_eq!(
        h.app.model3d.gl_counts().1,
        reads,
        "auto-lock must yield before a poster readback"
    );
    assert_eq!(h.app.model3d.active_poster(), Some((doc, id)));
    assert_eq!(
        h.app.model3d.held_model_texture(doc, &info, &adjust),
        Some(held)
    );
    assert_eq!(h.app.model3d.live_frame_pixels(id).unwrap(), displayed);

    // A new pose invalidates the partially drawn old camera before publication.
    let mut moved = cam;
    moved.yaw += 0.1;
    h.app.patch_nodes(&[id], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.model = moved
        }
    });
    let name = poster_file_name(&info.cache_key, &moved, aspect_q(info.rect.w, info.rect.h));
    for _ in 0..20000 {
        h.app.frame_no += 1;
        h.app.model3d_frame(&h.ctx);
        if !h.app.model3d.want_poster.contains_key(&(doc, id)) {
            break;
        }
    }
    let poster = h
        .app
        .model3d
        .cached_poster_pixels(&name)
        .expect("latest pose must finish");
    assert_eq!(h.app.model3d.active_poster(), None);
    assert_eq!(h.app.model3d.live_texture_id(id), None);
    let (w, hgt) = poster_size(aspect_q(info.rect.w, info.rect.h));
    let direct = h
        .app
        .model3d
        .render_image(&gl, &info.cache_key, &moved, w, hgt, None)
        .unwrap();
    assert_eq!(*poster, direct);
}
