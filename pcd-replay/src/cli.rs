use anyhow::anyhow;
use std::path::PathBuf;
use std::str::FromStr;

/// Observes a folder and sends all *.pcd (point cloud files as created by the pcl library) files
/// that are created to the point cloud server.
#[derive(clap::Parser, Debug, Clone)]
pub struct Arguments {
    #[clap(long, default_value = "info", possible_values = &["trace", "debug", "info", "warn", "error"])]
    pub log_level: log::Level,

    /// Host name for the point cloud server.
    #[clap(long, short, default_value = "::1")]
    pub host: String,

    /// Port for the point cloud server.
    #[clap(long, short, default_value = "4567")]
    pub port: u16,

    /// The offset moves each point, such that (offset-x, offset-y, offset-z) becomes the origin.
    #[clap(long, short = 'x', default_value = "0.0")]
    pub offset_x: f64,

    /// See offset-x.
    #[clap(long, short = 'y', default_value = "0.0")]
    pub offset_y: f64,

    /// See offset-x.
    #[clap(long, short = 'z', default_value = "0.0")]
    pub offset_z: f64,

    /// read rgb point colors
    #[clap(long)]
    pub color: bool,

    /// read intensities
    #[clap(long)]
    pub intensity: bool,

    /// Mode of operation.
    ///
    /// live: Observes the input folder using inotify and
    ///       sends all newly created *.pcd files to the LiDAR Server.
    ///
    /// replay: Lists all existing *.pcd files in a folder and sends them to the LiDAR Server.
    ///         The files should be named by the unix timestamp in microseconds when they were
    ///         captured (for example: '1651562862919501.pcd'). The files will be sent to the
    ///         server in the order and timing as indicated by the timestamps.
    #[clap(long, possible_values = &["live", "replay"], default_value = "replay")]
    pub mode: Mode,

    /// Path to the folder in which to look for *.pcd files
    #[clap()]
    pub input_folder: PathBuf,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Mode {
    /// Observes the input folder ising inotify and sends all newly created *.pcd files to the
    /// LiDAR Server.
    Live,

    /// Lists all existing *.pcd files in a folder and sends them to the LiDAR Server.
    ///
    /// The files should be named by the unix timestamp in milliseconds when they were
    /// captured (for example: '1651562862919501.pcd').
    /// The files will be sent to the server in the order and timing as indicated by the timestamp.
    Replay,
}

impl FromStr for Mode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "live" => Ok(Mode::Live),
            "replay" => Ok(Mode::Replay),
            _ => Err(anyhow!("Invalid value. Must be either 'Live' or 'Replay'.")),
        }
    }
}
