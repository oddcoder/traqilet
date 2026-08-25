//! The front end: tokens, a typed AST, and a parser producing it.
pub mod ast;
mod error;
pub mod lex;
pub mod parse;
pub mod span;

pub use error::Error;
pub use span::{DUMMY_SP, Span};
