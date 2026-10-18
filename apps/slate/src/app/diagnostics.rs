//! Infrequent graphics identity captured by the existing activity recorder.
use eframe::glow::{self, HasContext};

pub(super) fn record_graphics(gl: Option<&glow::Context>) {
    let Some(gl) = gl else {
        atlas_core::session_log::event("slate.graphics", "OpenGL context unavailable");
        return;
    };
    // CreationContext owns the current GL context. Only driver identity and
    // numeric limits enter diagnostics; no document text or secret values.
    let identity = unsafe {
        format!(
            "renderer=glow; vendor={}; device={}; version={}; max_texture_side={}; msaa={}",
            gl.get_parameter_string(glow::VENDOR),
            gl.get_parameter_string(glow::RENDERER),
            gl.get_parameter_string(glow::VERSION),
            gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE),
            atlas_shell::NATIVE_MSAA_SAMPLES
        )
    };
    atlas_core::session_log::event("slate.graphics", &identity);
}
