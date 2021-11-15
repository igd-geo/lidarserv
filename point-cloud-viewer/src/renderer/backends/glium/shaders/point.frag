#version 140

void discard_shape();

in vec4 point_color_frag;
out vec4 color;

void main() {
    discard_shape();
    color = point_color_frag;
}
