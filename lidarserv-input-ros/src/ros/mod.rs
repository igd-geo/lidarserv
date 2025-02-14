use anyhow::Result;
use nalgebra::{Quaternion, Vector3};
use std::{sync::mpsc, time::Duration};

use crate::cli::AppOptions;

mod ros1;

pub enum Command {
    Exit,
}

pub struct Transform {
    pub frame: String,
    pub parent_frame: String,
    pub time_stamp: Duration,
    pub translation: Vector3<f64>,
    pub rotation: Quaternion<f64>,
}

pub struct PointCloudFrame {
    // todo
}

pub fn ros_thread(
    args: AppOptions,
    commands_rx: mpsc::Receiver<Command>,
    transforms_tx: mpsc::Sender<Transform>,
    points_tx: mpsc::Sender<PointCloudFrame>,
) -> Result<()> {
    // this could call either ros1 or ros2 in the future
    // (once we add support for ros2)
    ros1::ros_thread(args, commands_rx, transforms_tx, points_tx)
}
