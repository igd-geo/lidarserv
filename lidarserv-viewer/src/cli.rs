use std::str::FromStr;
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

    #[structopt(long, default_value = "fixed", possible_values = &["fixed", "intensity"])]
    pub point_color: PointColorArg,

    #[structopt(long, default_value = "10")]
    pub point_size: f32,

    #[structopt(long, default_value = "10")]
    pub point_distance: f64,
}

#[derive(Debug)]
pub enum PointColorArg {
    Fixed,
    Intensity,
}

impl FromStr for PointColorArg {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fixed" => Ok(PointColorArg::Fixed),
            "intensity" => Ok(PointColorArg::Intensity),
            _ => Err(anyhow::Error::msg(
                "Invalid value - must be one of: 'fixed', 'intensity'",
            )),
        }
    }
}
