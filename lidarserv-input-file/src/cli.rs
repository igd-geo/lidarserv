use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Tool to replay pre-captured point clouds to the lidar server.
#[derive(Debug, Parser)]
pub struct AppOptions {
    /// Verbosity of the command line output.
    #[clap(long, default_value = "info")]
    pub log_level: log::Level,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Replays points from a las file
    Replay(ReplayOptions),

    /// Sorts the points in a las file by their gps time.
    ///
    /// So that they can be replaied in the correct order.
    Sort(SortOptions),
}

#[derive(Debug, Parser)]
pub struct ReplayOptions {
    /// Hostname of the lidarserv server
    #[clap(long, default_value = "::0")]
    pub host: String,

    /// Port of the lidarserv server
    #[clap(long, default_value = "4567")]
    pub port: u16,

    /// Defines the interval at which new points are sent to the server
    #[clap(long, default_value = "20")]
    pub fps: u32,

    /// If set, the points are replayed at a fixed rate.
    /// Otherwise, the points will be replayed based on the gps time attribute.
    #[clap(long)]
    pub points_per_second: Option<u32>,

    /// If the input las file is a combination of multiple flights,
    /// this can be used to automatically skip the pauses in between
    /// two flights.
    /// Ignored if points_per_second is set.
    #[clap(long)]
    pub autoskip: bool,

    /// Speed factor to replay the point cloud faster or slower
    /// than the original speed.
    /// Ignored if points_per_second is set.
    #[clap(long, default_value = "1.0")]
    pub accelerate: f64,

    /// A las or laz file to replay the points from
    #[clap()]
    pub file: PathBuf,
}

#[derive(Debug, Parser)]
pub struct SortOptions {
    /// List of input las/laz files.
    #[clap(required = true)]
    pub input_file: Vec<PathBuf>,

    /// The name of the output file.
    #[clap()]
    pub output_file: PathBuf,
}
