use structopt::StructOpt;

#[derive(StructOpt, Debug, Clone)]
pub struct Args {
    /// Verbosity of the command line output.
    #[structopt(long, default_value="info", possible_values = &["trace", "debug", "info", "warn", "error"])]
    pub log_level: log::Level,

    #[structopt(subcommand)]
    pub command: Command,
}

#[derive(StructOpt, Debug, Clone)]
pub enum Command {
    /// Reads the csv files with point and trajectory data and converts them to a laz file, that can
    /// be used with the replay command.
    Convert(PreConvertArgs),

    /// Replays the given laz file.
    /// Each frame sent to the server at the given frame rate (fps) contains exactly one chunk of
    /// compressed point data from the input file.
    Replay(ReplayPreconvertedArgs),

    /// Replays the point data directly from the csv files containing the point and trajectory information.
    /// Calculation of point positions and encoding of LAZ data is done on-the-fly.
    LiveReplay(LiveReplayArgs),
}

#[derive(StructOpt, Debug, Clone)]
pub struct LiveReplayArgs {
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

#[derive(StructOpt, Debug, Clone)]
pub struct PreConvertArgs {
    /// Input file with the sensor trajectory
    #[structopt(long)]
    pub trajectory_file: String,

    /// Input file with the point data
    #[structopt(long)]
    pub points_file: String,

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

    /// Frames per second at which to store point data.
    #[structopt(long, default_value = "20")]
    pub fps: u32,

    /// Host name for the point cloud server.
    /// The converter will briefly connect to this server to determine the correct settings for encoding the point data.
    #[structopt(long, short, default_value = "::1")]
    pub host: String,

    /// Port for the point cloud server.
    /// The converter will briefly connect to this server to determine the correct settings for encoding the point data.
    #[structopt(long, short, default_value = "4567")]
    pub port: u16,

    /// Name of the output file
    #[structopt(long)]
    pub output_file: String,
}

#[derive(StructOpt, Debug, Clone)]
pub struct ReplayPreconvertedArgs {
    /// Host name for the point cloud server.
    #[structopt(long, short, default_value = "::1")]
    pub host: String,

    /// Port for the point cloud server.
    #[structopt(long, short, default_value = "4567")]
    pub port: u16,

    /// Frames per second at which to replay point data.
    #[structopt(long, default_value = "20")]
    pub fps: u32,

    /// Name of the file containing the point data
    #[structopt()]
    pub input_file: String,
}
