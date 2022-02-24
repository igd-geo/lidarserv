use crate::utils::attributes_example_point_cloud;
use pasture_core::layout::attributes;
use pasture_core::nalgebra::Point3;
use point_cloud_viewer::renderer::settings::{
    BaseRenderSettings, CategoricalAttributeColoring, Color, ColorMap, ColorPalette,
    PointCloudRenderSettings, PointColor, ScalarAttributeColoring,
};
use point_cloud_viewer::renderer::viewer::RenderThreadBuilderExt;

pub mod utils;

fn main() {
    point_cloud_viewer::renderer::backends::glium::GliumRenderOptions::default().run(
        |render_thread| {
            let window = render_thread.open_window().unwrap();
            window
                .set_render_settings(BaseRenderSettings {
                    grid: Some(Default::default()),
                    ..Default::default()
                })
                .unwrap();

            // Until now, we only covered static colors.
            // Sometimes, we want to visualize point attributes, such as the intensity,
            // or the classification.
            // This is, what this example is all about.

            // First of all, for an attribute to be visualized, it has to be uploaded to the GPU.
            // If we use `window.add_point_cloud` to add our point cloud, as we did until now,
            // only the positions will be uploaded.
            // In order to also upload additional attributes, we need to use the slightly
            // more advanced function `add_point_cloud_with_attributes`.
            // It takes an additional parameter, that lets us specify, which attributes to upload,
            // in addition to the point positions, that will *always* be uploaded.
            // Here we upload the position and intensity attributes of a point buffer:
            let point_buffer = attributes_example_point_cloud(Point3::new(1.1, 1.1, 0.0));
            let point_cloud_id_1 = window
                .add_point_cloud_with_attributes(&point_buffer, &[&attributes::INTENSITY])
                .unwrap();

            // We can now visualize the intensity, by referencing that attribute in the point
            // cloud settings.
            // Using any attribute in there, that is not on the GPU, would result in an error.
            window
                .set_point_cloud_settings(
                    point_cloud_id_1,
                    PointCloudRenderSettings {
                        point_color: PointColor::ScalarAttribute(ScalarAttributeColoring {
                            // The attribute to use for the coloring
                            attribute: attributes::INTENSITY,

                            // which colors to use. This preset is a simple gradient from cyan to yellow.
                            color_map: ColorMap::cyan_yellow(),

                            // the min/max attribute values,
                            // that will be mapped to the beginning/end of the color map, respectively.
                            min: 0.0,
                            max: 100.0,
                        }),
                        ..Default::default()
                    },
                )
                .unwrap();

            // The function `add_point_cloud_with_attributes_and_settings` is a shortcut, that
            // allows us, to perform both these steps at once:
            let point_buffer = attributes_example_point_cloud(Point3::new(0.0, 1.1, 0.0));
            window
                .add_point_cloud_with_attributes_and_settings(
                    &point_buffer,
                    &[&attributes::INTENSITY],
                    PointCloudRenderSettings {
                        point_color: PointColor::ScalarAttribute(ScalarAttributeColoring {
                            attribute: attributes::INTENSITY,
                            color_map: ColorMap::rainbow(),
                            min: 0.0,
                            max: 100.0,
                        }),
                        ..Default::default()
                    },
                )
                .unwrap();

            // The ColorMap type offers a bunch of presets, that usually contain some useful
            // value.
            // If we need a more specific color map, we can create a custom one. For this purpose,
            // it offers the functions `gradient` and `equally_spaced`.
            // As an example, I will show the usage of the `equally_spaced` function here:
            let striped = ColorMap::equally_spaced(&[
                Color::YELLOW,
                Color::BLACK,
                Color::YELLOW,
                Color::BLACK,
                Color::YELLOW,
                Color::BLACK,
                Color::YELLOW,
                Color::BLACK,
                Color::YELLOW,
                Color::BLACK,
            ]);
            let point_buffer = attributes_example_point_cloud(Point3::new(-1.1, 1.1, 0.0));
            window
                .add_point_cloud_with_attributes_and_settings(
                    &point_buffer,
                    &[&attributes::INTENSITY],
                    PointCloudRenderSettings {
                        point_color: PointColor::ScalarAttribute(ScalarAttributeColoring {
                            attribute: attributes::INTENSITY,
                            color_map: striped,
                            min: 0.0,
                            max: 100.0,
                        }),
                        ..Default::default()
                    },
                )
                .unwrap();

            // While coloring points with a ColorMap is useful for attributes,
            // such as the intensity, it is not really, what we want for e.g. the classification.
            // Here, we want a well defined color for every (integer) value.
            // This is possible, by switching from `PointColor::ScalarAttribute` to
            // `PointColor::CategoricalAttribute`:
            let point_buffer = attributes_example_point_cloud(Point3::new(0.55, 0.0, 0.0));
            window
                .add_point_cloud_with_attributes_and_settings(
                    &point_buffer,
                    &[&attributes::CLASSIFICATION],
                    PointCloudRenderSettings {
                        point_color: PointColor::CategoricalAttribute(
                            CategoricalAttributeColoring {
                                attribute: attributes::CLASSIFICATION,
                                color_palette: ColorPalette::las_classification_colors(),
                            },
                        ),
                        ..Default::default()
                    },
                )
                .unwrap();

            // The CategoricalAttribute coloring uses a ColorPalette instead of a ColorMap to map
            // the attribute values to colors.
            // A good default for visualizing the classification will be the preset
            // `ColorPalette::las_classification_colors`, which defines matching colors for the
            // classes as defined in the specification for the LAS file format.
            // However, also here, we can define our own palettes:
            let rgb_palette = ColorPalette::new(Color::GREY_5)
                .with_color(0, Color::RED)
                .with_color(1, Color::GREEN)
                .with_color(2, Color::BLUE)
                .with_color(3, Color::RED)
                .with_color(4, Color::GREEN)
                .with_color(5, Color::BLUE)
                .with_color(6, Color::RED)
                .with_color(7, Color::GREEN)
                .with_color(8, Color::BLUE)
                .with_color(9, Color::RED)
                .with_color(10, Color::GREEN)
                .with_color(11, Color::BLUE)
                .with_color(12, Color::RED)
                .with_color(13, Color::GREEN)
                .with_color(14, Color::BLUE);
            let point_buffer = attributes_example_point_cloud(Point3::new(-0.55, 0.0, 0.0));
            window
                .add_point_cloud_with_attributes_and_settings(
                    &point_buffer,
                    &[&attributes::CLASSIFICATION],
                    PointCloudRenderSettings {
                        point_color: PointColor::CategoricalAttribute(
                            CategoricalAttributeColoring {
                                attribute: attributes::CLASSIFICATION,
                                color_palette: rgb_palette,
                            },
                        ),
                        ..Default::default()
                    },
                )
                .unwrap();

            window.join();
        },
    );
}
