#version 330 core
in vec2 v_uv;
uniform sampler2D u_color_tex;
uniform sampler2D u_ao_tex;
uniform sampler2D u_depth_tex;
uniform vec2 u_size;
uniform vec2 u_clip;
out vec4 frag;
float linearDepth(float d){return 2.0*u_clip.x*u_clip.y/(u_clip.y+u_clip.x-(2.0*d-1.0)*(u_clip.y-u_clip.x));}
void main(){
    vec4 color=texture(u_color_tex,v_uv);
    float center=linearDepth(texture(u_depth_tex,v_uv).r);
    float total=0.0,weight=0.0;
    int radius=textureSize(u_ao_tex,0).x<textureSize(u_color_tex,0).x?1:2;
    for(int y=-2;y<=2;y++)for(int x=-2;x<=2;x++){
        if(abs(x)>radius || abs(y)>radius)continue;
        vec2 uv=v_uv+vec2(x,y)/u_size;
        float z=linearDepth(texture(u_depth_tex,uv).r);
        float w=exp(-float(x*x+y*y)*.35)*exp(-abs(z-center)/max(center*.006,.00001));
        total+=texture(u_ao_tex,uv).r*w;weight+=w;
    }
    float ao=total/max(weight,.0001);
    // White surfaces return some bounced light. AO attenuates fill, never the key.
    ao=mix(ao,1.0,.28);
    vec3 linear=pow(color.rgb,vec3(2.2))*(1.0-color.a*(1.0-ao));
    frag=vec4(pow(max(linear,vec3(0.0)),vec3(1.0/2.2)),1.0);
}
