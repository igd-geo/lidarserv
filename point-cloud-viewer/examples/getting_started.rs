use crate::utils::small_example_point_cloud;
use pasture_core::nalgebra::Point3;
use point_cloud_viewer::renderer::settings::{
    BaseRenderSettings, Color, Grid, PointCloudRenderSettings, PointColor, PointShape, PointSize,
};
use point_cloud_viewer::renderer::viewer::RenderThreadBuilderExt;

pub mod utils;

// This is an (almost) minimal example for how to use the point cloud viewer.
// For more guidance, also look at the other, tutorial-style examples 01_init to 04_point_attributes.

fn main() {
    // start render thread
    let options = point_cloud_viewer::renderer::backends::glium::GliumRenderOptions::default();
    options.run(|render_thread| {
        // open window
        let window = render_thread.open_window().unwrap();

        // (optional) configure window settings - here we enable the grid.
        window
            .set_render_settings(BaseRenderSettings {
                grid: Some(Grid::default()),
                ..Default::default()
            })
            .unwrap();

        // add point cloud
        let point_buffer = small_example_point_cloud(Point3::new(0.0, 0.0, 0.0), 500);
        let point_cloud_id = window.add_point_cloud(&point_buffer).unwrap();

        // (optional) configure point cloud settings
        window
            .set_point_cloud_settings(
                point_cloud_id,
                PointCloudRenderSettings {
                    point_color: PointColor::Fixed(Color::BLUE),
                    point_shape: PointShape::Round,
                    point_size: PointSize::Depth(75.0),
                },
            )
            .unwrap();

        // wait for the user to close the window.
        window.join();
    });
}
