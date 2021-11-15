uniform float point_size_depth;

void set_point_size() {
    gl_PointSize = max(1.0, point_size_depth * scaleFactor / gl_Position.w);
}
