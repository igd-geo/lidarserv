in int point_color_attribute;

out vec4 point_color_frag;

uniform int point_color_max;
uniform vec3 point_color_default;
uniform sampler1D point_color_texture;

void set_point_color() {

    // use defsult color for points outside of min-max
    if (point_color_attribute < 0) {
        point_color_frag = vec4(point_color_default, 1.0);
    }
    if (point_color_attribute > point_color_max) {
        point_color_frag = vec4(point_color_default, 1.0);
    }

    // texture coordinate
    float f = (float(point_color_attribute) + 0.5) / float(point_color_max + 1);

    // look up color
    vec3 col = texture(point_color_texture, f).rgb;
    point_color_frag = vec4(col, 1.0);
}
