#![deny(unused_must_use)]

mod cli;
mod commands;
pub mod index;
pub mod net;
use crate::cli::{Args, Command};
use anyhow::Result;
use human_panic::setup_panic;
pub use lidarserv_common as common;

#[paw::main]
fn main(args: Args) -> Result<()> {
    // panic handler
    setup_panic!();

    // logger
    // unwrap: will only fail, if the logger is already initialized - which it clearly is not
    simple_logger::init_with_level(args.log_level).unwrap();

    // run the passed command
    match args.command {
        Command::Init(options) => commands::init::run(options),
        Command::Serve(options) => commands::serve::run(options),
    }
}
