use base64::Engine;
use model_preview::{load_preview, load_preview_with_reader, TextureSource};
use std::path::Path;

#[test]
fn obj_material_groups_keep_factors_uvs_and_safe_dependency_reads() {
    let obj = b"mtllib scene.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nusemtl red\nf 1/1 2/2 3/3\nusemtl glass\nf 1/1 3/3 2/2\n";
    let mtl = b"newmtl red\nKd 0.8 0.1 0.2\nPr 0.3\nPm 0.2\nmap_Kd brick.png\nnewmtl glass\nKd 0.7 0.8 0.9\nd 0.35\nNs 64\n";
    let mut paths = Vec::new();
    let scene = load_preview_with_reader(Path::new("assets/scene.obj"), obj, &mut |path| {
        paths.push(path.to_path_buf());
        Ok(mtl.to_vec())
    })
    .unwrap();
    assert_eq!(paths, [Path::new("assets/scene.mtl")]);
    assert_eq!(scene.meshes.len(), 2);
    let red = scene.meshes[0].material.as_ref().unwrap();
    assert_eq!(red.base_color, [0.8, 0.1, 0.2, 1.]);
    assert_eq!(red.roughness, 0.3);
    assert_eq!(red.metallic, 0.2);
    assert!(
        matches!(&red.base_color_texture,Some(TextureSource::LocalPath(p)) if p == Path::new("assets/brick.png"))
    );
    assert_eq!(scene.meshes[0].texcoords, [[0., 1.], [1., 1.], [0., 0.]]);
    assert_eq!(
        scene.meshes[1].material.as_ref().unwrap().base_color[3],
        0.35
    );
}

#[test]
fn unavailable_obj_library_keeps_geometry_and_reports_fallback() {
    let obj = b"mtllib online.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl missing\nf 1 2 3\n";
    let scene = load_preview_with_reader(Path::new("a.obj"), obj, &mut |_| {
        Err(std::io::Error::other("cloud-only file"))
    })
    .unwrap();
    assert_eq!(scene.meshes[0].indices.len(), 3);
    assert!(scene.meshes[0].material.is_none());
    assert!(scene.notes.iter().any(|n| n.contains("unavailable")));
}

fn textured_gltf_triangle() -> String {
    let mut bin = Vec::new();
    for value in [0f32, 0., 0., 1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 1.] {
        bin.extend(value.to_le_bytes());
    }
    let uri = base64::engine::general_purpose::STANDARD.encode(bin);
    format!(
        r#"{{"asset":{{"version":"2.0"}},"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"material":0}}]}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.5,0.2,0.1,1],"roughnessFactor":0.25,"metallicFactor":0.6,"baseColorTexture":{{"index":0}}}}}}],"images":[{{"uri":"data:image/png;base64,AQIDBA=="}}],"textures":[{{"source":0}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","max":[1,1,0],"min":[0,0,0]}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}}],"bufferViews":[{{"buffer":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}}],"buffers":[{{"uri":"data:application/octet-stream;base64,{uri}","byteLength":60}}]}}"#
    )
}

#[test]
fn gltf_keeps_linear_factors_uv0_and_embedded_base_color_image() {
    let json = textured_gltf_triangle();
    let scene = load_preview_with_reader(Path::new("a.gltf"), json.as_bytes(), &mut |_| {
        panic!("embedded material must not read external files")
    })
    .unwrap();
    let mesh = &scene.meshes[0];
    assert_eq!(mesh.texcoords, [[0., 0.], [1., 0.], [0., 1.]]);
    let material = mesh.material.as_ref().unwrap();
    assert_eq!(material.base_color, [0.5, 0.2, 0.1, 1.]);
    assert_eq!(material.roughness, 0.25);
    assert_eq!(material.metallic, 0.6);
    assert!(
        matches!(&material.base_color_texture,Some(TextureSource::Embedded {bytes,..}) if bytes.as_ref() == [1,2,3,4])
    );
}

#[test]
fn repeated_gltf_primitives_share_the_same_embedded_image_allocation() {
    let primitive = r#"{"attributes":{"POSITION":0,"TEXCOORD_0":1},"material":0}"#;
    let json = textured_gltf_triangle().replace(primitive, &format!("{primitive},{primitive}"));
    let scene = load_preview(Path::new("repeated.gltf"), json.as_bytes()).unwrap();
    assert_eq!(scene.meshes.len(), 2);
    let sources: Vec<_> = scene
        .meshes
        .iter()
        .map(|mesh| {
            match mesh
                .material
                .as_ref()
                .unwrap()
                .base_color_texture
                .as_ref()
                .unwrap()
            {
                TextureSource::Embedded { bytes, .. } => bytes,
                _ => panic!("expected an embedded base-color image"),
            }
        })
        .collect();
    assert!(std::sync::Arc::ptr_eq(sources[0], sources[1]));
}

#[test]
fn rhino_standard_material_survives_the_neutral_scene_boundary() {
    let bytes = include_bytes!("../../rhino-mesh/tests/fixtures/materials_v8.3dm");
    let scene = load_preview(Path::new("a.3dm"), bytes).unwrap();
    let material = scene.meshes[0].material.as_ref().unwrap();
    assert!((material.base_color[0] - 0.6038274).abs() < 1e-6); // sRGB 204 -> linear
    assert!((material.base_color[3] - 0.6).abs() < 1e-6);
    assert!(material.roughness < 0.2);
    assert_eq!(material.metallic, 0.);
}

#[test]
fn memory_entry_point_refuses_unguarded_external_buffers() {
    let json = br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"online.bin","byteLength":36}]}"#;
    let error = load_preview(Path::new("a.gltf"), json)
        .unwrap_err()
        .to_string();
    assert!(error.contains("residency-checked reader"));
}
