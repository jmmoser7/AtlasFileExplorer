pub(super) const MODEL_VS: &str = include_str!("surface.vert.glsl");
pub(super) const MODEL_FS: &str = include_str!("surface.frag.glsl");
pub(super) const AO_FS: &str = include_str!("ao.frag.glsl");
pub(super) const COMPOSE_FS: &str = include_str!("compose.frag.glsl");
pub(super) const EDGE_VS: &str = include_str!("edge.vert.glsl");
pub(super) const EDGE_FS: &str = include_str!("edge.frag.glsl");
pub(super) const SHADOW_VS: &str = "#version 330 core\nlayout(location=0) in vec3 a_pos; uniform mat4 u_mvp; void main(){gl_Position=u_mvp*vec4(a_pos,1.0);}";
pub(super) const SHADOW_FS: &str = "#version 330 core\nvoid main(){}";
pub(super) const BG_VS: &str = r#"#version 330 core
out vec2 v_uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

pub(super) const BG_FS: &str = r#"#version 330 core
in vec2 v_uv;
out vec4 frag;
void main() {
    vec3 top = vec3(0.792, 0.831, 0.874);
    vec3 bottom = vec3(0.545, 0.569, 0.600);
    frag = vec4(mix(bottom, top, v_uv.y), 1.0);
}
"#;

/// `u_flip` turns GL's bottom-up rows into egui's top-down texture rows for
/// the live display texture, which is sampled without a readback.
pub(super) const IMAGE_FX_VS: &str = r#"#version 330 core
uniform float u_flip;
out vec2 v_uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = vec2(p.x, mix(p.y, 1.0 - p.y, u_flip));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

pub(super) const IMAGE_FX_FS: &str = r#"#version 330 core
in vec2 v_uv;
uniform sampler2D u_tex;
uniform mat3 u_color_mat;
uniform float u_scale;
uniform float u_offset;
uniform float u_invert;
uniform vec4 u_overlay;
out vec4 frag;
void main() {
    vec4 t = texture(u_tex, v_uv);
    vec3 c = clamp(u_scale * t.rgb + u_offset, 0.0, 1.0);
    c = clamp(u_color_mat * c, 0.0, 1.0);
    if (u_invert > 0.0) {
        c = clamp(mix(c, 1.0 - c, u_invert), 0.0, 1.0);
    }
    if (u_overlay.a > 0.0) {
        float inv_oa = 1.0 - u_overlay.a;
        c = clamp(c * inv_oa + u_overlay.rgb * u_overlay.a, 0.0, 1.0);
    }
    frag = vec4(c, t.a);
}
"#;
