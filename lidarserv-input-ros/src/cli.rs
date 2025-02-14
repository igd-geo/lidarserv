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
#[derive(Debug, Parser)]
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
}
