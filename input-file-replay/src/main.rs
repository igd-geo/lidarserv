mod cli;
mod commands;
pub mod file_reader;
pub mod iter_points;

use crate::cli::{Args, Command};
use crate::commands::csvreplay::replay_csv;
use crate::commands::lazreplay::replay_laz;
use crate::commands::preconvert::preconvert;
use lidarserv_server::common::nalgebra::Vector3;
use log::error;

#[paw::main]
#[tokio::main]
async fn main(args: Args) {
    simple_logger::init_with_level(args.log_level).unwrap();
    let result = match args.command {
        Command::LiveReplay(replay_args) => replay_csv(replay_args).await,
        Command::Convert(preconvert_args) => preconvert(preconvert_args).await,
        Command::Replay(replay_args) => replay_laz(replay_args).await,
    };
    match result {
        Ok(()) => (),
        Err(e) => {
            error!("{}", e);
        }
    }
}
