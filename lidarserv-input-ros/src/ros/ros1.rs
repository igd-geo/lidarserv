use super::{Command, Endianess, Field, PointCloudMessage, Transform, Type};
use crate::{cli::AppOptions, status::Status};
use anyhow::{anyhow, Result};
use log::{info, trace, warn};
use nalgebra::{vector, Quaternion, UnitQuaternion};
use std::{
    sync::{atomic::Ordering, mpsc, Arc},
    thread,
    time::Duration,
};

pub fn ros_thread(
    args: AppOptions,
    commands_rx: mpsc::Receiver<Command>,
    transforms_tx: mpsc::Sender<Transform>,
    points_tx: mpsc::Sender<PointCloudMessage>,
    status: Arc<Status>,
) -> Result<()> {
    // ROS init
    info!("Connecting to ROS master...");
    if rosrust::try_init_with_options("lidarserv", false).is_err() {
        return Err(anyhow!("Failed to connect to ROS master."));
    }
    info!("Connected to ROS master.");

    // Subscribe to tf
    info!(
        "Subscribing to transform tree topics `{}`, `{}` ...",
        args.tf_topic, args.tf_static_topic
    );
    let transforms_tx_clone = transforms_tx.clone();
    let status2 = Arc::clone(&status);
    let status3 = Arc::clone(&status);
    let tf_callback = move |msg: messages::tf2_msgs::TFMessage| {
        trace!("TF message: {msg:?}");
        status2.nr_rx_msg_tf.fetch_add(1, Ordering::Relaxed);
        for tf in parse_tf_message(msg, false) {
            transforms_tx.send(tf).ok();
        }
    };
    let tf_static_callback = move |msg: messages::tf2_msgs::TFMessage| {
        trace!("TF static message: {msg:?}");
        status3.nr_rx_msg_tf.fetch_add(1, Ordering::Relaxed);
        for tf in parse_tf_message(msg, true) {
            transforms_tx_clone.send(tf).ok();
        }
    };
    let _tf_subscriber = match rosrust::subscribe(&args.tf_topic, 100, tf_callback) {
        Ok(s) => s,
        Err(_) => {
            return Err(anyhow!(
                "Failed to subscribe to transform tree topic `{}`.",
                args.tf_topic
            ))
        }
    };
    let _tf_static_subscriber =
        match rosrust::subscribe(&args.tf_static_topic, 100, tf_static_callback) {
            Ok(s) => s,
            Err(_) => {
                return Err(anyhow!(
                    "Failed to subscribe to transform tree topic `{}`.",
                    args.tf_static_topic
                ))
            }
        };
    info!("Subscribed to transform tree topics.");

    // Subscribe to pointcloud
    info!(
        "Subscribing to point cloud topic `{}` ...",
        args.pointcloud_topic
    );
    let status4 = Arc::clone(&status);
    let pointcloud_callback = move |msg: messages::sensor_msgs::PointCloud2| {
        trace!("PointCloud2 message: {} points", msg.width * msg.height);
        let paused = status.paused.load(Ordering::Relaxed);
        if !paused {
            status4.nr_rx_msg_pointcloud.fetch_add(1, Ordering::Relaxed);
            status4
                .nr_rx_points
                .fetch_add(msg.width as u64 * msg.height as u64, Ordering::Relaxed);
            points_tx.send(parse_pointcloud_message(msg)).ok();
        }
    };
    let _pointcloud_subscriber =
        match rosrust::subscribe(&args.pointcloud_topic, 100, pointcloud_callback) {
            Ok(s) => s,
            Err(_) => {
                return Err(anyhow!(
                    "Failed to subscribe to point cloud topic `{}`.",
                    args.pointcloud_topic
                ))
            }
        };
    info!("Subscribed to point cloud topic.");

    // Control thread (for exiting)
    thread::spawn(move || {
        for cmd in commands_rx {
            match cmd {
                Command::Exit => rosrust::shutdown(),
            }
        }
    });

    // ROS event loop
    rosrust::spin();
    Ok(())
}

fn parse_tf_message(
    msg: messages::tf2_msgs::TFMessage,
    is_static: bool,
) -> impl Iterator<Item = Transform> {
    msg.transforms.into_iter().map(move |t| Transform {
        frame: t.child_frame_id,
        parent_frame: t.header.frame_id,
        is_static,
        time_stamp: Duration::new(t.header.stamp.sec as u64, t.header.stamp.nsec),
        translation: vector![
            t.transform.translation.x,
            t.transform.translation.y,
            t.transform.translation.z
        ],
        rotation: UnitQuaternion::new_normalize(Quaternion::new(
            t.transform.rotation.w,
            t.transform.rotation.x,
            t.transform.rotation.y,
            t.transform.rotation.z,
        )),
    })
}

fn parse_pointcloud_message(msg: messages::sensor_msgs::PointCloud2) -> PointCloudMessage {
    PointCloudMessage {
        frame: msg.header.frame_id,
        time_stamp: Duration::new(msg.header.stamp.sec as u64, msg.header.stamp.nsec),
        endianess: if msg.is_bigendian {
            Endianess::BigEndian
        } else {
            Endianess::LittleEndian
        },
        width: msg.width as usize,
        height: msg.height as usize,
        point_step: msg.point_step as usize,
        row_step: msg.row_step as usize,
        fields: msg
            .fields
            .into_iter()
            .flat_map(|f| {
                Some(Field {
                    typ: match f.datatype {
                        1 => Type::I8,
                        2 => Type::U8,
                        3 => Type::I16,
                        4 => Type::U16,
                        5 => Type::I32,
                        6 => Type::U32,
                        7 => Type::F32,
                        8 => Type::F64,
                        t => {
                            warn!(
                                "Unrecognized type {} for field {} in PointCloud2 message. Ignoring this field.",
                                t, &f.name
                            );
                            return None;
                        }
                    },
                    name: f.name,
                    offset: f.offset as usize,
                    count: f.count as usize,
                })
            })
            .collect(),
        data: msg.data,
    }
}

mod messages {
    rosrust::rosmsg_include!(sensor_msgs / PointCloud2, tf2_msgs / TFMessage);
}
