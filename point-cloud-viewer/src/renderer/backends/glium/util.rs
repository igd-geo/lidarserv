use pasture_core::nalgebra::Matrix4;

/// Copies a [Matrix4] matrix from nalgebra into a static array, as
/// understood by glium, and converts it from row-major format (as used by nalgebra)
/// into column-major format (as expected by glium / open gl) during that process.
pub fn matrix_to_gl(mat: &Matrix4<f64>) -> [[f32; 4];4] {
    [
        [mat.m11 as f32, mat.m21 as f32, mat.m31 as f32, mat.m41 as f32],
        [mat.m12 as f32, mat.m22 as f32, mat.m32 as f32, mat.m42 as f32],
        [mat.m13 as f32, mat.m23 as f32, mat.m33 as f32, mat.m43 as f32],
        [mat.m14 as f32, mat.m24 as f32, mat.m34 as f32, mat.m44 as f32],
    ]
}