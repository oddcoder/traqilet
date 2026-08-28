use super::{PResult, Parser};
use crate::lang::lex::Tok;
use traqilet_lang::ast::*;

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

    /// `[str(128)]`: a growable list, host side only.
    fn list_ty(&mut self) -> PResult<Type> {
        let open = self.span();
        self.bump();
        let inner = self.ty()?;
        self.expect_close(&Tok::RBracket, "`]`", open)?;
        Ok(Type {
            ty: Ty::List(Box::new(inner)),
            span: open.to(self.prev_end()),
        })
    }

    /// `(16)`, `(ARGSIZE)`: the arguments of an applied type.
    fn ty_args(&mut self) -> PResult<Vec<TyArg>> {
        let open = self.span();
        self.bump();
        let args = self.comma_separated(&Tok::RParen, open, "`)`", |p| p.ty_arg())?;
        self.expect_close(&Tok::RParen, "`)`", open)?;
        Ok(args)
    }

    /// what `[str(8)]` in `map(u32, [str(8)])` has to.
    fn ty_arg(&mut self) -> PResult<TyArg> {
        if let Some(&Tok::Int(n)) = self.tok() {
            self.bump();
            return Ok(TyArg::Int(n));
        }
        Ok(TyArg::Type(self.ty()?))
    }
}
