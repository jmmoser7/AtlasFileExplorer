#version 330 core
in vec3 v_nrm;
in vec3 v_pos;
in vec2 v_uv;
in vec4 v_shadow;
uniform vec3 u_color;
uniform vec3 u_mask;
uniform vec3 u_emission;
uniform vec3 u_light;
uniform vec3 u_up;
uniform vec3 u_depth;
uniform float u_mode;
uniform float u_materials;
uniform float u_ground;
uniform float u_roughness;
uniform float u_metallic;
uniform float u_texture;
uniform sampler2D u_base_tex;
uniform sampler2D u_shadow_tex;
out vec4 frag;
const vec2 disk[16]=vec2[16](vec2(-.94,-.4),vec2(.94,-.76),vec2(-.09,-.93),vec2(.34,.29),
vec2(-.91,.46),vec2(-.81,-.88),vec2(-.38,.28),vec2(.97,.76),vec2(.44,-.98),
vec2(.54,-.47),vec2(-.26,-.42),vec2(.79,.19),vec2(-.24,.99),vec2(-.81,.91),vec2(.2,.79),vec2(.14,-.14));
float shadow(vec3 n) {
    vec3 q=v_shadow.xyz/v_shadow.w*.5+.5;
    if(any(lessThan(q,vec3(0.0)))||any(greaterThan(q,vec3(1.0)))) return 1.0;
    float bias=max(.00035*(1.0-max(dot(n,u_light),0.0)),.00012);
    float blockers=0.0, count=0.0;
    for(int i=0;i<8;i++) {
        float d=texture(u_shadow_tex,q.xy+disk[i]*.0025).r;
        if(d<q.z-bias){blockers+=d;count+=1.0;}
    }
    if(count<.5) return 1.0;
    float penumbra=clamp((q.z-blockers/count)*.10,.0008,.007);
    float visible=0.0;
    for(int i=0;i<16;i++) visible+=step(q.z-bias,texture(u_shadow_tex,q.xy+disk[i]*penumbra).r);
    return visible/16.0;
}
void main() {
    vec3 n=normalize(v_nrm), v=normalize(-v_pos);
    if(dot(n,v)<0.0)n=-n;
    if(u_mode>2.5 && u_mode<3.5) {
        // View-space distance, not reciprocal projection depth. When the eye
        // enters the model bounds the near limit approaches zero; reciprocal
        // normalization otherwise crushes all remaining surfaces to black.
        float d=clamp((u_depth.z-max(-v_pos.z,0.0))/max(u_depth.z-u_depth.y,1e-9),0.0,1.0);
        frag=vec4(vec3(d),1.0);return;
    }
    if(u_mode>1.5 && u_mode<2.5) {
        float edge=fwidth(n.x)+fwidth(n.y)+fwidth(n.z);
        frag=vec4(mix(u_mask,vec3(0.0),clamp(edge*1.6,0.0,1.0)),1.0);return;
    }
    bool clay=u_mode>.5 && u_mode<1.5;
    bool diagram=u_mode>3.5;
    vec3 base=(clay||u_materials<.5)?vec3(.91):u_color;
    if(clay && u_ground>.5)base=vec3(.82);
    if(!clay && u_materials>.5 && u_texture>.5)base*=texture(u_base_tex,v_uv).rgb;
    float ndl=max(dot(n,u_light),0.0);
    float hemi=.5+.5*dot(n,u_up);
    vec3 ambient=base*(.53+.15*hemi);
    vec3 direct=base*ndl*.43*shadow(n);
    if(diagram){ ambient=base*(.90+.10*ndl);direct=vec3(0.0); }
    if(!clay && !diagram && u_materials>.5) {
        float rough=clamp(u_roughness,.08,1.0);
        vec3 h=normalize(u_light+v);
        vec3 f0=mix(vec3(.04),base,u_metallic);
        float power=max(2.0,2.0/(rough*rough)-2.0);
        direct+=f0*pow(max(dot(n,h),0.0),power)*(.5+u_metallic)*shadow(n);
    }
    vec3 emission=(!clay && u_materials>.5)?u_emission:vec3(0.0);
    vec3 col=ambient+direct+emission;
    // Neutral highlight shoulder; preserve subtle differences in white faces.
    float peak=max(max(col.r,col.g),col.b);
    col/=1.0+max(peak-.9,0.0)*.4;
    float share=clay?clamp(dot(ambient,vec3(.333))/max(dot(ambient+direct+emission,vec3(.333)),.001),0.0,1.0):1.0;
    frag=vec4(pow(max(col,vec3(0.0)),vec3(1.0/2.2)),share);
}
