//! Logger setup: `-v`, `--color` and `--log-format` become an `env_logger`
//! whose format closure renders each record through [`crate::fmt`].

use crate::{
    cli::{Cli, Color},
    fmt::{Spec, Val},
};
use env_logger::{Builder, Target, WriteStyle, fmt::Formatter};
use log::Record;
use std::{
    env,
    io::{self, Write},
    mem,
    process::exit,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

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
                    out.push(Piece::Lit(mem::take(&mut lit)));
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

fn filter(verbose: u8) -> &'static str {
    match verbose {
        0 => "script=info,warn",
        1 => "script=debug,traqilet=info,warn",
        2 => "script=debug,traqilet=debug,warn",
        3 => "debug",
        _ => "trace",
    }
}

fn write_record(
    f: &mut Formatter,
    r: &Record<'_>,
    pieces: &[Piece],
    start: Instant,
) -> io::Result<()> {
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
    for piece in pieces {
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
}

pub fn init(cli: &Cli) {
    let filter = env::var("RUST_LOG").unwrap_or_else(|_| filter(cli.verbose).to_owned());

    let default = "[{time}] [{mono:>12.6}] [{delta:>12.6}] [{level:<5}] [{src}] {msg}";

    let pieces = parse_format(cli.log_format.as_deref().unwrap_or(default));
    let style = match cli.color {
        Color::Auto => WriteStyle::Auto,
        Color::Always => WriteStyle::Always,
        Color::Never => WriteStyle::Never,
    };
    let start = Instant::now();
    Builder::new()
        .parse_filters(&filter)
        .target(Target::Stdout)
        .write_style(style)
        .format(move |f, r| write_record(f, r, &pieces, start))
        .init();
}
