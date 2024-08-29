use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// A tool to index and query lidar point clouds, in soft real time.
#[derive(Debug, Parser)]
pub struct LidarservOptions {
    /// Verbosity of the command line output.
    #[clap(long, default_value = "info")]
    pub log_level: log::Level,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)] // this is ok, because the struct is only ever instantiated once at the beginning of the program.
pub enum Command {
    /// Initializes a new point cloud.
    Init(InitOptions),

    /// Runs the indexing server.
    Serve(ServeOptions),
}

#[derive(Args, Debug)]
pub struct InitOptions {
    /// Folder, that the point cloud will be created in. By default, the current folder will be used.
    #[clap(default_value = ".", hide_default_value = true)]
    pub path: PathBuf,
}

#[derive(Args, Debug)]
pub struct ServeOptions {
    /// Hostname to listen on.
    #[clap(long, default_value = "::1")]
    pub host: String,

    /// Port to listen on.
    #[clap(long, default_value = "4567")]
    pub port: u16,

    /// Folder, that the point cloud data will be stored in.
    ///
    /// Use the `init` command first, to initialize a new point cloud in that folder. By default, the current folder will be used.
    #[clap(default_value = ".", hide_default_value = true)]
    pub path: PathBuf,
}
