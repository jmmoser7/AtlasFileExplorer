use super::*;

#[derive(Clone, Copy)]
enum Contents {
    Launch(bool),
    Active(Phase),
}

impl Contents {
    fn height(self, width: f32) -> f32 {
        match self {
            Self::Launch(video) => launch_height(video, width),
            Self::Active(phase) => active_height(phase, width),
        }
    }
}

fn draw(
    ctx: &egui::Context,
    contents: Contents,
    width: f32,
    zoom: f32,
    pan: Vec2,
    dark: bool,
    events: Vec<egui::Event>,
) -> (Response, egui::FullOutput) {
    let mut response = None;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(4096.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let transform = egui::emath::TSTransform::new(pan, zoom);
                    let host = transform
                        * Rect::from_min_size(Pos2::new(80.0, 60.0), Vec2::new(width, 180.0));
                    let layout = image_presentation_layout(
                        host,
                        zoom,
                        ImagePanelKind::Custom {
                            main_height: contents.height(width),
                        },
                    );
                    let rect = layout.editor_rect(zoom);
                    response = Some(match contents {
                        Contents::Launch(video) => {
                            launch(ui, rect, video, zoom, Palette::for_mode(dark))
                        }
                        Contents::Active(phase) => {
                            active(ui, rect, phase, 65.5, zoom, Palette::for_mode(dark))
                        }
                    });
                });
        },
    );
    (response.unwrap(), output)
}

fn near(actual: Rect, expected: Rect) {
    assert!(
        (actual.min - expected.min).length() < 0.002,
        "{actual:?} != {expected:?}"
    );
    assert!(
        (actual.max - expected.max).length() < 0.002,
        "{actual:?} != {expected:?}"
    );
}

fn contents() -> [Contents; 6] {
    [
        Contents::Launch(false),
        Contents::Launch(true),
        Contents::Active(Phase::Starting),
        Contents::Active(Phase::Recording),
        Contents::Active(Phase::Paused),
        Contents::Active(Phase::Saving),
    ]
}

#[test]
fn capture_apron_and_real_controls_follow_the_host_camera_at_every_review_width() {
    for dark in [false, true] {
        for width in [220.0, 360.0, 680.0] {
            for contents in contents() {
                let (base, _) = draw(
                    &egui::Context::default(),
                    contents,
                    width,
                    1.0,
                    Vec2::ZERO,
                    dark,
                    vec![],
                );
                for zoom in [0.5, 1.0, 2.0] {
                    let pan = Vec2::new(170.0, 230.0);
                    let transform = egui::emath::TSTransform::new(pan, zoom);
                    let (moved, _) = draw(
                        &egui::Context::default(),
                        contents,
                        width,
                        zoom,
                        pan,
                        dark,
                        vec![],
                    );
                    near(moved.bounds(), transform * base.bounds());
                    assert_eq!(moved.controls().count(), base.controls().count());
                    for ((action, hit), (base_action, base_hit)) in
                        moved.controls().zip(base.controls())
                    {
                        assert_eq!(action, base_action);
                        near(*hit, transform * *base_hit);
                        assert!(moved
                            .bounds()
                            .shrink(IMAGE_PANEL_INSET * zoom - 0.001)
                            .contains_rect(*hit));
                        assert!((hit.height() / zoom - CORNER_HEIGHT).abs() < 0.001);
                    }
                    for (i, (_, hit)) in moved.controls().enumerate() {
                        for (_, other) in moved.controls().skip(i + 1) {
                            assert!(!hit.intersect(*other).is_positive());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn narrow_paused_hosts_use_two_action_rows_while_wide_hosts_keep_one() {
    for (width, row_count) in [(220.0, 2), (360.0, 2), (680.0, 1)] {
        let (response, _) = draw(
            &egui::Context::default(),
            Contents::Active(Phase::Paused),
            width,
            1.0,
            Vec2::ZERO,
            false,
            vec![],
        );
        let controls: Vec<_> = response.controls().collect();
        assert_eq!(controls.len(), 4);
        let first_y = controls[0].1.top();
        let last_y = controls[3].1.top();
        assert_eq!(usize::from(last_y > first_y) + 1, row_count);
        if row_count == 2 {
            assert_eq!(controls[0].1.top(), controls[1].1.top());
            assert_eq!(controls[2].1.top(), controls[3].1.top());
            assert!((last_y - first_y - CORNER_HEIGHT - STACK_GAP).abs() < 0.001);
        }
    }
}

#[test]
fn every_visible_capture_action_is_a_real_click_at_all_review_scales_and_themes() {
    for dark in [false, true] {
        for width in [220.0, 360.0, 680.0] {
            for zoom in [0.5, 1.0, 2.0] {
                for contents in contents() {
                    let ctx = egui::Context::default();
                    let pan = Vec2::new(40.0, 60.0);
                    draw(&ctx, contents, width, zoom, pan, dark, vec![]);
                    let (initial, _) = draw(&ctx, contents, width, zoom, pan, dark, vec![]);
                    for &(action, target) in initial.controls() {
                        let point = target.center();
                        let event = |pressed| egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        };
                        draw(
                            &ctx,
                            contents,
                            width,
                            zoom,
                            pan,
                            dark,
                            vec![egui::Event::PointerMoved(point), event(true)],
                        );
                        let (clicked, _) =
                            draw(&ctx, contents, width, zoom, pan, dark, vec![event(false)]);
                        assert_eq!(
                            clicked.action,
                            Some(action),
                            "width={width}, zoom={zoom}, dark={dark}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn transport_paints_scaled_status_and_segments_without_an_outer_capsule_or_tray() {
    for dark in [false, true] {
        for zoom in [0.5, 1.0, 2.0] {
            let (_, output) = draw(
                &egui::Context::default(),
                Contents::Active(Phase::Recording),
                680.0,
                zoom,
                Vec2::ZERO,
                dark,
                vec![],
            );
            let mut circles = 0;
            let mut borders = 0;
            for clipped in output.shapes {
                match clipped.shape {
                    egui::Shape::Circle(circle) => {
                        assert!((circle.radius / zoom - 2.6).abs() < 0.001);
                        circles += 1;
                    }
                    egui::Shape::Rect(rect)
                        if rect.rect.is_positive()
                            && (rect.stroke.width > 0.0 || rect.fill != Color32::TRANSPARENT) =>
                    {
                        assert_eq!(
                            rect.fill,
                            Color32::TRANSPARENT,
                            "the caller owns the solid apron surface"
                        );
                        assert!((rect.stroke.width / zoom - 0.8).abs() < 0.001);
                        borders += 1;
                    }
                    _ => {}
                }
            }
            assert_eq!(circles, 1);
            assert_eq!(
                borders, 1,
                "only the shared segmented controls draw a border"
            );
        }
    }
}

#[test]
fn hidden_capture_controls_have_no_hit_regions_at_the_shared_lod() {
    for contents in contents() {
        let (response, _) = draw(
            &egui::Context::default(),
            contents,
            360.0,
            0.1,
            Vec2::ZERO,
            false,
            vec![],
        );
        assert!(response.rects().is_empty());
        assert_eq!(response.controls().count(), 0);
        assert_eq!(response.action, None);
    }
}

#[test]
fn phases_keep_explicit_feedback_and_share_the_elapsed_time() {
    assert_eq!(elapsed_label(Phase::Recording, 65.5), "Recording 01:05");
    assert_eq!(elapsed_label(Phase::Paused, 65.5), "Paused 01:05");
    assert_eq!(elapsed_label(Phase::Starting, 0.0), "Starting… 00:00");
    assert_eq!(elapsed_label(Phase::Saving, 65.5), "Saving… 01:05");
    assert_eq!(elapsed_label(Phase::Paused, f32::NAN), "Paused 00:00");
}
