use lidarserv_common::index::octree::writer::TaskPriorityFunction;
use lidarserv_common::nalgebra::Vector3;
use std::path::PathBuf;
use std::str::FromStr;
use structopt::StructOpt;
use thiserror::Error;

/// A tool to index and query lidar point clouds, in soft real time.
#[derive(StructOpt, Debug)]
#[structopt(name = "lidarserv")]
pub struct Args {
    /// Verbosity of the command line output.
    #[structopt(long, default_value="info", possible_values = &["trace", "debug", "info", "warn", "error"])]
    pub log_level: log::Level,

    #[structopt(subcommand)]
    pub command: Command,
}

#[derive(StructOpt, Debug)]
#[allow(clippy::large_enum_variant)] // this is ok, because the struct is only ever instantiated once at the beginning of the program.
pub enum Command {
    /// Initializes a new point cloud.
    Init(InitOptions),

    /// Runs the indexing server.
    Serve(ServeOptions),
}

#[derive(StructOpt, Debug)]
pub struct InitOptions {
    /// The resolution used for storing point data.
    #[structopt(long, default_value = "0.001")]
    pub las_scale: VectorOption,

    /// The offset used for storing point data. (usually fine to be left at '0.0, 0.0, 0.0')
    #[structopt(long, default_value = "0")]
    pub las_offset: VectorOption,

    /// Disables laz compression of point data
    #[structopt(long)]
    pub las_no_compression: bool,

    /// Selection of LAS Point Record Format (0-3 supported)
    #[structopt(long, default_value = "0")]
    pub las_point_record_format: u8,

    /// Number of threads used for indexing the points.
    #[structopt(long, default_value = "4")]
    pub num_threads: usize,

    /// Maximum level of detail of the index.
    #[structopt(long, default_value = "10")]
    pub max_lod: u16,

    /// Maximum number of nodes to keep in memory, while indexing.
    #[structopt(long, default_value = "500")]
    pub cache_size: usize,

    /// The order, in which to process pending tasks.
    #[structopt(long, default_value="NrPoints", possible_values=&["NrPoints", "Lod", "OldestPoint", "TaskAge", "NrPointsTaskAge"],)]
    pub mno_task_priority: TaskPriorityFunction,

    /// The distance between two points at the coarsest level of detail. The value will be rounded towards the closest valid value.
    #[structopt(long, default_value = "1.024")]
    pub point_grid_size: f64,

    /// The size of the nodes at the coarsest level of detail. With each finer LOD, the node size will be halved. The value will be rounded towards the closest valid value.
    #[structopt(long, default_value = "131.072")]
    pub mno_node_grid_size: f64,

    /// Maximum number of bogus points per node.
    #[structopt(long, default_value = "0")]
    pub mno_bogus: usize,

    /// Maximum number of bogus points per inner (non-leaf) node. Overwrites the '--mno-bogus' option, if provided.
    #[structopt(long)]
    pub mno_bogus_inner: Option<usize>,

    /// Maximum number of bogus points per leaf node. Overwrites the '--mno-bogus' option, if provided.
    #[structopt(long)]
    pub mno_bogus_leaf: Option<usize>,

    /// Enable indexing attributes of points
    #[structopt(long)]
    pub enable_attribute_indexing: bool,

    /// Enable acceleration of the attribute indexing using additional histograms for each attribute
    #[structopt(long)]
    pub enable_histogram_acceleration: bool,

    /// Sets number of bins of the intensity histograms
    /// Only used if '--enable-histogram-acceleration' is enabled
    #[structopt(long)]
    pub bin_count_intensity: Option<usize>,

    /// Sets number of bins of the return number histograms
    /// Only used if '--enable-histogram-acceleration' is enabled
    #[structopt(long)]
    pub bin_count_return_number: Option<usize>,

    /// Sets number of bins of the classification histograms
    /// Only used if '--enable-histogram-acceleration' is enabled
    #[structopt(long)]
    pub bin_count_classification: Option<usize>,

    /// Sets number of bins of the scan angle rank histograms
    /// Only used if '--enable-histogram-acceleration' is enabled
    #[structopt(long)]
    pub bin_count_scan_angle_rank: Option<usize>,

    /// Sets number of bins of the user data histograms
    /// Only used if '--enable-histogram-acceleration' is enabled
    #[structopt(long)]
    pub bin_count_user_data: Option<usize>,

    /// Sets number of bins of the point source id histograms
    /// Only used if '--enable-histogram-acceleration' is enabled
    #[structopt(long)]
    pub bin_count_point_source_id: Option<usize>,

    /// Sets number of bins of the color histograms
    /// Only used if '--enable-histogram-acceleration' is enabled
    #[structopt(long)]
    pub bin_count_color: Option<usize>,

    /// If enabled, some metrics are collected during indexing and written to a file named 'metrics_%i.cbor',
    /// where %i is a sequentially increasing number.
    #[structopt(long)]
    pub mno_use_metrics: bool,

    /// Folder, that the point cloud will be created in. By default, the current folder will be used.
    #[structopt(default_value = ".", hide_default_value = true)]
    pub path: PathBuf,
}

#[derive(StructOpt, Debug)]
pub struct ServeOptions {
    /// Hostname to listen on.
    #[structopt(long, short, default_value = "::1")]
    pub host: String,

    /// Port to listen on.
    #[structopt(long, short, default_value = "4567")]
    pub port: u16,

    /// Folder, that the point cloud data will be stored in.
    ///
    /// Use the `init` command first, to initialize a new point cloud in that folder. By default, the current folder will be used.
    #[structopt(default_value = ".", hide_default_value = true)]
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct VectorOption(pub Vector3<f64>);

#[derive(Debug, Error)]
#[error("Must be a comma list of three numbers, separated by ';'. Example: 42;3.14;2.5")]
pub struct VectorOptionFromStrError;

impl FromStr for VectorOption {
    type Err = VectorOptionFromStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let elements: Vec<f64> = s
            .split(';')
            .map(|slice| f64::from_str(slice.trim()).map_err(|_| VectorOptionFromStrError))
            .collect::<Result<_, _>>()?;
        if elements.len() == 1 {
            Ok(VectorOption(Vector3::new(
                elements[0],
                elements[0],
                elements[0],
            )))
        } else if elements.len() == 3 {
            Ok(VectorOption(Vector3::new(
                elements[0],
                elements[1],
                elements[2],
            )))
        } else {
            Err(VectorOptionFromStrError)
        }
    }
}
