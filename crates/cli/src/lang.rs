//! The front end: tokens, and a parser building the tree out of them.
pub mod lex;
pub mod parse;

#[cfg(test)]
const EXAMPLES_DIR: &str = concat!(env!("CARGO_WORKSPACE_DIR"), "examples");
