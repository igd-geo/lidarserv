use crate::renderer::error::{RendererError, RendererResult};
use crate::renderer::viewer::private::RenderThreadHandle;
use pasture_core::containers::{BorrowedBuffer, BorrowedBufferExt};
use pasture_core::layout::{PointAttributeDataType, PointAttributeDefinition, PrimitiveType};
use pasture_core::nalgebra::Vector3;
use std::fmt::{Debug, Formatter};

/// Extracts the data for one point attribute from the point buffer
/// and converts it to the vertex data of an appropriate type for the attribute type, that is
/// supported by the given graphics backend.
pub fn point_attribute_to_vertex_data<B, P>(
    points: &P,
    attribute: &PointAttributeDefinition,
    backend: &B,
) -> RendererResult<VertexData>
where
    B: RenderThreadHandle + ?Sized,
    P: BorrowedBuffer,
{
    let vertex_data_type_candidates: &[VertexDataType] = match attribute.datatype() {
        PointAttributeDataType::U8 => &[VertexDataType::U8],
        PointAttributeDataType::I8 => &[],
        PointAttributeDataType::U16 => &[VertexDataType::F32],
        PointAttributeDataType::I16 => &[],
        PointAttributeDataType::U32 => &[],
        PointAttributeDataType::I32 => &[],
        PointAttributeDataType::U64 => &[],
        PointAttributeDataType::I64 => &[],
        PointAttributeDataType::F32 => &[VertexDataType::F32],
        PointAttributeDataType::F64 => &[VertexDataType::F32],
        PointAttributeDataType::Vec3u8 => &[],
        PointAttributeDataType::Vec3u16 => &[VertexDataType::Vec3F32],
        PointAttributeDataType::Vec3f32 => &[VertexDataType::Vec3F32],
        PointAttributeDataType::Vec3f64 => {
            &[VertexDataType::Vec3F32Transform, VertexDataType::Vec3F32]
        }
        PointAttributeDataType::Vec4u8 => &[],
        PointAttributeDataType::Vec3i32 => &[],
        PointAttributeDataType::ByteArray(_) => &[],
        PointAttributeDataType::Custom { .. } => &[],
    };

    let vertex_data_type = vertex_data_type_candidates
        .iter()
        .find(|candidate| backend.is_vertex_data_type_supported(**candidate));

    let vertex_data_type = match vertex_data_type {
        None => {
            return Err(RendererError::UnsupportedOperation {
                backend_name: backend.name(),
                operation_name: format!("Point Attribute Data Type {}", attribute.datatype()),
                platform_specific: false,
            });
        }
        Some(dt) => *dt,
    };

    let vertex_data = point_attribute_to_vertex_data_type(points, attribute, vertex_data_type);

    Ok(vertex_data)
}

/// Extracts the values for some point attribute from a point buffer and converts them to the
/// vertex buffer data of the given type.
/// Panicks, of the conversion of the point attribute data type to the vertex data type is not supported.
fn point_attribute_to_vertex_data_type(
    points: &impl BorrowedBuffer,
    attribute: &PointAttributeDefinition,
    vertex_buffer_data_type: VertexDataType,
) -> VertexData {
    match (attribute.datatype(), vertex_buffer_data_type) {
        (PointAttributeDataType::Vec3f32, VertexDataType::Vec3F32) => VertexData::Vec3F32(
            get_point_attribute_vertex_data(points, attribute, |v: Vector3<f32>| {
                Vec3F32Attribute::new(v.x, v.y, v.z)
            }),
        ),

        (PointAttributeDataType::Vec3f64, VertexDataType::Vec3F32) => VertexData::Vec3F32(
            get_point_attribute_vertex_data(points, attribute, |v: Vector3<f64>| {
                Vec3F32Attribute::new(v.x as f32, v.y as f32, v.z as f32)
            }),
        ),

        (PointAttributeDataType::Vec3f64, VertexDataType::Vec3F32Transform) => {
            // calculate the bounds of the values
            let mut min = Vector3::new(f64::MAX, f64::MAX, f64::MAX);
            let mut max = Vector3::new(f64::MIN, f64::MIN, f64::MIN);
            for attr in points.view_attribute::<Vector3<f64>>(attribute) {
                if attr.x < min.x {
                    min.x = attr.x
                }
                if attr.y < min.y {
                    min.y = attr.y
                }
                if attr.z < min.z {
                    min.z = attr.z
                }
                if attr.x > max.x {
                    max.x = attr.x
                }
                if attr.y > max.y {
                    max.y = attr.y
                }
                if attr.z > max.z {
                    max.z = attr.z
                }
            }

            // from the bounds, we calculate offset and scale,
            // such that every value will be between -5_000 and 5_000
            // so we keep an acceptable precision after converting to f32.
            let (offset, scale) = if !points.is_empty() {
                let offset = (min + max) / 2.0;
                let mut scale = (max - min) / 10_000.0;
                if scale.x < 1.0 {
                    scale.x = 1.0;
                }
                if scale.y < 1.0 {
                    scale.y = 1.0;
                }
                if scale.z < 1.0 {
                    scale.z = 1.0;
                }
                (offset, scale)
            } else {
                (Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 1.0))
            };

            // apply offset/scale, convert to f32
            let mut values = Vec::with_capacity(points.len());
            for attr in points.view_attribute::<Vector3<f64>>(attribute) {
                let value = (attr - offset).component_div(&scale);
                values.push(Vec3F32Attribute::new(
                    value.x as f32,
                    value.y as f32,
                    value.z as f32,
                ));
            }
            VertexData::Vec3F32Transform {
                values,
                offset,
                scale,
            }
        }

        (PointAttributeDataType::Vec3u16, VertexDataType::Vec3F32) => VertexData::Vec3F32(
            get_point_attribute_vertex_data(points, attribute, |v: Vector3<u16>| {
                Vec3F32Attribute::new(
                    v.x as f32 / 65535.0,
                    v.y as f32 / 65535.0,
                    v.z as f32 / 65535.0,
                )
            }),
        ),

        (PointAttributeDataType::F32, VertexDataType::F32) => VertexData::F32(
            get_point_attribute_vertex_data(points, attribute, F32Attribute::new),
        ),

        (PointAttributeDataType::F64, VertexDataType::F32) => VertexData::F32(
            get_point_attribute_vertex_data(points, attribute, |v: f64| {
                F32Attribute::new(v as f32)
            }),
        ),

        (PointAttributeDataType::U16, VertexDataType::F32) => VertexData::F32(
            get_point_attribute_vertex_data(points, attribute, |v: u16| {
                F32Attribute::new(v as f32)
            }),
        ),

        (PointAttributeDataType::U8, VertexDataType::U8) => VertexData::U8(
            get_point_attribute_vertex_data(points, attribute, U8Attribute::new),
        ),

        (dt, fm) => panic!(
            "The conversion from the point attribute data type {} to the vertex buffer type {} is not supported.",
            dt, fm
        ),
    }
}

/// The different types of vertex data.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum VertexDataType {
    F32,
    U8,
    Vec3F32,
    Vec3F32Transform,
}

/// Contains the data for a vertex buffer.
/// The enum variants correspond to the different values of [VertexDataType].
#[derive(Clone)]
pub enum VertexData {
    F32(Vec<F32Attribute>),
    U8(Vec<U8Attribute>),
    Vec3F32(Vec<Vec3F32Attribute>),
    Vec3F32Transform {
        /// position = value * scale + offset
        /// value = (position - offset) / scale
        values: Vec<Vec3F32Attribute>,
        offset: Vector3<f64>,
        scale: Vector3<f64>,
    },
}

impl Debug for VertexData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::F32(_) => f.debug_tuple("F32").finish(),
            Self::U8(_) => f.debug_tuple("U8").finish(),
            Self::Vec3F32(_) => f.debug_tuple("Vec3F32").finish(),
            Self::Vec3F32Transform {
                values: _,
                offset,
                scale,
            } => f
                .debug_struct("Vec3F32Transform")
                .field("offset", offset)
                .field("scale", scale)
                .finish(),
        }
    }
}

impl std::fmt::Display for VertexDataType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VertexDataType::F32 => write!(f, "float"),
            VertexDataType::Vec3F32 => write!(f, "vec3f"),
            VertexDataType::U8 => write!(f, "u8"),
            VertexDataType::Vec3F32Transform => write!(f, "vec3f+transformation"),
        }
    }
}

impl VertexData {
    /// The data type of this vertex buffer data
    pub fn data_type(&self) -> VertexDataType {
        match self {
            VertexData::F32(_) => VertexDataType::F32,
            VertexData::Vec3F32(_) => VertexDataType::Vec3F32,
            VertexData::U8(_) => VertexDataType::U8,
            VertexData::Vec3F32Transform { .. } => VertexDataType::Vec3F32Transform,
        }
    }
}

/// Generic wrapper for a scalar point attribute value.
/// It looks like glium _requires_ that each vertex in the vertex data is wrapped in a struct
/// (On which we then can call implemeent_vertex!() ...).
/// Even if we just want to have a single float, because impl_vertex!() does not work for primitives
/// or structs defined outside of the crate.
#[derive(Copy, Clone, Debug)]
pub struct Attribute<T> {
    pub value: T,
}

impl<T> Attribute<T> {
    /// Wraps the given value...
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

/// Generic wrapper for a 3d vector.
#[derive(Copy, Clone, Debug)]
pub struct Vec3<T> {
    pub position: [T; 3],
}

impl<T> Vec3<T> {
    pub fn new(x: T, y: T, z: T) -> Self {
        Self {
            position: [x, y, z],
        }
    }
}

/// Wraps a f32, for making glium's implement_vertex!() work with f32 point attributes
pub type F32Attribute = Attribute<f32>;

/// Wraps a u8, for making glium's implement_vertex!() work with u8 point attributes
pub type U8Attribute = Attribute<u8>;

/// Wraps three f32
pub type Vec3F32Attribute = Vec3<f32>;

/// Helper for copying one point attribute into a vector, while applying a conversion function to each element.
fn get_point_attribute_vertex_data<T, U, F, P>(
    points: &P,
    attribute: &PointAttributeDefinition,
    map_fn: F,
) -> Vec<U>
where
    F: Fn(T) -> U,
    T: PrimitiveType,
    P: BorrowedBuffer,
{
    let mut attr_data = Vec::with_capacity(points.len());
    for attr in points.view_attribute::<T>(attribute) {
        attr_data.push(map_fn(attr));
    }
    attr_data
}
