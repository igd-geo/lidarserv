mod pcd_reader;

use crate::pcd_reader::{read_pcd_file, PclHeader};
use anyhow::{anyhow, Result};
use lidarserv_server::common::geometry::points::PointType;
use lidarserv_server::common::geometry::position::F64Position;
use lidarserv_server::index::point::GlobalPoint;
use std::ffi::OsString;
use std::fs::File;
use std::io::SeekFrom::{Current, Start};
use std::io::{BufRead, BufReader, ErrorKind, Read, Seek};
use std::str::FromStr;
use std::time::Instant;

fn main() {
    println!("Hello, world!");
    let t_start = Instant::now();
    let body = read_pcd_file(&OsString::from("./data/point-cloud.pcd")).unwrap();
    let duration = Instant::now().duration_since(t_start);

    println!(
        "Read {} points in {}ms",
        body.len(),
        duration.as_secs_f64() * 1000.0
    );
}
