use super::*;

fn sample_input() -> ViewMetaInput {
    ViewMetaInput {
        camera: ModelCamera {
            target: [1.0, 2.0, 3.0],
            yaw: -0.7,
            pitch: 0.4,
            distance: 12.5,
            display: ModelDisplay::Arctic,
            materials: false,
        },
        eye: [4.0, 5.0, 6.0],
        up: [0.0, 0.0, 1.0],
        aspect: 1.6,
        width: 800,
        height: 500,
        model_name: "chair.3dm".into(),
        model_path: "assets/chair.3dm".into(),
        model_hash: "abc123".into(),
        model_size: 4096,
        node_id: 42,
        created_unix: 1_700_000_000,
        image_adjust_hash: None,
    }
}

type WriteWithXmp = fn(&Path, &[u8], u32, u32, &str) -> Result<(), ViewMetaError>;

fn round_trip_format(write: WriteWithXmp, rgba: &[u8], w: u32, h: u32, ext: &str) {
    let input = sample_input();
    let xmp = build_xmp_packet(&input).unwrap();
    assert!(!path_looks_absolute(&input.model_path));
    let dir = std::env::temp_dir().join(format!("slate-view-meta-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("shot.{ext}"));
    write(&path, rgba, w, h, &xmp).unwrap();
    let parsed = read_view_meta(&path)
        .unwrap()
        .unwrap_or_else(|| panic!("{ext} xmp"));
    assert_eq!(parsed.camera, input.camera);
    assert_eq!(parsed.model_hash, input.model_hash);
    assert_eq!(parsed.model_path, input.model_path);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn xmp_round_trip_png_jpeg_webp() {
    let w = 4u32;
    let h = 4u32;
    let mut rgba = Vec::new();
    for i in 0..(w * h) {
        let v = (i * 17 % 255) as u8;
        rgba.extend_from_slice(&[v, v / 2, 255 - v, 255]);
    }
    let rgb: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    round_trip_format(write_png_with_xmp, &rgba, w, h, "png");
    round_trip_format(
        |p, rgb, w, h, xmp| write_jpeg_with_xmp(p, rgb, w, h, 90, xmp),
        &rgb,
        w,
        h,
        "jpg",
    );
    round_trip_format(write_webp_with_xmp, &rgba, w, h, "webp");
}

#[test]
fn rejects_newer_version() {
    let xmp = build_xmp_packet(&sample_input())
        .unwrap()
        .replace("slateview:version=\"1\"", "slateview:version=\"2\"");
    let err = parse_xmp_packet(&xmp).unwrap_err();
    assert_eq!(err, ViewMetaError::UnsupportedVersion(2));
}

#[test]
fn malformed_xmp_reports_xml_error() {
    assert!(matches!(
        parse_xmp_packet("<x:xmpmeta"),
        Err(ViewMetaError::Parse(_))
    ));
}

#[test]
fn build_rejects_absolute_model_path() {
    let mut input = sample_input();
    input.model_path = r"C:\Users\secret\model.3dm".into();
    assert_eq!(build_xmp_packet(&input), Err(ViewMetaError::InvalidPath));
}

#[test]
fn hash_bytes_is_stable() {
    let a = hash_bytes(b"model-bytes");
    assert_eq!(a, hash_bytes(b"model-bytes"));
    assert_ne!(a, hash_bytes(b"other"));
}

#[test]
fn materials_setting_round_trips_and_older_xmp_defaults_to_on() {
    let xmp = build_xmp_packet(&sample_input()).unwrap();
    assert!(!parse_xmp_packet(&xmp).unwrap().camera.materials);
    let legacy = xmp.replace("    slateview:materials=\"false\"\n", "");
    assert!(parse_xmp_packet(&legacy).unwrap().camera.materials);
}
