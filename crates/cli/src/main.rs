mod cli;
mod diag;
mod fmt;
mod lang;
mod logging;

use crate::diag::Diags;
use clap::Parser;
use cli::Cli;
use log::{debug, error, info};
use std::{fs::read_to_string, process::exit};

fn main() {
    let cli = Cli::parse();
    logging::init(&cli);
    debug!("Starting traqilet");

    let path = cli.script.display().to_string();
    let src = match read_to_string(&cli.script) {
        Ok(src) => src,
        Err(e) => {
            error!("Failed to read {path}: {e}");
            exit(-1);
        }
    };

    let color = cli.color();
    let mut d = Diags::new(&path, &src, color);
    let parsed = lang::parse::parse(&src);

    for e in &parsed.errors {
        d.error(e);
    }
    if d.errors() > 0 {
        error!("{} error(s)", d.errors());
        exit(-1);
    }

    if parsed.script.items.is_empty() {
        d.plain("nothing to run: the script declares no items");
        exit(-1);
    }

    info!("{path} parsed, {} item(s)", parsed.script.items.len());
}
