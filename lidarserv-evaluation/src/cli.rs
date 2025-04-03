use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct EvaluationOptions {
    /// Verbosity of the command line output.
    #[clap(long, default_value = "info")]
    pub log_level: log::Level,

    /// If provided, then only the evaluation run with this name will be executed.
    /// The argument can be repeated to specify multiple runs. By default, all
    /// runs that are defined by the toml file will be executed.
    #[clap(long)]
    pub run: Vec<String>,

    /// Path to a toml file with all the evaluation settings.
    ///
    /// If the given file does not exist, an default file will be created and the program will terminate immediately.
    pub input_file: PathBuf,
}
