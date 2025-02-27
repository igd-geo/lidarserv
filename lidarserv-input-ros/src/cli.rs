use std::str::FromStr;

use anyhow::anyhow;
use clap::Parser;

/// Connector that forwards point clouds from ROS into lidarserv.
///
/// The connection to ROS should be established automatically if you are within the ROS environment. If the connection fails, you can set the following ROS connection options:
///
///  - The master URL:
///     via command line: `__master:=http://localhost:11311/`
///     via environment variable: `ROS_MASTER_URI=http://localhost:11311/`
///  - The ros hostname or ip address:
///     via command line: `__hostname:=localhost` or `__ip:=127.0.0.1`
///     via environment variables: `ROS_HOSTNAME=localhost` or `ROS_IP=127.0.0.1`
///  - The ros namespace:
///     via command line: `__ns:=my_namespace`
///     via environment variable: `ROS_NAMESPACE=my_namespace`
///  - The node name:
///     via command line: `__name:=lidarserv`
#[derive(Debug, Parser, Clone)]
#[command(verbatim_doc_comment)]
pub struct AppOptions {
    /// Verbosity of the command line output.
    #[clap(long, default_value = "info")]
    pub log_level: log::Level,

    /// The ROS topic where the PointCloud2 messages will be published to.
    #[clap(long, default_value = "/cloud_registered")]
    pub pointcloud_topic: String,

    /// The ros topic for the transform tree.
    /// Usually, the default value is already correct.
    #[clap(long, default_value = "/tf")]
    pub tf_topic: String,

    /// The ros topic for the static transforms in the transform tree.
    /// Usually, the default value is already correct.
    #[clap(long, default_value = "/tf_static")]
    pub tf_static_topic: String,

    /// Name of the fixed coordinate frame that the lidar points will be
    /// transformed to before sending to the lidarserv server.
    // note: The default should probably be "map" according to REP-105 https://www.ros.org/reps/rep-0105.html
    #[clap(long, default_value = "camera_init")]
    pub world_frame: String,

    /// If set, the coordinates are flipped along the given axis.
    #[clap(long)]
    pub transform_flip: Option<Axis>,

    /// Hostname of the lidarserv server
    #[clap(long, default_value = "::0")]
    pub host: String,

    /// Port of the lidarserv server
    #[clap(long, default_value = "4567")]
    pub port: u16,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl FromStr for Axis {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "x" | "X" => Ok(Axis::X),
            "y" | "Y" => Ok(Axis::Y),
            "z" | "Z" => Ok(Axis::Z),
            _ => Err(anyhow!(
                "'{s}' is not a valid axis. Must be 'x', 'y' or 'z'."
            )),
        }
    }
}
