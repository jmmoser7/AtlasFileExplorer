//! Recipe construction regressions.
use super::*;

fn ctx() -> BuildCtx {
    BuildCtx {
        accent: Rgba([10, 120, 240, 255]),
        next_frame_order: 3,
    }
}

fn r() -> WorldRect {
    WorldRect::new(0.0, 0.0, 100.0, 50.0)
}

#[test]
fn a_rect_recipe_builds_the_shape_the_board_would_have_hardcoded() {
    let toml = r#"
            kind = "shape"
            node = "rect"
            fill = "accent@60"
            stroke = { width = 2.0, color = "accent" }
        "#;
    let recipe: Recipe = toml::from_str(toml).unwrap();
    let specs = recipe.instantiate(r(), &ctx());
    assert_eq!(specs.len(), 1);
    let NodeKind::Shape(shape) = &specs[0].kind else {
        panic!("expected a shape, got {:?}", specs[0].kind);
    };
    assert_eq!(shape.shape, ShapeKind::Rect);
    assert_eq!(shape.fill, Some(Rgba([10, 120, 240, 60])));
    assert_eq!(shape.stroke.width, 2.0);
    assert_eq!(shape.stroke.color, Rgba([10, 120, 240, 255]));
    assert_eq!(specs[0].rect, r());
}

#[test]
fn a_frame_recipe_claims_the_next_slide_order_and_numbers_its_title() {
    let recipe: Recipe = toml::from_str(
        r##"
            kind = "shape"
            node = "frame"
        "##,
    )
    .unwrap();
    let specs = recipe.instantiate(r(), &ctx());
    let NodeKind::Frame(f) = &specs[0].kind else {
        panic!("expected a frame");
    };
    assert_eq!(f.order, 3);
    assert_eq!(f.title, "Slide 4");
    assert_eq!(f.fill, Rgba::WHITE);
}

#[test]
fn a_text_recipe_with_a_fill_is_a_sticky_note() {
    let recipe: Recipe = toml::from_str(
        r##"
            kind = "shape"
            node = "text"
            text = { text = "note", size = 18.0, fill = "#ffe066" }
        "##,
    )
    .unwrap();
    let specs = recipe.instantiate(r(), &ctx());
    let NodeKind::Text(t) = &specs[0].kind else {
        panic!("expected text");
    };
    assert_eq!(t.text, "note");
    assert_eq!(t.size, 18.0);
    assert_eq!(t.fill, Some(Rgba([0xff, 0xe0, 0x66, 255])));
}

#[test]
fn a_path_recipe_styles_but_does_not_build() {
    let recipe: Recipe = toml::from_str(
        r##"
            kind = "shape"
            node = "path"
            create_style = "pinned"
            stroke = { width = 2.0, color = "#e8443a", cap = "round", join = "round" }
        "##,
    )
    .unwrap();
    assert!(recipe.instantiate(r(), &ctx()).is_empty());
    let stroke = recipe.pinned_stroke(ctx().accent).expect("pinned stroke");
    assert_eq!(stroke.color, Rgba([0xe8, 0x44, 0x3a, 255]));
    assert_eq!(stroke.cap, slate_doc::scene::StrokeCap::Round);
}

#[test]
fn inherit_is_the_default_create_style() {
    let recipe: Recipe = toml::from_str(
        r##"
            kind = "shape"
            node = "rect"
            fill = "accent@60"
            "##,
    )
    .unwrap();
    assert!(recipe.inherits_create_style());
}

#[test]
fn an_inheriting_recipe_pins_nothing() {
    let recipe: Recipe = toml::from_str(
        r##"
            kind = "shape"
            node = "path"
            stroke = { color = "#e8443a" }
        "##,
    )
    .unwrap();
    assert_eq!(recipe.pinned_stroke(ctx().accent), None);
}

#[test]
fn a_portal_recipe_presets_a_source() {
    let recipe: Recipe = toml::from_str(
        r#"
            kind = "portal"
            portal = "file_atlas"
            title = "Shots"
            source = "shots"
            default_size = [960.0, 540.0]
        "#,
    )
    .unwrap();
    let specs = recipe.instantiate(r(), &ctx());
    let NodeKind::Portal(p) = &specs[0].kind else {
        panic!("expected a portal");
    };
    assert_eq!(p.class, PortalClass::Host);
    assert_eq!(p.kind, PortalKind::FileAtlas);
    assert_eq!(p.title, "Shots");
    assert_eq!(p.source.as_ref().unwrap().locator, "shots");
}

#[test]
fn a_retired_portal_kind_builds_nothing() {
    let recipe: Recipe = toml::from_str(
        r#"
            kind = "portal"
            portal = "status_board"
            title = "Status Board"
        "#,
    )
    .unwrap();
    assert!(recipe.instantiate(r(), &ctx()).is_empty());
}

#[test]
fn a_web_recipe_places_an_unbound_host_portal() {
    let recipe: Recipe = toml::from_str(
        r#"
            kind = "portal"
            portal = "web"
            title = "Web portal"
            default_size = [960.0, 540.0]
        "#,
    )
    .unwrap();
    let specs = recipe.instantiate(r(), &ctx());
    let NodeKind::Portal(p) = &specs[0].kind else {
        panic!("expected a portal");
    };
    assert_eq!(p.class, PortalClass::Host);
    assert_eq!(p.kind, PortalKind::Web);
    assert!(p.source.is_none());
}

#[test]
fn an_agent_recipe_places_an_unbound_host_portal() {
    let recipe: Recipe = toml::from_str(
        r#"
            kind = "portal"
            portal = "agent"
            title = "Agent portal"
            default_size = [960.0, 540.0]
        "#,
    )
    .unwrap();
    let specs = recipe.instantiate(r(), &ctx());
    let NodeKind::Portal(p) = &specs[0].kind else {
        panic!("expected a portal");
    };
    assert_eq!(p.class, PortalClass::Host);
    assert_eq!(p.kind, PortalKind::Agent);
    assert!(p.source.is_none());
}

#[test]
fn a_file_atlas_recipe_places_an_unbound_host_portal() {
    let recipe: Recipe = toml::from_str(
        r#"
            kind = "portal"
            portal = "file_atlas"
            title = "File Atlas"
            default_size = [960.0, 540.0]
        "#,
    )
    .unwrap();
    let specs = recipe.instantiate(r(), &ctx());
    let NodeKind::Portal(p) = &specs[0].kind else {
        panic!("expected a portal");
    };
    assert_eq!(p.class, PortalClass::Host);
    assert_eq!(p.kind, PortalKind::FileAtlas);
    assert!(p.source.is_none());
}

#[test]
fn an_unknown_portal_kind_builds_nothing_rather_than_guessing() {
    let recipe = Recipe::Portal(PortalRecipe {
        portal: PortalKindRef("figma_frame".into()),
        title: None,
        source: None,
        default_size: None,
        fill: None,
    });
    assert!(recipe.instantiate(r(), &ctx()).is_empty());
    assert!(!recipe.accepts(Grammar::DragRect));
}

#[test]
fn the_routing_rule_rejects_grammars_that_cannot_feed_the_recipe() {
    let rect: Recipe = toml::from_str("kind = \"shape\"\nnode = \"rect\"").unwrap();
    assert!(rect.accepts(Grammar::DragRect));
    // Select and Sample create nothing, so no recipe can ride them.
    assert!(!rect.accepts(Grammar::Select));
    assert!(!rect.accepts(Grammar::Sample));
    assert!(!rect.accepts(Grammar::Sweep));
    // A freehand stroke cannot produce a rectangle.
    assert!(!rect.accepts(Grammar::Freehand));

    let pen: Recipe = toml::from_str("kind = \"shape\"\nnode = \"path\"").unwrap();
    assert!(pen.accepts(Grammar::Freehand));
    assert!(pen.accepts(Grammar::MultiPoint));
    assert!(!pen.accepts(Grammar::DragRect));
}

#[test]
fn a_click_to_place_default_lets_a_rect_recipe_ride_place_point() {
    let sticky: Recipe = toml::from_str(
        r#"
            kind = "shape"
            node = "text"
            default_size = [180.0, 140.0]
        "#,
    )
    .unwrap();
    assert!(sticky.accepts(Grammar::PlacePoint));
    assert_eq!(sticky.default_size(), Some([180.0, 140.0]));
}

#[test]
fn unknown_recipe_fields_are_rejected_rather_than_dropped() {
    assert!(toml::from_str::<Recipe>(
        r##"
            kind = "shape"
            node = "rect"
            fillColor = "#ff0000"
        "##
    )
    .is_err());
}
