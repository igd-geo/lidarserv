#version 140

uniform sampler2D color_texture;
uniform sampler2D depth_texture;
uniform float size_x;
uniform float size_y;
uniform mat4 inverse_projection_matrix;

in vec2 frag_screen_coord;

out vec4 color;

vec3 unproject(vec2 screen_coord) {
    vec2 tex_coord = screen_coord * 0.5 + vec2(0.5, 0.5);
    float depth = texture(depth_texture, tex_coord).x;
    vec4 position_hom = inverse_projection_matrix * vec4(screen_coord, depth, 1.0);
    return position_hom.xyz / position_hom.w;
}

vec3 unproject_smooth_z(vec2 screen_coord) {
    float dx = 2.0 / size_x;
    float dy = 2.0 / size_y;
    vec3 p00 = unproject(screen_coord + vec2(-dx, -dy));
    vec3 p01 = unproject(screen_coord + vec2(-dx, 0));
    vec3 p02 = unproject(screen_coord + vec2(-dx, dy));
    vec3 p10 = unproject(screen_coord + vec2(0, -dy));
    vec3 p11 = unproject(screen_coord + vec2(0, 0));
    vec3 p12 = unproject(screen_coord + vec2(0, dy));
    vec3 p20 = unproject(screen_coord + vec2(dx, -dy));
    vec3 p21 = unproject(screen_coord + vec2(dx, 0));
    vec3 p22 = unproject(screen_coord + vec2(dx, dy));
    float f1 = 0.51503; // gaussian with variance 0.6, sampled at 0.0, 1.0 and sqrt(2)
    float f2 = 0.22383;
    float f3 = 0.09728;
    float z = (
        p00.z * f3 + p01.z * f2 + p02.z * f3 +
        p10.z * f2 + p11.z * f1 + p12.z * f2 +
        p20.z * f3 + p21.z * f2 + p22.z * f3)
    / (4 * f3 + 4 * f2 + f1);
    return vec3(p11.xy, z);
}

void main() {
    vec2 tex_coordinates = frag_screen_coord * 0.5 + vec2(0.5, 0.5);
    float frag_depth_value = texture(depth_texture, tex_coordinates).x;
    vec4 frag_color = texture(color_texture, tex_coordinates);

    float rx = 2.0 / size_x;
    float ry = 2.0 / size_y;
    vec3 sample_1 = unproject_smooth_z(frag_screen_coord + vec2(rx, 0.0));
    vec3 sample_2 = unproject_smooth_z(frag_screen_coord + vec2(0.0, ry));
    vec3 sample_3 = unproject_smooth_z(frag_screen_coord + vec2(0.0, -ry));
    vec3 sample_4 = unproject_smooth_z(frag_screen_coord + vec2(-rx, 0));

    vec3 max_z_sample = sample_1;
    if (sample_2.z > max_z_sample.z) max_z_sample = sample_2;
    if (sample_3.z > max_z_sample.z) max_z_sample = sample_3;
    if (sample_4.z > max_z_sample.z) max_z_sample = sample_4;

    vec3 pos_center = unproject(frag_screen_coord);
    float z_diff = max(max_z_sample.z - pos_center.z, 0.0);
    float xy_diff = length(pos_center.xy - max_z_sample.xy);
    float light = normalize(vec2(xy_diff, z_diff)).x;

    color = vec4(frag_color.rgb * light, frag_color.a);
    gl_FragDepth = frag_depth_value;
}
