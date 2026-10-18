//! Regressions from the native review: regular text and a scaled chat's clip.
use super::*;

fn screen_response(f: &Fixture, id: Id) -> (Rect, Rect) {
    let response = f.ctx.read_response(id).expect("rendered input target");
    let transform = f
        .ctx
        .layer_transform_to_global(response.layer_id)
        .unwrap_or_default();
    (
        transform * response.rect,
        transform * response.interact_rect,
    )
}

fn encloses_panel(shape: &egui::Shape, panel: Rect, fill: egui::Color32) -> bool {
    match shape {
        // A frame with a shadow emits a group containing the shadow and fill.
        egui::Shape::Vec(shapes) => shapes.iter().any(|s| encloses_panel(s, panel, fill)),
        egui::Shape::Rect(r) => r.fill == fill && r.rect.contains_rect(panel.shrink(1.0)),
        _ => false,
    }
}

#[test]
fn attached_shell_composer_and_send_remain_reachable_at_each_camera_scale() {
    for (zoom, top) in [(0.8, 750.0), (1.0, 650.0), (2.0, 300.0)] {
        let mut f = Fixture::new();
        f.screen = Rect::from_min_size(Pos2::new(80.0, 40.0), Vec2::new(1440.0, 1000.0));
        f.state.open_chat_at("dot", Pos2::new(500.0, top), zoom);
        let out = f.settle();
        let panel = f.state.last_popup_rect;
        assert!((panel.left() - 500.0).abs() < 1.0 && (panel.top() - top).abs() < 1.0);
        assert!(
            f.screen.contains_rect(panel),
            "the complete shell fits: {panel:?}"
        );
        let shell = out
            .shapes
            .iter()
            .find(|s| encloses_panel(&s.shape, panel, Palette::light().card))
            .expect("the enclosing surface is painted");
        assert_eq!(
            shell.clip_rect, f.screen,
            "only the real viewport clips at {zoom}"
        );
        let (composer, interaction) = screen_response(&f, Id::new(("presence-composer", "dot")));
        assert!(
            interaction.contains_rect(composer.shrink(1.0)),
            "whole composer at {zoom}"
        );
        f.click(composer.center());
        let draft = format!("A message at {zoom}");
        let actions = f.frame(vec![egui::Event::Text(draft.clone())]).0;
        assert!(actions.contains(&PresenceAction::Draft {
            id: "dot".into(),
            text: draft.clone()
        }));
        let (send, interaction) = screen_response(&f, Id::new(("presence-send", "dot")));
        assert!(
            interaction.contains_rect(send),
            "whole send button at {zoom}"
        );
        assert!(
            f.click(send.center()).contains(&PresenceAction::Send {
                id: "dot".into(),
                text: draft,
            }),
            "the visible send target actually responds at {zoom}"
        );
    }
}

#[test]
fn an_offscreen_attached_composer_cannot_accept_invisible_pointer_input() {
    let mut f = Fixture::new();
    f.screen = Rect::from_min_size(Pos2::new(80.0, 40.0), Vec2::new(1440.0, 1000.0));
    f.state.open_chat_at("dot", Pos2::new(500.0, 900.0), 2.0);
    f.settle();
    let (composer, interaction) = screen_response(&f, Id::new(("presence-composer", "dot")));
    assert!(composer.top() > f.screen.bottom());
    assert!(
        !interaction.is_positive(),
        "the real viewport also clips hit testing"
    );
}

#[test]
fn registered_regular_family_reaches_painted_text_without_changing_other_chrome() {
    let mut f = Fixture::new();
    let family = egui::FontFamily::Name("presence-regular-fixture".into());
    let mut fonts = egui::FontDefinitions::default();
    fonts.families.insert(
        family.clone(),
        fonts.families[&egui::FontFamily::Proportional].clone(),
    );
    f.ctx.set_fonts(fonts);
    let original = f.ctx.style().text_styles.clone();
    f.state.set_font_family(Some(family.clone()));
    let out = f.open_chat("dot");
    for expected in [
        "Dot",
        "Ready in proposal mode",
        "I am arranging the images in this frame.",
        "Message",
    ] {
        let texts: Vec<_> = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == expected => Some(t),
                _ => None,
            })
            .collect();
        assert!(
            !texts.is_empty(),
            "the actual header/status/transcript/composer text is painted"
        );
        if expected == "Dot" {
            assert_eq!(
                texts.len(),
                2,
                "both roster and expanded header are inspected"
            );
        }
        for text in texts {
            assert!(text
                .galley
                .job
                .sections
                .iter()
                .all(|s| s.format.font_id.family == family));
        }
    }
    assert_eq!(
        f.ctx.style().text_styles,
        original,
        "presence typography stays scoped"
    );
    f.state
        .set_font_family(Some(egui::FontFamily::Name("not-installed".into())));
    let out = f.settle();
    let text = out
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t)
                if t.galley.text() == "I am arranging the images in this frame." =>
            {
                Some(t)
            }
            _ => None,
        })
        .unwrap();
    assert!(text
        .galley
        .job
        .sections
        .iter()
        .all(|s| s.format.font_id.family == egui::FontFamily::Proportional));
}

#[test]
fn presence_muted_text_is_readable_and_keeps_following_custom_theme_slots() {
    let base = Palette::light();
    let resolved = chrome::presentation_palette(&base);
    let contrast = |color: egui::Color32| {
        let rgba = egui::Rgba::from(color);
        let luminance = rgba.r() * 0.2126 + rgba.g() * 0.7152 + rgba.b() * 0.0722;
        1.05 / (luminance + 0.05)
    };
    assert!(contrast(resolved.sub) >= 4.5);
    assert!(contrast(resolved.sub) > contrast(base.sub));
    assert_eq!(resolved.card, base.card);
    assert_eq!(resolved.ink, base.ink);
    let custom = Palette {
        sub: egui::Color32::from_rgb(95, 125, 105),
        link: base.accent,
        ..base
    };
    assert_ne!(chrome::presentation_palette(&custom).sub, resolved.sub);
}

#[test]
fn populated_transcripts_keep_the_approved_gutter_and_multiline_rhythm() {
    for attached in [false, true] {
        for outgoing in [false, true] {
            let mut f = Fixture::new();
            f.message_text = "First line\nSecond line".into();
            f.message_outgoing = outgoing;
            if attached {
                f.state.open_chat_at("dot", Pos2::new(300.0, 100.0), 1.0);
            } else {
                f.state.open_chat("dot");
            }
            let out = f.settle();
            let text = out
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == f.message_text => Some(t),
                    _ => None,
                })
                .expect("the populated transcript is painted");
            let panel = f.state.last_popup_rect;
            let bounds = Rect::from_min_size(text.pos, text.galley.size());
            let side = if attached { 14.0 } else { 16.0 };
            let padding = if attached { 13.0 } else { 18.0 };
            if outgoing {
                assert!(
                    (panel.right() - bounds.right() - (8.0 + side + padding)).abs() < 0.1,
                    "outgoing bubble shares the approved right gutter"
                );
            } else {
                let avatar_and_gap = if attached { 25.0 + 9.0 } else { 36.0 + 15.0 };
                assert!(
                    (bounds.left() - panel.left() - (8.0 + side + avatar_and_gap + padding)).abs()
                        < 0.1,
                    "incoming avatar and bubble retain the content gutter"
                );
            }
            assert_eq!(text.galley.rows.len(), 2);
            let leading = text.galley.rows[1].rect.top() - text.galley.rows[0].rect.top();
            assert!(
                // egui rounds each row's leading to the physical pixel grid.
                (leading - if attached { 20.3 } else { 24.3 }).abs() <= 0.51,
                "painted multiline rhythm matches the accepted face: {leading}"
            );
        }
    }
}

#[test]
fn world_name_uses_the_registered_family_and_scales_with_its_marble() {
    let ctx = egui::Context::default();
    let family = egui::FontFamily::Name("world-name-regular-fixture".into());
    let mut fonts = egui::FontDefinitions::default();
    fonts.families.insert(
        family.clone(),
        fonts.families[&egui::FontFamily::Proportional].clone(),
    );
    ctx.set_fonts(fonts);
    let mut original_size = None;
    for zoom in [1.0, 0.8, 2.0] {
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(1500.0))),
                ..Default::default()
            },
            |ctx| {
                super::super::super::world_participant_name(
                    &ctx.layer_painter(egui::LayerId::new(
                        egui::Order::Middle,
                        Id::new("world-name"),
                    )),
                    Pos2::new(500.0, 500.0),
                    "Dot",
                    &Palette::light(),
                    zoom,
                    Some(&family),
                );
            },
        );
        let text = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t),
                _ => None,
            })
            .expect("the existing canvas text painter paints the world name");
        assert!(text
            .galley
            .job
            .sections
            .iter()
            .all(|s| s.format.font_id.family == family));
        assert!((text.pos.y - (500.0 + 26.0 * zoom)).abs() < 0.01);
        // The canvas owner scales this logical box exactly. Ink bounds include
        // pixel padding/bearings of whichever raster ladder rung was selected.
        let actual = text.galley.size();
        let base = *original_size.get_or_insert(actual);
        assert!(
            (actual.x - base.x * zoom).abs() < 0.01 && (actual.y - base.y * zoom).abs() < 0.01,
            "world label layout scales with its marble at {zoom}: {actual:?} vs {base:?}"
        );
        assert!(
            (text.pos.x + actual.x * 0.5 - 500.0).abs() < 0.01,
            "the scaled world label stays centered below its marble at {zoom}"
        );
    }
}
