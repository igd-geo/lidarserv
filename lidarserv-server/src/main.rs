#![deny(unused_must_use)]
use anyhow::Result;
use clap::Parser;
use cli::{Command, LidarservOptions};
use human_panic::setup_panic;

mod cli;
mod commands;

fn main() -> Result<()> {
    // panic handler
    setup_panic!();

    // arg parsing
    let args = LidarservOptions::parse();

    // logger
    // unwrap: will only fail, if the logger is already initialized - which it clearly is not
    simple_logger::init_with_level(args.log_level).unwrap();

    // run the passed command
    match args.command {
        Command::Init(options) => commands::init::run(options),
        Command::Serve(options) => commands::serve::run(options),
    }
}
