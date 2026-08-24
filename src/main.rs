mod cli;
mod fmt;
mod logging;

use clap::Parser;
use cli::Cli;
use log::{debug, error};
use std::{fs::read_to_string, process::exit};

fn main() {
    let cli = Cli::parse();
    logging::init(&cli);
    debug!("Starting traqilet");

    let _src = match read_to_string(&cli.script) {
        Ok(src) => src,
        Err(e) => {
            error!("Failed to read {}: {e}", cli.script.display());
            exit(-1);
        }
    };
}
