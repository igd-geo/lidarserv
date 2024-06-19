use clap::Parser;

/// Command line arguments for the application
#[derive(Debug, Parser)]
#[command(
    version,
    about = "Connector that subscribes to a ROS PointCloud2 topic and forwards all incoming points to lidarserv."
)]
pub struct Cli {
    /// Host name for the point cloud server.
    #[structopt(long, default_value = "::1")]
    pub host: String,

    /// Port for the point cloud server.
    #[structopt(long, default_value = "4567")]
    pub port: u16,

    #[structopt(long)]
    pub enable_compression: bool,

    /// The ros topic to subscribe to, that the captured points are published on.
    /// The topic should be of type sensor_msgs/PointCloud2.
    #[arg(long, default_value = "point_cloud")]
    pub subscribe_topic_name: String,
}
