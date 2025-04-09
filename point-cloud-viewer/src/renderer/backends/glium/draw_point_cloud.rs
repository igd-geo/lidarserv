use crate::renderer::backends::glium::BACKEND_NAME;
use crate::renderer::backends::glium::draw_point_cloud::shaders::{ProgramManager, ShaderConfig};
use crate::renderer::backends::glium::dynamic_drawing::{
    DynamicMultiVerticesSource, DynamicUniforms, ModifyVertexFormat,
};
use crate::renderer::backends::glium::util::matrix_to_gl;
use crate::renderer::error::{AttributeMismatchType, RendererError, RendererResult};
use crate::renderer::renderer_command::{PointAttribute, PointCloudId};
use crate::renderer::settings::{PointCloudRenderSettings, PointColor, PointSize};
use crate::renderer::vertex_data::{
    F32Attribute, U8Attribute, Vec3F32Attribute, VertexData, VertexDataType,
};
use glium::BlendingFunction;
use glium::framebuffer::SimpleFrameBuffer;
use glium::implement_vertex;
use glium::index::NoIndices;
use glium::texture::texture1d::Texture1d;
use glium::texture::{DepthTexture2d, DepthTexture2dMultisample, Texture2d, Texture2dMultisample};
use glium::uniform;
use glium::uniforms::{MagnifySamplerFilter, UniformValue};
use glium::vertex::VertexBufferAny;
use glium::{
    Blend, BlitMask, BlitTarget, DepthTest, Display, DrawParameters, Frame, LinearBlendingFactor,
    Program, Rect, Surface, Vertex, VertexBuffer,
};
use ouroboros::self_referencing;
use pasture_core::layout::PointAttributeDefinition;
use pasture_core::math::AABB;
use pasture_core::nalgebra::{Matrix4, Point3, Vector3};
use std::collections::HashMap;
use std::mem;
use std::rc::Rc;

implement_vertex!(Vec3F32Attribute, position);
implement_vertex!(F32Attribute, value);
implement_vertex!(U8Attribute, value);

mod shaders {
    //! Functionality for managing the shaders related to point cloud drawing.
    //!
    //! The shaders for the point clouds are built dynamically based on the render settings for a point cloud.
    //! More specifically, each shader's source consists of multiple parts, that are concatenated together.
    //! The struct [ShaderConfig] defines, which parts should be used, and can be derived from the
    //! [PointCloudRenderSettings].
    //!
    //! To avoid unnecessary shader compilations, there is the [ProgramManager], that keeps track of the
    //! shader programs, that have been built before, so existing programs can be reused if possible.

    use crate::renderer::backends::glium::draw_point_cloud::GpuAttribute;
    use crate::renderer::error::{RendererError, RendererResult};
    use crate::renderer::settings::{PointCloudRenderSettings, PointColor, PointShape, PointSize};
    use crate::renderer::vertex_data::VertexDataType;
    use glium::program::ProgramCreationInput;
    use glium::{Display, Program, ProgramCreationError};
    use log::debug;
    use std::collections::HashMap;
    use std::rc::{Rc, Weak};

    pub type PointShapeShader = PointShape;

    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
    pub enum PointSizeShader {
        Fixed,
        Depth,
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
    pub enum ScalarAttributeType {
        Float,
        Int,
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
    pub enum PointColorShader {
        Fixed,
        ScalarAttribute(ScalarAttributeType),
        CategoricalAttribute,
        Rgb,
    }

    /// Settings for building a shader.
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
    pub struct ShaderConfig {
        point_color: PointColorShader,
        point_shape: PointShapeShader,
        point_size: PointSizeShader,
    }

    impl ShaderConfig {
        pub(super) fn new(
            settings: &PointCloudRenderSettings,
            attribute_buffers: &[GpuAttribute],
        ) -> Self {
            ShaderConfig {
                point_color: match settings.point_color {
                    PointColor::Fixed(_) => PointColorShader::Fixed,
                    PointColor::ScalarAttribute(ref coloring) => {
                        let attribute_type = attribute_buffers
                            .iter()
                            .find(|it| it.attribute == coloring.attribute)
                            .map(|it| it.gpu_data.data_type);
                        let attribute_read_shader = match attribute_type {
                            Some(VertexDataType::U8) => ScalarAttributeType::Int,
                            _ => ScalarAttributeType::Float,
                        };
                        PointColorShader::ScalarAttribute(attribute_read_shader)
                    }
                    PointColor::CategoricalAttribute(_) => PointColorShader::CategoricalAttribute,
                    PointColor::Rgb(_) => PointColorShader::Rgb,
                },
                point_shape: settings.point_shape,
                point_size: match settings.point_size {
                    PointSize::Fixed(_) => PointSizeShader::Fixed,
                    PointSize::Depth(_) => PointSizeShader::Depth,
                },
            }
        }
    }

    /// GLSL source code for the different parts of the shaders.
    mod shader_src_parts {
        pub const POINT_VERT: &str = include_str!("shaders/point.vert");
        pub const POINT_FRAG: &str = include_str!("shaders/point.frag");

        pub const FN_SHAPE_ROUND: &str = include_str!("shaders/fn__discard_shape__round.glsl");
        pub const FN_SHAPE_SQUARE: &str = include_str!("shaders/fn__discard_shape__square.glsl");

        pub const FN_COLOR_CONST: &str = include_str!("shaders/fn__set_point_color__fixed.glsl");
        pub const FN_COLOR_SCALAR_ATTRIBUTE: &str =
            include_str!("shaders/fn__set_point_color__scalar_attribute.glsl");

        pub const FN_COLOR_SCALAR_ATTRIBUTE_FLOAT: &str =
            include_str!("shaders/fn__read_point_color_scalar_attribute__float.glsl");
        pub const FN_COLOR_SCALAR_ATTRIBUTE_INT: &str =
            include_str!("shaders/fn__read_point_color_scalar_attribute__int.glsl");

        pub const FN_COLOR_CATEGORICAL_ATTRIBUTE: &str =
            include_str!("shaders/fn__set_point_color__categorical_attribute.glsl");

        pub const FN_COLOR_RGB: &str = include_str!("shaders/fn__set_point_color__rgb.glsl");

        pub const FN_SIZE_CONST: &str = include_str!("shaders/fn__set_point_size__fixed.glsl");
        pub const FN_SIZE_DEPTH: &str = include_str!("shaders/fn__set_point_size__depth.glsl");
    }

    /// Builds a new shader program according to the given config.
    fn build_program(
        display: &Display,
        config: &ShaderConfig,
    ) -> Result<Program, ProgramCreationError> {
        let mut vertex_shader = shader_src_parts::POINT_VERT.to_owned();

        vertex_shader += &match config.point_color {
            PointColorShader::Fixed => shader_src_parts::FN_COLOR_CONST.to_owned(),
            PointColorShader::ScalarAttribute(typ) => {
                shader_src_parts::FN_COLOR_SCALAR_ATTRIBUTE.to_owned()
                    + match typ {
                        ScalarAttributeType::Float => {
                            shader_src_parts::FN_COLOR_SCALAR_ATTRIBUTE_FLOAT
                        }
                        ScalarAttributeType::Int => shader_src_parts::FN_COLOR_SCALAR_ATTRIBUTE_INT,
                    }
            }
            PointColorShader::CategoricalAttribute => {
                shader_src_parts::FN_COLOR_CATEGORICAL_ATTRIBUTE.to_owned()
            }
            PointColorShader::Rgb => shader_src_parts::FN_COLOR_RGB.to_owned(),
        };

        vertex_shader += match config.point_size {
            PointSizeShader::Fixed => shader_src_parts::FN_SIZE_CONST,
            PointSizeShader::Depth => shader_src_parts::FN_SIZE_DEPTH,
        };

        let mut fragment_shader = shader_src_parts::POINT_FRAG.to_owned();

        fragment_shader += match config.point_shape {
            PointShape::Square => shader_src_parts::FN_SHAPE_SQUARE,
            PointShape::Round => shader_src_parts::FN_SHAPE_ROUND,
        };

        debug!("Vertex shader source: \n{}", vertex_shader);
        debug!("Fragment shader source: \n{}", fragment_shader);

        Program::new(
            display,
            ProgramCreationInput::SourceCode {
                vertex_shader: &vertex_shader,
                tessellation_control_shader: None,
                tessellation_evaluation_shader: None,
                geometry_shader: None,
                fragment_shader: &fragment_shader,
                transform_feedback_varyings: None,
                outputs_srgb: false,
                uses_point_size: true,
            },
        )
    }

    /// Manages the shader programs, that are currently in use.
    /// Allows to re-use existing shader programs, that use the same config,
    /// to avoid building more shader programs than necessary.
    pub struct ProgramManager {
        programs: HashMap<ShaderConfig, Weak<Program>>,
    }

    impl ProgramManager {
        pub fn new() -> Self {
            ProgramManager {
                programs: HashMap::new(),
            }
        }

        pub fn get_or_create(
            &mut self,
            display: &Display,
            config: &ShaderConfig,
        ) -> RendererResult<Rc<Program>> {
            // look for existing entry in cache
            if let Some(weak) = self.programs.get(config) {
                if let Some(program) = weak.upgrade() {
                    debug!("Reusing existing shader: {:?}", config);
                    return Ok(program);
                }
            }

            // create program
            debug!("Assembling shader: {:?}", config);
            let program = match build_program(display, config) {
                Err(e) => {
                    return Err(RendererError::Graphics {
                        source: Box::new(e),
                    });
                }
                Ok(p) => p,
            };
            let program = Rc::new(program);

            // store in cache
            self.programs.insert(*config, Rc::downgrade(&program));
            Ok(program)
        }
    }
}

pub struct GpuPointCloud {
    position_gpu_data: GpuVertexData,
    attributes: Vec<GpuAttribute>,
    settings: Option<PointCloudRenderSettings>,
}

struct GpuAttribute {
    attribute: PointAttributeDefinition,
    gpu_data: GpuVertexData,
}

struct GpuVertexData {
    data_type: VertexDataType,
    buffer: Rc<VertexBufferAny>,
}

#[doc(hidden)]
const DRAW_POINTS: NoIndices = NoIndices(glium::index::PrimitiveType::Points);

#[doc(hidden)]
const TRIANGLE_LIST: NoIndices = NoIndices(glium::index::PrimitiveType::TrianglesList);

struct PointCloudDrawCall {
    program: Rc<Program>,
    vertex_buffers: Vec<(Vec<&'static str>, Rc<VertexBufferAny>)>,
    uniforms_vec3f: Vec<(&'static str, [f32; 3])>,
    uniforms_f: Vec<(&'static str, f32)>,
    uniforms_i: Vec<(&'static str, i32)>,
    uniforms_texture_1d: Vec<(&'static str, Texture1d)>,
    transform_scale: Option<Vector3<f64>>,
    transform_offset: Option<Vector3<f64>>,
}

impl PointCloudDrawCall {
    fn new(
        settings: &PointCloudRenderSettings,
        program_manager: &mut ProgramManager,
        display: &Display,
        position_buffer: &GpuVertexData,
        attribute_buffers: &[GpuAttribute],
        transform_scale: Option<Vector3<f64>>,
        transform_offset: Option<Vector3<f64>>,
    ) -> RendererResult<Self> {
        // shader program
        let shader_config = ShaderConfig::new(settings, attribute_buffers);
        let program = program_manager.get_or_create(display, &shader_config)?;

        // uniforms
        let mut uniforms_vec3f = Vec::new();
        let mut uniforms_f = Vec::new();
        let mut uniforms_i = Vec::new();
        match &settings.point_color {
            PointColor::Fixed(color) => {
                uniforms_vec3f.push(("point_color_fixed", [color.r, color.g, color.b]))
            }
            PointColor::ScalarAttribute(scalar_color) => {
                uniforms_f.push(("point_color_min", scalar_color.min));
                uniforms_f.push(("point_color_max", scalar_color.max));
            }
            PointColor::CategoricalAttribute(coloring) => {
                uniforms_i.push((
                    "point_color_max",
                    coloring.color_palette.colors().len() as i32 - 1,
                ));
                uniforms_vec3f.push((
                    "point_color_default",
                    [
                        coloring.color_palette.default_color().r,
                        coloring.color_palette.default_color().g,
                        coloring.color_palette.default_color().b,
                    ],
                ))
            }
            PointColor::Rgb(_) => {}
        }
        match settings.point_size {
            PointSize::Fixed(size) => uniforms_f.push(("point_size_fixed", size)),
            PointSize::Depth(size) => uniforms_f.push(("point_size_depth", size)),
        }

        // textures 1d
        let mut uniforms_texture_1d = Vec::new();
        if let PointColor::ScalarAttribute(scalar_color) = &settings.point_color {
            let mut texture_data = Vec::new();
            let texture_size = 128;
            texture_data.reserve(texture_size);
            for i in 0..texture_size {
                let val = (i as f32) / ((texture_size - 1) as f32);
                let col = scalar_color.color_map.color_at(val);
                texture_data.push((col.r, col.g, col.b));
            }
            let tex =
                Texture1d::new(display, texture_data).map_err(|e| RendererError::Graphics {
                    source: Box::new(e),
                })?;
            uniforms_texture_1d.push(("point_color_texture", tex));
        }
        if let PointColor::CategoricalAttribute(categorical_color) = &settings.point_color {
            let texture_data = categorical_color
                .color_palette
                .colors()
                .iter()
                .map(|c| (c.r, c.g, c.b))
                .collect::<Vec<_>>();
            let tex =
                Texture1d::new(display, texture_data).map_err(|e| RendererError::Graphics {
                    source: Box::new(e),
                })?;
            uniforms_texture_1d.push(("point_color_texture", tex));
        }

        // buffers
        let mut vertex_buffers = Vec::new();
        if position_buffer.data_type != VertexDataType::Vec3F32
            && position_buffer.data_type != VertexDataType::Vec3F32Transform
        {
            return Err(RendererError::UnsupportedOperation {
                backend_name: BACKEND_NAME,
                operation_name: format!("Position of data type {}", position_buffer.data_type),
                platform_specific: false,
            });
        }
        vertex_buffers.push((vec!["position"], Rc::clone(&position_buffer.buffer)));
        match &settings.point_color {
            PointColor::Fixed(_) => {}
            PointColor::ScalarAttribute(scalar_color) => {
                let color_attribute_buffer = attribute_buffers
                    .iter()
                    .find(|it| it.attribute == scalar_color.attribute);
                let color_attribute_buffer = if let Some(buf) = color_attribute_buffer {
                    buf
                } else {
                    return Err(RendererError::AttributeMismatch {
                        attribute: scalar_color.attribute.clone(),
                        problem: AttributeMismatchType::DoesNotExist,
                    });
                };
                if color_attribute_buffer.gpu_data.data_type != VertexDataType::F32
                    && color_attribute_buffer.gpu_data.data_type != VertexDataType::U8
                {
                    return Err(RendererError::UnsupportedOperation {
                        backend_name: BACKEND_NAME,
                        operation_name: format!(
                            "Scalar Color Attribute of data type {}",
                            color_attribute_buffer.gpu_data.data_type
                        ),
                        platform_specific: false,
                    });
                }
                vertex_buffers.push((
                    vec!["point_color_attribute"],
                    Rc::clone(&color_attribute_buffer.gpu_data.buffer),
                ));
            }
            PointColor::CategoricalAttribute(categorical_color) => {
                let color_attribute_buffer = attribute_buffers
                    .iter()
                    .find(|it| it.attribute == categorical_color.attribute);
                let color_attribute_buffer = if let Some(buf) = color_attribute_buffer {
                    buf
                } else {
                    return Err(RendererError::AttributeMismatch {
                        attribute: categorical_color.attribute.clone(),
                        problem: AttributeMismatchType::DoesNotExist,
                    });
                };
                if color_attribute_buffer.gpu_data.data_type != VertexDataType::U8 {
                    return Err(RendererError::UnsupportedOperation {
                        backend_name: BACKEND_NAME,
                        operation_name: format!(
                            "Categorical Color Attribute of data type {}",
                            color_attribute_buffer.gpu_data.data_type
                        ),
                        platform_specific: false,
                    });
                }
                vertex_buffers.push((
                    vec!["point_color_attribute"],
                    Rc::clone(&color_attribute_buffer.gpu_data.buffer),
                ));
            }
            PointColor::Rgb(rgb_color) => {
                let color_rgb_buffer = attribute_buffers
                    .iter()
                    .find(|it| it.attribute == rgb_color.attribute);
                let color_rgb_buffer = if let Some(buf) = color_rgb_buffer {
                    buf
                } else {
                    return Err(RendererError::AttributeMismatch {
                        attribute: rgb_color.attribute.clone(),
                        problem: AttributeMismatchType::DoesNotExist,
                    });
                };
                if color_rgb_buffer.gpu_data.data_type != VertexDataType::Vec3F32 {
                    return Err(RendererError::UnsupportedOperation {
                        backend_name: BACKEND_NAME,
                        operation_name: format!(
                            "RGB Point Color of data type {}",
                            color_rgb_buffer.gpu_data.data_type
                        ),
                        platform_specific: false,
                    });
                }
                vertex_buffers.push((
                    vec!["point_color_rgb"],
                    Rc::clone(&color_rgb_buffer.gpu_data.buffer),
                ));
            }
        }

        let draw_call = PointCloudDrawCall {
            program,
            vertex_buffers,
            uniforms_vec3f,
            uniforms_f,
            uniforms_i,
            uniforms_texture_1d,
            transform_scale,
            transform_offset,
        };
        Ok(draw_call)
    }

    pub fn draw<S>(
        &self,
        frame: &mut S,
        scale_factor: f64,
        view_projection_matrix: &Matrix4<f64>,
    ) -> RendererResult<()>
    where
        S: Surface + ?Sized,
    {
        // uniforms
        let mut uniforms = DynamicUniforms::new();
        for (name, value) in &self.uniforms_vec3f {
            uniforms.add(name, value);
        }
        for (name, value) in &self.uniforms_f {
            uniforms.add(name, value);
        }
        for (name, value) in &self.uniforms_i {
            uniforms.add(name, value);
        }
        for (name, texture) in &self.uniforms_texture_1d {
            uniforms.add_uniform_value(name, UniformValue::Texture1d(texture, None));
        }

        // projection matrix
        let mut view_projection_matrix = view_projection_matrix.to_owned();
        if let Some(offset) = self.transform_offset {
            view_projection_matrix *= Matrix4::new(
                1.0, 0.0, 0.0, offset.x, 0.0, 1.0, 0.0, offset.y, 0.0, 0.0, 1.0, offset.z, 0.0,
                0.0, 0.0, 1.0,
            );
        }
        if let Some(scale) = self.transform_scale {
            view_projection_matrix *= Matrix4::new(
                scale.x, 0.0, 0.0, 0.0, 0.0, scale.y, 0.0, 0.0, 0.0, 0.0, scale.z, 0.0, 0.0, 0.0,
                0.0, 1.0,
            );
        }
        let matrix_data = matrix_to_gl(&view_projection_matrix);
        uniforms.add("viewProjectionMatrix", &matrix_data);

        // scale factor
        let float_scale_factor = scale_factor as f32;
        uniforms.add("scaleFactor", &float_scale_factor);

        // vertex buffers
        let vertex_buffers_renamed = self
            .vertex_buffers
            .iter()
            .map(|(names, buffer)| ModifyVertexFormat::rename_bindings(buffer.as_ref(), names))
            .collect::<Vec<_>>();
        let mut multi_vertex_source = DynamicMultiVerticesSource::new();
        for source in &vertex_buffers_renamed {
            multi_vertex_source.add(source);
        }

        // draw parameters
        let draw_parameters = DrawParameters {
            depth: glium::Depth {
                write: true,
                test: DepthTest::IfLess,
                ..Default::default()
            },
            ..Default::default()
        };

        // draw!
        frame
            .draw(
                multi_vertex_source,
                DRAW_POINTS,
                &self.program,
                &uniforms,
                &draw_parameters,
            )
            .map_err(|e| RendererError::Graphics {
                source: Box::new(e),
            })
    }
}

struct PointCloudItem {
    gpu_point_cloud: GpuPointCloud,
    draw_call: PointCloudDrawCall,
    aabb: Option<AABB<f64>>,
    transform_offset: Option<Vector3<f64>>,
    transform_scale: Option<Vector3<f64>>,
}

#[self_referencing]
struct SizeDependentDeferredRenderingData {
    tex_color: Box<Texture2dMultisample>,
    tex_depth: Box<DepthTexture2dMultisample>,
    tex_color_resolved: Box<Texture2d>,
    tex_depth_resolved: Box<DepthTexture2d>,

    #[borrows(tex_color, tex_depth)]
    #[covariant]
    frame_buffer: SimpleFrameBuffer<'this>,

    #[borrows(tex_color_resolved, tex_depth_resolved)]
    #[covariant]
    frame_buffer_resolve: SimpleFrameBuffer<'this>,
}

struct DeferredRenderingData {
    quad_program: Program,
    quad_vertex_buffer: VertexBufferAny,
}

pub struct PointCloudsRenderer {
    next_id: PointCloudId,
    default_settings: PointCloudRenderSettings,
    program_manager: ProgramManager,
    point_clouds: HashMap<PointCloudId, PointCloudItem>,
    deferred_rendering_sized: Option<SizeDependentDeferredRenderingData>,
    deferred_rendering_unsized: Option<DeferredRenderingData>,
}

impl PointCloudsRenderer {
    pub fn new() -> Self {
        PointCloudsRenderer {
            next_id: PointCloudId::begin(),
            point_clouds: HashMap::new(),
            default_settings: Default::default(),
            program_manager: ProgramManager::new(),
            deferred_rendering_sized: None,
            deferred_rendering_unsized: None,
        }
    }

    fn calculate_aabb_f32(data: &[Vec3F32Attribute]) -> Option<AABB<f64>> {
        if data.is_empty() {
            return None;
        }
        let mut min = Point3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Point3::new(f32::MIN, f32::MIN, f32::MIN);
        for point in data {
            if point.position[0] < min.x {
                min.x = point.position[0];
            }
            if point.position[0] > max.x {
                max.x = point.position[0];
            }
            if point.position[1] < min.y {
                min.y = point.position[1];
            }
            if point.position[1] > max.y {
                max.y = point.position[1];
            }
            if point.position[2] < min.z {
                min.z = point.position[2];
            }
            if point.position[2] > max.z {
                max.z = point.position[2];
            }
        }
        let min = Point3::new(min.x as f64, min.y as f64, min.z as f64);
        let max = Point3::new(max.x as f64, max.y as f64, max.z as f64);
        Some(AABB::from_min_max_unchecked(min, max))
    }

    fn calculate_aabb(points: &VertexData) -> Option<AABB<f64>> {
        match points {
            VertexData::F32(_) => None,
            VertexData::U8(_) => None,
            VertexData::Vec3F32(data) => Self::calculate_aabb_f32(data),
            VertexData::Vec3F32Transform {
                values,
                offset,
                scale,
            } => Self::calculate_aabb_f32(values).map(|aabb| {
                let mut bounds_1 = aabb.min().coords.component_mul(scale) + offset;
                let mut bounds_2 = aabb.max().coords.component_mul(scale) + offset;
                if bounds_1.x > bounds_2.x {
                    mem::swap(&mut bounds_1.x, &mut bounds_2.x)
                }
                if bounds_1.y > bounds_2.y {
                    mem::swap(&mut bounds_1.y, &mut bounds_2.y)
                }
                if bounds_1.z > bounds_2.z {
                    mem::swap(&mut bounds_1.z, &mut bounds_2.z)
                }
                AABB::from_min_max_unchecked(bounds_1.into(), bounds_2.into())
            }),
        }
    }

    pub fn bounding_box(&self) -> Option<AABB<f64>> {
        let mut aabb = None;

        for point_cloud in self.point_clouds.values() {
            aabb = match (&aabb, &point_cloud.aabb) {
                (None, o) | (o, None) => *o,
                (Some(a), Some(b)) => Some(AABB::union(a, b)),
            };
        }

        aabb
    }

    pub fn point_cloud_bounding_box(&self, id: PointCloudId) -> RendererResult<Option<AABB<f64>>> {
        match self.point_clouds.get(&id) {
            None => Err(RendererError::PointCloudDoesNotExist { id }),
            Some(cloud) => Ok(cloud.aabb),
        }
    }

    fn add_or_update_point_cloud(
        &mut self,
        id: PointCloudId,
        display: &Display,
        positions: &VertexData,
        attributes: &[PointAttribute],
        render_settings: &Option<PointCloudRenderSettings>,
    ) -> RendererResult<()> {
        // calculate bounding box
        let aabb = Self::calculate_aabb(positions);

        // upload position buffer to gpu
        let gpu_positions = transfer_vertex_data_to_gpu(display, positions)?;

        // upload point attributes to GPU
        let mut gpu_attributes = Vec::with_capacity(attributes.len());
        for attribute in attributes {
            let gpu_attribute = transfer_vertex_data_to_gpu(display, &attribute.data)?;
            gpu_attributes.push(GpuAttribute {
                attribute: attribute.attribute.clone(),
                gpu_data: gpu_attribute,
            });
        }

        // transform
        let mut transform_offset = None;
        let mut transform_scale = None;
        if let VertexData::Vec3F32Transform { scale, offset, .. } = positions {
            transform_offset = Some(*offset);
            transform_scale = Some(*scale);
        }

        // prepare drawing with the specific settings
        // (shader, textures, uniforms, ...)
        let settings = render_settings.as_ref().unwrap_or(&self.default_settings);
        let draw_call = PointCloudDrawCall::new(
            settings,
            &mut self.program_manager,
            display,
            &gpu_positions,
            &gpu_attributes,
            transform_scale,
            transform_offset,
        )?;

        // insert new point cloud, or overwrite the old one.
        self.point_clouds.insert(
            id,
            PointCloudItem {
                draw_call,
                aabb,
                gpu_point_cloud: GpuPointCloud {
                    position_gpu_data: gpu_positions,
                    attributes: gpu_attributes,
                    settings: render_settings.to_owned(),
                },
                transform_offset,
                transform_scale,
            },
        );
        Ok(())
    }

    /// Adds a point cloud and returns the ID for the newly created point cloud.
    pub fn add(
        &mut self,
        display: &Display,
        positions: &VertexData,
        attributes: &[PointAttribute],
        render_settings: &Option<PointCloudRenderSettings>,
    ) -> RendererResult<PointCloudId> {
        // new unique id
        let id = self.next_id.increment();

        // insert
        self.add_or_update_point_cloud(id, display, positions, attributes, render_settings)?;

        Ok(id)
    }

    pub fn update_points(
        &mut self,
        display: &Display,
        id: PointCloudId,
        positions: &VertexData,
        attributes: &[PointAttribute],
    ) -> RendererResult<()> {
        // get old point cloud
        let cloud = self.point_clouds.get(&id);
        let cloud = match cloud {
            None => return Err(RendererError::PointCloudDoesNotExist { id }),
            Some(c) => c,
        };

        // simply re-insert points with the same settings
        // todo: If the attributes and their types do not change, we should re-upload the data into the existing buffers.
        let settings = cloud.gpu_point_cloud.settings.clone();
        self.add_or_update_point_cloud(id, display, positions, attributes, &settings)
    }

    pub fn update_default_settings(
        &mut self,
        display: &Display,
        new_settings: &PointCloudRenderSettings,
    ) -> RendererResult<()> {
        // update the draw call for every point cloud, that uses the default settings.
        let mut new_draw_calls = Vec::new();
        for (&id, item) in &self.point_clouds {
            if item.gpu_point_cloud.settings.is_some() {
                continue;
            }
            let new_draw_call = PointCloudDrawCall::new(
                new_settings,
                &mut self.program_manager,
                display,
                &item.gpu_point_cloud.position_gpu_data,
                &item.gpu_point_cloud.attributes,
                item.transform_scale,
                item.transform_offset,
            )?;
            new_draw_calls.push((id, new_draw_call));
        }

        // only actually update the existing draw calls, once all new draw calls have been created,
        // so that we return to the original state, if there is a failure.
        for (id, draw_call) in new_draw_calls {
            self.point_clouds.get_mut(&id).unwrap().draw_call = draw_call;
        }

        // update default settings
        self.default_settings = new_settings.to_owned();

        Ok(())
    }

    pub fn update_settings(
        &mut self,
        display: &Display,
        id: PointCloudId,
        new_settings: Option<PointCloudRenderSettings>,
    ) -> RendererResult<()> {
        // get the point cloud
        let cloud = self.point_clouds.get_mut(&id);
        let cloud = match cloud {
            None => return Err(RendererError::PointCloudDoesNotExist { id }),
            Some(c) => c,
        };

        // new render call for the updated settings
        let settings = new_settings.as_ref().unwrap_or(&self.default_settings);
        let draw_call = PointCloudDrawCall::new(
            settings,
            &mut self.program_manager,
            display,
            &cloud.gpu_point_cloud.position_gpu_data,
            &cloud.gpu_point_cloud.attributes,
            cloud.transform_scale,
            cloud.transform_offset,
        )?;

        // if  this was successful, we can actually update thhe point cloud.
        cloud.draw_call = draw_call;
        cloud.gpu_point_cloud.settings = new_settings;
        Ok(())
    }

    pub fn remove(&mut self, id: PointCloudId) -> RendererResult<()> {
        if self.point_clouds.remove(&id).is_none() {
            return Err(RendererError::PointCloudDoesNotExist { id });
        }
        Ok(())
    }

    pub fn draw(
        &mut self,
        frame: &mut Frame,
        scale_factor: f64,
        view_projection_matrix: &Matrix4<f64>,
    ) -> RendererResult<()> {
        // release resource that would only be required for EDL
        self.deferred_rendering_unsized = None;
        self.deferred_rendering_sized = None;

        // Draw each point cloud
        for cloud in self.point_clouds.values() {
            cloud
                .draw_call
                .draw(frame, scale_factor, view_projection_matrix)?;
        }

        Ok(())
    }

    pub fn draw_with_edl(
        &mut self,
        display: &Display,
        frame: &mut Frame,
        scale_factor: f64,
        view_projection_matrix: &Matrix4<f64>,
        inverse_projection_matrix: &Matrix4<f64>,
    ) -> RendererResult<()> {
        // (1) Create program & vertex buffer for drawing the quad
        let deferred_data_unsized = match self.deferred_rendering_unsized.take() {
            Some(d) => d,
            None => {
                let quad_program = Program::from_source(
                    display,
                    include_str!("./shaders/deferred_shading_quad.vert"),
                    include_str!("./shaders/deferred_shading_quad.frag"),
                    None,
                )
                .map_err(|e| RendererError::Graphics {
                    source: Box::new(e),
                })?;

                let quad_vertex_buffer = {
                    #[derive(Copy, Clone, Debug)]
                    struct QuadVertex {
                        position: [f32; 2],
                    }

                    implement_vertex!(QuadVertex, position);

                    let data = vec![
                        QuadVertex {
                            position: [-1.0, -1.0],
                        },
                        QuadVertex {
                            position: [1.0, -1.0],
                        },
                        QuadVertex {
                            position: [-1.0, 1.0],
                        },
                        QuadVertex {
                            position: [1.0, -1.0],
                        },
                        QuadVertex {
                            position: [1.0, 1.0],
                        },
                        QuadVertex {
                            position: [-1.0, 1.0],
                        },
                    ];

                    VertexBuffer::new(display, data.as_slice())
                        .map_err(|e| RendererError::Graphics {
                            source: Box::new(e),
                        })?
                        .into()
                };

                DeferredRenderingData {
                    quad_program,
                    quad_vertex_buffer,
                }
            }
        };

        // (2) Prepare the textures & frame buffer for deferred rendering
        let multisample: u32 = display
            .gl_window()
            .get_pixel_format()
            .multisampling
            .unwrap_or(0)
            .into();
        let use_multisampling = multisample > 0;
        let mut deferred_data = {
            let (w, h) = frame.get_dimensions();

            // reset frame buffer, if the size has changed
            if let Some(d) = &self.deferred_rendering_sized {
                if d.borrow_frame_buffer().get_dimensions() != (w, h) {
                    self.deferred_rendering_sized = None
                }
            };

            // (re)create framebuffer
            match self.deferred_rendering_sized.take() {
                Some(d) => d,
                None => {
                    // color texture
                    let tex_color = Texture2dMultisample::empty_with_format(
                        display,
                        glium::texture::UncompressedFloatFormat::F32F32F32F32,
                        glium::texture::MipmapsOption::NoMipmap,
                        w,
                        h,
                        if use_multisampling { multisample } else { 1 },
                    )
                    .map_err(|e| RendererError::Graphics {
                        source: Box::new(e),
                    })?;

                    let tex_color_resolved = Texture2d::empty_with_format(
                        display,
                        glium::texture::UncompressedFloatFormat::F32F32F32F32,
                        glium::texture::MipmapsOption::NoMipmap,
                        w,
                        h,
                    )
                    .map_err(|e| RendererError::Graphics {
                        source: Box::new(e),
                    })?;

                    // depth texture
                    let tex_depth = DepthTexture2dMultisample::empty_with_format(
                        display,
                        glium::texture::DepthFormat::F32,
                        glium::texture::MipmapsOption::NoMipmap,
                        w,
                        h,
                        if use_multisampling { multisample } else { 1 },
                    )
                    .map_err(|e| RendererError::Graphics {
                        source: Box::new(e),
                    })?;

                    let tex_depth_resolved = DepthTexture2d::empty_with_format(
                        display,
                        glium::texture::DepthFormat::F32,
                        glium::texture::MipmapsOption::NoMipmap,
                        w,
                        h,
                    )
                    .map_err(|e| RendererError::Graphics {
                        source: Box::new(e),
                    })?;

                    // frame buffer
                    SizeDependentDeferredRenderingDataTryBuilder {
                        tex_color: Box::new(tex_color),
                        tex_depth: Box::new(tex_depth),
                        tex_color_resolved: Box::new(tex_color_resolved),
                        tex_depth_resolved: Box::new(tex_depth_resolved),
                        frame_buffer_builder: |color, depth| {
                            SimpleFrameBuffer::with_depth_buffer(
                                display,
                                color.as_ref(),
                                depth.as_ref(),
                            )
                        },
                        frame_buffer_resolve_builder: |color_resolved, depth_resolved| {
                            SimpleFrameBuffer::with_depth_buffer(
                                display,
                                color_resolved.as_ref(),
                                depth_resolved.as_ref(),
                            )
                        },
                    }
                    .try_build()
                    .map_err(|e| RendererError::Graphics {
                        source: Box::new(e),
                    })?
                }
            }
        };

        // (3) Render to textures
        fn render_clouds<F>(
            frame_buffer: &mut F,
            point_clouds: &HashMap<PointCloudId, PointCloudItem>,
            scale_factor: f64,
            view_projection_matrix: &Matrix4<f64>,
        ) -> RendererResult<()>
        where
            F: Surface + ?Sized,
        {
            frame_buffer.clear_color_and_depth((0.0, 0.0, 0.0, 0.0), 1.0);
            for cloud in point_clouds.values() {
                cloud
                    .draw_call
                    .draw(frame_buffer, scale_factor, view_projection_matrix)?;
            }
            Ok(())
        }
        if use_multisampling {
            deferred_data.with_frame_buffer_mut(|frame_buffer_mut| {
                render_clouds(
                    frame_buffer_mut,
                    &self.point_clouds,
                    scale_factor,
                    view_projection_matrix,
                )
            })?;
        } else {
            deferred_data.with_frame_buffer_resolve_mut(|frame_buffer_mut| {
                render_clouds(
                    frame_buffer_mut,
                    &self.point_clouds,
                    scale_factor,
                    view_projection_matrix,
                )
            })?;
        }

        // (4) Resolve multisampling
        if use_multisampling {
            deferred_data.with_mut(|f| {
                let (w, h) = f.frame_buffer_resolve.get_dimensions();
                f.frame_buffer_resolve.blit_buffers_from_simple_framebuffer(
                    f.frame_buffer,
                    &Rect {
                        left: 0,
                        bottom: 0,
                        width: w,
                        height: h,
                    },
                    &BlitTarget {
                        left: 0,
                        bottom: 0,
                        width: w as i32,
                        height: h as i32,
                    },
                    MagnifySamplerFilter::Linear,
                    BlitMask::color(),
                );
                f.frame_buffer_resolve.blit_buffers_from_simple_framebuffer(
                    f.frame_buffer,
                    &Rect {
                        left: 0,
                        bottom: 0,
                        width: w,
                        height: h,
                    },
                    &BlitTarget {
                        left: 0,
                        bottom: 0,
                        width: w as i32,
                        height: h as i32,
                    },
                    MagnifySamplerFilter::Nearest,
                    BlitMask::depth(),
                );
            });
        }

        // (5) Combine textures to final output on screen
        {
            let draw_parameteers = DrawParameters {
                blend: Blend {
                    color: BlendingFunction::Addition {
                        source: LinearBlendingFactor::SourceAlpha,
                        destination: LinearBlendingFactor::OneMinusSourceAlpha,
                    },
                    alpha: BlendingFunction::Addition {
                        source: LinearBlendingFactor::One,
                        destination: LinearBlendingFactor::OneMinusSourceAlpha,
                    },
                    ..Default::default()
                },
                depth: glium::Depth {
                    write: true,
                    test: DepthTest::IfLess,
                    ..Default::default()
                },
                ..Default::default()
            };
            let (w, h) = frame.get_dimensions();
            let uniforms = uniform! {
                color_texture: deferred_data.borrow_tex_color_resolved().as_ref(),
                depth_texture: deferred_data.borrow_tex_depth_resolved().as_ref(),
                inverse_projection_matrix: matrix_to_gl(inverse_projection_matrix),
                size_x: w as f32,
                size_y: h as f32,
            };

            frame
                .draw(
                    &deferred_data_unsized.quad_vertex_buffer,
                    TRIANGLE_LIST,
                    &deferred_data_unsized.quad_program,
                    &uniforms,
                    &draw_parameteers,
                )
                .map_err(|e| RendererError::Graphics {
                    source: Box::new(e),
                })?
        }

        // (4) Keep open gl objects for the next frames
        self.deferred_rendering_sized = Some(deferred_data);
        self.deferred_rendering_unsized = Some(deferred_data_unsized);

        Ok(())
    }
}

fn transfer_vertex_data_to_gpu(
    display: &Display,
    data: &VertexData,
) -> RendererResult<GpuVertexData> {
    fn upload<T>(display: &Display, data: &[T]) -> RendererResult<VertexBufferAny>
    where
        T: Vertex + Send + 'static,
    {
        VertexBuffer::new(display, data)
            .map(|b| b.into())
            .map_err(|e| RendererError::Graphics {
                source: Box::new(e),
            })
    }

    let buffer = match data {
        VertexData::F32(data) => upload(display, data)?,
        VertexData::Vec3F32(data) => upload(display, data)?,
        VertexData::U8(data) => upload(display, data)?,
        VertexData::Vec3F32Transform { values, .. } => upload(display, values)?,
    };
    let buffer = Rc::new(buffer);

    let gpu_vertex_data = GpuVertexData {
        data_type: data.data_type(),
        buffer,
    };

    Ok(gpu_vertex_data)
}
