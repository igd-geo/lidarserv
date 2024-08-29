pub mod bounding_box;
pub mod coordinate_system;
pub mod grid;
pub mod plane;
pub mod position;
pub mod sampling;

#[cfg(test)]
pub mod test {
    use bytemuck::{Pod, Zeroable};
    use nalgebra::Vector3;
    use pasture_derive::PointType;

    /// A pasture point type for testing with a f64 position.
    #[repr(C)]
    #[derive(Debug, Copy, Clone, PointType, Pod, Zeroable)]
    pub struct F64Point {
        #[pasture(BUILTIN_POSITION_3D)]
        pub position: Vector3<f64>,
    }

    /// A pasture point type for testing with a i32 position.
    #[repr(C)]
    #[derive(Debug, Copy, Clone, PointType, Pod, Zeroable)]
    pub struct I32Point {
        #[pasture(BUILTIN_POSITION_3D)]
        pub position: Vector3<i32>,
    }
}
