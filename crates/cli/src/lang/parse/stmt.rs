use super::{PResult, Parser};
use crate::lang::lex::Tok;
use traqilet_lang::Error;
use traqilet_lang::ast::*;

impl Parser {
    /// `{ .. }`, recovering statement by statement so one bad statement does not
    /// cost the rest of the block.
    pub(super) fn block(&mut self) -> PResult<Block> {
        let open = self.expect(&Tok::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        while !self.at(&Tok::RBrace) && self.tok().is_some() {
            self.process_one_stmt(&mut stmts);
        }
        let end = self.expect_close(&Tok::RBrace, "`}`", open)?;
        Ok(Block {
            stmts,
            span: open.to(end),
        })
    }

    fn process_one_stmt(&mut self, stmts: &mut Vec<Stmt>) {
        match self.stmt() {
            Ok(s) => stmts.push(s),
            Err(e) => {
                self.emit(e);
                self.sync_to_stmt_end();
            }
        }
    }

    pub(super) fn stmt(&mut self) -> PResult<Stmt> {
        let start = self.span();
        let stmt = match self.tok() {
            Some(Tok::If) => self.if_stmt()?,
            Some(Tok::For) => self.for_stmt()?,
            Some(Tok::While) => self.while_stmt()?,
            Some(Tok::Break) => self.break_stmt()?,
            Some(Tok::Continue) => self.continue_stmt()?,
            Some(Tok::Return) => self.return_stmt()?,
            _ => self.assign_or_call()?,
        };
        Ok(Stmt {
            stmt,
            span: start.to(self.prev_end()),
        })
    }

    fn if_stmt(&mut self) -> PResult<Statement> {
        self.bump(); // `if`
        let cond = self.expr()?;
        let then = self.block()?;
        if !self.eat(&Tok::Else) {
            return Ok(Statement::If {
                cond,
                then,
                els: None,
            });
        }
        let els = if self.at(&Tok::If) {
            let start = self.span();
            let inner = self.if_stmt()?;
            let span = start.to(self.prev_end());
            Some(Block {
                stmts: vec![Stmt { stmt: inner, span }],
                span,
            })
        } else {
            Some(self.block()?)
        };
        Ok(Statement::If { cond, then, els })
    }

    fn for_stmt(&mut self) -> PResult<Statement> {
        self.bump(); // `for`
        let pat = self.pat()?;
        self.expect(&Tok::In, "`in`")?;
        let iter = self.expr()?;
        let body = self.block()?;
        Ok(Statement::For { pat, iter, body })
    }

    fn while_stmt(&mut self) -> PResult<Statement> {
        self.bump(); // `while`
        let cond = self.expr()?;
        let body = self.block()?;
        Ok(Statement::While { cond, body })
    }

    fn break_stmt(&mut self) -> PResult<Statement> {
        self.bump(); // `break`
        self.expect_semi()?;
        Ok(Statement::Break)
    }

    fn continue_stmt(&mut self) -> PResult<Statement> {
        self.bump(); // `continue`
        self.expect_semi()?;
        Ok(Statement::Continue)
    }

    /// `return;` leaves with no value, `return e;` with one. The `;` is required
    /// either way, as for every other statement — `expect_semi` below is not
    /// conditional, and `return` with none is an error.
    ///
    /// `ends_here` decides only whether a value follows, not whether the `;` may
    /// be dropped. At `}` or end of input there is no value to read, so the
    /// missing `;` gets reported as itself; without the test, `expr` would run
    /// there and demand an expression, naming a mistake the writer did not make.
    fn return_stmt(&mut self) -> PResult<Statement> {
        self.bump(); // `return`
        let ends_here = self.at(&Tok::Semi) || self.at(&Tok::RBrace) || self.tok().is_none();
        let value = if ends_here { None } else { Some(self.expr()?) };
        self.expect_semi()?;
        Ok(Statement::Return { value })
    }

    /// `x = 1;` binds, `counts[k] += 1;` updates, and `info(x);` calls for effect.
    fn assign_or_call(&mut self) -> PResult<Statement> {
        let target = self.expr()?;
        let op = match self.tok() {
            Some(Tok::Eq) => None,
            Some(Tok::PlusEq) => Some(BinOp::Add),
            Some(Tok::MinusEq) => Some(BinOp::Sub),
            _ => {
                if !matches!(target.expr, Expression::Call(..) | Expression::Method(..)) {
                    return Err(Error::new("this expression has no effect", target.span));
                }
                self.expect_semi()?;
                return Ok(Statement::Call(target));
            }
        };
        self.bump();
        let value = self.expr()?;
        self.expect_semi()?;
        Ok(Statement::Assign { target, op, value })
    }

    /// `x`, or `(dev, rw)` to unpack a tuple.
    fn pat(&mut self) -> PResult<Pat> {
        if !self.at(&Tok::LParen) {
            let name = self.ident("a loop variable")?;
            return Ok(Pat {
                span: name.span,
                pat: Pattern::Name(name),
            });
        }

        let start = self.span();
        self.bump(); // `(`
        let names =
            self.comma_separated(&Tok::RParen, start, "`)`", |p| p.ident("a loop variable"))?;
        let end = self.expect_close(&Tok::RParen, "`)`", start)?;
        Ok(Pat {
            pat: Pattern::Tuple(names),
            span: start.to(end),
        })
    }

    /// To the end of the offending statement: the next `;`, or the `}` closing
    /// the block. Nested groups are stepped over whole, so a brace *inside* the
    /// bad statement is not mistaken for the end of the block.
    fn sync_to_stmt_end(&mut self) {
        let mut depth = 0usize;
        while let Some(t) = self.tok() {
            match t {
                Tok::RBrace if depth == 0 => return,
                Tok::Semi if depth == 0 => {
                    self.bump();
                    return;
                }
                Tok::LBrace | Tok::LParen | Tok::LBracket => depth += 1,
                Tok::RBrace | Tok::RParen | Tok::RBracket => depth = depth.saturating_sub(1),
                _ => {}
            }
            self.bump();
        }
    }
}
