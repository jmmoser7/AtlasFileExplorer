use std::io::Write;
use std::path::Path;

use model_preview::{
    gap_message, load_preview, read_sample_limited, sample_is_enscape, LoadError, ENSCAPE_CARD,
};

#[test]
fn obj_quad_becomes_two_triangles() {
    let src = b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n";
    let scene = load_preview(Path::new("box.obj"), src).unwrap();
    assert_eq!(scene.format, "obj");
    assert_eq!(scene.meshes[0].indices.len(), 6);
    assert!(scene.bounds_max[0] > 0.9);
}

#[test]
fn obj_without_faces_is_empty() {
    let err = load_preview(Path::new("pts.OBJ"), b"v 0 0 0\n").unwrap_err();
    assert!(matches!(err, LoadError::Empty(_)));
}

#[test]
fn binary_stl_triangle() {
    let mut bytes = vec![0u8; 84];
    bytes[80..84].copy_from_slice(&1u32.to_le_bytes());
    let mut tri = vec![0u8; 50];
    put_f32(&mut tri[12..], 0.0);
    put_f32(&mut tri[16..], 0.0);
    put_f32(&mut tri[20..], 0.0);
    put_f32(&mut tri[24..], 1.0);
    put_f32(&mut tri[28..], 0.0);
    put_f32(&mut tri[32..], 0.0);
    put_f32(&mut tri[36..], 0.0);
    put_f32(&mut tri[40..], 1.0);
    put_f32(&mut tri[44..], 0.0);
    bytes.extend(tri);
    let scene = load_preview(Path::new("a.stl"), &bytes).unwrap();
    assert_eq!(scene.meshes[0].indices.len(), 3);
}

#[test]
fn ascii_stl_triangle() {
    let src = b"solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
    let scene = load_preview(Path::new("a.stl"), src).unwrap();
    assert_eq!(scene.format, "stl");
    assert_eq!(scene.meshes[0].indices, vec![0, 1, 2]);
}

#[test]
fn gap_formats_do_not_read_as_meshes() {
    let err = load_preview(Path::new("house.blend"), b"not a blend").unwrap_err();
    let LoadError::Gap(msg) = err else {
        panic!("gap")
    };
    assert!(msg.contains("Blender"));
    assert!(gap_message(Path::new("plan.DWG")).unwrap().contains("DWG"));
    assert!(gap_message(Path::new("mass.skp")).is_some());
    assert!(gap_message(Path::new("tower.3dm")).is_none());
}

#[test]
fn rhino_fixture_still_previews() {
    let bytes = include_bytes!("../../rhino-mesh/tests/fixtures/brep_with_render_mesh.3dm");
    let scene = load_preview(Path::new("brep.3dm"), bytes).unwrap();
    assert_eq!(scene.format, "3dm");
    assert!(!scene.meshes[0].indices.is_empty());
}

#[test]
fn glb_triangle_and_base_color() {
    let glb = triangle_glb();
    let dir = std::env::temp_dir().join("slate-model-preview-glb");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("tri.glb");
    std::fs::write(&path, &glb).unwrap();
    let scene = load_preview(&path, &glb).unwrap();
    assert_eq!(scene.format, "gltf");
    assert_eq!(scene.meshes[0].indices.len(), 3);
    assert_eq!(scene.meshes[0].color, Some([255, 0, 0]));
}

#[test]
fn enscape_marker_is_ascii_or_utf16_and_can_sit_in_the_tail() {
    assert!(!sample_is_enscape(b"MZ not enscape"));
    assert!(sample_is_enscape(b"xx EnscapeClient.exe yy"));
    let mut utf16 = Vec::new();
    for b in b"EnscapeStandalone" {
        utf16.push(*b);
        utf16.push(0);
    }
    assert!(sample_is_enscape(&utf16));

    let dir = std::env::temp_dir().join("slate-model-preview-enscape");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("walk.exe");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&[0u8; 40]).unwrap();
    file.write_all(b"EnscapeClient.exe").unwrap();
    let sample = read_sample_limited(&path, 16, 24).unwrap();
    assert!(sample_is_enscape(&sample));
    // Tail longer than the file must still cover bytes past the prefix.
    let sample = read_sample_limited(&path, 8, 10_000).unwrap();
    assert!(sample_is_enscape(&sample));
    assert!(ENSCAPE_CARD.contains("Double-click"));
}

fn put_f32(dst: &mut [u8], v: f32) {
    dst[..4].copy_from_slice(&v.to_le_bytes());
}

/// Minimal GLB: one red triangle, no textures.
fn triangle_glb() -> Vec<u8> {
    let mut bin = Vec::new();
    for v in [[0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for c in v {
            bin.extend(c.to_le_bytes());
        }
    }
    for i in [0u16, 1, 2] {
        bin.extend(i.to_le_bytes());
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1,"material":0}}]}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[1,0,0,1]}}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","max":[1,1,0],"min":[0,0,0]}},{{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":6}}],"buffers":[{{"byteLength":{}}}]}}"#,
        bin.len()
    );
    let mut json_bytes = json.into_bytes();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let mut glb = Vec::new();
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
    glb.extend(0x46546C67u32.to_le_bytes());
    glb.extend(2u32.to_le_bytes());
    glb.extend((total as u32).to_le_bytes());
    glb.extend((json_bytes.len() as u32).to_le_bytes());
    glb.extend(0x4E4F534Au32.to_le_bytes());
    glb.extend(json_bytes);
    glb.extend((bin.len() as u32).to_le_bytes());
    glb.extend(0x004E4942u32.to_le_bytes());
    glb.extend(bin);
    glb
}
