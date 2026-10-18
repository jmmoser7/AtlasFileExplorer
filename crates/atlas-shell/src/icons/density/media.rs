//! Single-coverage material geometry for catalog glyphs placed over imagery.
//!
//! The red vertex channel marks foreground material (1) versus keyline (0);
//! alpha is coverage. Placement resolves those materials to caller colors.
use super::*;

type MaterialEntry = ((Icon, u32, u32), Arc<egui::Mesh>);

thread_local! {
    static CACHE: RefCell<Vec<MaterialEntry>> = const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
const PROFILE_RECORD_LIMIT: usize = 32;

#[cfg(test)]
pub(in crate::icons) struct BuildProfileRecord {
    pub icon: &'static str,
    pub physical_size: f32,
    pub physical_size_bits: u32,
    pub source_material_ns: u128,
    pub max_coverage_mesh_ns: u128,
    pub to_egui_ns: u128,
    pub source_vertices: usize,
    pub source_indices: usize,
    pub covered_vertices: usize,
    pub covered_indices: usize,
    pub egui_vertices: usize,
    pub egui_indices: usize,
}

#[cfg(test)]
pub(in crate::icons) struct BuildProfileFrame {
    pub records: Vec<BuildProfileRecord>,
    pub dropped_records: usize,
}

#[cfg(test)]
struct BuildProfileState {
    records: Vec<BuildProfileRecord>,
    dropped_records: usize,
}

#[cfg(test)]
thread_local! {
    static BUILD_PROFILE: RefCell<Option<BuildProfileState>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(in crate::icons) struct BuildProfiler {
    _thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl BuildProfiler {
    pub(in crate::icons) fn enable() -> Self {
        BUILD_PROFILE.with(|profile| {
            let mut profile = profile.borrow_mut();
            assert!(
                profile.is_none(),
                "media build profiler already enabled on this thread"
            );
            *profile = Some(BuildProfileState {
                records: Vec::with_capacity(PROFILE_RECORD_LIMIT),
                dropped_records: 0,
            });
        });
        Self {
            _thread_bound: std::marker::PhantomData,
        }
    }

    pub(in crate::icons) fn drain_frame(&self) -> BuildProfileFrame {
        BUILD_PROFILE.with(|profile| {
            let mut profile = profile.borrow_mut();
            let state = profile
                .as_mut()
                .expect("media build profiler is enabled while draining");
            BuildProfileFrame {
                records: std::mem::take(&mut state.records),
                dropped_records: std::mem::take(&mut state.dropped_records),
            }
        })
    }
}

#[cfg(test)]
impl Drop for BuildProfiler {
    fn drop(&mut self) {
        BUILD_PROFILE.with(|profile| *profile.borrow_mut() = None);
    }
}

pub(in crate::icons) fn mesh(icon: Icon, physical_size: f32) -> Arc<egui::Mesh> {
    let source = &sources()[icon.name()];
    let key = (icon, physical_size.to_bits(), source.width.to_bits());
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, mesh)) = cache.iter().find(|(old, _)| *old == key) {
            return Arc::clone(mesh);
        }
        let mesh = Arc::new(build(icon, source, physical_size));
        if cache.len() >= 128 {
            cache.remove(0);
        }
        cache.push((key, Arc::clone(&mesh)));
        mesh
    })
}

fn build(_icon: Icon, source: &Source, physical_size: f32) -> egui::Mesh {
    #[cfg(test)]
    let profile_enabled = BUILD_PROFILE.with(|profile| profile.borrow().is_some());
    #[cfg(test)]
    let source_started = profile_enabled.then(std::time::Instant::now);
    let material = source_material(source, physical_size);
    #[cfg(test)]
    let source_material_ns = source_started.map_or(0, |start| start.elapsed().as_nanos());

    #[cfg(test)]
    let coverage_started = profile_enabled.then(std::time::Instant::now);
    let covered = vector_ink::max_coverage_mesh(&material);
    #[cfg(test)]
    let max_coverage_mesh_ns = coverage_started.map_or(0, |start| start.elapsed().as_nanos());

    #[cfg(test)]
    let adapter_started = profile_enabled.then(std::time::Instant::now);
    let mesh = to_egui(&covered);
    #[cfg(test)]
    let to_egui_ns = adapter_started.map_or(0, |start| start.elapsed().as_nanos());

    #[cfg(test)]
    if profile_enabled {
        BUILD_PROFILE.with(|profile| {
            if let Some(state) = profile.borrow_mut().as_mut() {
                if state.records.len() < PROFILE_RECORD_LIMIT {
                    state.records.push(BuildProfileRecord {
                        icon: _icon.name(),
                        physical_size,
                        physical_size_bits: physical_size.to_bits(),
                        source_material_ns,
                        max_coverage_mesh_ns,
                        to_egui_ns,
                        source_vertices: material.vertices.len(),
                        source_indices: material.indices.len(),
                        covered_vertices: covered.vertices.len(),
                        covered_indices: covered.indices.len(),
                        egui_vertices: mesh.vertices.len(),
                        egui_indices: mesh.indices.len(),
                    });
                } else {
                    state.dropped_records = state.dropped_records.saturating_add(1);
                }
            }
        });
    }

    mesh
}

#[cfg(test)]
pub(in crate::icons) fn source_material_for_audit(icon: Icon, physical_size: f32) -> InkMesh {
    source_material(&sources()[icon.name()], physical_size)
}

fn source_material(source: &Source, physical_size: f32) -> InkMesh {
    let feather = 24.0 / physical_size.max(1.0);
    let foreground = build_source(source, source.width, 0.0);
    let keyline = build_source(source, source.width + 1.0, feather);
    let mut material = vector_ink::InkMesh::default();
    append_material(&mut material, &foreground, true);
    append_material(&mut material, &keyline, false);
    material
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
