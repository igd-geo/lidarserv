use anyhow::Result;
use clap::Parser;
use cli::AppOptions;
use human_panic::setup_panic;
use log::{debug, error, info};
use ros::{ros_thread, Command};
use rosrust::api::resolve::get_unused_args;
use std::{
    process::ExitCode,
    sync::mpsc::{self, channel},
    thread,
};

mod cli;
mod ros;

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
    let (commands_tx, commands_rx) = mpsc::channel();
    let (transforms_tx, _transforms_rx) = mpsc::channel();
    let (points_tx, _points_rx) = mpsc::channel();
    let join_ros = thread::spawn(move || ros_thread(args, commands_rx, transforms_tx, points_tx));

    // Install signal handler.
    let (ctrlc_tx, ctrlc_rx) = channel();
    ctrlc::set_handler(move || {
        ctrlc_tx.send(()).unwrap();
    })
    .unwrap();

    // wait for exit
    info!("Press Ctrl+C to exit.");
    ctrlc_rx.recv().unwrap();

    // stop ROS thread
    commands_tx.send(Command::Exit).ok();
    join_ros.join().unwrap()?;

    // Exit
    info!("Bye. 👋");
    Ok(())
}
