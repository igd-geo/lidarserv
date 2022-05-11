mod cli;
mod pcd_reader;

use crate::cli::Arguments;
use crate::pcd_reader::read_pcd_file;
use anyhow::anyhow;
use anyhow::Result;
use clap::Parser;
use crossbeam_channel::{Receiver, Sender};
use inotify::{Events, Inotify, WatchMask};
use lidarserv_server::common::geometry::points::PointType;
use lidarserv_server::common::geometry::position::F64Position;
use log::{error, info, trace};
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{mem, thread};

fn main() {
    let args = Arguments::parse();
    simple_logger::init_with_level(args.log_level).ok();
    match main_result(args) {
        Ok(_) => {}
        Err(e) => {
            error!("{:?}", e)
        }
    }
}

#[tokio::main]
async fn network_thread(args: Arguments, files_receiver: Receiver<PathBuf>) -> Result<()> {
    let (_shutdown_sender, mut shutdown_receiver) = tokio::sync::broadcast::channel(1);
    let mut connection =
        lidarserv_server::net::client::capture_device::CaptureDeviceClient::connect(
            (args.host.as_str(), args.port),
            &mut shutdown_receiver,
            false,
        )
        .await?;

    let mut last_info = Instant::now();
    let mut info_cnt_points = 0_u64;
    let mut info_cnt_files = 0_u64;
    let origin = (args.offset_x, args.offset_y, args.offset_z);

    for filename in files_receiver {
        // read
        trace!("Reading file: {:?}", &filename);
        let points = read_pcd_file(filename.as_os_str(), origin, args.color, args.intensity)?;
        let nr_points = points.len();
        trace!("Read {} points from {:?}.", nr_points, &filename);

        // send to server
        connection.insert_points(points).await?;

        // log some info
        info_cnt_files += 1;
        info_cnt_points += nr_points as u64;
        let now = Instant::now();
        let duration_since_last_info = now.duration_since(last_info);
        if duration_since_last_info > Duration::from_secs(1) {
            info!(
                "Read {} points from {} files in {} seconds.",
                info_cnt_points,
                info_cnt_files,
                duration_since_last_info.as_secs_f64()
            );
            info_cnt_points = 0;
            info_cnt_files = 0;
            last_info = now;
        }
    }

    Ok(())
}

fn main_result(args: Arguments) -> Result<(), anyhow::Error> {
    // Install a signal handler for SIGUSR1
    // The signal handler itself does nothing, but with a signal handler in place,
    // the blocking read() call from inotify will be interrupted by SIGUSR1.
    // We use that to abort the file watcher when there is an error in another thread.
    unsafe {
        extern "C" fn nop_handler(s: i32) {
            trace!("Captured signal {}.", s)
        }
        let mut act = libc::sigaction {
            sa_sigaction: nop_handler as libc::sighandler_t,
            sa_mask: mem::zeroed(),
            sa_flags: 0,
            sa_restorer: None,
        };
        if libc::sigfillset(&mut act.sa_mask) == 0 {
            if libc::sigaction(libc::SIGUSR1, &act, null_mut()) != 0 {
                return Err(anyhow!(
                    "Failed setting signal handler (sigaction). Errno is {}",
                    *libc::__errno_location()
                ));
            }
        } else {
            return Err(anyhow!("Failed setting signal handler (sigfillset)."));
        }
    }

    let abort = Arc::new(AtomicBool::new(false));
    let aborted = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = crossbeam_channel::bounded(1024);
    let network_thread = {
        let args = args.clone();
        let abort = Arc::clone(&abort);
        let aborted = Arc::clone(&aborted);
        thread::spawn(move || {
            let result: Result<()> = network_thread(args, receiver);
            unsafe {
                abort.store(true, Release);
                while !aborted.load(Acquire) {
                    let pid = libc::getpid();
                    libc::kill(pid, libc::SIGUSR1);
                    thread::sleep(Duration::from_millis(200));
                }
            }
            result
        })
    };

    let path = args.base_path;
    let mut buffer = [0; 1024];
    let mut notify = Inotify::init()?;
    notify.add_watch(&path, WatchMask::CLOSE)?;

    let mut dedup: [HashSet<OsString>; 2] = [HashSet::new(), HashSet::with_capacity(1000)];

    let inotify_thread_result = loop {
        // get changed files
        let r = notify.read_events_blocking(&mut buffer);

        // check, if we should abort
        // and do so with the correct error (if any)
        let events = if abort.load(Acquire) {
            match r {
                Ok(_) => break Ok(()),
                Err(e) => {
                    if e.kind() == ErrorKind::Interrupted {
                        break Ok(());
                    } else {
                        break Err(e.into());
                    }
                }
            }
        } else {
            match r {
                Ok(ev) => ev,
                Err(e) => break Err(e.into()),
            }
        };

        // send events to network thread
        process_events(events, &mut dedup, &path, &sender);
    };

    aborted.store(true, Release);
    drop(sender);
    network_thread.join().unwrap()?;
    inotify_thread_result
}

fn process_events(
    events: Events,
    dedup: &mut [HashSet<OsString>; 2],
    base_path: &Path,
    sender: &Sender<PathBuf>,
) {
    for event in events {
        trace!("FS event: {:?}", event);
        if let Some(name) = event.name {
            if Path::new(name).extension() == Some(OsStr::new("pcd")) {
                if !dedup[0].contains(name) && !dedup[1].contains(name) {
                    dedup[1].insert(name.to_os_string());
                    if dedup[1].len() > 999 {
                        dedup[0] = mem::replace(&mut dedup[1], HashSet::with_capacity(1000));
                    }
                    let mut full_path = base_path.to_path_buf();
                    full_path.push(name);
                    sender.send(full_path).ok();
                    trace!("File {:?}: Triggered read", name);
                } else {
                    trace!("File {:?}: Dedup, already known", name);
                }
            } else {
                trace!("File: {:?} Skip, not a *.pcd file", name);
            }
        }
    }
}
