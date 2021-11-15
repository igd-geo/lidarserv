#version 140

in vec2 position;

uniform mat4 view_projection_matrix;
uniform float x_min;
uniform float x_max;
uniform float y_min;
uniform float y_max;

void main() {
    vec2 size = vec2(x_max - x_min, y_max - y_min);
    vec2 pos = vec2(x_min, y_min);
    gl_Position = view_projection_matrix * vec4(position * size + pos, 0.0, 1.0);
}
