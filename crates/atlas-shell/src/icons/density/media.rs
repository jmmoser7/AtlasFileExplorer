//! Single-coverage material geometry for catalog glyphs placed over imagery.
//!
//! The red vertex channel marks foreground material (1) versus keyline (0);
//! alpha is coverage. Placement resolves those materials to caller colors.
use super::*;

type MaterialEntry = ((Icon, u32, u32), Arc<egui::Mesh>);

thread_local! {
    static CACHE: RefCell<Vec<MaterialEntry>> = const { RefCell::new(Vec::new()) };
}

pub(in crate::icons) fn mesh(icon: Icon, physical_size: f32) -> Arc<egui::Mesh> {
    let source = &sources()[icon.name()];
    let key = (icon, physical_size.to_bits(), source.width.to_bits());
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, mesh)) = cache.iter().find(|(old, _)| *old == key) {
            return Arc::clone(mesh);
        }
        let mesh = Arc::new(build(source, physical_size));
        if cache.len() >= 128 {
            cache.remove(0);
        }
        cache.push((key, Arc::clone(&mesh)));
        mesh
    })
}

fn build(source: &Source, physical_size: f32) -> egui::Mesh {
    let feather = 24.0 / physical_size.max(1.0);
    let foreground = build_source(source, source.width, 0.0);
    let keyline = build_source(source, source.width + 1.0, feather);
    let mut material = vector_ink::InkMesh::default();
    append_material(&mut material, &foreground, true);
    append_material(&mut material, &keyline, false);
    let covered = vector_ink::max_coverage_mesh(&material);
    to_egui(&covered)
}

fn append_material(target: &mut vector_ink::InkMesh, source: &InkMesh, foreground: bool) {
    let base = target.vertices.len() as u32;
    target.vertices.extend(source.vertices.iter().copied());
    target
        .indices
        .extend(source.indices.iter().map(|index| index + base));
    target.colors.extend(
        source
            .vertices
            .iter()
            .map(|_| [if foreground { 1.0 } else { 0.0 }, 0.0, 0.0, 1.0]),
    );
}

fn to_egui(source: &InkMesh) -> egui::Mesh {
    egui::Mesh {
        indices: source.indices.clone(),
        vertices: source
            .vertices
            .iter()
            .zip(source.colors.iter())
            .map(|(vertex, material)| egui::epaint::Vertex {
                pos: egui::pos2(vertex.pos[0], vertex.pos[1]),
                uv: egui::epaint::WHITE_UV,
                color: Color32::from_rgba_unmultiplied(
                    (material[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                    0,
                    0,
                    (vertex.alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
                ),
            })
            .collect(),
        ..Default::default()
    }
}
