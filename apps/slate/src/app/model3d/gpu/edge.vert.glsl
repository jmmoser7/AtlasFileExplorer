#version 330 core
layout(location=0) in vec3 a_start;
layout(location=1) in vec3 a_end;
layout(location=2) in vec3 a_normal0;
layout(location=3) in vec3 a_normal1;
layout(location=4) in float a_flags;
uniform mat4 u_mvp;
uniform vec3 u_eye;
uniform vec2 u_size;
out float v_side;
out float v_silhouette;
void main(){
    const vec2 corners[6]=vec2[6](vec2(-1,0),vec2(1,0),vec2(1,1),vec2(-1,0),vec2(1,1),vec2(-1,1));
    vec2 corner=corners[gl_VertexID];
    vec3 sight=u_eye-(a_start+a_end)*.5;
    bool silhouette=dot(a_normal0,sight)*dot(a_normal1,sight)<0.0;
    bool boundary=a_flags>=2.0;
    bool crease=mod(a_flags,2.0)>.5;
    if(!silhouette && !boundary && !crease){gl_Position=vec4(2.0,2.0,2.0,1.0);return;}
    vec4 p=u_mvp*vec4(a_start,1.0), q=u_mvp*vec4(a_end,1.0);
    if(p.w<=0.0 || q.w<=0.0){gl_Position=vec4(2.0,2.0,2.0,1.0);return;}
    vec2 d=(q.xy/q.w-p.xy/p.w)*u_size;
    vec2 normal=vec2(-d.y,d.x)/max(length(d),.0001);
    vec4 point=mix(p,q,corner.y);
    float width=(silhouette||boundary)?1.7:1.25;
    point.xy+=normal*corner.x*width/u_size*point.w;
    point.z-=.000002*point.w;
    gl_Position=point;v_side=corner.x;v_silhouette=(silhouette||boundary)?1.0:0.0;
}
