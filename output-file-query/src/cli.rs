use std::path::PathBuf;
use std::str::FromStr;
use structopt::StructOpt;


#[derive(StructOpt, Debug)]
pub struct Args {
    /// Verbosity of the command line output.
    #[structopt(long, default_value="info", possible_values = &["trace", "debug", "info", "warn", "error"])]
    pub log_level: log::Level,

    /// Host to bind to.
    #[structopt(long, short, default_value = "::1")]
    pub host: String,

    /// Port to bind to.
    #[structopt(long, short, default_value = "4567")]
    pub port: u16,

    /// Folder, that the las file will be stored in. Default is the current directory.
    #[structopt(long, default_value = "")]
    pub output_file: String,

    /// Level of detail of the point cloud.
    #[structopt(long, default_value = "0")]
    pub lod: u16,

    // AABB PARAMETER
    /// Minimum x value of the bounding box.
    #[structopt(long, default_value = "0")]
    pub min_x: f64,

    /// Minimum y value of the bounding box.
    #[structopt(long, default_value = "0")]
    pub min_y: f64,

    /// Minimum z value of the bounding box.
    #[structopt(long, default_value = "0")]
    pub min_z: f64,

    /// Maximum x value of the bounding box.
    #[structopt(long, default_value = "0")]
    pub max_x: f64,

    /// Maximum y value of the bounding box.
    #[structopt(long, default_value = "0")]
    pub max_y: f64,

    /// Maximum z value of the bounding box.
    #[structopt(long, default_value = "0")]
    pub max_z: f64,


}