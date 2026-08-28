//! Recursive descent over the [`logos`](super::lex) token stream.

use crate::lex::{Tok, Token, lex};
use traqilet_lang::ast::{Ident, Script};
use traqilet_lang::{DUMMY_SP, Error, Span};

// One file per construct, as rustc's parser is laid out.
mod expr;
mod item;
mod stmt;
#[cfg(test)]
mod tests;
mod ty;

pub struct Parsed {
    pub script: Script,
    pub errors: Vec<Error>,
}

type PResult<T> = Result<T, Error>;

fn opener_of(close: &Tok) -> &'static str {
    match close {
        Tok::RParen => "`(`",
        Tok::RBracket => "`[`",
        _ => "`{`",
    }
}

struct Parser {
    /// Trivia already dropped, so the grammar never sees whitespace or comments.
    toks: Vec<Token>,
    pos: usize,
    /// The empty span at end of input, for reporting past the last token.
    eof: Span,
    errors: Vec<Error>,
}

impl Parser {
    fn tok(&self) -> Option<&Tok> {
        self.nth(0)
    }

    fn nth(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.pos + n).map(|t| &t.tok)
    }

    fn span(&self) -> Span {
        self.toks.get(self.pos).map_or(self.eof, |t| t.span)
    }

    fn prev_end(&self) -> Span {
        match self.pos.checked_sub(1).and_then(|i| self.toks.get(i)) {
            Some(t) => t.span.shrink_to_hi(),
            None => self.span().shrink_to_lo(),
        }
    }

    fn bump(&mut self) {
        self.pos += 1;
    }

    fn at(&self, t: &Tok) -> bool {
        self.tok() == Some(t)
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.bump();
            return true;
        }
        false
    }

    /// `expected <what>, found <x>` at the cursor.
    ///
    /// A token the lexer already rejected carries a better message of its own,
    /// so this stays silent there and the caller's recovery resynchronises.
    /// Without that, every lexing error collects a parse error on top of it.
    fn expected(&self, what: &str) -> Error {
        if self.at(&Tok::Error) {
            return Error::new(String::new(), self.span());
        }
        let found = match self.tok() {
            Some(t) => format!("found `{t}`"),
            None => "found end of input".to_owned(),
        };
        Error::new(format!("expected {what}, {found}"), self.span())
    }

    fn expect(&mut self, t: &Tok, what: &str) -> PResult<Span> {
        let span = self.span();
        if self.eat(t) {
            return Ok(span);
        }
        Err(self.expected(what))
    }

    fn expect_semi(&mut self) -> PResult<()> {
        if self.eat(&Tok::Semi) {
            return Ok(());
        }
        if self.at(&Tok::Error) {
            return Err(Error::new(String::new(), self.span()));
        }
        Err(Error::new("expected `;`", self.prev_end()))
    }

    fn expect_close(&mut self, close: &Tok, what: &str, open: Span) -> PResult<Span> {
        let span = self.span();
        if self.eat(close) {
            return Ok(span);
        }
        Err(self
            .expected(what)
            .with_note(format!("unclosed {}", opener_of(close)), open))
    }

    /// Steps past the rest of a group that failed to parse, closer included.
    /// Needed for error recovery.
    fn skip_past(&mut self, close: &Tok) {
        let mut depth = 0usize;
        while let Some(t) = self.tok() {
            let last = depth == 0 && t == close;
            match t {
                Tok::LBrace | Tok::LParen | Tok::LBracket => depth += 1,
                Tok::RBrace | Tok::RParen | Tok::RBracket => depth = depth.saturating_sub(1),
                _ => {}
            }
            self.bump();
            if last {
                return;
            }
        }
    }

    fn ident(&mut self, what: &str) -> PResult<Ident> {
        let Some(Token { tok, span }) = self.toks.get(self.pos) else {
            return Err(self.expected(what));
        };
        let Tok::Ident(name) = tok else {
            return Err(self.expected(what));
        };
        let i = Ident {
            name: name.to_owned(),
            span: *span,
        };
        self.bump();
        Ok(i)
    }

    /// Records an error unless it is the silent kind standing in for one the
    /// lexer already reported.
    fn emit(&mut self, e: Error) {
        if !e.msg.is_empty() {
            self.errors.push(e);
        }
    }

    /// A comma-separated list up to `close`, which is *not* consumed. Trailing
    /// commas allowed. Stops at end of input rather than spinning.
    fn comma_separated<T>(
        &mut self,
        close: &Tok,
        open: Span,
        close_what: &str,
        mut each: impl FnMut(&mut Parser) -> PResult<T>,
    ) -> PResult<Vec<T>> {
        let mut out = Vec::new();
        while !self.at(close) {
            if self.tok().is_none() {
                return Err(self
                    .expected(close_what)
                    .with_note(format!("unclosed {}", opener_of(close)), open));
            }
            out.push(each(self)?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        Ok(out)
    }
}

pub fn parse(src: &str) -> Parsed {
    if src.len() > u32::MAX as usize {
        return Parsed {
            script: Script {
                shebang: None,
                items: Vec::new(),
            },
            errors: vec![Error::new("script is larger than 4 GiB", DUMMY_SP)],
        };
    }
    let (toks, lex_errors) = lex(src);
    let mut p = Parser {
        toks: toks.into_iter().filter(|t| !t.tok.is_trivia()).collect(),
        pos: 0,
        eof: Span::new(src.len(), src.len()),
        errors: lex_errors,
    };
    let script = p.script();
    p.errors.sort_by_key(|e| e.span.lo());
    Parsed {
        script,
        errors: p.errors,
    }
}
