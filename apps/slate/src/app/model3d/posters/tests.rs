use super::*;

fn paint_source(space: &mut ModelSpace, ctx: &egui::Context, name: &str, loads: &mut usize) {
    if space.cached_poster_texture(name).is_some() {
        return;
    }
    // Two 8×8 CPU/GPU pairs fit. This guard is the same admission path the
    // real paint caller checks before image::open and ctx.load_texture.
    if !space.reserve_poster(name, 8 * 8 * 8, true, 2, 2 * 8 * 8 * 8) {
        return;
    }
    *loads += 1;
    let image = pixels(8);
    space.cache_poster_pixels(name.into(), image.clone());
    space.cache_poster_texture(
        name.into(),
        ctx.load_texture(name, (*image).clone(), egui::TextureOptions::LINEAR),
    );
}

#[test]
fn cyclic_over_budget_view_settles_and_new_sources_enter_after_visibility_changes() {
    let mut space = ModelSpace::default();
    let ctx = egui::Context::default();
    let mut loads = [0; 3];
    for pass in 1..=100 {
        space.poster_residency.frame = pass;
        for (index, name) in ["a", "b", "c"].into_iter().enumerate() {
            paint_source(&mut space, &ctx, name, &mut loads[index]);
        }
        assert!(space.poster_residency.entries.len() <= 2);
        let bytes = space
            .poster_residency
            .entries
            .keys()
            .map(|name| space.poster_budget_bytes(name))
            .sum::<usize>();
        assert!(bytes <= 2 * 8 * 8 * 8);
    }
    assert_eq!(
        loads,
        [1, 1, 0],
        "fallback c must not evict and reload a/b every frame"
    );
    space.poster_residency.frame = 101;
    paint_source(&mut space, &ctx, "c", &mut loads[2]);
    assert_eq!(
        loads,
        [1, 1, 0],
        "protect the preceding pass while the next view settles"
    );
    space.poster_residency.frame = 102;
    paint_source(&mut space, &ctx, "c", &mut loads[2]);
    assert_eq!(loads, [1, 1, 1]);
    assert!(space.posters.contains_key("c"));
}

#[test]
fn refused_larger_upgrade_is_remembered_before_the_next_decode_attempt() {
    let mut space = ModelSpace::default();
    space.poster_residency.frame = 1;
    assert!(space.reserve_poster("a", 512, true, 3, 1024));
    assert!(space.reserve_poster("large", 256, true, 3, 1024));
    assert!(!space.reserve_poster("large", 768, true, 3, 1024));
    space.poster_residency.frame = 2;
    assert!(!space.reserve_poster("large", 256, true, 3, 1024));
    space.poster_residency.frame = 3;
    assert!(space.reserve_poster("large", 256, true, 3, 1024));
    assert!(!space.poster_residency.entries.contains_key("a"));
}

#[test]
fn failed_missing_poster_does_not_reserve_memory_from_healthy_sources() {
    let mut space = ModelSpace::default();
    assert!(space.reserve_poster("missing", 512, true, 1, 512));
    space.release_empty_poster("missing");
    assert!(space.reserve_poster("healthy", 512, true, 1, 512));
    assert!(!space.poster_residency.entries.contains_key("missing"));
}

fn pixels(edge: usize) -> Arc<egui::ColorImage> {
    Arc::new(egui::ColorImage::new([edge, edge], egui::Color32::RED))
}

#[test]
fn oldest_poster_pair_evicts_without_flushing_recent_poses() {
    let mut space = ModelSpace::default();
    let ctx = egui::Context::default();
    for name in ["old", "recent", "new"] {
        let image = pixels(2);
        space.cache_poster_pixels(name.into(), image.clone());
        space.cache_poster_texture(
            name.into(),
            ctx.load_texture(name, (*image).clone(), egui::TextureOptions::LINEAR),
        );
    }
    let _ = space.cached_poster_texture("old").unwrap();
    space.evict_posters_to(2, usize::MAX);
    assert!(!space.poster_pixels.contains_key("recent"));
    assert!(!space.posters.contains_key("recent"));
    assert!(space.poster_pixels.contains_key("old"));
    assert!(space.posters.contains_key("new"));
}

#[test]
fn byte_budget_counts_both_tiers_and_writer_arc_survives_eviction() {
    let mut space = ModelSpace::default();
    let ctx = egui::Context::default();
    let writer = pixels(8);
    space.cache_poster_pixels("old".into(), writer.clone());
    space.cache_poster_texture(
        "old".into(),
        ctx.load_texture("old", (*writer).clone(), egui::TextureOptions::LINEAR),
    );
    space.cache_poster_pixels("new".into(), pixels(8));
    space.evict_posters_to(64, 512);
    assert!(!space.poster_pixels.contains_key("old"));
    assert!(!space.posters.contains_key("old"));
    assert!(space.poster_pixels.contains_key("new"));
    assert_eq!(writer.pixels.len(), 64);
    assert_eq!(Arc::strong_count(&writer), 1);
}

#[test]
fn more_than_sixty_four_poses_keeps_the_recent_working_set() {
    let mut space = ModelSpace::default();
    for index in 0..80 {
        space.cache_poster_pixels(format!("pose-{index}"), pixels(1));
    }
    assert_eq!(space.poster_pixels.len(), MAX_POSTERS);
    assert!(!space.poster_pixels.contains_key("pose-0"));
    assert!(space.poster_pixels.contains_key("pose-79"));
    assert_eq!(space.poster_residency.entries.len(), MAX_POSTERS);
}
