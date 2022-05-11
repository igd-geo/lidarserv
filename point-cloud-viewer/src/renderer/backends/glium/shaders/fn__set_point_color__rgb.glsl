
out vec4 point_color_frag;
in vec3 point_color_rgb;

float read_point_color_scalar_attribute();

void set_point_color() {
    point_color_frag = vec4(point_color_rgb, 1.0);
}
