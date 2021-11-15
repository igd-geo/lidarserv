uniform float point_size_fixed;

void set_point_size() {
    gl_PointSize = point_size_fixed * scaleFactor;
}
