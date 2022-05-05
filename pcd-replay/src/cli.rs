use std::path::PathBuf;

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

    /// Path to the folder in which to look for *.pcd files
    #[clap()]
    pub base_path: PathBuf,
}
