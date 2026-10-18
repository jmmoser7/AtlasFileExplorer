//! The one GPU interpreter for model views, frozen posters and captures.
use super::*;

mod filters;
mod lighting;
mod shaders;
mod surface;
mod targets;
mod upload;
#[cfg(test)]
mod visual_tests;
use shaders::*;

const GROUND_EXTENT: f32 = 60.0;
const DEFAULT_PART_COLOR: [f32; 3] = [0.78, 0.78, 0.76];

struct DrawRange {
    offset: i32,
    count: i32,
    color: [f32; 3],
    mask: [f32; 3],
    roughness: f32,
    metallic: f32,
    emission: [f32; 3],
    texture: Option<glow::Texture>,
}

pub struct GpuModel {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    draws: Vec<DrawRange>,
    textures: Vec<glow::Texture>,
    ground_offset: i32,
    edge_vao: glow::VertexArray,
    edge_vbo: glow::Buffer,
    edge_count: i32,
    shadow: lighting::Shadow,
    pub(super) bounds_min: [f32; 3],
    pub(super) bounds_max: [f32; 3],
}

pub struct ModelEngine {
    gl: Arc<glow::Context>,
    program: glow::Program,
    bg_program: glow::Program,
    bg_vao: glow::VertexArray,
    fx_program: glow::Program,
    ao_program: glow::Program,
    compose_program: glow::Program,
    shadow_program: glow::Program,
    edge_program: glow::Program,
    uniforms: HashMap<(glow::Program, &'static str), Option<glow::UniformLocation>>,
    u_fx_tex: glow::UniformLocation,
    u_fx_mat: glow::UniformLocation,
    u_fx_scale: glow::UniformLocation,
    u_fx_offset: glow::UniformLocation,
    u_fx_invert: glow::UniformLocation,
    u_fx_overlay: glow::UniformLocation,
    u_fx_flip: glow::UniformLocation,
    srgb_display: bool,
    srgb_framebuffer: bool,
    pub(super) render_passes: AtomicU64,
    pub(super) readbacks: AtomicU64,
    pub(super) gl_creates: AtomicU64,
}

pub(super) struct LiveTarget {
    msaa_fbo: glow::Framebuffer,
    color_rb: glow::Renderbuffer,
    depth_rb: glow::Renderbuffer,
    resolve_fbo: glow::Framebuffer,
    resolve_tex: glow::Texture,
    depth_tex: glow::Texture,
    ao_fbo: glow::Framebuffer,
    ao_tex: glow::Texture,
    compose_fbo: glow::Framebuffer,
    compose_tex: glow::Texture,
    out_fbo: glow::Framebuffer,
    pub(super) display: glow::Texture,
    pub(super) egui_id: Option<egui::TextureId>,
    pub(super) size: (i32, i32),
}

const LIVE_TARGET_IDLE_PX: i32 = 16;

fn compile_program(gl: &glow::Context, vs: &str, fs: &str) -> Option<glow::Program> {
    unsafe {
        let program = gl.create_program().ok()?;
        for (kind, source) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let shader = gl.create_shader(kind).ok()?;
            gl.shader_source(shader, source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                eprintln!("Slate model shader: {}", gl.get_shader_info_log(shader));
                gl.delete_shader(shader);
                gl.delete_program(program);
                return None;
            }
            gl.attach_shader(program, shader);
            gl.delete_shader(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            eprintln!("Slate model pipeline: {}", gl.get_program_info_log(program));
            gl.delete_program(program);
            return None;
        }
        Some(program)
    }
}

impl ModelEngine {
    pub fn new(gl: Arc<glow::Context>) -> Option<Self> {
        let program = compile_program(&gl, MODEL_VS, MODEL_FS)?;
        let bg_program = compile_program(&gl, BG_VS, BG_FS)?;
        let fx_program = compile_program(&gl, IMAGE_FX_VS, IMAGE_FX_FS)?;
        let ao_program = compile_program(&gl, BG_VS, AO_FS)?;
        let compose_program = compile_program(&gl, BG_VS, COMPOSE_FS)?;
        let shadow_program = compile_program(&gl, SHADOW_VS, SHADOW_FS)?;
        let edge_program = compile_program(&gl, EDGE_VS, EDGE_FS)?;
        unsafe {
            let mut uniforms = HashMap::new();
            for p in [
                program,
                ao_program,
                compose_program,
                shadow_program,
                edge_program,
            ] {
                for name in [
                    "u_mvp",
                    "u_view",
                    "u_shadow_matrix",
                    "u_light",
                    "u_up",
                    "u_depth",
                    "u_mode",
                    "u_materials",
                    "u_ground",
                    "u_shadow_tex",
                    "u_base_tex",
                    "u_color",
                    "u_mask",
                    "u_emission",
                    "u_roughness",
                    "u_metallic",
                    "u_texture",
                    "u_eye",
                    "u_size",
                    "u_depth_tex",
                    "u_projection",
                    "u_radius",
                    "u_color_tex",
                    "u_ao_tex",
                    "u_clip",
                ] {
                    uniforms.insert((p, name), gl.get_uniform_location(p, name));
                }
            }
            Some(Self {
                bg_vao: gl.create_vertex_array().ok()?,
                u_fx_tex: gl.get_uniform_location(fx_program, "u_tex")?,
                u_fx_mat: gl.get_uniform_location(fx_program, "u_color_mat")?,
                u_fx_scale: gl.get_uniform_location(fx_program, "u_scale")?,
                u_fx_offset: gl.get_uniform_location(fx_program, "u_offset")?,
                u_fx_invert: gl.get_uniform_location(fx_program, "u_invert")?,
                u_fx_overlay: gl.get_uniform_location(fx_program, "u_overlay")?,
                u_fx_flip: gl.get_uniform_location(fx_program, "u_flip")?,
                srgb_display: gl.supported_extensions().iter().any(|e| e.contains("sRGB")),
                srgb_framebuffer: gl
                    .supported_extensions()
                    .iter()
                    .any(|e| e.contains("framebuffer_sRGB")),
                gl,
                program,
                bg_program,
                fx_program,
                ao_program,
                compose_program,
                shadow_program,
                edge_program,
                uniforms,
                render_passes: AtomicU64::new(0),
                readbacks: AtomicU64::new(0),
                gl_creates: AtomicU64::new(0),
            })
        }
    }

    fn uniform(&self, program: glow::Program, name: &'static str) -> Option<glow::UniformLocation> {
        self.uniforms.get(&(program, name)).cloned().flatten()
    }

    pub fn render(
        &self,
        model: &GpuModel,
        cam: &ModelCamera,
        w: u32,
        h: u32,
        adjust: Option<&ImageAdjust>,
    ) -> Option<egui::ColorImage> {
        let mut target = self.new_live_target()?;
        let rendered = self.render_frame(
            &mut target,
            model,
            cam,
            (w, h, w as f32 / h as f32),
            adjust,
            false,
        );
        let image = rendered.then(|| self.read_target(&target)).flatten();
        self.free_target(target);
        image
    }

    pub fn render_capture(
        &self,
        model: &GpuModel,
        cam: &ModelCamera,
        w: u32,
        h: u32,
        depth: bool,
        adjust: Option<&ImageAdjust>,
    ) -> Option<egui::ColorImage> {
        let mut pose = *cam;
        pose.display = if depth {
            ModelDisplay::Depth
        } else {
            ModelDisplay::Shaded
        };
        let mut target = self.new_live_target()?;
        let rendered = self.render_frame(
            &mut target,
            model,
            &pose,
            (w, h, w as f32 / h as f32),
            adjust,
            true,
        );
        let image = rendered.then(|| self.read_target(&target)).flatten();
        self.free_target(target);
        image
    }
}

pub fn mask_palette(index: usize) -> [f32; 3] {
    let h = (index as f32 * 0.618_034).fract() * 6.0;
    let (p, q, t) = (
        0.2576,
        0.92 * (1.0 - 0.72 * h.fract()),
        0.92 * (1.0 - 0.72 * (1.0 - h.fract())),
    );
    match h.floor() as i32 % 6 {
        0 => [0.92, t, p],
        1 => [q, 0.92, p],
        2 => [p, 0.92, t],
        3 => [p, q, 0.92],
        4 => [t, p, 0.92],
        _ => [0.92, p, q],
    }
}
