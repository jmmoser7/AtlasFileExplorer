//! Cached world mesh and its native paint conversion. Geometry is unchanged.
use super::*;

#[cfg(test)]
thread_local! { static LEGACY_COLORS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
pub(crate) fn legacy_mesh_colors_for_test(legacy: bool) {
    LEGACY_COLORS.with(|flag| flag.set(legacy));
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
enum InkEncoding {
    #[default]
    Explicit,
    /// One four-vertex-per-station strip with implied alpha and indices.
    Strip,
    /// The two inner vertices coincide exactly. The three stored rails
    /// retain both feather bands and omit only the zero-area core.
    CoincidentInnerStrip,
}

// The two left-band and two right-band triangles, in the original order.
const COINCIDENT_INNER_LINK: [u32; 12] = [0, 1, 4, 0, 4, 3, 1, 2, 5, 1, 5, 4];

#[derive(Default)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct CachedInkMesh {
    pub(super) vertices: Vec<[f32; 2]>,
    pub(super) alphas: Vec<f32>,
    pub(super) indices: Vec<u32>,
    /// Per-vertex stroke colors (`InkMesh::colors`); empty paints the
    /// caller's one color.
    pub(super) colors: Vec<Color32>,
    encoding: InkEncoding,
}

impl CachedInkMesh {
    pub(crate) fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub(crate) fn append_to_wire_batch(
        &self,
        mesh: &mut egui::Mesh,
        xf: &BoardXf,
        color: Color32,
        vertex_limit: usize,
    ) {
        let existing = mesh.vertices.len();
        let next = existing + self.vertex_count();
        // Dense batches otherwise repeatedly copy their growing vertex and
        // index arrays. Leave one-wire batches at their natural size.
        if existing != 0
            && (2_048..=vertex_limit).contains(&next)
            && mesh.vertices.capacity() < vertex_limit
        {
            mesh.vertices.reserve_exact(vertex_limit - existing);
            // The full AA strip uses up to 18 indices per four vertices.
            let index_capacity = vertex_limit * 18 / 4;
            mesh.indices
                .reserve_exact(index_capacity.saturating_sub(mesh.indices.len()));
        }
        append_ink_mesh_to_epaint(mesh, self, xf, color, |c| c);
    }

    pub(crate) fn from_wire(ink: InkMesh) -> Self {
        if ink.colors.is_empty()
            && ink.vertices.len() >= 8
            && ink.vertices.len() % 4 == 0
            && ink.vertices.chunks_exact(4).all(|station| {
                station
                    .iter()
                    .zip(vector_ink::AA_STRIP_ALPHAS)
                    .all(|(v, alpha)| v.alpha.to_bits() == alpha.to_bits())
            })
            && vector_ink::aa_strip_indices(ink.vertices.len()).eq(ink.indices.iter().copied())
        {
            let stations = ink.vertices.chunks_exact(4);
            let (vertices, encoding) = if stations
                .clone()
                .all(|station| station[1].pos.map(f32::to_bits) == station[2].pos.map(f32::to_bits))
            {
                let mut vertices = Vec::with_capacity(stations.len() * 3);
                for station in stations {
                    vertices.extend_from_slice(&[station[0].pos, station[1].pos, station[3].pos]);
                }
                (vertices, InkEncoding::CoincidentInnerStrip)
            } else {
                (
                    ink.vertices.iter().map(|v| v.pos).collect(),
                    InkEncoding::Strip,
                )
            };
            Self {
                vertices,
                encoding,
                ..Self::default()
            }
        } else {
            Self::from(ink)
        }
    }

    pub(super) fn index_count(&self) -> usize {
        match self.encoding {
            InkEncoding::Explicit => self.indices.len(),
            InkEncoding::Strip => (self.vertices.len() / 4 - 1) * 18,
            InkEncoding::CoincidentInnerStrip => {
                (self.vertices.len() / 3 - 1) * COINCIDENT_INNER_LINK.len()
            }
        }
    }
}

impl From<InkMesh> for CachedInkMesh {
    fn from(ink: InkMesh) -> Self {
        CachedInkMesh {
            vertices: ink.vertices.iter().map(|v| v.pos).collect(),
            alphas: ink.vertices.iter().map(|v| v.alpha).collect(),
            // Tessellation reserves scratch space even for short paths.
            indices: ink.indices.into_boxed_slice().into_vec(),
            colors: ink
                .colors
                .iter()
                .map(|c| {
                    let [r, g, b, a] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
                    Color32::from_rgba_unmultiplied(r, g, b, a)
                })
                .collect(),
            encoding: InkEncoding::Explicit,
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
    let mut mesh = egui::Mesh::default();
    append_ink_mesh_to_epaint(&mut mesh, cached, xf, base_color, fade);
    mesh
}

fn append_ink_mesh_to_epaint(
    mesh: &mut egui::Mesh,
    cached: &CachedInkMesh,
    xf: &BoardXf,
    base_color: Color32,
    fade: impl Fn(Color32) -> Color32,
) {
    use egui::epaint::{Vertex, WHITE_UV};
    #[cfg(test)]
    let _probe_conversion = cache_probe::Conversion::begin(cached);
    let offset = mesh.vertices.len() as u32;
    mesh.vertices.reserve(cached.vertices.len());
    let tinted = cached.colors.len() == cached.vertices.len();
    #[cfg(test)]
    let legacy = LEGACY_COLORS.with(std::cell::Cell::get);
    #[cfg(not(test))]
    let legacy = false;
    if cached.encoding != InkEncoding::Explicit {
        let clear = fade(if legacy {
            base_color.gamma_multiply(0.0)
        } else {
            Color32::TRANSPARENT
        });
        let solid = fade(if legacy {
            base_color.gamma_multiply(1.0)
        } else {
            base_color
        });
        mesh.indices.reserve(cached.index_count());
        if cached.encoding == InkEncoding::CoincidentInnerStrip {
            append_strip_vertices(mesh, &cached.vertices, xf, [clear, solid, clear]);
            for link in 0..cached.vertex_count() as u32 / 3 - 1 {
                let base = offset + link * 3;
                mesh.indices
                    .extend_from_slice(&COINCIDENT_INNER_LINK.map(|index| base + index));
            }
        } else {
            append_strip_vertices(mesh, &cached.vertices, xf, [clear, solid, solid, clear]);
            mesh.indices.extend(
                vector_ink::aa_strip_indices(cached.vertex_count()).map(|index| offset + index),
            );
        }
        return;
    }
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
    mesh.indices
        .extend(cached.indices.iter().map(|index| offset + index));
}

fn append_strip_vertices<const N: usize>(
    mesh: &mut egui::Mesh,
    positions: &[[f32; 2]],
    xf: &BoardXf,
    colors: [Color32; N],
) {
    use egui::epaint::{Vertex, WHITE_UV};
    for station in positions.chunks_exact(N) {
        for (&[x, y], color) in station.iter().zip(colors) {
            mesh.vertices.push(Vertex {
                pos: xf.w2s(Pos2::new(x, y)),
                uv: WHITE_UV,
                color,
            });
        }
    }
}

#[cfg(test)]
mod tests;
