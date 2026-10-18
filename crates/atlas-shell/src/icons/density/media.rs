//! Single-coverage material geometry for catalog glyphs placed over imagery.
//!
//! The red vertex channel marks foreground material (1) versus keyline (0);
//! alpha is coverage. Placement resolves those materials to the caller's two
//! colors and applies the caller alpha once.
use super::*;

type MaterialEntry = ((Icon, u32, u32), Arc<egui::Mesh>);

thread_local! {
    static CACHE: RefCell<Vec<MaterialEntry>> = const { RefCell::new(Vec::new()) };
}

pub(in crate::icons) fn mesh(icon: Icon, physical_size: f32) -> Arc<egui::Mesh> {
    let source = &sources()[icon.name()];
    let key = (icon, physical_size.to_bits(), source.width.to_bits());
    CACHE.with(|cache| cached(cache, key, source, physical_size))
}

fn cached(
    cache: &RefCell<Vec<MaterialEntry>>,
    key: (Icon, u32, u32),
    source: &Source,
    physical_size: f32,
) -> Arc<egui::Mesh> {
    let mut cache = cache.borrow_mut();
    if let Some((_, mesh)) = cache.iter().find(|(old, _)| *old == key) {
        return Arc::clone(mesh);
    }
    let mesh = Arc::new(build(source, 24.0 / physical_size.max(1.0)));
    if cache.len() >= 128 {
        cache.remove(0);
    }
    cache.push((key, Arc::clone(&mesh)));
    mesh
}

fn build(source: &Source, feather: f32) -> egui::Mesh {
    let original = region(source, source.width);
    let expanded = region(source, source.width + 1.0);
    let mut mesh = egui::Mesh::default();
    append_region(&mut mesh, &original, true);
    append_ring(&mut mesh, &expanded, &original, feather);
    mesh
}

fn region(source: &Source, width: f32) -> vector_ink::TrimPolys {
    let mut shapes = stroked_subpaths(&source.path, width);
    if source.filled {
        let filled = vector_ink::flatten_contours(&source.path, 0.02);
        if !filled.is_empty() {
            shapes.push(filled);
        }
    }
    vector_ink::boolean_union_all_raw(&shapes)
}

fn stroked_subpaths(path: &BezPath, width: f32) -> Vec<vector_ink::Polygon> {
    split_subpaths(path)
        .iter()
        .map(|part| vector_ink::stroke_outline(part, &stroke_style(width), 0.02))
        .map(|outline| vector_ink::flatten_contours(&outline, 0.02))
        .collect()
}

fn split_subpaths(path: &BezPath) -> Vec<BezPath> {
    let mut parts = Vec::new();
    let mut elements = Vec::new();
    for &element in path.elements() {
        if matches!(element, vector_ink::kurbo::PathEl::MoveTo(_)) && !elements.is_empty() {
            parts.push(elements.drain(..).collect());
        }
        elements.push(element);
    }
    if !elements.is_empty() {
        parts.push(elements.into_iter().collect());
    }
    parts
}

fn append_region(mesh: &mut egui::Mesh, polygons: &[vector_ink::Polygon], foreground: bool) {
    let contours = all_contours(polygons);
    let (positions, indices) = vector_ink::fill_triangles(&contours);
    let base = mesh.vertices.len() as u32;
    mesh.vertices
        .extend(positions.iter().map(|p| material_vertex(*p, foreground)));
    mesh.indices.extend(indices.into_iter().map(|i| i + base));
}

fn append_ring(
    mesh: &mut egui::Mesh,
    expanded: &[vector_ink::Polygon],
    original: &[vector_ink::Polygon],
    feather: f32,
) {
    let mut contours = all_contours(expanded);
    let expanded_count = contours.len();
    contours.extend(all_contours(original));
    let (positions, indices, boundaries) = vector_ink::fill_triangles_with_boundaries(&contours);
    let base = mesh.vertices.len() as u32;
    mesh.vertices
        .extend(positions.iter().map(|p| material_vertex(*p, false)));
    mesh.indices.extend(indices.into_iter().map(|i| i + base));
    for boundary in boundaries {
        if boundary.source_contour < expanded_count {
            let indices: Vec<u32> = boundary.indices.into_iter().map(|i| i + base).collect();
            crate::mesh_aa::feather_media_boundary(mesh, &indices, feather, boundary.hole, 0.5);
        }
    }
}

fn all_contours(polygons: &[vector_ink::Polygon]) -> vector_ink::Polygon {
    polygons
        .iter()
        .flat_map(|poly| poly.iter().cloned())
        .collect()
}

fn material_vertex(pos: [f32; 2], foreground: bool) -> egui::epaint::Vertex {
    egui::epaint::Vertex {
        pos: egui::pos2(pos[0], pos[1]),
        uv: egui::epaint::WHITE_UV,
        color: Color32::from_rgba_unmultiplied(if foreground { 255 } else { 0 }, 0, 0, 255),
    }
}
