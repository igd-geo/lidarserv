use crate::cli::ReplayPreconvertedArgs;
use anyhow::Result;
use lidarserv_server::common::las::async_split_compressed_las;
use lidarserv_server::net::client::capture_device::CaptureDeviceClient;
use log::{info, warn};
use std::fs::File;
use std::io::BufReader;
use std::thread;
use std::time::{Duration, Instant};

pub async fn replay_laz(args: ReplayPreconvertedArgs) -> Result<()> {
    // file read thread
    let (data_sender, data_receiver) = crossbeam_channel::bounded(args.fps as usize * 5); // allow to buffer max 5 seconds of point data
    let file_thread = {
        let args = args.clone();
        thread::spawn(move || read_file(&args, data_sender))
    };

    // network thread
    send_data(&args, data_receiver).await?;

    // wait for file read thread to complete
    file_thread.join().unwrap()?;

    Ok(())
}

fn read_file(
    args: &ReplayPreconvertedArgs,
    sender: crossbeam_channel::Sender<Vec<u8>>,
) -> Result<()> {
    let file = File::open(&args.input_file)?;
    let file = BufReader::new(file);
    async_split_compressed_las(sender, file)?;
    Ok(())
}

async fn send_data(
    args: &ReplayPreconvertedArgs,
    receiver: crossbeam_channel::Receiver<Vec<u8>>,
) -> Result<()> {
    // connect
    let (_sender, mut shutdown) = tokio::sync::broadcast::channel(1);
    let mut client =
        CaptureDeviceClient::connect((args.host.as_str(), args.port), &mut shutdown, true).await?;

    // replay frames
    let start_time = Instant::now();
    let mut next_message = start_time;
    for (frame_number, frame_data) in receiver.iter().enumerate() {
        // wait until the current frame is due
        let frame_time =
            start_time + Duration::from_secs_f64(frame_number as f64 / args.fps as f64);
        let now = Instant::now();
        if frame_time > now {
            let wait_for = frame_time - now;
            tokio::time::sleep(wait_for).await;
        }

        // log some info about the timing + progress
        if next_message < now {
            if frame_time + Duration::from_secs(1) < now {
                warn!(
                "Cannot send point data fast enough - we have fallen  behind by {} seconds. I will keep sending point data as fast as I can.", 
                (now - frame_time).as_secs_f64()
            );
            }
            info!(
                "Sent {} frames in {} seconds.",
                frame_number,
                (now - start_time).as_secs_f64()
            );
            next_message = now + Duration::from_secs(1);
        }

        // send point data
        client.insert_raw_point_data(frame_data).await?;
    }

    Ok(())
}
