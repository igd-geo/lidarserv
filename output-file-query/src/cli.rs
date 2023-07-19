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

    /// Enable usage of range-based attribute-index acceleration structure.
    /// Only works, if the attribute index was created during the indexing process.
    #[structopt(long)]
    pub enable_attribute_acceleration: bool,

    /// Enable usage of additional histogram-based acceleration structure.
    /// Only works, if the histograms were created during the indexing process.
    #[structopt(long)]
    pub enable_histogram_acceleration: bool,

    /// Enable point based filtering for spatial queries and attribute filters.
    #[structopt(long)]
    pub enable_point_filtering: bool,

    // AABB PARAMETER
    /// Minimum x value of the bounding box.
    #[structopt(long)]
    pub min_x: f64,

    /// Minimum y value of the bounding box.
    #[structopt(long)]
    pub min_y: f64,

    /// Minimum z value of the bounding box.
    #[structopt(long)]
    pub min_z: f64,

    /// Maximum x value of the bounding box.
    #[structopt(long)]
    pub max_x: f64,

    /// Maximum y value of the bounding box.
    #[structopt(long)]
    pub max_y: f64,

    /// Maximum z value of the bounding box.
    #[structopt(long)]
    pub max_z: f64,

    /// Minimum intensity attribute filter.
    #[structopt(long)]
    pub min_intensity: Option<u16>,

    /// Maximum intensity attribute filter.
    #[structopt(long)]
    pub max_intensity: Option<u16>,

    /// Minimum return number attribute filter.
    #[structopt(long)]
    pub min_return_number: Option<u8>,

    /// Maximum return number attribute filter.
    #[structopt(long)]
    pub max_return_number: Option<u8>,

    /// Minimum number of returns attribute filter.
    #[structopt(long)]
    pub min_number_of_returns: Option<u8>,

    /// Maximum number of returns attribute filter.
    #[structopt(long)]
    pub max_number_of_returns: Option<u8>,

    /// Minimum scan direction attribute filter.
    #[structopt(long)]
    pub min_scan_direction: Option<u8>,

    /// Maximum scan direction attribute filter.
    #[structopt(long)]
    pub max_scan_direction: Option<u8>,

    /// Minimum edge of flight line attribute filter.
    #[structopt(long)]
    pub min_edge_of_flight_line: Option<u8>,

    /// Maximum edge of flight line attribute filter.
    #[structopt(long)]
    pub max_edge_of_flight_line: Option<u8>,

    /// Minimum classification attribute filter.
    #[structopt(long)]
    pub min_classification: Option<u8>,

    /// Maximum classification attribute filter.
    #[structopt(long)]
    pub max_classification: Option<u8>,

    /// Minimum scan angle attribute filter.
    #[structopt(long)]
    pub min_scan_angle: Option<i8>,

    /// Maximum scan angle attribute filter.
    #[structopt(long)]
    pub max_scan_angle: Option<i8>,

    /// Minimum user data attribute filter.
    #[structopt(long)]
    pub min_user_data: Option<u8>,

    /// Maximum user data attribute filter.
    #[structopt(long)]
    pub max_user_data: Option<u8>,

    /// Minimum point source id attribute filter.
    #[structopt(long)]
    pub min_point_source_id: Option<u16>,

    /// Maximum point source id attribute filter.
    #[structopt(long)]
    pub max_point_source_id: Option<u16>,

    /// Minimum gps time attribute filter.
    #[structopt(long)]
    pub min_gps_time: Option<f64>,

    /// Maximum gps time attribute filter.
    #[structopt(long)]
    pub max_gps_time: Option<f64>,

    /// Minimum red color attribute filter.
    #[structopt(long)]
    pub min_color_r: Option<u16>,

    /// Maximum red color attribute filter.
    #[structopt(long)]
    pub max_color_r: Option<u16>,

    /// Minimum green color attribute filter.
    #[structopt(long)]
    pub min_color_g: Option<u16>,

    /// Maximum green color attribute filter.
    #[structopt(long)]
    pub max_color_g: Option<u16>,

    /// Minimum blue color attribute filter.
    #[structopt(long)]
    pub min_color_b: Option<u16>,

    /// Maximum blue color attribute filter.
    #[structopt(long)]
    pub max_color_b: Option<u16>,

}