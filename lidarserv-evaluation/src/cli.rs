use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct EvaluationOptions {
    /// Verbosity of the command line output.
    #[clap(long, default_value = "info")]
    pub log_level: log::Level,

    pub input_file: PathBuf,
}
