pub mod lex;
pub mod parse;

pub use parse::{Parsed, parse};

#[cfg(test)]
const EXAMPLES_DIR: &str = concat!(env!("CARGO_WORKSPACE_DIR"), "examples");
