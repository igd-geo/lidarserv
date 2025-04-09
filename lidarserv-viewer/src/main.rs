use crate::cli::{Args, PointColorArg};
use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use lidarserv_common::geometry::bounding_box::Aabb;
use lidarserv_common::query::view_frustum::ViewFrustumQuery;
use lidarserv_server::common::nalgebra::{Matrix4, Point3};
use lidarserv_server::index::query::Query;
use lidarserv_server::net::client::viewer::{PartialResult, QueryConfig, ViewerClient};
use log::debug;
use nalgebra::{point, vector};
use pasture_core::containers::{
    BorrowedBuffer, BorrowedBufferExt, BorrowedMutBufferExt, VectorBuffer,
};
use pasture_core::layout::PointType;
use pasture_core::layout::attributes::{COLOR_RGB, INTENSITY, POSITION_3D};
use pasture_core::layout::conversion::BufferLayoutConverter;
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
use std::f64::consts::FRAC_PI_4;
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
        // disable eye dome lighting for macos
        let edl = !cfg!(target_os = "macos") && !args.disable_eye_dome_lighting;
        window
            .set_render_settings(BaseRenderSettings {
                window_title: "LidarServ Viewer".to_string(),
                grid: Some(Default::default()),
                enable_edl: edl,
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
        let (bbox_sender, bbox_receiver) = crossbeam_channel::bounded(1);

        let args_clone = args.clone();
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(network_thread(
                args_clone,
                &mut exit_receiver,
                updates_sender,
                tokio_camera_receiver,
                bbox_sender,
            ))
            .unwrap();
        });
        thread::spawn(move || {
            forward_camera(camera_receiver, tokio_camera_sender, offset);
            exit_sender.send(()).ok();
        });

        // get initial bounding box
        let initial_bounding_box = bbox_receiver.recv().unwrap();
        let pasture_aabb = if !initial_bounding_box.is_empty() {
            PastureAABB::from_min_max(
                Point3::new(
                    initial_bounding_box.min.x,
                    initial_bounding_box.min.y,
                    initial_bounding_box.min.z,
                ),
                Point3::new(
                    initial_bounding_box.max.x,
                    initial_bounding_box.max.y,
                    initial_bounding_box.max.z,
                ),
            )
        } else {
            PastureAABB::from_min_max(
                Point3::new(-50.0, -50.0, 0.0),
                Point3::new(50.0, 50.0, 50.0),
            )
        };

        // move to initial bounding box
        if !initial_bounding_box.is_empty() {
            window
                .camera_movement()
                .focus_on_bounding_box(pasture_aabb)
                .execute()
                .unwrap();
        }

        // keep track of largest and smallest intensity for coloration.
        let mut min_intensity = u16::MAX;
        let mut max_intensity = u16::MIN;

        // keep applying updates
        let mut point_clouds = HashMap::new();
        for update in updates_receiver.iter() {
            match update {
                PartialResult::DeleteNode(node_id) => {
                    let point_cloud_id = point_clouds.remove(&node_id).unwrap();
                    window.remove_point_cloud(point_cloud_id).unwrap();
                }
                PartialResult::UpdateNode(update) => {
                    if update.points.point_layout().has_attribute(&INTENSITY) {
                        for intensity in update.points.view_attribute::<u16>(&INTENSITY) {
                            let mut changed = false;
                            if intensity > max_intensity {
                                max_intensity = intensity;
                                changed = true;
                            }
                            if intensity < min_intensity {
                                min_intensity = intensity;
                                changed = true;
                            }
                            if changed && args.point_color == PointColorArg::Intensity {
                                window
                                    .set_default_point_cloud_settings(PointCloudRenderSettings {
                                        point_color: PointColor::ScalarAttribute(
                                            ScalarAttributeColoring {
                                                attribute: INTENSITY,
                                                color_map: ColorMap::fire(),
                                                min: min_intensity as f32,
                                                max: max_intensity as f32,
                                            },
                                        ),
                                        point_shape: PointShape::Round,
                                        point_size: PointSize::Fixed(args.point_size),
                                    })
                                    .unwrap();
                            }
                        }
                    }
                    if let Some(point_cloud_id) = point_clouds.get(&update.node_id) {
                        // update
                        window
                            .update_point_cloud(
                                *point_cloud_id,
                                &node_to_pasture(update.points, offset),
                                &[&INTENSITY, &COLOR_RGB],
                            )
                            .unwrap();
                    } else {
                        // insert
                        let point_cloud_id = window
                            .add_point_cloud_with_attributes(
                                &node_to_pasture(update.points, offset),
                                &[&INTENSITY, &COLOR_RGB],
                            )
                            .unwrap();
                        point_clouds.insert(update.node_id, point_cloud_id);
                    }
                }
                PartialResult::Complete => (),
            };
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

async fn network_thread(
    args: Args,
    shutdown: &mut Receiver<()>,
    updates_sender: crossbeam_channel::Sender<PartialResult<VectorBuffer>>,
    mut camera_reciver: tokio::sync::mpsc::Receiver<Matrices>,
    bbox_sender: crossbeam_channel::Sender<Aabb<f64>>,
) -> Result<()> {
    // connect
    let ViewerClient {
        read: mut client_read,
        write: client_write,
    } = ViewerClient::connect((args.host, args.port), shutdown).await?;

    let inital_bounding_box = client_read.initial_bounding_box();
    bbox_sender.send(inital_bounding_box)?;

    // task to send query to server
    tokio::spawn(async move {
        loop {
            let mut camera_matrix = match camera_reciver.recv().await {
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
            let camera_pos = camera_matrix
                .view_matrix_inv
                .transform_point(&Point3::origin());
            let camera_dir = (camera_matrix.view_matrix_inv * vector![0.0, 0.0, -1.0, 0.0])
                .xyz()
                .normalize();
            let z_far = camera_matrix
                .projection_matrix_inv
                .transform_point(&point![0.0, 0.0, 1.0])
                .z
                .abs();
            let z_near = camera_matrix
                .projection_matrix_inv
                .transform_point(&point![0.0, 0.0, -1.0])
                .z
                .abs();
            let cli_query = if args.json {
                serde_json::from_str(&args.query).unwrap()
            } else {
                Query::parse(&args.query).unwrap()
            };
            let query = Query::And(vec![
                Query::ViewFrustum(ViewFrustumQuery {
                    camera_pos,
                    camera_dir,
                    camera_up: vector![0.0, 0.0, 1.0],
                    fov_y: FRAC_PI_4,
                    z_near,
                    z_far,
                    window_size: camera_matrix.window_size,
                    max_distance: args.point_distance,
                }),
                cli_query.clone(),
            ]);
            // convert query to toml and print it
            debug!("Sending query to server: {:?}", &query);
            client_write
                .query(
                    query,
                    &QueryConfig {
                        point_filtering: !args.disable_point_filtering,
                    },
                )
                .await
                .unwrap();
        }
    });

    // keep receiving updates
    loop {
        let update = client_read
            .receive_update_global_coordinates(shutdown)
            .await?;
        debug!("{:?}", update);
        updates_sender.send(update).unwrap();
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Default, PointType, Pod, Zeroable)]
pub struct PasturePointt {
    #[pasture(BUILTIN_POSITION_3D)]
    pub position: Vector3<f64>,
    #[pasture(BUILTIN_INTENSITY)]
    pub intensity: u16,
    #[pasture(BUILTIN_COLOR_RGB)]
    pub color: Vector3<u16>,
}

fn node_to_pasture(points: VectorBuffer, offset: Vector3<f64>) -> VectorBuffer {
    let to_layout = PasturePointt::layout();
    let converter =
        BufferLayoutConverter::for_layouts_with_default(points.point_layout(), &to_layout);
    let mut converted: VectorBuffer = converter.convert(&points);
    let mut positions = converted.view_attribute_mut::<Point3<f64>>(&POSITION_3D);

    for i in 0..points.len() {
        let pos = positions.at(i) - offset;
        positions.set_at(i, pos);
    }
    converted
}
