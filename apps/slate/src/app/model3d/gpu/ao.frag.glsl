#version 330 core
in vec2 v_uv;
uniform sampler2D u_depth_tex;
// near, far, tan(fov/2), aspect
uniform vec4 u_projection;
uniform vec2 u_size;
uniform float u_radius;
out float frag;
vec3 position(vec2 uv) {
    float d=texture(u_depth_tex,uv).r;
    float z=2.0*u_projection.x*u_projection.y/(u_projection.y+u_projection.x-(2.0*d-1.0)*(u_projection.y-u_projection.x));
    return vec3((uv*2.0-1.0)*vec2(u_projection.w,1.0)*u_projection.z*z,-z);
}
void main() {
    if(texture(u_depth_tex,v_uv).r>.999999){frag=1.0;return;}
    vec2 px=1.0/u_size;
    vec3 p=position(v_uv);
    vec3 l=p-position(v_uv-vec2(px.x,0)),r=position(v_uv+vec2(px.x,0))-p;
    vec3 b=p-position(v_uv-vec2(0,px.y)),t=position(v_uv+vec2(0,px.y))-p;
    vec3 n=normalize(cross(abs(l.z)<abs(r.z)?l:r,abs(b.z)<abs(t.z)?b:t));
    if(dot(n,-p)<0.0)n=-n;
    float radiusPx=clamp(u_radius*u_size.y/(2.0*u_projection.z*max(-p.z,.001)),2.0,90.0);
    float rotation=fract(dot(mod(floor(v_uv*u_size),4.0),vec2(.25,.5625)))*6.2831853;
    float occ=0.0;
    for(int dir=0;dir<8;dir++) {
        float angle=rotation+float(dir)*.78539816;
        vec2 ray=vec2(cos(angle),sin(angle));
        float horizon=0.0;
        for(int s=1;s<=3;s++) {
            vec2 uv=v_uv+ray*(radiusPx*float(s)/3.0+.5)*px;
            if(any(lessThan(uv,vec2(0.0)))||any(greaterThan(uv,vec2(1.0))))continue;
            if(texture(u_depth_tex,uv).r>.999999)continue;
            vec3 delta=position(uv)-p;
            float dist=length(delta);
            float range=clamp(1.0-dist/u_radius,0.0,1.0);
            // Finite radius limits thin-surface halos and distant occluders.
            float elevation=max(dot(n,delta/max(dist,.00001))-.08,0.0);
            horizon=max(horizon,elevation*range);
        }
        occ+=horizon;
    }
    frag=clamp(1.0-occ/8.0*1.8,.18,1.0);
}
