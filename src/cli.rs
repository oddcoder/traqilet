//! The command line surface: the flags and the types they parse into, kept
//! apart from the code that consumes them so `--help` reads as one unit.

use clap::{ArgAction, Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "compile tracing scripts to bpf and run them")]
pub struct Cli {
    /// script file to run
    pub script: PathBuf,

    /// increase verbosity: -v script debug, -vv internals, -vvv dependencies, -vvvv trace
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,

    /// when to colourise; auto detects a terminal and honours NO_COLOR
    #[arg(long, value_enum, default_value_t = Color::Auto, value_name = "WHEN")]
    pub color: Color,

    /// log line template, Rust format syntax. Fields {time} {mono} {delta}
    /// {level} {src} {msg}, each accepting a spec such as {mono:>12.6}; {{ and
    /// }} are literal braces, so a JSON line is
    /// --log-format '{{"ts":"{time}","level":"{level}","msg":"{msg}"}}'
    #[arg(long, value_name = "TEMPLATE")]
    pub log_format: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Color {
    Auto,
    Always,
    Never,
}
