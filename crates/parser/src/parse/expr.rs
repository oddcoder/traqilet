use super::{PResult, Parser};
use crate::lex::Tok;
use std::ops::Bound;
use traqilet_lang::Error;
use traqilet_lang::Span;
use traqilet_lang::ast::{BinOp, Expr, Expression, Ident, StructInit, UnOp};

/// Binding power, loosest first. Declaration order *is* the table: `PartialOrd`
/// does the comparing, so no level names a number and inserting one is a
/// one-line edit. This is rustc's `ExprPrecedence`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
enum Prec {
    Or,
    And,
    Equality,
    Compare,
    BitOr,
    BitXor,
    BitAnd,
    Shift,
    Sum,
    Product,
}

/// The binary operator spelled by a token.
fn assoc_op(t: &Tok) -> Option<(BinOp, Prec)> {
    Some(match t {
        Tok::OrOr => (BinOp::OrOr, Prec::Or),
        Tok::AndAnd => (BinOp::AndAnd, Prec::And),
        Tok::EqEq => (BinOp::Eq, Prec::Equality),
        Tok::In => (BinOp::In, Prec::Equality),
        Tok::Ne => (BinOp::Ne, Prec::Equality),
        Tok::Lt => (BinOp::Lt, Prec::Compare),
        Tok::Le => (BinOp::Le, Prec::Compare),
        Tok::Gt => (BinOp::Gt, Prec::Compare),
        Tok::Ge => (BinOp::Ge, Prec::Compare),
        Tok::Pipe => (BinOp::Or, Prec::BitOr),
        Tok::Caret => (BinOp::Xor, Prec::BitXor),
        Tok::Amp => (BinOp::And, Prec::BitAnd),
        Tok::Shl => (BinOp::Shl, Prec::Shift),
        Tok::Shr => (BinOp::Shr, Prec::Shift),
        Tok::Plus => (BinOp::Add, Prec::Sum),
        Tok::Minus => (BinOp::Sub, Prec::Sum),
        Tok::Star => (BinOp::Mul, Prec::Product),
        Tok::Slash => (BinOp::Div, Prec::Product),
        Tok::Percent => (BinOp::Rem, Prec::Product),
        _ => return None,
    })
}

impl Parser {
    /// A range is the loosest thing there is, and does not associate.
    pub(super) fn expr(&mut self) -> PResult<Expr> {
        let lo = self.expr_assoc(Bound::Unbounded)?;
        if !self.eat(&Tok::DotDot) {
            return Ok(lo);
        }
        let hi = self.expr_assoc(Bound::Unbounded)?;
        Ok(Expr {
            span: lo.span.to(hi.span),
            expr: Expression::Range(Box::new(lo), Box::new(hi)),
        })
    }

    /// An index: arithmetic on sizes, with no range and nothing looser.
    pub(super) fn index_expr(&mut self) -> PResult<Expr> {
        self.expr_assoc(Bound::Unbounded)
    }

    /// Whether the token `n` ahead is a binary operator, which is what tells a size
    /// from a type in `Slice(u64, B + 5)`.
    pub(super) fn at_binary_op(&self, n: usize) -> bool {
        self.nth(n).and_then(assoc_op).is_some()
    }

    /// Precedence climbing: one loop, no rule per level.
    fn expr_assoc(&mut self, min: Bound<Prec>) -> PResult<Expr> {
        let mut lhs = self.unary()?;
        while let Some((op, prec)) = self.tok().and_then(assoc_op) {
            let too_loose = match min {
                Bound::Included(m) => prec < m,
                Bound::Excluded(m) => prec <= m,
                Bound::Unbounded => false,
            };
            if too_loose {
                break;
            }
            self.bump();
            let rhs = self.expr_assoc(Bound::Excluded(prec))?;
            lhs = Expr {
                span: lhs.span.to(rhs.span),
                expr: Expression::Binary(op, Box::new(lhs), Box::new(rhs)),
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Expr> {
        let start = self.span();
        let op = match self.tok() {
            Some(Tok::Bang) => UnOp::Not,
            Some(Tok::Minus) => UnOp::Neg,
            _ => return self.postfix(),
        };
        self.bump();
        let operand = self.unary()?;
        Ok(Expr {
            span: start.to(operand.span),
            expr: Expression::Unary(op, Box::new(operand)),
        })
    }

    /// Suffixes bind tightest and chain, so each one takes what is built so far
    /// and hands back the larger expression: `a.b[k].f(x)`.
    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.atomic_expr()?;
        loop {
            e = match self.tok() {
                Some(Tok::Dot) => self.field_or_method(e)?,
                Some(Tok::LBracket) => self.index(e)?,
                Some(Tok::LParen) => self.call(e)?,
                _ => return Ok(e),
            };
        }
    }

    /// `e.name`, or `e.name(args)` when a paren follows.
    fn field_or_method(&mut self, e: Expr) -> PResult<Expr> {
        self.bump(); // `.`
        let name = self.ident("a field or method name")?;
        if !self.at(&Tok::LParen) {
            return Ok(Expr {
                span: e.span.to(name.span),
                expr: Expression::Field(Box::new(e), name),
            });
        }
        let open = self.span();
        self.bump(); // `(`
        let args = self.call_args(open)?;
        Ok(Expr {
            span: e.span.to(self.prev_end()),
            expr: Expression::Method(Box::new(e), name, args),
        })
    }

    /// `m[k]`, and `m[a, b]` as sugar for the tuple key `m[(a, b)]`.
    fn index(&mut self, e: Expr) -> PResult<Expr> {
        let open = self.span();
        self.bump(); // `[`
        let index = self.tuple_tail(self.span())?;
        let end = self.expect_close(&Tok::RBracket, "`]`", open)?;
        Ok(Expr {
            span: e.span.to(end),
            expr: Expression::Index(Box::new(e), Box::new(index)),
        })
    }

    /// `f(args)`, where `f` is whatever the suffix is applied to.
    fn call(&mut self, e: Expr) -> PResult<Expr> {
        let open = self.span();
        self.bump(); // `(`
        let args = self.call_args(open)?;
        Ok(Expr {
            span: e.span.to(self.prev_end()),
            expr: Expression::Call(Box::new(e), args),
        })
    }

    fn call_args(&mut self, open: Span) -> PResult<Vec<Expr>> {
        let args = self.comma_separated(&Tok::RParen, open, "`)`", Parser::expr)?;
        self.expect_close(&Tok::RParen, "`)`", open)?;
        Ok(args)
    }

    /// One expression, or several comma-separated collapsed into a tuple.
    fn tuple_tail(&mut self, start: Span) -> PResult<Expr> {
        let first = self.expr()?;
        if !self.at(&Tok::Comma) {
            return Ok(first);
        }
        let mut parts = vec![first];
        while self.eat(&Tok::Comma) {
            if self.at(&Tok::RParen) || self.at(&Tok::RBracket) {
                break;
            }
            parts.push(self.expr()?);
        }
        Ok(Expr {
            expr: Expression::Tuple(parts),
            span: start.to(self.prev_end()),
        })
    }

    fn atomic_expr(&mut self) -> PResult<Expr> {
        let start = self.span();
        let expr = match self.tok() {
            Some(&Tok::Int(n)) => {
                self.bump();
                Expression::Int(n)
            }
            Some(Tok::Str(s)) => {
                let s = s.clone();
                self.bump();
                Expression::Str(s)
            }
            Some(Tok::True) => {
                self.bump();
                Expression::Bool(true)
            }
            Some(Tok::False) => {
                self.bump();
                Expression::Bool(false)
            }
            Some(Tok::If) => return self.if_expr(start),
            Some(Tok::LParen) => return self.paren_expr(start),
            Some(Tok::LBracket) => return self.list_expr(start),
            Some(Tok::Ident(_)) => return self.name_expr(),
            _ => return Err(self.expected("an expression")),
        };
        Ok(Expr { expr, span: start })
    }

    /// `(a)` groups without making a tuple; only a comma makes one.
    fn paren_expr(&mut self, start: Span) -> PResult<Expr> {
        self.bump(); // `(`
        let inner = self.tuple_tail(start)?;
        let end = self.expect_close(&Tok::RParen, "`)`", start)?;
        Ok(Expr {
            span: start.to(end),
            expr: inner.expr,
        })
    }

    /// `[]`, `["a", "b"]`
    fn list_expr(&mut self, start: Span) -> PResult<Expr> {
        self.bump(); // `[`
        let items = self.comma_separated(&Tok::RBracket, start, "`]`", Parser::expr)?;
        let end = self.expect_close(&Tok::RBracket, "`]`", start)?;
        Ok(Expr {
            span: start.to(end),
            expr: Expression::List(items),
        })
    }

    /// A name, and the one place a struct literal can start.
    fn name_expr(&mut self) -> PResult<Expr> {
        let name = self.ident("an expression")?;
        if self.at_struct_lit() {
            return self.struct_lit(name);
        }
        Ok(Expr {
            span: name.span,
            expr: Expression::Ident(name.name),
        })
    }

    /// Whether `name {` starts a literal rather than a name followed by a block.
    ///
    /// A named field says so in two tokens. Positional fields are told by their
    /// commas, which no block can have at its own level — a statement's commas are
    /// always inside parens or brackets.
    ///
    /// So a one-field positional literal is written `KV { v, }`, as a one-element
    /// tuple is `(v,)`. That comma is what buys `{ expr }` back as a block, and
    /// with it an `if` used as a value — including as a field: `KV { a: if c { 1 }
    /// else { 2 } }` needs `{ 1 }` to be an arm and not a literal of its own.
    fn at_struct_lit(&self) -> bool {
        if !self.at(&Tok::LBrace) {
            return false;
        }
        if matches!(self.nth(1), Some(Tok::Ident(_))) && matches!(self.nth(2), Some(Tok::Colon)) {
            return true;
        }
        // scanning the group, not the file: this stops at its own `}`
        let mut depth = 0usize;
        let mut n = 1;
        while let Some(t) = self.nth(n) {
            match t {
                Tok::LBrace | Tok::LParen | Tok::LBracket => depth += 1,
                Tok::RBrace if depth == 0 => return false,
                Tok::RBrace | Tok::RParen | Tok::RBracket => depth = depth.saturating_sub(1),
                Tok::Comma if depth == 0 => return true,
                _ => {}
            }
            n += 1;
        }
        false
    }

    /// Whether the cursor is on `name :`, which only a named field can be.
    fn at_named_field(&self) -> bool {
        matches!(self.tok(), Some(Tok::Ident(_))) && matches!(self.nth(1), Some(Tok::Colon))
    }

    /// `Ev { kind: EXEC, pid: linux.pid }`, or `Ev { EXEC, linux.pid }` to give
    /// the fields in declaration order.
    ///
    /// The first field decides which of the two the whole literal is, so a name
    /// is either required of every field or of none.
    fn struct_lit(&mut self, name: Ident) -> PResult<Expr> {
        let open = self.span();
        self.bump(); // `{`
        let init = match self.struct_init(open) {
            Ok(init) => init,
            Err(e) => {
                self.skip_past(&Tok::RBrace);
                return Err(e);
            }
        };
        let end = self.expect_close(&Tok::RBrace, "`}`", open)?;
        Ok(Expr {
            span: name.span.to(end),
            expr: Expression::StructLit(name, init),
        })
    }

    fn struct_init(&mut self, open: Span) -> PResult<StructInit> {
        if self.at_named_field() {
            let fields = self.comma_separated(&Tok::RBrace, open, "`}`", |p| {
                let f = p.ident("a field name")?;
                p.expect(&Tok::Colon, "`:`")?;
                Ok((f, p.expr()?))
            })?;
            return Ok(StructInit::Named(fields));
        }
        let values = self.comma_separated(&Tok::RBrace, open, "`}`", |p| {
            let value = p.expr()?;
            if p.at(&Tok::Colon) {
                return Err(Error::new(
                    "named and positional fields cannot be mixed",
                    p.span(),
                ));
            }
            Ok(value)
        })?;
        Ok(StructInit::Positional(values))
    }

    /// `if c { a } else { b }` as a value
    fn if_expr(&mut self, start: Span) -> PResult<Expr> {
        self.bump(); // `if`
        let cond = self.expr()?;
        let open = self.expect(&Tok::LBrace, "`{`")?;
        let then = self.expr()?;
        self.expect_close(&Tok::RBrace, "`}`", open)?;
        self.expect(&Tok::Else, "`else`")?;
        let open = self.expect(&Tok::LBrace, "`{`")?;
        let els = self.expr()?;
        let end = self.expect_close(&Tok::RBrace, "`}`", open)?;
        Ok(Expr {
            span: start.to(end),
            expr: Expression::If(Box::new(cond), Box::new(then), Box::new(els)),
        })
    }
}
