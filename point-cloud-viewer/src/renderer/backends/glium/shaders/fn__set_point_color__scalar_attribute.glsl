
out vec4 point_color_frag;

uniform float point_color_min;
uniform float point_color_max;
uniform sampler1D point_color_texture;

float read_point_color_scalar_attribute();

void set_point_color() {

    // get texture coordinate, such that point_color_min maps to 0.0 and point_color_max max maps to 1.0
    float f = clamp((read_point_color_scalar_attribute() - point_color_min) / (point_color_max - point_color_min), 0.0, 1.0);

    // begin / end in the center of the first/last pixel, rather than on the edge of that pixel.
    float texture_size = 128.0;
    f = f / texture_size * (texture_size - 1) + 0.5 / texture_size;

    // look up color
    vec3 col = texture(point_color_texture, f).rgb;
    point_color_frag = vec4(col, 1.0);
}
