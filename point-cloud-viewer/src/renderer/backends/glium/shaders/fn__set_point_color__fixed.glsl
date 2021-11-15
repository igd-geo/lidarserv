
out vec4 point_color_frag;
uniform vec3 point_color_fixed;

void set_point_color() {
    point_color_frag = vec4(point_color_fixed, 1.0);
}
