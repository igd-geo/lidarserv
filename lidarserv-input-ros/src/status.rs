use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError},
        Arc,
    },
    thread,
    time::Duration,
};

use console::{style, Key};
use log::info;

/// Holds status information, that is printed regularily.
#[derive(Debug, Default)]
pub struct Status {
    pub paused: AtomicBool,
    pub shutdown: AtomicBool,
    pub nr_rx_msg_pointcloud: AtomicU64,
    pub nr_rx_msg_tf: AtomicU64,
    pub nr_rx_points: AtomicU64,
    pub nr_process_in: AtomicU64,
    pub nr_process_out: AtomicU64,
    pub nr_tx_msg: AtomicU64,
}

pub fn status_thread(status: Arc<Status>, shutdown_rx: mpsc::Receiver<()>) {
    let mut buffer1: i64 = 0; // signed integers, because we use relaxed ordering for the atomic counters, so we could observe the increment of the counter that removes messages from the buffer before the one that inserts messages into the buffer.
    let mut buffer2: i64 = 0;
    let mut all_stopped_prev = false;

    {
        let status = Arc::clone(&status);
        thread::spawn(move || control_thread(status));
    }

    while let Err(RecvTimeoutError::Timeout) = shutdown_rx.recv_timeout(Duration::from_secs(1)) {
        let rx_msg = status.nr_rx_msg_pointcloud.swap(0, Ordering::Relaxed);
        let rx_tf = status.nr_rx_msg_tf.swap(0, Ordering::Relaxed);
        let rx_pts = status.nr_rx_points.swap(0, Ordering::Relaxed);
        let nr_process_in = status.nr_process_in.swap(0, Ordering::Relaxed);
        let nr_process_out = status.nr_process_out.swap(0, Ordering::Relaxed);
        let nr_tx_msg = status.nr_tx_msg.swap(0, Ordering::Relaxed);
        let paused = status.paused.load(Ordering::Relaxed);
        let shutdown = status.shutdown.load(Ordering::Relaxed);
        buffer1 += rx_msg as i64;
        buffer1 -= nr_process_in as i64;
        buffer2 += nr_process_out as i64;
        buffer2 -= nr_tx_msg as i64;

        let state_part = if shutdown {
            "[⏹]"
        } else if paused {
            "[⏸︎]"
        } else {
            "[⏵]"
        };

        let mut all_stopped = paused || shutdown;
        let stop_reason = if shutdown {
            "shut down"
        } else if paused {
            "paused"
        } else {
            ""
        };
        let rx_part = if all_stopped && rx_msg == 0 {
            stop_reason.to_string()
        } else {
            all_stopped = false;
            format!(
                "{:3} msg/s {:6} pts/s | tf: {:3} msg/s",
                rx_msg, rx_pts, rx_tf,
            )
        };
        let process_part = if all_stopped && buffer1 == 0 && nr_process_out == 0 {
            stop_reason.to_string()
        } else {
            all_stopped = false;
            format!("queue: {:2} msg | {:3} msg/s", buffer1, nr_process_out,)
        };
        let tx_part = if all_stopped && buffer2 == 0 && nr_tx_msg == 0 {
            stop_reason.to_string()
        } else {
            all_stopped = false;
            format!("queue: {:2} msg | {:3} msg/s", buffer2, nr_tx_msg)
        };
        if !all_stopped || !all_stopped_prev {
            println!(
                "{}[{} {}] [{} {}] [{} {}]",
                state_part,
                style("RX").bold(),
                rx_part,
                style("PROCESS").bold(),
                process_part,
                style("TX").bold(),
                tx_part
            );
        }
        if all_stopped && shutdown {
            break;
        }
        all_stopped_prev = all_stopped;
    }
}

pub fn control_thread(status: Arc<Status>) {
    let term = console::Term::stdout();
    if !term.features().is_attended() {
        return;
    }
    info!("Press space to pause / unpause.");

    loop {
        match term.read_key() {
            Ok(Key::Char(' ')) => {
                let paused = !status.paused.fetch_not(Ordering::Relaxed);
                if paused {
                    term.write_line("[⏸︎] PAUSE").unwrap();
                    term.move_cursor_up(1).ok();
                } else {
                    term.write_line("[⏵] RESUME").unwrap();
                    term.move_cursor_up(1).ok();
                }
            }
            Ok(Key::Unknown) => return,
            Ok(_) => (),
            Err(_) => return,
        }
    }
}
