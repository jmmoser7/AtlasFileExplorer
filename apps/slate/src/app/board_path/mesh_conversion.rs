//! Cached world mesh and its native paint conversion. Geometry is unchanged.
use super::*;

#[cfg(test)]
thread_local! { static LEGACY_COLORS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
pub(crate) fn legacy_mesh_colors_for_test(legacy: bool) {
    LEGACY_COLORS.with(|flag| flag.set(legacy));
}

#[derive(Default)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct CachedInkMesh {
    pub(super) vertices: Vec<[f32; 2]>,
    pub(super) alphas: Vec<f32>,
    pub(super) indices: Vec<u32>,
    /// Per-vertex stroke colors (`InkMesh::colors`); empty paints the
    /// caller's one color.
    pub(super) colors: Vec<Color32>,
}

impl From<InkMesh> for CachedInkMesh {
    fn from(ink: InkMesh) -> Self {
        CachedInkMesh {
            vertices: ink.vertices.iter().map(|v| v.pos).collect(),
            alphas: ink.vertices.iter().map(|v| v.alpha).collect(),
            indices: ink.indices,
            colors: ink
                .colors
                .iter()
                .map(|c| {
                    let [r, g, b, a] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
                    Color32::from_rgba_unmultiplied(r, g, b, a)
                })
                .collect(),
        }
    }
}

/// `base_color` and the mesh's own tints come unfaded; `fade` applies the
/// node's opacity to each vertex exactly once.
pub(crate) fn ink_mesh_to_epaint(
    cached: &CachedInkMesh,
    xf: &BoardXf,
    base_color: Color32,
    fade: impl Fn(Color32) -> Color32,
) -> egui::Mesh {
    use egui::epaint::{Vertex, WHITE_UV};
    #[cfg(test)]
    let _probe_conversion = cache_probe::Conversion::begin(cached);
    let mut mesh = egui::Mesh::default();
    mesh.vertices.reserve(cached.vertices.len());
    let tinted = cached.colors.len() == cached.vertices.len();
    #[cfg(test)]
    let legacy = LEGACY_COLORS.with(std::cell::Cell::get);
    #[cfg(not(test))]
    let legacy = false;
    // Coverage endpoints repeat throughout an untinted AA mesh. Apply the
    // same pure opacity mapping once per endpoint, retaining byte rounding.
    let endpoints = (!legacy && !tinted).then(|| (fade(Color32::TRANSPARENT), fade(base_color)));
    for (i, (pos, alpha)) in cached.vertices.iter().zip(cached.alphas.iter()).enumerate() {
        let sp = xf.w2s(Pos2::new(pos[0], pos[1]));
        let base = if tinted { cached.colors[i] } else { base_color };
        let c = if let Some((clear, solid)) = endpoints {
            if *alpha == 0.0 {
                clear
            } else if *alpha == 1.0 {
                solid
            } else {
                fade(base.gamma_multiply(*alpha))
            }
        } else {
            let covered = if !legacy && *alpha == 0.0 {
                Color32::TRANSPARENT
            } else if !legacy && *alpha == 1.0 {
                base
            } else {
                base.gamma_multiply(*alpha)
            };
            fade(covered)
        };
        mesh.vertices.push(Vertex {
            pos: sp,
            uv: WHITE_UV,
            color: c,
        });
    }
    mesh.indices = cached.indices.clone();
    mesh
}

#[cfg(test)]
mod tests;
