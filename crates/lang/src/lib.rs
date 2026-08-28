//! The language every pass operates on.
pub mod ast;
mod error;
pub mod span;

pub use error::Error;
pub use span::{DUMMY_SP, Span};
