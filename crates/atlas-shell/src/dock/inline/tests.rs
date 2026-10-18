use super::*;

fn frame(ctx: &egui::Context, time: f64, events: Vec<egui::Event>) -> Vec<&'static str> {
    render_frame(ctx, time, events, 1000.0).0
}

fn render_frame(
    ctx: &egui::Context,
    time: f64,
    events: Vec<egui::Event>,
    width: f32,
) -> (Vec<&'static str>, egui::FullOutput) {
    let items = [
        DockItem {
            id: "feedback",
            label: "Feedback",
            description: "",
            icon: DockIcon::Feedback,
            kind: DockItemKind::Action,
            active: false,
            visible: true,
            gap_before: false,
        },
        DockItem {
            id: "shapes",
            label: "Shapes",
            description: "",
            icon: DockIcon::Grid,
            kind: DockItemKind::Tool,
            active: false,
            visible: true,
            gap_before: false,
        },
        DockItem {
            id: "text",
            label: "Text",
            description: "",
            icon: DockIcon::Tags,
            kind: DockItemKind::Tool,
            active: false,
            visible: true,
            gap_before: false,
        },
    ];
    let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 700.0));
    let mut choices = Vec::new();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(canvas),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ctx| {
            let outcome = floating_dock(
                ctx,
                "inline-test",
                canvas,
                &Palette::dark(),
                DockSide::BottomCenter,
                &items,
                &[],
                &[],
                &[],
                &[],
                false,
                |ui, _| {
                    let tools = [
                        FlyoutItem::new("rect", "Rectangle", "", None, DockIcon::Grid, false),
                        FlyoutItem::new("ellipse", "Ellipse", "", None, DockIcon::Tags, false),
                    ];
                    if let Some(id) = flyout_items(ui, &tools) {
                        choices.push(id);
                    }
                },
            );
            if outcome.clicked == Some("feedback") {
                choices.push("feedback");
            }
        },
    );
    (choices, output)
}

#[test]
fn the_leftmost_feedback_singleton_fires_without_expanding_a_palette() {
    let ctx = egui::Context::default();
    for i in 0..4 {
        frame(&ctx, i as f64 * 0.02, vec![]);
    }
    let before = state(&ctx).last_icon_rects;
    assert!(before["feedback"].right() < before["shapes"].left());
    assert_eq!(
        click(&ctx, before["feedback"].center(), 0.2),
        vec!["feedback"]
    );
    let after = state(&ctx);
    assert!(after.pinned.is_empty());
    assert!(after.body_preview.is_none());
    for i in 0..10 {
        frame(&ctx, 0.3 + i as f64 * 0.02, vec![]);
    }
    let after = state(&ctx).last_icon_rects;
    assert!((before["shapes"].center() - after["shapes"].center()).length() < 1.0);
}

fn state(ctx: &egui::Context) -> DockState {
    ctx.data(|d| {
        d.get_temp(egui::Id::new(("floating_dock", "inline-test")))
            .unwrap()
    })
}

fn click(ctx: &egui::Context, pos: Pos2, time: f64) -> Vec<&'static str> {
    let event = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(ctx, time, vec![egui::Event::PointerMoved(pos), event(true)]);
    frame(ctx, time + 0.016, vec![event(false)])
}

#[test]
fn real_clicks_toggle_reveal_to_the_right_and_blister_restores_families() {
    let ctx = egui::Context::default();
    for i in 0..4 {
        frame(&ctx, i as f64 * 0.02, vec![]);
    }
    let before = state(&ctx).last_icon_rects;
    click(&ctx, before["shapes"].center(), 0.2);
    for i in 0..20 {
        frame(&ctx, 0.25 + i as f64 * 0.02, vec![]);
    }
    assert!(state(&ctx).pinned.contains(&"shapes"));
    let primary = state(&ctx).last_icon_rects["shapes"];
    let neighbor = state(&ctx).last_icon_rects["text"];
    assert!((primary.center().y - neighbor.center().y).abs() < 0.1);
    assert!((neighbor.left() - primary.right()) > before["text"].left() - before["shapes"].right());
    let tokens = crate::tokens::current().dock.clone();
    let tool = primary.right_center() + Vec2::new(icon_gap(&tokens) + primary.width() / 2.0, 0.0);
    assert_eq!(click(&ctx, tool, 0.8), vec!["rect"]);
    set_bar_collapsed(&ctx, "inline-test", true);
    frame(&ctx, 1.0, vec![]);
    assert!(state(&ctx).pinned.contains(&"shapes"));
    set_bar_collapsed(&ctx, "inline-test", false);
    frame(&ctx, 1.1, vec![]);
    assert!(state(&ctx).pinned.contains(&"shapes"));
    click(&ctx, state(&ctx).last_icon_rects["shapes"].center(), 1.3);
    for i in 0..20 {
        frame(&ctx, 1.35 + i as f64 * 0.02, vec![]);
    }
    assert!(!state(&ctx).pinned.contains(&"shapes"));
    let after = state(&ctx).last_icon_rects;
    assert!(
        (after["text"].left()
            - after["shapes"].right()
            - (before["text"].left() - before["shapes"].right()))
        .abs()
            < 0.1
    );
}

#[test]
fn easing_has_soft_ends_and_reverses_without_a_jump() {
    let mut motion = Motion::default();
    assert_eq!(motion.step(true, 1.0, 0.2), 0.0);
    let early = motion.value(1.02, 0.2);
    let middle = motion.value(1.1, 0.2);
    assert!(early < 0.04 && (middle - 0.5).abs() < 0.001);
    assert!((motion.value(1.2, 0.2) - 1.0).abs() < 0.001);
    assert!((motion.step(false, 1.1, 0.2) - middle).abs() < 0.001);
    assert!((motion.value(1.3, 0.2)).abs() < 0.001);
}

#[test]
fn all_secondary_slots_share_one_datum_and_pitch_even_across_groups() {
    let tokens = DockTokens::default();
    let a = FlyoutItem::new("a", "A", "", None, DockIcon::Grid, false).grouped("one");
    let b = FlyoutItem::new("b", "B", "", None, DockIcon::Grid, false).grouped("two");
    let c = FlyoutItem::new("c", "C", "", None, DockIcon::Grid, false).toggle();
    let laid = layout(&[&a, &b, &c], &tokens);
    let slots = &laid.groups[0].slots;
    let icon = flyout_icon_size(&tokens);
    for (i, slot) in slots.iter().enumerate() {
        assert_eq!(slot.size, Vec2::splat(icon));
        assert_eq!(
            slot.off,
            Vec2::new(i as f32 * (icon + icon_gap(&tokens)), 0.0)
        );
    }
}

#[test]
fn dashboard_forms_never_use_the_inline_tool_host() {
    let form = DockItem {
        id: "filters",
        label: "Filters",
        description: "",
        icon: DockIcon::Grid,
        kind: DockItemKind::Dashboard,
        active: false,
        visible: true,
        gap_before: false,
    };
    assert!(!hosts(DockSide::BottomCenter, &[&form]));
    assert!(!hosts(DockSide::LeftCenter, &[&form]));
    assert_eq!(
        body_layout_for(&DockState::default(), form.id, form.kind),
        DockBodyLayout::List
    );
    let tool = DockItem {
        kind: DockItemKind::Tool,
        ..form
    };
    assert!(hosts(DockSide::BottomCenter, &[&tool]));
    assert!(!hosts(DockSide::LeftCenter, &[&tool]));
}

#[test]
fn expanded_row_stays_centered_and_paints_whole_circles_on_hover_and_resize() {
    let ctx = egui::Context::default();
    for i in 0..4 {
        frame(&ctx, i as f64 * 0.02, vec![]);
    }
    set_panel_open(&ctx, "inline-test", "shapes", true);
    set_panel_open(&ctx, "inline-test", "text", true);
    for i in 0..20 {
        frame(&ctx, 0.2 + i as f64 * 0.02, vec![]);
        let bar = last_bar_rect(&ctx).unwrap();
        assert!((bar.center().x - 500.0).abs() <= 0.51, "{bar:?}");
    }
    let tokens = crate::tokens::current().dock.clone();
    let primary = state(&ctx).last_icon_rects["text"];
    let hover = primary.right_center() + Vec2::new(icon_gap(&tokens) + primary.width() * 0.5, 0.0);
    let (_, output) = render_frame(&ctx, 0.8, vec![egui::Event::PointerMoved(hover)], 1000.0);
    let mut circles = 0;
    for painted in output.shapes {
        if let egui::epaint::Shape::Circle(circle) = &painted.shape {
            if circle.radius > flyout_icon_size(&tokens) * 0.4 {
                circles += 1;
                assert!(
                    painted
                        .clip_rect
                        .contains_rect(circle.visual_bounding_rect().expand(0.5)),
                    "circle {:?} clipped by {:?}",
                    circle,
                    painted.clip_rect
                );
            }
        }
    }
    assert!(
        circles >= 8,
        "both primaries and four secondary outlines must paint"
    );
    for (i, width) in [700.0, 1200.0, 800.0].into_iter().enumerate() {
        render_frame(&ctx, 0.9 + i as f64 * 0.02, vec![], width);
        let bar = last_bar_rect(&ctx).unwrap();
        assert!((bar.center().x - width * 0.5).abs() <= 0.51, "{bar:?}");
        assert!(bar.left() >= 0.0 && bar.right() <= width);
    }
    set_panel_open(&ctx, "inline-test", "shapes", false);
    for i in 0..20 {
        frame(&ctx, 1.2 + i as f64 * 0.02, vec![]);
        let bar = last_bar_rect(&ctx).unwrap();
        assert!((bar.center().x - 500.0).abs() <= 0.51, "{bar:?}");
    }
}

#[test]
fn horizontal_wheel_reaches_named_tags_and_returns_to_the_add_control() {
    use crate::selection_tools::{tag_add_button, tag_palette, TagPaletteItem};

    fn tags_frame(
        ctx: &egui::Context,
        time: f64,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Rect, bool) {
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(420.0, 700.0));
        let items = [DockItem {
            id: "tags",
            label: "Tags",
            description: "",
            icon: DockIcon::Tags,
            kind: DockItemKind::Tool,
            active: false,
            visible: true,
            gap_before: false,
        }];
        let tags = [
            "Architectural material references awaiting review",
            "QA",
            "Final",
        ]
        .map(|label| TagPaletteItem {
            label,
            color: [254, 87, 90],
            selected: false,
            visible: true,
        });
        let mut add = Rect::NOTHING;
        let mut added = false;
        let modifiers = events
            .iter()
            .find_map(|event| match event {
                egui::Event::MouseWheel { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(canvas),
                time: Some(time),
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                let raw_wheel = ctx.input(|input| input.raw_scroll_delta);
                if modifiers.shift {
                    assert!(dock_pointer_nav(ctx).canvas_wheel());
                    assert_ne!(raw_wheel, Vec2::ZERO);
                    assert_ne!(ctx.input(|input| input.smooth_scroll_delta), Vec2::ZERO);
                }
                floating_dock(
                    ctx,
                    "inline-test",
                    canvas,
                    &Palette::light(),
                    DockSide::BottomCenter,
                    &items,
                    &[],
                    &[],
                    &[],
                    &[],
                    false,
                    |ui, _| {
                        let diameter = flyout_icon_size(&crate::tokens::current().dock);
                        let response = tag_add_button(ui, false, diameter, Palette::light());
                        add = response.rect;
                        added = response.clicked();
                        tag_palette(ui, &tags, diameter, Palette::light());
                    },
                );
                if modifiers.shift {
                    assert_eq!(ctx.input(|input| input.raw_scroll_delta), raw_wheel);
                    assert_eq!(ctx.input(|input| input.smooth_scroll_delta), Vec2::ZERO);
                }
            },
        );
        (output, add, added)
    }

    let ctx = egui::Context::default();
    ctx.set_visuals(egui::Visuals::light());
    tags_frame(&ctx, 0.0, vec![]);
    set_panel_open(&ctx, "inline-test", "tags", true);
    let mut time = 0.02;
    let mut rendered = tags_frame(&ctx, time, vec![]);
    for _ in 0..30 {
        time += 0.02;
        rendered = tags_frame(&ctx, time, vec![]);
    }
    let viewport = last_bar_rect(&ctx).unwrap();
    let primary = state(&ctx).last_icon_rects["tags"];
    let initial_add = rendered.1;
    assert!(viewport.contains_rect(primary));
    assert!(viewport.contains_rect(initial_add));
    assert!(
        rendered.0.shapes.iter().any(|painted| {
            matches!(&painted.shape, egui::Shape::Text(text)
            if text.galley.job.text == "Architectural material references awaiting review"
                && !text.galley.elided)
        }),
        "the long name must remain intact while the row overflows"
    );
    let pointer = viewport.center();
    time += 0.02;
    tags_frame(
        &ctx,
        time,
        vec![
            egui::Event::PointerMoved(pointer),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, -6.0),
                modifiers: egui::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            },
        ],
    );
    time += 0.02;
    rendered = tags_frame(&ctx, time, vec![]);
    assert!(
        (rendered.1.center() - initial_add.center()).length() < 0.1,
        "Shift+wheel belongs to the canvas and must leave the tag row still"
    );
    for delta in [-6.0, 6.0] {
        for _ in 0..180 {
            time += 0.02;
            rendered = tags_frame(
                &ctx,
                time,
                vec![
                    egui::Event::PointerMoved(pointer),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: Vec2::new(delta, 0.0),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        if delta < 0.0 {
            let (text, clip) = rendered
                .0
                .shapes
                .iter()
                .find_map(|painted| match &painted.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "Final" => {
                        Some((text, painted.clip_rect))
                    }
                    _ => None,
                })
                .expect("the last tag must actually paint after horizontal wheel input");
            assert!(!text.galley.elided);
            assert!(viewport
                .intersect(clip)
                .contains_rect(text.visual_bounding_rect()));
            assert!(
                !viewport.intersects(rendered.1),
                "the row must traverse its overflow"
            );
            let diameter = flyout_icon_size(&crate::tokens::current().dock);
            assert!(
                rendered.0.shapes.iter().any(|painted| {
                    matches!(&painted.shape, egui::Shape::Circle(circle)
                    if circle.fill == Color32::from_rgb(254, 87, 90)
                        && (circle.radius - (diameter * 0.5 - 0.5)).abs() < 0.01
                        && viewport.intersect(painted.clip_rect)
                            .contains_rect(circle.visual_bounding_rect()))
                }),
                "overflow must preserve full-size visible tag discs"
            );
        }
    }
    let returned_primary = state(&ctx).last_icon_rects["tags"];
    assert!(viewport.contains_rect(returned_primary));
    assert!(viewport.contains_rect(rendered.1));
    assert!((returned_primary.center() - primary.center()).length() < 0.1);
    assert!((rendered.1.center() - initial_add.center()).length() < 0.1);
    let point = rendered.1.center();
    let mut added = false;
    for pressed in [true, false] {
        time += 0.02;
        added = tags_frame(
            &ctx,
            time,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
        .2;
    }
    assert!(added, "the restored Add control must receive a real click");
}
