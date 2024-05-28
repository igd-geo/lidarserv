//! This is not an example.
//! It just contains code to create/load some point clouds, that is used by the examples.
use bytemuck::{Pod, Zeroable};
use pasture_core::containers::VectorBuffer;
use pasture_core::nalgebra::{Point3, Vector3};
use pasture_derive::PointType;

/// A simple point type with a position
/// and intensity and classification attributes
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Default, PointType, Pod, Zeroable)]
pub struct SimplePoint {
    #[pasture(BUILTIN_POSITION_3D)]
    pub position: Vector3<f64>,
    #[pasture(BUILTIN_INTENSITY)]
    pub intensity: u16,
    #[pasture(BUILTIN_CLASSIFICATION)]
    pub classification: u8,
    #[pasture(BUILTIN_COLOR_RGB)]
    pub color: Vector3<u16>,
}

pub fn small_example_point_cloud(center: Point3<f64>, nr_points: usize) -> VectorBuffer {
    (0..nr_points)
        .map(|i| {
            let r = i as f64 / 100.0;
            let angle = i as f64 / 100.0 * 6.0 * std::f64::consts::PI; // 3 full rotations per 100 points
            let x = r * angle.sin();
            let y = r * angle.cos();
            SimplePoint {
                position: center.coords + Vector3::new(x, y, 0.0),
                intensity: 0,
                classification: 0,
                color: Vector3::new(0, 0, 0),
            }
        })
        .collect()
}

pub fn attributes_example_point_cloud(center: Point3<f64>) -> VectorBuffer {
    (0..130)
        .flat_map(|x| (0..130).map(move |y| (x, y)))
        .map(|(x, y)| {
            let intensity = (x as f64 / 130.0 * 100.0) as u16;
            let classification = y / 10;
            let px = x as f64 / 130.0 - 0.5;
            let py = y as f64 / 130.0 - 0.5;
            let pz = ((x as f64 - 65.0).powi(2) + (y as f64 - 64.0).powi(2))
                .sqrt()
                .sin()
                * 0.02;
            let r = rand::random();
            let g = rand::random();
            let b = rand::random();
            SimplePoint {
                position: Vector3::new(px, py, pz) + center.coords,
                intensity,
                classification,
                color: Vector3::new(r, g, b),
            }
        })
        .collect()
}
