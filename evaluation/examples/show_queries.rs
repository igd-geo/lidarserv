use evaluation::config::Config;
use evaluation::indexes::create_octree_index;
use evaluation::insertion_rate::measure_insertion_rate;
use evaluation::point::Point;
use evaluation::queries::{preset_query_1, preset_query_2, preset_query_3, preset_query_4};
use evaluation::{read_points, reset_data_folder};
use lidarserv_common::geometry::points::PointType;
use lidarserv_common::geometry::position::{I32CoordinateSystem, I32Position, Position};
use lidarserv_common::index::Index;
use lidarserv_common::las::I32LasReadWrite;
use lidarserv_common::query::empty::EmptyQuery;
use lidarserv_common::query::{Query, QueryExt};
use nalgebra::Vector3;
use pasture_core::containers::{PerAttributeVecPointStorage, PointBuffer};
use pasture_core::layout::PointType as PasturePointType;
use pasture_derive::PointType;
use point_cloud_viewer::renderer::settings::{
    BaseRenderSettings, Color, PointCloudRenderSettings, PointColor, PointShape, PointSize,
};
use point_cloud_viewer::renderer::viewer::RenderThreadBuilderExt;

fn main() {
    dotenv::dotenv().unwrap();
    pretty_env_logger::init();

    // read point data
    let config = Config::from_env();
    let coordinate_system = I32CoordinateSystem::from_las_transform(
        Vector3::new(0.001, 0.001, 0.001),
        Vector3::new(0.0, 0.0, 0.0),
    );
    let points = read_points(&coordinate_system, &config);

    // create index (octree index in this case)
    reset_data_folder(&config);
    let mut octree_index = create_octree_index(coordinate_system.clone(), &config);
    measure_insertion_rate(&mut octree_index, &points, &config);

    // start up viewer
    point_cloud_viewer::renderer::backends::glium::GliumRenderOptions::default().run(
        move |render_thread| {
            // run queries
            let queries = vec![
                (
                    Box::new(preset_query_1())
                        as Box<dyn Query<I32Position, I32CoordinateSystem> + Send + Sync>,
                    Box::new(preset_query_1())
                        as Box<dyn Query<I32Position, I32CoordinateSystem> + Send + Sync>,
                ),
                (
                    Box::new(preset_query_2())
                        as Box<dyn Query<I32Position, I32CoordinateSystem> + Send + Sync>,
                    Box::new(preset_query_2())
                        as Box<dyn Query<I32Position, I32CoordinateSystem> + Send + Sync>,
                ),
                (
                    Box::new(preset_query_3())
                        as Box<dyn Query<I32Position, I32CoordinateSystem> + Send + Sync>,
                    Box::new(preset_query_3())
                        as Box<dyn Query<I32Position, I32CoordinateSystem> + Send + Sync>,
                ),
                (
                    Box::new(preset_query_4())
                        as Box<dyn Query<I32Position, I32CoordinateSystem> + Send + Sync>,
                    Box::new(preset_query_4())
                        as Box<dyn Query<I32Position, I32CoordinateSystem> + Send + Sync>,
                ),
            ];
            let las_loader = I32LasReadWrite::new(false);
            let mut windows = Vec::new();
            for (query, filter_query) in queries {
                let window = render_thread.open_window().unwrap();
                window
                    .set_render_settings(BaseRenderSettings {
                        grid: Some(Default::default()),
                        enable_edl: true,
                        ..Default::default()
                    })
                    .unwrap();
                window
                    .set_default_point_cloud_settings(PointCloudRenderSettings {
                        point_color: PointColor::Fixed(Color::GREY_5),
                        point_size: PointSize::Fixed(2.0),
                        ..Default::default()
                    })
                    .unwrap();

                let mut reader = octree_index.reader(EmptyQuery::new());
                reader.set_query(query);
                while let Some((node_id, page)) = reader.load_one() {
                    let points = page
                        .get_points(&las_loader)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|point| {
                            filter_query.matches_point(point, &coordinate_system, &node_id.lod)
                        })
                        .collect();
                    let points = to_pasture(points, &coordinate_system);
                    window.add_point_cloud(&points).unwrap();
                }
                window.camera_movement().focus_on_all().execute().unwrap();
                windows.push(window);
            }
            for window in windows {
                window.join();
            }
        },
    );
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Default, PointType)]
pub struct PasturePointt {
    #[pasture(BUILTIN_POSITION_3D)]
    pub position: Vector3<f64>,
}

fn to_pasture(points: Vec<Point>, coordinate_system: &I32CoordinateSystem) -> impl PointBuffer {
    let mut point_buf = PerAttributeVecPointStorage::new(PasturePointt::layout());
    for point in points {
        let pasture_point = PasturePointt {
            position: point.position().decode(coordinate_system).coords,
        };
        point_buf.push_point(pasture_point);
    }
    point_buf
}
