use crate::cli::Args;
use anyhow::Result;
use lidarserv_server::common::geometry::bounding_box::{BaseAABB, AABB};
use lidarserv_server::common::geometry::grid::LodLevel;
use lidarserv_server::common::nalgebra::Point3;
use lidarserv_server::net::client::viewer::ViewerClient;
use log::{debug, error};
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
    let (sender, mut receiver) = tokio::sync::broadcast::channel(1);
    let net = thread::spawn(move || network_thread(args, &mut receiver));

    net.join().unwrap()
}

#[tokio::main]
async fn network_thread(args: Args, shutdown: &mut Receiver<()>) -> Result<()> {
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
    client.query_aabb(&aabb, &LodLevel::base()).await?;

    // keep receiving updates
    loop {
        let update = client.receive_update(shutdown).await?;
        debug!("{:?}", update)
    }
}
