//! One error type for the whole front end.

use super::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub msg: String,
    /// What to underline.
    pub span: Span,
    /// A second place worth looking, such as the `(` that was never closed.
    pub note: Option<(String, Span)>,
}

impl Error {
    pub fn new(msg: impl Into<String>, span: Span) -> Error {
        Error {
            msg: msg.into(),
            span,
            note: None,
        }
    }

    pub fn with_note(mut self, msg: impl Into<String>, span: Span) -> Error {
        self.note = Some((msg.into(), span));
        self
    }
}
