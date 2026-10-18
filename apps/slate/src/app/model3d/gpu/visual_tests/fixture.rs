//! Architectural geometry and assets used by the production GPU checks.
use super::*;

/// A pavilion with overhangs, a tower, deep reveals, steps and a pergola.
/// Winding is outward; OBJ deliberately duplicates triangle vertices during
/// import, exercising the production welded-edge path rather than a test mesh.
pub(super) fn architectural_obj() -> String {
    let mut obj = String::from("mtllib pavilion.mtl\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n");
    let mut vertex = 1;
    let boxes = [
        ([-4.3, -3.1, 0.0], [4.1, 3.0, 0.18], "concrete"),
        ([-2.8, -1.35, 0.18], [0.6, 1.65, 2.5], "brick"),
        ([0.6, 0.1, 0.18], [2.5, 2.2, 4.2], "concrete"),
        ([0.25, -0.25, 4.2], [2.85, 2.55, 4.4], "concrete"),
        ([-3.3, -2.25, 2.5], [1.0, 1.9, 2.7], "concrete"),
        ([-3.0, -2.05, 0.18], [-2.84, -1.89, 2.5], "concrete"),
        ([0.5, -2.05, 0.18], [0.66, -1.89, 2.5], "concrete"),
        ([-2.6, -1.43, 0.4], [-1.1, -1.36, 2.28], "glazing"),
        ([-0.96, -1.43, 0.4], [0.4, -1.36, 2.28], "glazing"),
        ([0.85, 0.0, 2.9], [2.25, 0.08, 3.96], "glazing"),
        ([-3.75, -3.6, 0.0], [2.1, -3.1, 0.06], "concrete"),
        ([-3.5, -3.35, 0.06], [1.85, -2.95, 0.12], "concrete"),
        ([-3.25, -3.1, 0.12], [1.6, -2.7, 0.18], "concrete"),
        ([2.65, -1.9, 0.18], [2.8, -1.75, 2.15], "timber"),
        ([3.65, -1.9, 0.18], [3.8, -1.75, 2.15], "timber"),
        ([2.6, -2.0, 2.15], [3.85, 0.0, 2.3], "timber"),
    ];
    for (min, max, material) in boxes {
        writeln!(obj, "usemtl {material}").unwrap();
        for [x, y, z] in [
            [min[0], min[1], min[2]],
            [max[0], min[1], min[2]],
            [max[0], max[1], min[2]],
            [min[0], max[1], min[2]],
            [min[0], min[1], max[2]],
            [max[0], min[1], max[2]],
            [max[0], max[1], max[2]],
            [min[0], max[1], max[2]],
        ] {
            writeln!(obj, "v {x} {y} {z}").unwrap();
        }
        for face in [
            [0, 3, 2, 1],
            [4, 5, 6, 7],
            [0, 1, 5, 4],
            [1, 2, 6, 5],
            [2, 3, 7, 6],
            [3, 0, 4, 7],
        ] {
            writeln!(
                obj,
                "f {}/1 {}/2 {}/3 {}/4",
                vertex + face[0],
                vertex + face[1],
                vertex + face[2],
                vertex + face[3]
            )
            .unwrap();
        }
        vertex += 8;
    }
    obj
}

pub(super) const VISIBLE_POST: [[f32; 3]; 2] = [[3.8, -1.75, 0.4], [3.8, -1.75, 1.9]];
pub(super) const WHITE_ROOFS: [[f32; 3]; 3] =
    [[-1.2, -0.1, 2.7], [1.5, 1.0, 4.4], [3.2, -1.0, 2.3]];

pub(super) fn write_scene(directory: &Path) -> Result<PreviewScene, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let source = architectural_obj();
    let path = directory.join("pavilion.obj");
    std::fs::write(&path, &source).map_err(|e| e.to_string())?;
    std::fs::write(directory.join("pavilion.mtl"),
        "newmtl concrete\nKd 0.75 0.73 0.69\nPr 0.85\n\nnewmtl brick\nKd 0.65 0.18 0.08\nPr 0.8\n\nnewmtl glazing\nKd 0.08 0.35 0.45\nPr 0.2\n\nnewmtl timber\nKd 0.42 0.23 0.08\nPr 0.7\nmap_Kd pavilion-base.png\n")
        .map_err(|e| e.to_string())?;
    image::RgbaImage::from_fn(8, 8, |x, _| {
        image::Rgba(if x % 3 == 0 {
            [190, 175, 140, 255]
        } else {
            [255, 245, 220, 255]
        })
    })
    .save(directory.join("pavilion-base.png"))
    .map_err(|e| e.to_string())?;
    let mut scene = model_preview::load_preview_with_reader(
        &path,
        source.as_bytes(),
        &mut super::super::super::materials::read_dependency,
    )
    .map_err(|e| e.to_string())?;
    if !scene.meshes.iter().any(|mesh| mesh.material.is_some()) {
        return Err("Visual fixture must exercise imported materials".into());
    }
    super::super::super::materials::decode_textures(&mut scene);
    Ok(scene)
}
