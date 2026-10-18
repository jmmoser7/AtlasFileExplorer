use super::*;
use std::cell::Cell;

fn key(name: &str, tier: u32) -> Key {
    (name.into(), 7, tier)
}

fn cache(bytes: usize, entries: usize) -> Residency<u32> {
    Residency {
        limits: Limits {
            bytes,
            entries,
            thumbnail_bytes: 0,
            thumbnail_entries: 0,
            uploads: 2,
        },
        ..Residency::default()
    }
}

fn create(
    cache: &mut Residency<u32>,
    name: &str,
    size: [usize; 2],
    pass: u64,
    class: TextureClass,
    calls: &Cell<u32>,
) -> Result<u32, Denied> {
    let tier = match class {
        TextureClass::Full => 2048,
        TextureClass::Thumbnail => 0,
    };
    cache.get_or_create(key(name, tier), size, class, pass, 8192, |_| {
        calls.set(calls.get() + 1);
        calls.get()
    })
}

#[test]
fn byte_limit_is_checked_before_filtering_and_uploading() {
    let mut cache = cache(64, 256);
    let calls = Cell::new(0);
    assert_eq!(
        create(&mut cache, "a", [4, 4], 0, TextureClass::Full, &calls),
        Ok(1)
    );
    assert_eq!(
        create(&mut cache, "b", [1, 1], 0, TextureClass::Full, &calls),
        Err(Denied::Capacity)
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(cache.bytes, 64);
    assert_eq!(cache.entries.len(), 1);
}

#[test]
fn cached_hits_do_not_spend_the_two_uploads_per_pass() {
    let mut cache = cache(256, 8);
    let calls = Cell::new(0);
    for name in ["a", "b"] {
        assert!(create(&mut cache, name, [1, 1], 0, TextureClass::Full, &calls).is_ok());
    }
    for _ in 0..5 {
        assert_eq!(
            create(&mut cache, "a", [1, 1], 0, TextureClass::Full, &calls),
            Ok(1)
        );
    }
    assert_eq!(
        create(&mut cache, "c", [1, 1], 0, TextureClass::Full, &calls),
        Err(Denied::UploadQuota)
    );
    assert_eq!(calls.get(), 2);
    assert_eq!(
        create(&mut cache, "c", [1, 1], 1, TextureClass::Full, &calls),
        Ok(3)
    );
}

#[test]
fn three_visible_sources_with_two_slots_do_not_reload_cyclically() {
    let mut cache = cache(256, 2);
    let calls = Cell::new(0);
    for pass in 0..40 {
        for name in ["a", "b"] {
            assert!(create(&mut cache, name, [1, 1], pass, TextureClass::Full, &calls).is_ok());
        }
        assert!(create(&mut cache, "c", [1, 1], pass, TextureClass::Full, &calls).is_err());
    }
    assert_eq!(calls.get(), 2);
    assert_eq!(cache.entries.len(), 2);
}

#[test]
fn the_previous_pass_is_protected_before_this_pass_reaches_its_hits() {
    let mut cache = cache(256, 2);
    let calls = Cell::new(0);
    for name in ["a", "b"] {
        create(&mut cache, name, [1, 1], 3, TextureClass::Full, &calls).unwrap();
    }
    assert_eq!(
        create(&mut cache, "c", [1, 1], 4, TextureClass::Full, &calls),
        Err(Denied::Capacity)
    );
    assert_eq!(cache.get(&key("a", 2048), 4), Some(1));
    assert_eq!(cache.get(&key("b", 2048), 4), Some(2));
    assert_eq!(calls.get(), 2);
}

#[test]
fn eviction_removes_only_the_oldest_unprotected_entry() {
    let mut cache = cache(256, 2);
    let calls = Cell::new(0);
    for name in ["a", "b"] {
        create(&mut cache, name, [1, 1], 0, TextureClass::Full, &calls).unwrap();
    }
    cache.get(&key("a", 2048), 1).unwrap();
    create(&mut cache, "c", [1, 1], 3, TextureClass::Full, &calls).unwrap();
    assert!(cache.entries.contains_key(&key("a", 2048)));
    assert!(!cache.entries.contains_key(&key("b", 2048)));
    assert!(cache.entries.contains_key(&key("c", 2048)));
    assert_eq!(cache.bytes, 8);
}

#[test]
fn over_budget_full_sources_converge_on_a_stable_filtered_thumbnail() {
    let mut cache = cache(36, 4);
    cache.limits.thumbnail_bytes = 4;
    cache.limits.thumbnail_entries = 1;
    let calls = Cell::new(0);
    for name in ["a", "b"] {
        create(&mut cache, name, [2, 2], 0, TextureClass::Full, &calls).unwrap();
    }
    for pass in 1..40 {
        for name in ["a", "b"] {
            create(&mut cache, name, [2, 2], pass, TextureClass::Full, &calls).unwrap();
        }
        let thumbnail = cache.get(&key("c", 0), pass);
        assert_eq!(
            create(&mut cache, "c", [2, 2], pass, TextureClass::Full, &calls),
            Err(Denied::Capacity)
        );
        assert_eq!(
            thumbnail.or_else(|| {
                create(
                    &mut cache,
                    "c",
                    [1, 1],
                    pass,
                    TextureClass::Thumbnail,
                    &calls,
                )
                .ok()
            }),
            Some(3)
        );
    }
    assert_eq!(calls.get(), 3);
    assert_eq!(cache.bytes, 36);
    assert_eq!(cache.entries.len(), 3);
}

#[test]
fn thumbnail_count_reserve_survives_many_small_full_entries() {
    let mut cache = cache(4096, 3);
    cache.limits.thumbnail_entries = 1;
    let calls = Cell::new(0);
    for name in ["a", "b"] {
        create(&mut cache, name, [1, 1], 0, TextureClass::Full, &calls).unwrap();
    }
    assert_eq!(
        create(&mut cache, "c", [1, 1], 1, TextureClass::Full, &calls),
        Err(Denied::Capacity)
    );
    assert!(create(&mut cache, "c", [1, 1], 1, TextureClass::Thumbnail, &calls).is_ok());
    assert_eq!(cache.entries.len(), 3);
}

#[test]
fn invalid_or_hardware_oversized_dimensions_do_no_pixel_work() {
    let mut cache = cache(usize::MAX, 256);
    let calls = Cell::new(0);
    for size in [[0, 1], [1, 0], [8193, 1], [usize::MAX, 2]] {
        assert_eq!(
            create(&mut cache, "a", size, 0, TextureClass::Full, &calls),
            Err(Denied::Dimensions)
        );
    }
    assert_eq!(
        cache.get_or_create(
            key("overflow", 0),
            [usize::MAX, 2],
            TextureClass::Full,
            0,
            usize::MAX,
            |_| {
                calls.set(calls.get() + 1);
                calls.get()
            },
        ),
        Err(Denied::Dimensions)
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(cache.uploads, 0);
    assert!(cache.entries.is_empty());
}

#[test]
fn source_adjustment_and_tier_remain_distinct_cache_identities() {
    let mut cache = cache(4096, 16);
    let identities = [
        key("a", 0),
        key("a", 2048),
        ("a".into(), 8, 2048),
        key("b", 2048),
    ];
    for (index, identity) in identities.iter().enumerate() {
        cache
            .get_or_create(
                identity.clone(),
                [1, 1],
                TextureClass::Thumbnail,
                index as u64 / 2,
                8192,
                |_| index as u32,
            )
            .unwrap();
    }
    for (index, identity) in identities.iter().enumerate() {
        assert_eq!(cache.get(identity, 2), Some(index as u32));
    }
}

#[test]
fn linked_generator_result_uses_filtered_fallback_without_ordinary_thumb_pixels() {
    use crate::app::preview::{linked_image_key, PreviewEntry, PX_EXACT};
    use crate::app::tests::Harness;
    use eframe::egui::{Color32, ImageData};
    use slate_doc::scene::ImageAdjust;
    use std::sync::Arc;

    let mut h = Harness::new("linked-generator-fx-fallback");
    h.app.fx_textures.resident.limits = Limits {
        bytes: 96 * 1024,
        entries: 3,
        thumbnail_bytes: 80 * 1024,
        thumbnail_entries: 1,
        uploads: 2,
    };
    let path = h.base.join("resident-generated-result.png");
    let source_key = linked_image_key(&path, "shown");
    let mut source = ColorImage::new([384, 192], Color32::from_rgba_unmultiplied(160, 10, 20, 64));
    for y in 0..192 {
        for x in 0..96 {
            source.pixels[y * 384 + x] = Color32::from_rgba_unmultiplied(40, 100, 220, 128);
        }
    }
    let plain = h.ctx.load_texture(
        "linked-generated-plain",
        source.clone(),
        TextureOptions::LINEAR,
    );
    h.app.preview_cache.insert(
        source_key.clone(),
        PreviewEntry {
            tex: plain.clone(),
            bytes: source.pixels.len() * 4,
            pixels: Arc::new(source),
            px: PX_EXACT,
            last_used: 0,
        },
    );
    assert!(!h.app.thumb_pixels.contains_key(&source_key));
    let adjust = ImageAdjust {
        invert: 1.0,
        ..ImageAdjust::default()
    };
    let shown = h
        .app
        .agent_picture_texture(&h.ctx, &path, &adjust, 384.0)
        .unwrap();
    assert_ne!(
        shown.id(),
        plain.id(),
        "must retain authored filters under pressure"
    );
    assert_eq!(shown.size(), [192, 96]);
    assert!(!h.app.fx_textures.resident.entries.contains_key(&(
        source_key.clone(),
        adjust.cache_hash(),
        PX_EXACT
    )));
    let delta = h.ctx.tex_manager().write().take_delta();
    let (_, upload) = delta
        .set
        .into_iter()
        .find(|(id, _)| *id == shown.id())
        .unwrap();
    let ImageData::Color(filtered) = upload.image else {
        panic!("expected color fallback")
    };
    let [r, g, b, a] = filtered.pixels[0].to_srgba_unmultiplied();
    for (actual, expected) in [(r, 215), (g, 155), (b, 35)] {
        assert!(actual.abs_diff(expected) <= 2);
    }
    assert_eq!(a, 128);
    assert_eq!(filtered.pixels[191].a(), 64);
    for _ in 0..10 {
        assert_eq!(
            h.app
                .agent_picture_texture(&h.ctx, &path, &adjust, 384.0)
                .unwrap()
                .id(),
            shown.id()
        );
    }
    assert_eq!(h.app.fx_textures.resident.uploads, 1);
    assert_eq!(h.app.fx_textures.resident.bytes, 192 * 96 * 4);
    assert!(!h.app.thumb_pixels.contains_key(&source_key));
}
