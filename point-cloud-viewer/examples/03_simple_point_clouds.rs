use pasture_core::nalgebra::Point3;
use point_cloud_viewer::renderer::settings::{
    BaseRenderSettings, Color, PointCloudRenderSettings, PointColor, PointShape, PointSize,
};
use point_cloud_viewer::renderer::viewer::RenderThreadBuilderExt;
use std::thread::sleep;
use std::time::Duration;

mod utils;

fn main() {
    point_cloud_viewer::renderer::backends::glium::GliumRenderOptions::default().run(
        |render_thread| {
            let window = render_thread.open_window().unwrap();

            // The simplest way to add a point cloud is the `add_point_cloud` function of the window.
            // It takes a point buffer with the points to visualize, and returns a point cloud ID.
            let point_buffer = utils::small_example_point_cloud(Point3::origin(), 100);
            let point_cloud_id = window.add_point_cloud(&point_buffer).unwrap();

            // The point cloud id can be used, to reference the point cloud later.
            // For example, we can pass it to the `remove_point_cloud` function, to remove it again.
            sleep(Duration::from_secs(3));
            window.remove_point_cloud(point_cloud_id).unwrap();

            // Should we want to update the points of a point cloud,
            // then we can re-upload a new point buffer to the GPU, by using `update_point_cloud`:
            let empty_point_buffer = utils::small_example_point_cloud(Point3::origin(), 0);
            let point_cloud_id = window.add_point_cloud(&empty_point_buffer).unwrap();
            for i in 0..100 {
                let new_point_buffer = utils::small_example_point_cloud(Point3::origin(), i);
                window
                    .update_point_cloud(point_cloud_id, &new_point_buffer, &[])
                    .unwrap();
                sleep(Duration::from_secs_f64(0.1));
            }
            window.remove_point_cloud(point_cloud_id).unwrap();

            // Every point cloud has a bunch of settings, such as which color to use when
            // drawing the points.
            // These settings are defined using the `PointCloudRenderSettings` struct.
            // One simple way of changing the point cloud settings
            // is the `set_default_point_cloud_settings` function:
            let point_cloud_id = window.add_point_cloud(&point_buffer).unwrap();
            window
                .set_default_point_cloud_settings(PointCloudRenderSettings {
                    // All points will be red
                    point_color: PointColor::Fixed(Color::RED),

                    // The points will have a size of 15 pixels
                    point_size: PointSize::Fixed(15.0),

                    // By default, points are rendered as little squares. This changes them to be round.
                    point_shape: PointShape::Round,
                })
                .unwrap();
            sleep(Duration::from_secs(3));
            window.remove_point_cloud(point_cloud_id).unwrap();

            // In general, we can add as many point clouds to a window, as we want.
            // As the name suggests, `set_default_point_cloud_settings` will equally influence
            // all point clouds.
            // Here, we add three point clouds, and make all points grey:
            let point_buf_1 = utils::small_example_point_cloud(Point3::new(0.0, -0.5, 0.0), 50);
            let point_buf_2 = utils::small_example_point_cloud(Point3::new(0.0, 0.1, 0.0), 50);
            let point_buf_3 = utils::small_example_point_cloud(Point3::new(0.0, 0.7, 0.0), 50);
            let point_cloud_id_1 = window.add_point_cloud(&point_buf_1).unwrap();
            let point_cloud_id_2 = window.add_point_cloud(&point_buf_2).unwrap();
            let point_cloud_id_3 = window.add_point_cloud(&point_buf_3).unwrap();
            window
                .set_default_point_cloud_settings(PointCloudRenderSettings {
                    // This makes **all three** point clouds blue.
                    point_color: PointColor::Fixed(Color::GREY_5),
                    point_size: PointSize::Fixed(15.0),
                    point_shape: PointShape::Square,
                })
                .unwrap();
            sleep(Duration::from_secs(3));

            // One common use case will however be, to color each point cloud differently, so that
            // they can be distinguished by the user.
            // This can be accomplished through the function `set_point_cloud_settings`, which
            // allows us to set the settings per point cloud.
            // In the following example, we color the point clouds red, green and blue, respectively:
            let settings_base = PointCloudRenderSettings {
                point_color: PointColor::Fixed(Color::BLUE),
                point_size: PointSize::Fixed(15.0),
                point_shape: PointShape::Round,
            };
            window
                .set_point_cloud_settings(
                    point_cloud_id_1,
                    PointCloudRenderSettings {
                        point_color: PointColor::Fixed(Color::RED),
                        ..settings_base
                    },
                )
                .unwrap();
            window
                .set_point_cloud_settings(
                    point_cloud_id_2,
                    PointCloudRenderSettings {
                        point_color: PointColor::Fixed(Color::GREEN),
                        ..settings_base
                    },
                )
                .unwrap();
            window
                .set_point_cloud_settings(
                    point_cloud_id_3,
                    PointCloudRenderSettings {
                        point_color: PointColor::Fixed(Color::BLUE),
                        ..settings_base
                    },
                )
                .unwrap();
            sleep(Duration::from_secs(3));

            // The "per point cloud settings" always overwrite the "default settings".
            // If a point cloud has its own settings,
            // then a call to `set_default_point_cloud_settings` will not influence this point cloud.
            // We can call `reset_point_cloud_settings` to remove the "per point cloud settings"
            // of a point cloud again. Only then will it fall back to the default settings again.
            {
                // resetting the settings will make a point cloud fall back to the default
                // settings, which right now is grey,square points.
                window.reset_point_cloud_settings(point_cloud_id_1).unwrap();
                window.reset_point_cloud_settings(point_cloud_id_2).unwrap();
                sleep(Duration::from_secs(3));

                // at this point, only point cloud 3 has specialized settings,
                // so changing the default settings will influence point clouds 1 and 2,
                // while point cloud 3 stays as it is.
                window
                    .set_default_point_cloud_settings(PointCloudRenderSettings {
                        point_color: PointColor::Fixed(Color::GREY_5),
                        point_size: PointSize::Fixed(15.0),
                        point_shape: PointShape::Round, // changes the points from square to round.
                    })
                    .unwrap();
            }

            window.join()
        },
    );
}
