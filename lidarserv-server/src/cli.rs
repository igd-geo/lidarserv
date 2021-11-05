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
pub enum Command {
    /// Initializes a new point cloud.
    Init(InitOptions),

    /// Runs the indexing server.
    Serve(ServeOptions),
}

#[derive(StructOpt, Debug)]
pub struct InitOptions {
    /// Index structure to use so the point cloud can be queried efficiently.
    #[structopt(long, possible_values=&["mno", "bvg"], default_value = "mno")]
    pub index: Index,

    /// The resolution used for storing point data.
    #[structopt(long, default_value = "0.001")]
    pub las_scale: VectorOption,

    /// The offset used for storing point data. (usually fine to be left at '0.0, 0.0, 0.0')
    #[structopt(long, default_value = "0")]
    pub las_offset: VectorOption,

    /// Number of threads used for indexing the points.
    #[structopt(long, default_value = "4")]
    pub num_threads: usize,

    /// Maximum level of detail of the index.
    #[structopt(long, default_value = "10")]
    pub max_lod: u16,

    /// Maximum number of nodes to keep in memory, while indexing.
    #[structopt(long, default_value = "500")]
    pub cache_size: usize,

    /// The order, in which to process pending tasks. This option only applies to the mno index.
    #[structopt(long, default_value="nr_points", possible_values=&["nr_points", "lod", "newest_point", "oldest_point", "task_age"],)]
    pub mno_task_priority: String, // todo define enum

    /// The distance between two points at the coarsest level of detail.
    #[structopt(long, default_value = "8.0")]
    pub point_grid_size: f64,

    /// The size of the nodes at the coarsest level of detail. With each finer LOD, the node size will be halved. This option only applies to the mno index.
    #[structopt(long, default_value = "1024.0")]
    pub mno_node_grid_size: f64,

    /// The maximum number of points that can be inserted into a node, before that node is split. This option only applies to the bvg index.
    #[structopt(long, default_value = "100000")]
    pub bvg_max_points_per_node: usize,

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
pub enum Index {
    Mno,
    Bvg,
}

impl FromStr for Index {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mno" => Ok(Index::Mno),
            "bvg" => Ok(Index::Bvg),
            _ => Err(anyhow::Error::msg(
                "Unrecognized index structure. Valid values are: mno, bvg",
            )),
        }
    }
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
