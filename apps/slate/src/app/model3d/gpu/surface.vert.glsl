#version 330 core
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_nrm;
layout(location=2) in vec2 a_uv;
uniform mat4 u_mvp;
uniform mat4 u_view;
uniform mat4 u_shadow_matrix;
out vec3 v_nrm;
out vec3 v_pos;
out vec2 v_uv;
out vec4 v_shadow;
void main() {
    gl_Position=u_mvp*vec4(a_pos,1.0);
    v_pos=(u_view*vec4(a_pos,1.0)).xyz;
    v_nrm=mat3(u_view)*a_nrm;
    v_uv=a_uv;
    v_shadow=u_shadow_matrix*vec4(a_pos,1.0);
}
