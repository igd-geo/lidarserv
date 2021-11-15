use crate::cli::Args;
use anyhow::Result;
use lidarserv_server::common::geometry::bounding_box::{BaseAABB, AABB};
use lidarserv_server::common::geometry::grid::LodLevel;
use lidarserv_server::common::geometry::points::PointType;
use lidarserv_server::common::geometry::position::Position;
use lidarserv_server::common::las::LasPointAttributes;
use lidarserv_server::common::nalgebra::Point3;
use lidarserv_server::index::point::GlobalPoint;
use lidarserv_server::net::client::viewer::{IncrementalUpdate, ViewerClient};
use log::{debug, error, trace};
use pasture_core::containers::{PerAttributeVecPointStorage, PointBuffer};
use pasture_core::layout::PointType as PasturePointType;
use pasture_core::math::AABB as PastureAABB;
use pasture_core::nalgebra::Vector3;
use pasture_derive::PointType;
use point_cloud_viewer::renderer::settings::{
    BaseRenderSettings, Color, PointCloudRenderSettings, PointColor, PointShape, PointSize,
};
use point_cloud_viewer::renderer::viewer::RenderThreadBuilderExt;
use std::collections::HashMap;
use std::thread;
use tokio::sync::broadcast::Receiver;

mod cli;

#[paw::main]
fn main(args: Args) {
    simple_logger::init_with_level(args.log_level).unwrap();
    match main_with_errorhandling(args) {
        Ok(()) => (),
        Err(e) => {
            error!("{}", e);
        }
    }
}

fn main_with_errorhandling(args: Args) -> Result<()> {
    let (exit_sender, mut exit_receiver) = tokio::sync::broadcast::channel(1);
    let (update_sender, update_receiver) = crossbeam_channel::bounded(5);
    let net = thread::spawn(move || network_thread(args, &mut exit_receiver, update_sender));
    viewer_thread(update_receiver);
    net.join().unwrap()
}

#[tokio::main]
async fn network_thread(
    args: Args,
    shutdown: &mut Receiver<()>,
    updates_sender: crossbeam_channel::Sender<IncrementalUpdate>,
) -> Result<()> {
    // connect
    let mut client = ViewerClient::connect((args.host, args.port), shutdown).await?;

    // set query
    let aabb = AABB::new(
        Point3::new(
            412785.340004 - 213.7,
            5318821.784996 - 282.33,
            315.510010 - 50.86,
        ),
        Point3::new(
            412785.340004 + 213.7,
            5318821.784996 + 282.33,
            315.510010 + 50.86,
        ),
    );
    client.query_aabb(&aabb, &LodLevel::from_level(5)).await?;

    // keep receiving updates
    loop {
        let update = client.receive_update(shutdown).await?;
        trace!("{:?}", update);
        updates_sender.send(update).unwrap();
    }
}

fn viewer_thread(updates_receiver: crossbeam_channel::Receiver<IncrementalUpdate>) {
    point_cloud_viewer::renderer::backends::glium::GliumRenderOptions::default().run(
        move |render_thread| {
            // create window
            let window = render_thread.open_window().unwrap();
            window
                .set_render_settings(BaseRenderSettings {
                    window_title: "LidarServ Viewer".to_string(),
                    grid: Some(Default::default()),
                    enable_edl: true,
                    ..Default::default()
                })
                .unwrap();
            window
                .set_default_point_cloud_settings(PointCloudRenderSettings {
                    point_color: PointColor::Fixed(Color::GREY_5),
                    point_shape: PointShape::Round,
                    point_size: PointSize::Fixed(5.0),
                })
                .unwrap();

            // move to where the query is
            window
                .camera_movement()
                .focus_on_bounding_box(PastureAABB::from_min_max(
                    Point3::new(
                        412785.340004 - 213.7,
                        5318821.784996 - 282.33,
                        315.510010 - 50.86,
                    ),
                    Point3::new(
                        412785.340004 + 213.7,
                        5318821.784996 + 282.33,
                        315.510010 + 50.86,
                    ),
                ))
                .execute()
                .unwrap();

            // keep applying updates
            let mut point_clouds = HashMap::new();
            for update in updates_receiver.iter() {
                let IncrementalUpdate {
                    mut remove,
                    mut insert,
                } = update;

                // update
                let mut update_points = None;
                if let Some(remove_node_id) = &remove {
                    let matching_index = insert.iter().position(|n| n.node_id == *remove_node_id);
                    if let Some(i) = matching_index {
                        update_points = Some(insert.swap_remove(i));
                        remove = None;
                    }
                }
                if let Some(node) = update_points {
                    let point_cloud_id = *point_clouds.get(&node.node_id).unwrap();
                    window.update_point_cloud(point_cloud_id, &node_to_pasture(node.points), &[]);
                }

                // insert new pcs
                for node in insert {
                    let point_cloud_id = window
                        .add_point_cloud(&node_to_pasture(node.points))
                        .unwrap();
                    point_clouds.insert(node.node_id, point_cloud_id);
                }

                // if there is a pc to remove:
                if let Some(node_id) = remove {
                    let point_cloud_id = point_clouds.remove(&node_id).unwrap();
                    window.remove_point_cloud(point_cloud_id).unwrap();
                }
            }
        },
    );
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Default, PointType)]
pub struct PasturePointt {
    #[pasture(BUILTIN_POSITION_3D)]
    pub position: Vector3<f64>,
    #[pasture(BUILTIN_INTENSITY)]
    pub intensity: u16,
}

fn node_to_pasture(points: Vec<GlobalPoint>) -> impl PointBuffer {
    let mut point_buf = PerAttributeVecPointStorage::new(PasturePointt::layout());
    for point in points {
        let pasture_point = PasturePointt {
            position: Vector3::new(
                point.position().x(),
                point.position().y(),
                point.position().z(),
            ),
            intensity: point.attribute::<LasPointAttributes>().intensity,
        };
        point_buf.push_point(pasture_point);
    }
    point_buf
}
