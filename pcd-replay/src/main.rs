mod cli;
mod pcd_reader;

use crate::cli::{Arguments, Mode};
use crate::pcd_reader::read_pcd_file;
use anyhow::anyhow;
use anyhow::Result;
use clap::Parser;
use crossbeam_channel::{Receiver, Sender};
use inotify::{Inotify, WatchMask};
use lidarserv_server::common::geometry::points::PointType;
use lidarserv_server::common::geometry::position::F64Position;
use log::{error, info, trace, warn};
use std::collections::{BTreeMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{cmp, mem, thread};

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

fn inotify_mode(
    path: PathBuf,
    abort: Arc<AtomicBool>,
    sender: Sender<PathBuf>,
) -> Result<(), anyhow::Error> {
    let mut buffer = [0; 1024];
    let mut notify = Inotify::init()?;
    notify.add_watch(&path, WatchMask::CLOSE)?;

    let mut dedup: [HashSet<OsString>; 2] = [HashSet::new(), HashSet::with_capacity(1000)];

    loop {
        // get changed files
        let r = notify.read_events_blocking(&mut buffer);

        // check, if we should abort
        // and do so with the correct error (if any)
        let events = if abort.load(Acquire) {
            return match r {
                Ok(_) => Ok(()),
                Err(e) => {
                    if e.kind() == ErrorKind::Interrupted {
                        Ok(())
                    } else {
                        Err(e.into())
                    }
                }
            };
        } else {
            match r {
                Ok(ev) => ev,
                Err(e) => break Err(e.into()),
            }
        };

        // send events to network thread
        for event in events {
            trace!("FS event: {:?}", event);
            if let Some(name) = event.name {
                if Path::new(name).extension() == Some(OsStr::new("pcd")) {
                    if !dedup[0].contains(name) && !dedup[1].contains(name) {
                        dedup[1].insert(name.to_os_string());
                        if dedup[1].len() > 999 {
                            dedup[0] = mem::replace(&mut dedup[1], HashSet::with_capacity(1000));
                        }
                        let mut full_path = path.clone();
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
}

fn replay_mode(
    base_path: PathBuf,
    abort: Arc<AtomicBool>,
    sender: Sender<PathBuf>,
) -> Result<(), anyhow::Error> {
    // scan for files
    let mut pcd_files = BTreeMap::new();
    for file in base_path.read_dir()? {
        let path = file?.path();
        if path.extension() != Some(OsStr::new("pcd")) {
            trace!(
                "Ignoring file, because it is not a *.pcd file: {}",
                path.to_string_lossy()
            );
            continue;
        }
        if let Some(file_name) = path.file_name().and_then(OsStr::to_str) {
            // remove file extension
            let time_stamp_str = &file_name[..file_name.len() - ".pcd".len()];

            // convert to number
            let time_stamp = match u128::from_str(time_stamp_str) {
                Ok(v) => v,
                Err(_) => continue,
            };
            pcd_files.insert(time_stamp, path);
        }
    }

    // log some info
    if pcd_files.is_empty() {
        error!(
            "No input files were found in the folder '{}'. Make sure, that the files are named based on the pattern 'timesstamp.pcd' (timestamp is in µs (microseconds))",
            base_path.to_string_lossy()
        );
        return Ok(());
    } else {
        info!("Found {} input files.", pcd_files.len())
    }

    // replay!
    let time_start = Instant::now();
    let timestamp_base = *pcd_files.keys().min().unwrap();
    let mut next_timestamp = timestamp_base;
    let mut last_message = Instant::now();
    loop {
        let now = Instant::now();
        let replay_until = now.duration_since(time_start).as_micros() + timestamp_base;
        for (_, path) in pcd_files.range(next_timestamp..replay_until) {
            sender.send(path.clone()).ok();
        }
        next_timestamp = replay_until;
        match pcd_files.range(next_timestamp..).min() {
            None => break,
            Some((ts, _)) => {
                let target_time = time_start + Duration::from_micros((ts - timestamp_base) as u64);
                let current_time = Instant::now();
                if current_time < target_time {
                    interruptable_sleep(target_time - current_time);
                } else {
                    let behind_seconds = (current_time - target_time).as_secs_f64() / 1_000_000.0;
                    if behind_seconds > 1.0
                        && current_time.duration_since(last_message) > Duration::from_secs(1)
                    {
                        warn!(
                            "Cannot send files fast enough. Currently behind by {} seconds. (Going as fast as I can to catch up)",
                            behind_seconds
                        );
                        last_message = current_time;
                    }
                }
                if abort.load(Acquire) {
                    return Ok(());
                }
            }
        };
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

    let inotify_thread_result = match args.mode {
        Mode::Live => inotify_mode(args.input_folder, abort, sender),
        Mode::Replay => replay_mode(args.input_folder, abort, sender),
    };

    aborted.store(true, Release);
    network_thread.join().unwrap()?;
    inotify_thread_result
}

/// taken from the std rust lib (std::thread::sleep) and modified to return in case of an interrupt
/// Returns true, if the sleep was able to complete the given duration, false if it was interrupted.
pub fn interruptable_sleep(dur: Duration) -> bool {
    let mut secs = dur.as_secs();
    let mut nsecs = dur.subsec_nanos() as _;
    unsafe {
        while secs > 0 || nsecs > 0 {
            let mut ts = libc::timespec {
                tv_sec: cmp::min(libc::time_t::MAX as u64, secs) as libc::time_t,
                tv_nsec: nsecs,
            };
            secs -= ts.tv_sec as u64;
            let ts_ptr = &mut ts as *mut _;
            if libc::nanosleep(ts_ptr, ts_ptr) == -1 {
                assert_eq!((*libc::__errno_location()), libc::EINTR);
                return false;
            } else {
                nsecs = 0;
            }
        }
    }
    true
}
