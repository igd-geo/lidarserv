use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct Args {
    /// Verbosity of the command line output.
    #[structopt(long, default_value="info", possible_values = &["trace", "debug", "info", "warn", "error"])]
    pub log_level: log::Level,

    /// File with the sensor trajectory
    #[structopt(long)]
    pub trajectory_file: String,

    /// File with the point data
    #[structopt(long)]
    pub points_file: String,

    /// Disables laz compression of point data
    #[structopt(long)]
    pub no_compression: bool,

    /// Frames per second at which to send point data.
    ///
    /// Note: A higher fps will NOT send more points per second.
    /// It will just smaller packages of points being sent more frequently.
    #[structopt(long, default_value = "20")]
    pub fps: u32,

    #[structopt(long, short, default_value = "::1")]
    pub host: String,

    #[structopt(long, short, default_value = "4567")]
    pub port: u16,
}
