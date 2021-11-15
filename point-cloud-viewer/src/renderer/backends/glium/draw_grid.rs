//! Functionality to draw the grid on the xy-plane.

use crate::renderer::backends::glium::util::matrix_to_gl;
use crate::renderer::error::{RendererError, RendererResult};
use crate::renderer::settings;
use glium::implement_vertex;
use glium::index::{NoIndices, PrimitiveType};
use glium::uniform;
use glium::{
    BlendingFunction, DepthTest, Display, DrawParameters, Frame, LinearBlendingFactor, Program,
    Surface, VertexBuffer,
};
use pasture_core::nalgebra::{Matrix4, Vector3, Vector4};

#[derive(Copy, Clone)]
struct GridVertex {
    position: [f32; 2],
}

implement_vertex!(GridVertex, position);

mod shaders {
    pub const GRID_VERT: &str = include_str!("./shaders/grid.vert");
    pub const GRID_FRAG: &str = include_str!("./shaders/grid.frag");
}

pub struct GridRenderer {
    vertex_buffer: VertexBuffer<GridVertex>,
    shader_program: Program,
    settings: settings::Grid,
}

impl GridRenderer {
    fn make_vertex_buffer(
        display: &Display,
        nr_cells: u8,
    ) -> RendererResult<VertexBuffer<GridVertex>> {
        let mut vertex_buffer_data = Vec::new();
        for x in 0..=nr_cells {
            let frac = (x as f32) / (nr_cells as f32);
            vertex_buffer_data.push(GridVertex {
                position: [frac, 0.0],
            });
            vertex_buffer_data.push(GridVertex {
                position: [frac, 1.0],
            });
            vertex_buffer_data.push(GridVertex {
                position: [0.0, frac],
            });
            vertex_buffer_data.push(GridVertex {
                position: [1.0, frac],
            });
        }
        let vertex_buffer = VertexBuffer::new(display, &vertex_buffer_data).map_err(|e| {
            RendererError::Graphics {
                source: Box::new(e),
            }
        })?;
        Ok(vertex_buffer)
    }

    pub fn new(display: &Display, settings: settings::Grid) -> RendererResult<Self> {
        // vertex data
        let vertex_buffer = Self::make_vertex_buffer(display, settings.nr_cells)?;

        // shader
        let shader_program =
            Program::from_source(display, shaders::GRID_VERT, shaders::GRID_FRAG, None).map_err(
                |e| RendererError::Graphics {
                    source: Box::new(e),
                },
            )?;

        let renderer = GridRenderer {
            vertex_buffer,
            shader_program,
            settings,
        };
        Ok(renderer)
    }

    pub fn update_settings(
        &mut self,
        display: &Display,
        settings: settings::Grid,
    ) -> RendererResult<()> {
        // Only recreate vertex data, if necessary
        if self.settings.nr_cells != settings.nr_cells {
            self.vertex_buffer = Self::make_vertex_buffer(display, settings.nr_cells)?;
        }

        // Update settings
        self.settings = settings;
        Ok(())
    }

    pub fn draw(
        &self,
        frame: &mut Frame,
        view_projection_matrix: &Matrix4<f64>,
        inverse_view_matrix: &Matrix4<f64>,
        scale_factor: f64,
    ) -> RendererResult<()> {
        // calculate the distance of the camera to the xy-plane
        let camera_pos: Vector4<f64> = inverse_view_matrix * Vector4::new(0.0, 0.0, 0.0, 1.0);
        let camera_pos: Vector3<f64> = Vector3::new(
            camera_pos.x / camera_pos.w,
            camera_pos.y / camera_pos.w,
            camera_pos.z / camera_pos.w,
        );
        let camera_dir: Vector4<f64> = inverse_view_matrix * Vector4::new(0.0, 0.0, -1.0, 0.0);
        let camera_dir: Vector3<f64> =
            Vector3::new(camera_dir.x, camera_dir.y, camera_dir.z).normalize();
        let dist = -camera_pos.z / camera_dir.z;
        let focus_point = camera_pos + camera_dir * dist;

        // size of the grid is based on the distance to the camera
        let base = 5.0;
        let size_step_float = dist.log(base);
        let size_step = size_step_float.ceil();
        let size = base.powf(size_step) * self.settings.size;

        // center of the grid: below the camera
        let cell_size = size / self.settings.nr_cells as f64;
        let center_x = (focus_point.x / cell_size).floor() * cell_size;
        let center_y = (focus_point.y / cell_size).floor() * cell_size;

        let vpm_data = matrix_to_gl(view_projection_matrix);
        let color = [
            self.settings.color.r,
            self.settings.color.g,
            self.settings.color.b,
            self.settings.opacity,
        ];
        let uniforms = uniform! {
            color: color,
            view_projection_matrix: vpm_data,
            x_min: (-size / 2.0 + center_x) as f32,
            x_max: (size / 2.0 + center_x) as f32,
            y_min: (-size / 2.0 + center_y) as f32,
            y_max: (size / 2.0 + center_y) as f32,
        };

        let draw_parameters = DrawParameters {
            line_width: Some(self.settings.line_width * scale_factor as f32),
            depth: glium::Depth {
                write: true,
                test: DepthTest::IfLess,
                ..Default::default()
            },
            blend: glium::Blend {
                color: BlendingFunction::Addition {
                    source: LinearBlendingFactor::SourceAlpha,
                    destination: LinearBlendingFactor::OneMinusSourceAlpha,
                },
                alpha: BlendingFunction::Addition {
                    source: LinearBlendingFactor::One,
                    destination: LinearBlendingFactor::OneMinusSourceAlpha,
                },
                constant_value: (0.0, 0.0, 0.0, 0.0),
            },
            ..Default::default()
        };

        frame
            .draw(
                &self.vertex_buffer,
                &NoIndices(PrimitiveType::LinesList),
                &self.shader_program,
                &uniforms,
                &draw_parameters,
            )
            .map_err(|e| RendererError::Graphics {
                source: Box::new(e),
            })?;

        // fade in the next level of detail of the grid
        let size = size / base;
        let fadein_opacity = size_step - size_step_float;
        let color = [
            self.settings.color.r,
            self.settings.color.g,
            self.settings.color.b,
            self.settings.opacity * fadein_opacity as f32,
        ];
        let uniforms = uniform! {
            color: color,
            view_projection_matrix: vpm_data,
            x_min: (-size / 2.0 + center_x) as f32,
            x_max: (size / 2.0 + center_x) as f32,
            y_min: (-size / 2.0 + center_y) as f32,
            y_max: (size / 2.0 + center_y) as f32,
        };
        frame
            .draw(
                &self.vertex_buffer,
                &NoIndices(PrimitiveType::LinesList),
                &self.shader_program,
                &uniforms,
                &draw_parameters,
            )
            .map_err(|e| RendererError::Graphics {
                source: Box::new(e),
            })?;

        Ok(())
    }
}
