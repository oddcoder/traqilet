mod fmt;

use clap::{ArgAction, Parser, ValueEnum};
use fmt::{Spec, Val};
use log::{debug, error};
use std::{
    fs::read_to_string,
    io::Write,
    path::PathBuf,
    process::exit,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(version, about = "compile tracing scripts to bpf and run them")]
struct Cli {
    /// script file to run
    script: PathBuf,

    /// increase verbosity: -v script debug, -vv internals, -vvv dependencies, -vvvv trace
    #[arg(short, long, action = ArgAction::Count)]
    verbose: u8,

    /// when to colourise; auto detects a terminal and honours NO_COLOR
    #[arg(long, value_enum, default_value_t = Color::Auto, value_name = "WHEN")]
    color: Color,

    /// log line template, Rust format syntax. Fields {time} {mono} {delta}
    /// {level} {src} {msg}, each accepting a spec such as {mono:>12.6}; {{ and
    /// }} are literal braces, so a JSON line is
    /// --log-format '{{"ts":"{time}","level":"{level}","msg":"{msg}"}}'
    #[arg(long, value_name = "TEMPLATE")]
    log_format: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Color {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy)]
enum Field {
    /// wall clock, e.g. 2026-08-24T21:59:07.008Z
    Time,
    /// seconds since traqilet started. Instant is CLOCK_MONOTONIC underneath,
    /// so a difference of two of these equals a difference of two
    /// linux.monotonic_ns values; only the origin differs.
    Mono,
    /// seconds since the previous log line
    Delta,
    /// ERROR, WARN, INFO, DEBUG, TRACE
    Level,
    /// emitting module, e.g. aya_obj::relocation
    Src,
    Msg,
}

/// A parsed --log-format template. Parsed once at startup rather than per
/// record, since the spec grammar is the same one script format strings use.
enum Piece {
    Lit(String),
    Field(Field, Spec),
}

/// Errors exit rather than report, because the logger does not exist yet.
fn parse_format(t: &str) -> Vec<Piece> {
    let die = |m: String| -> ! {
        eprintln!("--log-format: {m}");
        exit(-1);
    };
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut it = t.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' if it.peek() == Some(&'{') => {
                it.next();
                lit.push('{');
            }
            '}' if it.peek() == Some(&'}') => {
                it.next();
                lit.push('}');
            }
            '}' => die("stray `}`, use `}}` for a literal brace".into()),
            '{' => {
                let mut body = String::new();
                loop {
                    match it.next() {
                        Some('}') => break,
                        Some(ch) => body.push(ch),
                        None => die("unterminated `{`".into()),
                    }
                }
                let (name, spec) = body.split_once(':').unwrap_or((body.as_str(), ""));
                let field = match name {
                    "time" => Field::Time,
                    "mono" => Field::Mono,
                    "delta" => Field::Delta,
                    "level" => Field::Level,
                    "src" => Field::Src,
                    "msg" => Field::Msg,
                    other => die(format!(
                        "unknown field `{other}`, expected one of \
                         time, mono, delta, level, src, msg"
                    )),
                };
                let spec = Spec::parse(spec).unwrap_or_else(|e| die(e));
                if !lit.is_empty() {
                    out.push(Piece::Lit(std::mem::take(&mut lit)));
                }
                out.push(Piece::Field(field, spec));
            }
            _ => lit.push(c),
        }
    }
    if !lit.is_empty() {
        out.push(Piece::Lit(lit));
    }
    out
}

fn log_filter(verbose: u8) -> &'static str {
    match verbose {
        0 => "script=info,warn",
        1 => "script=debug,traqilet=info,warn",
        2 => "script=debug,traqilet=debug,warn",
        3 => "debug",
        _ => "trace",
    }
}

fn init_log(cli: &Cli) {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| log_filter(cli.verbose).to_owned());

    let default = "[{time}] [{mono:>12.6}] [{delta:>12.6}] [{level:<5}] [{src}] {msg}";

    let pieces = parse_format(cli.log_format.as_deref().unwrap_or(default));
    let style = match cli.color {
        Color::Auto => env_logger::WriteStyle::Auto,
        Color::Always => env_logger::WriteStyle::Always,
        Color::Never => env_logger::WriteStyle::Never,
    };
    let start = Instant::now();
    env_logger::Builder::new()
        .parse_filters(&filter)
        .target(env_logger::Target::Stdout)
        .write_style(style)
        .format(move |f, r| {
            // a script's own output is the tool's data: bare, so it pipes cleanly
            if r.target() == "script" {
                return writeln!(f, "{}", r.args());
            }
            static LAST: AtomicU64 = AtomicU64::new(0);
            let mono = start.elapsed();
            let prev = LAST.swap(mono.as_nanos() as u64, Ordering::Relaxed);
            let delta = match prev {
                0 => Duration::ZERO,
                p => mono.saturating_sub(Duration::from_nanos(p)),
            };
            let human = f.timestamp_millis();
            let lvl_style = f.default_level_style(r.level());
            for piece in &pieces {
                let (field, spec) = match piece {
                    Piece::Lit(t) => {
                        write!(f, "{t}")?;
                        continue;
                    }
                    Piece::Field(field, spec) => (field, spec),
                };
                match field {
                    Field::Time => write!(f, "{}", spec.render(Val::Str(&human.to_string())))?,
                    Field::Mono => write!(f, "{}", spec.render(Val::Num(mono.as_secs_f64())))?,
                    Field::Delta => write!(f, "{}", spec.render(Val::Num(delta.as_secs_f64())))?,
                    // colour wraps the padded text, so alignment is unaffected
                    Field::Level => {
                        let t = spec.render(Val::Str(r.level().as_str()));
                        write!(f, "{lvl_style}{t}{lvl_style:#}")?;
                    }
                    Field::Src => write!(f, "{}", spec.render(Val::Str(r.target())))?,
                    Field::Msg => write!(f, "{}", spec.render(Val::Str(&r.args().to_string())))?,
                }
            }
            writeln!(f)
        })
        .init();
}

fn main() {
    let cli = Cli::parse();
    init_log(&cli);
    debug!("Starting traqilet");

    let _src = match read_to_string(&cli.script) {
        Ok(src) => src,
        Err(e) => {
            error!("Failed to read {}: {e}", cli.script.display());
            exit(-1);
        }
    };
}
