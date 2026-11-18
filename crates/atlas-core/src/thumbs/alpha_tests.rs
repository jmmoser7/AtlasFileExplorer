use super::*;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn png_webp_and_webp_named_png_preserve_alpha_across_local_and_shared_cache_reopens() {
    let unique = format!(
        "thumb-alpha-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let base = std::env::temp_dir().join(&unique);
    let shared = Arc::new(base.join("project-cache"));
    std::fs::create_dir_all(shared.as_ref()).unwrap();
    let pixels = [
        240, 20, 72, 255, 34, 125, 213, 128, 0, 0, 0, 255, 0, 0, 0, 0,
    ];
    let source = image::RgbaImage::from_raw(2, 2, pixels.to_vec()).unwrap();
    let pool = ThumbPool::new();

    for (index, (filename, format)) in [
        ("source.png", image::ImageFormat::Png),
        ("source.webp", image::ImageFormat::WebP),
        ("webp-content.PNG", image::ImageFormat::WebP),
    ]
    .into_iter()
    .enumerate()
    {
        let key = format!("{unique}-{index}");
        let path = base.join(filename);
        source.save_with_format(&path, format).unwrap();
        let legacy = pool.cache_dir().join(format!("{key}.jpg"));
        save_cached(&legacy, 2, 2, &pixels);
        std::fs::copy(&legacy, shared.join(format!("{key}.jpg"))).unwrap();
        assert!(pool.has_local(&key));
        assert!(!pool.has_local_for_path(&key, &path));

        let fetch = || {
            pool.request(ThumbRequest {
                id: index as u32,
                generation: 0,
                path: path.clone(),
                key: key.clone(),
                color_only: false,
                shared_dir: Some(shared.clone()),
                src_bytes: 0,
                pdf_page: None,
            });
            let result = pool.rx.recv_timeout(Duration::from_secs(15)).unwrap();
            assert_eq!(result.image, Some((2, 2, pixels.to_vec())), "{filename}");
        };
        fetch();
        let cache = pool.cache_dir().join(format!("{key}.png"));
        assert!(pool.has_local_for_path(&key, &path));
        assert_eq!(
            std::fs::read(&cache).unwrap(),
            std::fs::read(shared.join(format!("{key}.png"))).unwrap()
        );
        std::fs::remove_file(&path).unwrap();
        fetch(); // A new request reads the persisted, lossless local entry.
        std::fs::remove_file(&cache).unwrap();
        fetch(); // The shared copy must win over both legacy JPEGs.
        std::fs::remove_file(&cache).unwrap();
        std::fs::remove_file(&legacy).unwrap();
    }
    std::fs::remove_dir_all(base).unwrap();
}
