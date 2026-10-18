//! One physical-pixel coverage fringe at every glyph size and display density.
use super::*;

pub(super) mod media;

struct Source {
    path: BezPath,
    filled: bool,
    width: f32,
}

fn sources() -> &'static BTreeMap<&'static str, Source> {
    static SOURCES: OnceLock<BTreeMap<&'static str, Source>> = OnceLock::new();
    SOURCES.get_or_init(|| {
        let defs: BTreeMap<String, Definition> =
            serde_json::from_str(include_str!("../../assets/tool-icons.json"))
                .expect("valid shared icon catalog");
        Icon::ALL
            .iter()
            .map(|&icon| {
                let definition = &defs[icon.name()];
                let generated = frame_sheet_for(icon);
                let mut path = BezPath::from_svg(generated.as_deref().unwrap_or(&definition.path))
                    .expect("valid icon path");
                if let Some([x, y, w, h]) = definition.view_box {
                    let scale = 20.0 / w.max(h);
                    path = vector_ink::kurbo::Affine::translate((
                        (24.0 - w * scale) * 0.5,
                        (24.0 - h * scale) * 0.5,
                    )) * vector_ink::kurbo::Affine::scale(scale)
                        * vector_ink::kurbo::Affine::translate((-x, -y))
                        * path;
                }
                (
                    icon.name(),
                    Source {
                        path,
                        filled: definition.filled,
                        width: definition.stroke_width.unwrap_or(1.5),
                    },
                )
            })
            .collect()
    })
}

fn stroke_style(width: f32) -> StrokeStyle {
    StrokeStyle {
        width,
        cap: Cap::Round,
        join: Join::Round,
        taper: None,
        dash: None,
    }
}

fn build(icon: Icon, feather: f32) -> InkMesh {
    let source = &sources()[icon.name()];
    build_source(source, source.width, feather)
}

fn build_source(source: &Source, width: f32, feather: f32) -> InkMesh {
    let mut mesh = InkMesh::default();
    if source.filled {
        let (vertices, indices) =
            vector_ink::fill_triangles(&vector_ink::flatten_contours(&source.path, 0.02));
        mesh.vertices = vertices
            .into_iter()
            .map(|pos| InkVertex { pos, alpha: 1.0 })
            .collect();
        mesh.indices = indices;
    }
    let stroke = vector_ink::stroke_mesh(&source.path, &stroke_style(width), feather, 0.02);
    let base = mesh.vertices.len() as u32;
    mesh.vertices.extend(stroke.vertices);
    mesh.indices
        .extend(stroke.indices.into_iter().map(|index| index + base));
    mesh
}

#[cfg(test)]
pub(super) fn master_meshes() -> &'static BTreeMap<&'static str, InkMesh> {
    static MESHES: OnceLock<BTreeMap<&'static str, InkMesh>> = OnceLock::new();
    MESHES.get_or_init(|| {
        Icon::ALL
            .iter()
            .map(|&icon| (icon.name(), build(icon, 0.5)))
            .collect()
    })
}

type DensityMeshEntry = ((Icon, u32), Arc<InkMesh>);

thread_local! {
    static CACHE: RefCell<Vec<DensityMeshEntry>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn mesh(icon: Icon, physical_size: f32) -> Arc<InkMesh> {
    let feather = 24.0 / physical_size.max(1.0);
    let key = (icon, feather.to_bits());
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, mesh)) = cache.iter().find(|(old, _)| *old == key) {
            return mesh.clone();
        }
        let mesh = Arc::new(build(icon, feather));
        if cache.len() >= 256 {
            cache.remove(0);
        }
        cache.push((key, mesh.clone()));
        mesh
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icon_coverage_fringe_tracks_device_pixels_and_caches_each_density() {
        let a = mesh(Icon::Rect, 20.0);
        assert!(Arc::ptr_eq(&a, &mesh(Icon::Rect, 20.0)));
        let b = mesh(Icon::Rect, 40.0);
        assert!(!Arc::ptr_eq(&a, &b));
        for physical in [16.0, 20.0, 25.0, 30.0, 40.0] {
            let mesh = mesh(Icon::Rect, physical);
            let edge = mesh
                .vertices
                .iter()
                .filter(|vertex| vertex.alpha == 0.0)
                .map(|vertex| vertex.pos[0])
                .fold(f32::INFINITY, f32::min);
            let solid = mesh
                .vertices
                .iter()
                .filter(|vertex| vertex.alpha == 1.0)
                .map(|vertex| vertex.pos[0])
                .fold(f32::INFINITY, f32::min);
            assert!(
                ((solid - edge) * physical / 24.0 - 1.0).abs() < 0.02,
                "{physical}: {edge} {solid}"
            );
        }
    }
}
