use anyhow::{Context, Result};
use clap::Parser;
use cli::AppOptions;
use human_panic::setup_panic;
use lidarserv::lidarserv_thread;
use log::{debug, error, info};
use processing::processing_thread;
use ros::{ros_thread, Command};
use rosrust::api::resolve::get_unused_args;
use status::{status_thread, Status};
use std::{
    fmt::{Debug, Display},
    process::ExitCode,
    sync::{
        atomic::Ordering,
        mpsc::{self, channel},
        Arc,
    },
    thread,
};

mod cli;
mod lidarserv;
mod processing;
mod ros;
mod status;
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
    let (stop_lidarserv_tx, stop_lidarserv_rx) = tokio::sync::broadcast::channel(1);
    let (stop_status_tx, stop_status_rx) = mpsc::channel();
    let (stop_process_tx, stop_process_rx) = mpsc::channel();
    let (exit_tx, exit_rx) = channel();
    {
        let exit_tx = exit_tx.clone();
        let mut first_ctrlc = true;
        ctrlc::set_handler(move || {
            if first_ctrlc {
                exit_tx.send(()).ok();
                first_ctrlc = false;
            } else {
                stop_lidarserv_tx.send(()).ok();
                stop_status_tx.send(()).ok();
                stop_process_tx.send(()).ok();
            }
        })
        .context("Failed to install Ctrl+C signal handler.")?;
        info!("Press Ctrl+C to exit.");
    }

    // ros thread
    let (commands_tx, commands_rx) = mpsc::channel();
    let (transforms_tx, transforms_rx) = mpsc::channel();
    let (ros_tx, ros_rx) = mpsc::channel();
    let status = Arc::new(Status::default());
    let join_ros = {
        let exit_tx = exit_tx.clone();
        let args = args.clone();
        let status = Arc::clone(&status);
        thread::spawn(move || {
            ros_thread(args, commands_rx, transforms_tx, ros_tx, status).log_error();
            exit_tx.send(()).ok();
        })
    };

    // lidarserv thread
    let (infor_tx, info_rx) = mpsc::channel();
    let (points_tx, points_rx) = tokio::sync::mpsc::unbounded_channel();
    let join_lidarserv = {
        let exit_tx = exit_tx.clone();
        let args = args.clone();
        let status = Arc::clone(&status);
        thread::spawn(move || {
            lidarserv_thread(args, infor_tx, stop_lidarserv_rx, points_rx, status).log_error();
            exit_tx.send(()).ok();
        })
    };

    // processing thread
    let join_processing = {
        let args = args.clone();
        let exit_tx = exit_tx.clone();
        let status = Arc::clone(&status);
        thread::spawn(move || {
            processing_thread(
                args,
                info_rx,
                ros_rx,
                transforms_rx,
                points_tx,
                stop_process_rx,
                status,
            )
            .log_error();
            exit_tx.send(()).ok();
        })
    };

    // status thread
    let join_status = {
        let status = Arc::clone(&status);
        thread::spawn(move || {
            status_thread(status, stop_status_rx);
        })
    };

    // wait for exit (user pressed ctrl+c, or one of the thread terminated unexpectedly)
    exit_rx.recv().unwrap();
    let term = console::Term::stdout();
    if !term.features().is_attended() {
        info!("Shutting down...");
    } else {
        term.write_line("[⏹] Shutting down...").unwrap();
        term.move_cursor_up(1).ok();
    }
    status.shutdown.store(true, Ordering::Relaxed);

    // stop ROS thread
    commands_tx.send(Command::Exit).ok();
    join_ros.join().unwrap();

    // stop processing thread
    join_processing.join().unwrap();

    // stop lidarserv thread
    join_lidarserv.join().unwrap();

    // stop status thread
    join_status.join().unwrap();

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
