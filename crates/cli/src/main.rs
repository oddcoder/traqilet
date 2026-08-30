mod cli;
mod diag;
mod fmt;
mod logging;

use crate::diag::Diags;
use clap::Parser;
use cli::Cli;
use log::{debug, error, info};
use std::{fs::read_to_string, process::exit};
use traqilet_btf::Btf;

fn main() {
    let cli = Cli::parse();
    if cli.licenses {
        print!("{}", cli::LICENSES);
        return;
    }
    logging::init(&cli);
    debug!("Starting traqilet");

    let script = cli.script.as_deref().expect("required unless --licenses");
    let path = script.display().to_string();
    let src = match read_to_string(script) {
        Ok(src) => src,
        Err(e) => {
            error!("Failed to read {path}: {e}");
            exit(-1);
        }
    };

    let color = cli.color();
    let mut d = Diags::new(&path, &src, color);
    let parsed = traqilet_parser::parse(&src);

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
    let res = if let Some(path) = cli.btf.as_deref() {
        Btf::from_file(path)
    } else {
        Btf::from_live_kernel()
    };
    let _types = match res {
        Ok(types) => types,
        Err(e) => {
            error!("Failed to load BTF: {e}");
            exit(-1);
        }
    };

    info!("{path} parsed, {} item(s)", parsed.script.items.len());
}
