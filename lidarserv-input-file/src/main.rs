use clap::Parser;
use cli::{AppOptions, Command};
use commands::{replay::replay, sort::sort};
use human_panic::setup_panic;
use log::{debug, error};
use std::process::ExitCode;

mod cli;
mod commands;

#[tokio::main]
async fn main() -> ExitCode {
    setup_panic!();

    // arg parsing
    let args = AppOptions::parse();

    // logger
    simple_logger::init_with_level(args.log_level).unwrap();

    // run
    let result = match args.command {
        Command::Replay(options) => replay(options).await,
        Command::Sort(options) => sort(options).await,
    };
    if let Err(e) = result {
        error!("{e}");
        debug!("{e:?}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
