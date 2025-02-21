use std::sync::mpsc;

use lidarserv_server::net::client::capture_device::CaptureDeviceClient;
use pasture_core::{containers::VectorBuffer, layout::PointAttributeDefinition};
use tokio::select;

use crate::cli::AppOptions;

pub struct LidarservPointCloudInfo {
    pub attributes: Vec<PointAttributeDefinition>,
    //pub coordinate_system: CoordinateSystem,
}

#[tokio::main]
pub async fn lidarserv_thread(
    args: AppOptions,
    info_tx: mpsc::Sender<LidarservPointCloudInfo>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    mut points_rx: tokio::sync::mpsc::Receiver<VectorBuffer>,
) -> Result<(), anyhow::Error> {
    // connect to lidarserv server
    let mut client =
        CaptureDeviceClient::connect((args.host.as_str(), args.port), &mut shutdown_rx).await?;

    // send point cloud info to processing thread
    info_tx
        .send(LidarservPointCloudInfo {
            attributes: client.attributes().to_vec(),
            //coordinate_system: client.coordinate_system(),
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

        // send
        // todo - do coordinate system transform and encoding on processing thread.
        client.insert_points_global_coordinates(&points).await?;
    }

    Ok(())
}
