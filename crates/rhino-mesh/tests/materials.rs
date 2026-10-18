use rhino_mesh::read_render_meshes_from;

#[test]
fn standard_materials_and_layer_inheritance_survive_v5_and_v8() {
    for bytes in [
        include_bytes!("fixtures/materials_v5.3dm").as_slice(),
        include_bytes!("fixtures/materials_v8.3dm").as_slice(),
    ] {
        let model = read_render_meshes_from(bytes).unwrap();
        assert_eq!(model.parts.len(), 3);
        let inherited = &model.parts[0];
        assert_eq!(inherited.color, Some([70, 80, 90]));
        let material = inherited.material.as_ref().unwrap();
        assert_eq!(material.diffuse, [204, 51, 26]);
        assert!((material.transparency - 0.4).abs() < 1e-6);
        assert!((material.shine - 64.).abs() < 1e-6);
        assert_eq!(inherited.texcoords, [[0., 0.], [1., 0.], [0., 1.]]);
        assert_eq!(
            model.parts[1].material.as_ref().unwrap().diffuse,
            [26, 179, 77]
        );
        assert_eq!(model.parts[2].color, Some([30, 60, 210]));
        assert!(model.parts[2].material.is_none());
    }
}
