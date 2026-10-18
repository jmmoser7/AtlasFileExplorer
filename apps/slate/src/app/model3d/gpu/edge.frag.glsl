#version 330 core
in float v_side;
in float v_silhouette;
out vec4 frag;
void main(){frag=vec4(vec3(mix(.32,.16,v_silhouette)),1.0-smoothstep(.55,1.0,abs(v_side)));}
