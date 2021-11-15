void discard_shape() {
    float r = sqrt(pow(gl_PointCoord.x - .5, 2) + pow(gl_PointCoord.y - .5, 2)) * 2.0;
    if (r > 1.0) {
        discard;
    }
}
