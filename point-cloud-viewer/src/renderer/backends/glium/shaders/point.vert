#version 140

in vec3 position;

uniform mat4 viewProjectionMatrix;
uniform float scaleFactor;

void set_point_size();
void set_point_color();

void main() {
    gl_Position = viewProjectionMatrix * vec4(position, 1.0);
    set_point_size();
    set_point_color();
}
