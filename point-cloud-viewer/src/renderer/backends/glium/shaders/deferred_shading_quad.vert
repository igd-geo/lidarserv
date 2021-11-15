#version 140

in vec2 position;

out vec2 frag_screen_coord;

void main() {
    frag_screen_coord = position;
    gl_Position = vec4(position, 0.0, 1.0);
}
