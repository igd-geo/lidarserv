use crate::cli::AppOptions;

use super::{Command, PointCloudFrame, Transform};
use anyhow::{anyhow, Result};
use log::info;
use nalgebra::{vector, Quaternion};
use std::{sync::mpsc, thread, time::Duration};

pub fn ros_thread(
    args: AppOptions,
    commands_rx: mpsc::Receiver<Command>,
    transforms_tx: mpsc::Sender<Transform>,
    points_tx: mpsc::Sender<PointCloudFrame>,
) -> Result<()> {
    // ROS init
    info!("Connecting to ROS master...");
    if let Err(e) = rosrust::try_init_with_options("lidarserv", false) {
        return Err(anyhow!("Failed to connect to ROS master: {e}"));
    }
    info!("Connected to ROS master.");

    // Subscribe to tf
    info!(
        "Subscribing to transform tree topic `{}` ...",
        args.tf_topic
    );
    let tf_callback = move |msg: messages::tf2_msgs::TFMessage| {
        for tf in parse_tf_message(msg) {
            transforms_tx.send(tf).ok();
        }
    };
    let _tf_subscriber = match rosrust::subscribe(&args.tf_topic, 100, tf_callback) {
        Ok(s) => s,
        Err(e) => {
            return Err(anyhow!(
                "Failed to subscribe to transform tree topic `{}`: {e}",
                args.tf_topic
            ))
        }
    };
    info!("Subscribed to transform tree topic.");

    // Subscribe to pointcloud
    info!(
        "Subscribing to point cloud topic `{}` ...",
        args.pointcloud_topic
    );
    let pointcloud_callback = move |_msg: messages::sensor_msgs::PointCloud2| {
        points_tx.send(PointCloudFrame {}).ok();
    };
    let _pointcloud_subscriber =
        match rosrust::subscribe(&args.pointcloud_topic, 100, pointcloud_callback) {
            Ok(s) => s,
            Err(e) => {
                return Err(anyhow!(
                    "Failed to subscribe to point cloud topic `{}`: {e}",
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

    rosrust::spin();
    Ok(())
}

fn parse_tf_message(msg: messages::tf2_msgs::TFMessage) -> impl Iterator<Item = Transform> {
    msg.transforms.into_iter().map(|t| Transform {
        frame: t.child_frame_id,
        parent_frame: t.header.frame_id,
        time_stamp: Duration::new(t.header.stamp.sec as u64, t.header.stamp.nsec),
        translation: vector![
            t.transform.translation.x,
            t.transform.translation.y,
            t.transform.translation.z
        ],
        rotation: Quaternion::new(
            t.transform.rotation.w,
            t.transform.rotation.x,
            t.transform.rotation.y,
            t.transform.rotation.z,
        ),
    })
}

mod messages {
    rosrust::rosmsg_include!(sensor_msgs / PointCloud2, tf2_msgs / TFMessage);
}
