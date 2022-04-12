use std::fs::File;
use std::io::{Read, Write};
use std::thread::{sleep, Thread};
use std::time::{Duration, Instant};

fn main() {
    let mut file = File::open("data/point-cloud.pcd").unwrap();
    let mut dest = File::create("data/point-cloud-2.pcd").unwrap();

    let chunk_size = 128;
    let bytes_per_sec = 1024;

    let mut buf = vec![0_u8; chunk_size];
    let mut last = Instant::now();
    loop {
        // read
        let bytes_read = file.read(&mut buf).unwrap();
        if bytes_read == 0 {
            // EOF
            break;
        }

        // sleep
        let wait_for = Duration::from_secs_f64(bytes_read as f64 / bytes_per_sec as f64);
        let wait_until = last + wait_for;
        let now = Instant::now();
        if now < wait_until {
            sleep(wait_until - now);
            last = wait_until;
        } else {
            last = now;
        }

        // write
        dest.write_all(&buf[..bytes_read]).unwrap();
    }
}
