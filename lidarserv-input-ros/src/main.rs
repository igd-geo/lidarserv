use anyhow::{Context, Result};
use clap::Parser;
use cli::AppOptions;
use human_panic::setup_panic;
use lidarserv::lidarserv_thread;
use log::{debug, error, info};
use processing::processing_thread;
use ros::{ros_thread, Command};
use rosrust::api::resolve::get_unused_args;
use std::{
    fmt::{Debug, Display},
    process::ExitCode,
    sync::mpsc::{self, channel},
    thread,
};

mod cli;
mod lidarserv;
mod processing;
mod ros;
mod transform_tree;

fn main() -> ExitCode {
    setup_panic!();

    // arg parsing
    let args = AppOptions::parse_from(get_unused_args());

    // logger
    simple_logger::init_with_level(args.log_level).unwrap();

    // run
    let result = run(args);
    if let Err(e) = result {
        error!("{e}");
        debug!("{e:?}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run(args: AppOptions) -> Result<()> {
    // Install signal handler.
    let (exit_tx, exit_rx) = channel();
    {
        let exit_tx = exit_tx.clone();
        ctrlc::set_handler(move || {
            exit_tx.send(()).ok();
        })
        .context("Failed to install Ctrl+C signal handler.")?;
        info!("Press Ctrl+C to exit.");
    }

    // ros thread
    let (commands_tx, commands_rx) = mpsc::channel();
    let (transforms_tx, transforms_rx) = mpsc::channel();
    let (ros_tx, ros_rx) = mpsc::sync_channel(10);
    let join_ros = {
        let exit_tx = exit_tx.clone();
        let args = args.clone();
        thread::spawn(move || {
            ros_thread(args, commands_rx, transforms_tx, ros_tx).log_error();
            exit_tx.send(()).ok();
        })
    };

    // lidarserv thread
    let (infor_tx, info_rx) = mpsc::channel();
    let (stop_lidarserv_tx, stop_lidarserv_rx) = tokio::sync::broadcast::channel(1);
    let (points_tx, points_rx) = tokio::sync::mpsc::channel(10);
    let join_lidarserv = {
        let exit_tx = exit_tx.clone();
        let args = args.clone();
        thread::spawn(move || {
            lidarserv_thread(args, infor_tx, stop_lidarserv_rx, points_rx).log_error();
            exit_tx.send(()).ok();
        })
    };

    // processing thread
    let join_processing = {
        let args = args.clone();
        let exit_tx = exit_tx.clone();
        thread::spawn(move || {
            processing_thread(args, info_rx, ros_rx, transforms_rx, points_tx).log_error();
            exit_tx.send(()).ok();
        })
    };

    // wait for exit (user pressed ctrl+c, or one of the thread terminated unexpectedly)
    exit_rx.recv().unwrap();
    info!("Shutting down...");

    // stop ROS thread
    commands_tx.send(Command::Exit).ok();
    join_ros.join().unwrap();

    // stop processing thread
    join_processing.join().unwrap();

    // stop lidarserv thread
    stop_lidarserv_tx.send(()).ok();
    join_lidarserv.join().unwrap();

    // Be polite.
    info!("Bye. 👋");

    // We are done.
    Ok(())
}

trait LogErrors {
    type Ok;
    fn log_error(self) -> Option<Self::Ok>;
}

impl<T, E> LogErrors for Result<T, E>
where
    E: Display + Debug,
{
    type Ok = T;

    fn log_error(self) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                error!("{e}");
                debug!("{e:?}");
                None
            }
        }
    }
}
