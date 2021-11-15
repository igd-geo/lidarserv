use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct Args {
    /// Verbosity of the command line output.
    #[structopt(long, default_value="info", possible_values = &["trace", "debug", "info", "warn", "error"])]
    pub log_level: log::Level,

    #[structopt(long, short, default_value = "::1")]
    pub host: String,

    #[structopt(long, short, default_value = "4567")]
    pub port: u16,
}
