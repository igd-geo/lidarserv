use std::{
    panic::resume_unwind,
    sync::{
        Arc,
        atomic::Ordering,
        mpsc::{self, TryRecvError},
    },
    time::Duration,
};

use log::warn;
use nalgebra::{Quaternion, UnitQuaternion, vector};
use r2r::{QosProfile, sensor_msgs::msg::PointCloud2, tf2_msgs::msg::TFMessage};
use tokio::{spawn, task::spawn_blocking};

use crate::{
    cli::AppOptions,
    ros::{Command, Endianess, Field, PointCloudMessage, Transform, Type},
    status::Status,
};
use futures::StreamExt;

#[tokio::main]
pub async fn ros_thread(
    args: AppOptions,
    commands_rx: mpsc::Receiver<Command>,
    transforms_tx: mpsc::Sender<Transform>,
    points_tx: mpsc::Sender<PointCloudMessage>,
    status: Arc<Status>,
) -> anyhow::Result<()> {
    let ctx = r2r::Context::create()?;
    let mut node = r2r::Node::create(ctx, "lidarserv", "")?;

    // handle tf messages
    let status1 = Arc::clone(&status);
    let status2 = Arc::clone(&status);
    let tf_topic = node.subscribe::<TFMessage>(&args.tf_topic, QosProfile::default())?;
    let tf_static_topic = node.subscribe::<TFMessage>(&args.tf_topic, QosProfile::default())?;
    {
        let transforms_tx = transforms_tx.clone();
        spawn(async move {
            tf_topic
                .for_each(async |msg| {
                    status1.nr_rx_msg_tf.fetch_add(1, Ordering::Relaxed);
                    for tf in parse_tf_message(msg, false) {
                        transforms_tx.send(tf).ok();
                    }
                })
                .await;
        });
    }
    spawn(async move {
        tf_static_topic
            .for_each(async |msg| {
                status2.nr_rx_msg_tf.fetch_add(1, Ordering::Relaxed);
                for tf in parse_tf_message(msg, true) {
                    transforms_tx.send(tf).ok();
                }
            })
            .await;
    });

    // handle pint cloud messages
    let point_cloud_topic =
        node.subscribe::<PointCloud2>(&args.pointcloud_topic, QosProfile::default())?;
    spawn(async move {
        point_cloud_topic
            .for_each(async |msg| {
                let paused = status.paused.load(Ordering::Relaxed);
                if !paused {
                    status.nr_rx_msg_pointcloud.fetch_add(1, Ordering::Relaxed);
                    status
                        .nr_rx_points
                        .fetch_add(msg.width as u64 * msg.height as u64, Ordering::Relaxed);
                    points_tx.send(parse_pointcloud_message(msg)).ok();
                }
            })
            .await;
    });

    let r = spawn_blocking(move || {
        loop {
            node.spin_once(Duration::from_secs_f64(0.1));

            // check for exit command every 0.1 seconds.
            // not ideal, but works for now.
            match commands_rx.try_recv() {
                Ok(Command::Exit) => break,
                Err(TryRecvError::Disconnected) => break,
                Err(TryRecvError::Empty) => (),
            }
        }
    })
    .await;
    if let Err(e) = r {
        if e.is_panic() {
            resume_unwind(e.into_panic());
        } else {
            return Err(e.into());
        }
    }

    Ok(())
}

fn parse_pointcloud_message(msg: r2r::sensor_msgs::msg::PointCloud2) -> PointCloudMessage {
    PointCloudMessage {
        frame: msg.header.frame_id,
        time_stamp: Duration::new(msg.header.stamp.sec as u64, msg.header.stamp.nanosec),
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

fn parse_tf_message(
    msg: r2r::tf2_msgs::msg::TFMessage,
    is_static: bool,
) -> impl Iterator<Item = Transform> {
    msg.transforms.into_iter().map(move |t| Transform {
        frame: t.child_frame_id,
        parent_frame: t.header.frame_id,
        is_static,
        time_stamp: Duration::new(t.header.stamp.sec as u64, t.header.stamp.nanosec),
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
