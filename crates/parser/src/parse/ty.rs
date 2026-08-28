use super::{PResult, Parser};
use crate::lex::Tok;
use traqilet_lang::Error;
use traqilet_lang::ast::{Ty, TyArg, Type};

impl Parser {
    pub(super) fn ty(&mut self) -> PResult<Type> {
        if self.at(&Tok::LBracket) {
            return self.list_ty();
        }
        let name = self.ident("a type")?;
        let args = if self.at(&Tok::LParen) {
            self.ty_args()?
        } else {
            Vec::new()
        };
        Ok(Type {
            span: name.span.to(self.prev_end()),
            ty: Ty::Name(name, args),
        })
    }

    /// `[str(128)]`, and `[u64; B + 5]`.
    fn list_ty(&mut self) -> PResult<Type> {
        let open = self.span();
        self.bump();
        let inner = self.ty()?;
        let len = if self.eat(&Tok::Semi) {
            Some(self.index_expr()?)
        } else {
            None
        };
        self.expect_close(&Tok::RBracket, "`]`", open)?;
        Ok(Type {
            ty: Ty::List(Box::new(inner), len),
            span: open.to(self.prev_end()),
        })
    }

    /// `(16)`, `(ARGSIZE)`: the arguments of an applied type.
    fn ty_args(&mut self) -> PResult<Vec<TyArg>> {
        let open = self.span();
        self.bump();
        let args = self.comma_separated(&Tok::RParen, open, "`)`", Parser::ty_arg)?;
        self.expect_close(&Tok::RParen, "`)`", open)?;
        Ok(args)
    }

    /// what `[str(8)]` in `map(u32, [str(8)])` has to.
    fn ty_arg(&mut self) -> PResult<TyArg> {
        if self.at_index() {
            return Ok(TyArg::Index(self.index_expr()?));
        }
        let t = self.ty()?;
        // `str(str(8) + 1)`: the operator wanted a size on its left and found a type
        if self.at_binary_op(0) {
            return Err(Error::new(
                "this is a type, and only a size can be an operand",
                t.span,
            ));
        }
        Ok(TyArg::Type(t))
    }

    /// Whether what comes next is arithmetic rather than a type: a number, a group in
    /// parentheses, or a name with an operator after it.
    fn at_index(&self) -> bool {
        match self.tok() {
            Some(Tok::Int(_) | Tok::LParen) => true,
            Some(Tok::Ident(_)) => self.at_binary_op(1),
            _ => false,
        }
    }
}
