use crate::cli::{Args, PointColorArg};
use anyhow::Result;
use lidarserv_server::common::geometry::points::PointType;
use lidarserv_server::common::geometry::position::Position;
use lidarserv_server::common::index::octree::attribute_bounds::LasPointAttributeBounds;
use lidarserv_server::common::las::LasPointAttributes;
use lidarserv_server::common::nalgebra::{Matrix4, Point3};
use lidarserv_server::index::point::GlobalPoint;
use lidarserv_server::net::client::viewer::{IncrementalUpdate, ViewerClient};
use lidarserv_server::net::protocol::messages::QueryConfig;
use log::{debug, trace};
use pasture_core::containers::{PerAttributeVecPointStorage, PointBuffer};
use pasture_core::layout::attributes::{COLOR_RGB, INTENSITY};
use pasture_core::layout::PointType as PasturePointType;
use pasture_core::math::AABB as PastureAABB;
use pasture_core::nalgebra::Vector3;
use pasture_derive::PointType;
use point_cloud_viewer::navigation::Matrices;
use point_cloud_viewer::renderer::backends::glium::GliumRenderOptions;
use point_cloud_viewer::renderer::settings::{
    BaseRenderSettings, Color, ColorMap, PointCloudRenderSettings, PointColor, PointShape,
    PointSize, RgbPointColoring, ScalarAttributeColoring,
};
use point_cloud_viewer::renderer::viewer::RenderThreadBuilderExt;
use std::collections::HashMap;
use std::thread;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::error::TryRecvError;

mod cli;

#[paw::main]
fn main(args: Args) {
    simple_logger::init_with_level(args.log_level).unwrap();
    let options = GliumRenderOptions {
        multisampling: args.multisampling,
    };
    options.run(move |render_thread| {
        // create window
        let window = render_thread.open_window().unwrap();
        window
            .set_render_settings(BaseRenderSettings {
                window_title: "LidarServ Viewer".to_string(),
                grid: Some(Default::default()),
                enable_edl: !args.disable_eye_dome_lighting,
                ..Default::default()
            })
            .unwrap();
        window
            .set_default_point_cloud_settings(PointCloudRenderSettings {
                point_color: match args.point_color {
                    PointColorArg::Fixed => PointColor::Fixed(Color::BLUE),
                    PointColorArg::Intensity => {
                        PointColor::ScalarAttribute(ScalarAttributeColoring {
                            attribute: INTENSITY,
                            color_map: ColorMap::fire(),
                            min: 0.0,
                            max: u16::MAX as f32,
                        })
                    }
                    PointColorArg::Rgb => PointColor::Rgb(RgbPointColoring {
                        attribute: COLOR_RGB,
                    }),
                },
                point_shape: PointShape::Round,
                point_size: PointSize::Fixed(args.point_size),
            })
            .unwrap();

        // start network thread and continuosly
        //  - send camera matrix to server
        //  - receive point cloud updates from server
        let offset = Vector3::new(0.0, 0.0, 0.0);
        let camera_receiver = window.subscribe_to_camera().unwrap();
        let (tokio_camera_sender, tokio_camera_receiver) = tokio::sync::mpsc::channel(500);
        let (exit_sender, mut exit_receiver) = tokio::sync::broadcast::channel(1);
        let (updates_sender, updates_receiver) = crossbeam_channel::bounded(5);
        thread::spawn(move || {
            network_thread(
                args,
                &mut exit_receiver,
                updates_sender,
                tokio_camera_receiver,
            )
        });
        thread::spawn(move || {
            forward_camera(camera_receiver, tokio_camera_sender, offset);
            exit_sender.send(()).ok();
        });

        // move to where the query is
        // todo do not hardcode
        window
            .camera_movement()
            .focus_on_bounding_box(PastureAABB::from_min_max(
                Point3::new(-213.7, -282.33, 0.0),
                Point3::new(213.7, 282.33, 50.0),
            ))
            .execute()
            .unwrap();

        // keep applying updates
        let mut point_clouds = HashMap::new();
        for update in updates_receiver.iter() {
            let IncrementalUpdate {
                mut remove,
                mut insert,
                ..
            } = update;

            debug!("Received update with {} insertions", insert.len());

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
                window
                    .update_point_cloud(
                        point_cloud_id,
                        &node_to_pasture(node.points, offset),
                        &[&INTENSITY, &COLOR_RGB],
                    )
                    .unwrap();
            }

            // insert new pcs
            for node in insert {
                let point_cloud_id = window
                    .add_point_cloud_with_attributes(
                        &node_to_pasture(node.points, offset),
                        &[&INTENSITY, &COLOR_RGB],
                    )
                    .unwrap();
                point_clouds.insert(node.node_id, point_cloud_id);
            }

            // if there is a pc to remove:
            if let Some(node_id) = remove {
                let point_cloud_id = point_clouds.remove(&node_id).unwrap();
                window.remove_point_cloud(point_cloud_id).unwrap();
            }
        }
    });
}

#[allow(clippy::assign_op_pattern)]
// the 'a = a * x' syntax is better than 'a *= x' for matrix multiplication, to make clear
// what is the left and what is the right operand. Therefore silenced that clippy lint...
fn forward_camera(
    receiver: crossbeam_channel::Receiver<Matrices>,
    sender: tokio::sync::mpsc::Sender<Matrices>,
    offset: Vector3<f64>,
) {
    let mut last_message = None;
    for mut message in receiver {
        if let Some(last_message) = &last_message {
            if *last_message == message {
                continue;
            }
        }
        last_message = Some(message.clone());
        message.view_matrix = message.view_matrix
            * Matrix4::new(
                // damn, rustfmt...
                1.0, 0.0, 0.0, -offset.x, 0.0, 1.0, 0.0, -offset.y, 0.0, 0.0, 1.0, -offset.z, 0.0,
                0.0, 0.0, 1.0,
            );
        message.view_matrix_inv = Matrix4::new(
            1.0, 0.0, 0.0, offset.x, 0.0, 1.0, 0.0, offset.y, 0.0, 0.0, 1.0, offset.z, 0.0, 0.0,
            0.0, 1.0,
        ) * message.view_matrix_inv;
        sender.blocking_send(message).ok();
    }
}

#[tokio::main]
async fn network_thread(
    args: Args,
    shutdown: &mut Receiver<()>,
    updates_sender: crossbeam_channel::Sender<IncrementalUpdate>,
    mut camera_reciver: tokio::sync::mpsc::Receiver<Matrices>,
) -> Result<()> {
    // connect
    let client = ViewerClient::connect((args.host, args.port), shutdown).await?;
    let (mut client_read, mut client_write) = client.into_split();
    let (ack_sender, mut ack_receiver) = tokio::sync::mpsc::unbounded_channel();

    // task to send query to server
    tokio::spawn(async move {
        loop {
            tokio::select! {
                c = camera_reciver.recv() => {
                    // get latest query
                    let mut camera_matrix = match c {
                        None => return,
                        Some(m) => m,
                    };
                    loop {
                        match camera_reciver.try_recv() {
                            Ok(m) => camera_matrix = m,
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => return,
                        }
                    }

                    // send to server
                    let view_projection_matrix =
                        camera_matrix.projection_matrix * camera_matrix.view_matrix;
                    let view_projection_matrix_inv =
                        camera_matrix.view_matrix_inv * camera_matrix.projection_matrix_inv;
                    debug!("Query: {:?}", view_projection_matrix);
                    client_write
                        .query_view_frustum(
                            view_projection_matrix,
                            view_projection_matrix_inv,
                            camera_matrix.window_size.x,
                            args.point_distance,
                            LasPointAttributeBounds::default(),
                            QueryConfig { enable_attribute_acceleration: false, enable_histogram_acceleration: false, enable_point_filtering: false },
                        )
                        .await
                        .unwrap()

                },
                c = ack_receiver.recv() => {
                    match c {
                        None => return,
                        Some(()) => {
                            client_write.ack().await.unwrap();
                        },
                    }
                },
            }
        }
    });

    // keep receiving updates
    loop {
        let update = client_read.receive_update(shutdown).await?;
        ack_sender.send(()).ok();
        trace!("{:?}", update);
        updates_sender.send(update).unwrap();
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Default, PointType)]
pub struct PasturePointt {
    #[pasture(BUILTIN_POSITION_3D)]
    pub position: Vector3<f64>,
    #[pasture(BUILTIN_INTENSITY)]
    pub intensity: u16,
    #[pasture(BUILTIN_COLOR_RGB)]
    pub color: Vector3<u16>,
}

fn node_to_pasture(points: Vec<GlobalPoint>, offset: Vector3<f64>) -> impl PointBuffer {
    let mut point_buf = PerAttributeVecPointStorage::new(PasturePointt::layout());
    for point in points {
        let pasture_point = PasturePointt {
            position: Vector3::new(
                point.position().x(),
                point.position().y(),
                point.position().z(),
            ) - offset,
            intensity: point.attribute::<LasPointAttributes>().intensity,
            color: Vector3::new(
                point.attribute::<LasPointAttributes>().color.0,
                point.attribute::<LasPointAttributes>().color.1,
                point.attribute::<LasPointAttributes>().color.2,
            ),
        };
        point_buf.push_point(pasture_point);
    }
    point_buf
}
