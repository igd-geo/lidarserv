use lidarserv_common::geometry::coordinate_system::CoordinateSystem;
use lidarserv_server::net::{
    client::capture_device::CaptureDeviceClient, protocol::messages::PointDataCodec,
};
use pasture_core::layout::PointAttributeDefinition;
use std::sync::{atomic::Ordering, mpsc, Arc};
use tokio::select;

use crate::{cli::AppOptions, status::Status};

pub struct LidarservPointCloudInfo {
    pub attributes: Vec<PointAttributeDefinition>,
    pub coordinate_system: CoordinateSystem,
    pub codec: PointDataCodec,
}

#[tokio::main]
pub async fn lidarserv_thread(
    args: AppOptions,
    info_tx: mpsc::Sender<LidarservPointCloudInfo>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    mut points_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    status: Arc<Status>,
) -> Result<(), anyhow::Error> {
    // connect to lidarserv server
    let mut client =
        CaptureDeviceClient::connect((args.host.as_str(), args.port), &mut shutdown_rx).await?;

    // send point cloud info to processing thread
    info_tx
        .send(LidarservPointCloudInfo {
            attributes: client.attributes().to_vec(),
            coordinate_system: client.coordinate_system(),
            codec: client.codec(),
        })
        .ok();

    'send_loop: loop {
        // get points to send
        let points = select! {
            r = points_rx.recv() => match r {
                Some(p) => p,
                None => break 'send_loop,
            },
            _ = shutdown_rx.recv() => break 'send_loop,
        };

        // update status
        status.nr_tx_msg.fetch_add(1, Ordering::Relaxed);

        // send
        client.insert_raw_point_data(&points).await?;
    }

    Ok(())
}
