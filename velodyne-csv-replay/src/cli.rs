use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct Args {
    /// Verbosity of the command line output.
    #[structopt(long, default_value="info", possible_values = &["trace", "debug", "info", "warn", "error"])]
    pub log_level: log::Level,

    /// speeds up or slows down the reader by the given factor
    #[structopt(long, default_value = "1.0")]
    pub speed_factor: f64,

    /// The offset moves each point, such that (offset-x, offset-y, offset-z) becomes the origin.
    #[structopt(long, short = "x", default_value = "0.0")]
    pub offset_x: f64,

    /// See offset-x.
    #[structopt(long, short = "y", default_value = "0.0")]
    pub offset_y: f64,

    /// See offset-x.
    #[structopt(long, short = "z", default_value = "0.0")]
    pub offset_z: f64,

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
