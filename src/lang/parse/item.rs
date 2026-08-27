use super::{PResult, Parser};
use crate::lang::Span;
use crate::lang::ast::*;
use crate::lang::lex::Tok;

impl Parser {
    pub(super) fn script(&mut self) -> Script {
        let shebang = self.shebang();
        let mut items = Vec::new();
        while self.tok().is_some() {
            self.process_one_item(&mut items);
        }
        Script { shebang, items }
    }

    fn shebang(&mut self) -> Option<String> {
        let Some(Tok::Shebang(text)) = self.tok() else {
            return None;
        };
        let text = text.clone();
        self.bump();
        Some(text)
    }

    fn process_one_item(&mut self, items: &mut Vec<Item>) {
        let entry = self.pos;
        match self.item() {
            Ok(item) => items.push(item),
            Err(e) => {
                self.emit(e);
                self.sync_to_item();
                // guarantee progress
                if self.pos == entry {
                    self.bump();
                }
            }
        }
    }

    pub(super) fn item(&mut self) -> PResult<Item> {
        let start = self.span();
        let attrs = self.attrs()?;

        let is_extern = self.eat(&Tok::Extern);
        if self.at(&Tok::Struct) {
            return self.struct_item(attrs, start, is_extern).map(Item::Struct);
        }
        if self.at(&Tok::Fn) {
            return self.fn_item(attrs, start, is_extern).map(Item::Fn);
        }
        if is_extern {
            return Err(self.expected("`fn` or `struct` after `extern`"));
        }
        self.global(attrs, start).map(Item::Global)
    }

    /// `#[name]` and `#[name(args)]`, repeated.
    fn attrs(&mut self) -> PResult<Vec<Attr>> {
        let mut out = Vec::new();
        while self.at(&Tok::Hash) {
            let attr = self.attr()?;
            out.push(attr);
        }
        Ok(out)
    }

    /// `#[name]` and `#[name(args)]`
    fn attr(&mut self) -> PResult<Attr> {
        let start = self.span();
        self.bump(); // # symbol
        let open = self.expect(&Tok::LBracket, "`[` after `#`")?;
        let name = self.ident("an attribute name")?;
        let mut args = Vec::new();
        if self.at(&Tok::LParen) {
            let paren = self.span();
            self.bump();
            args = self.comma_separated(&Tok::RParen, paren, "`)`", |p| p.attr_arg())?;
            self.expect_close(&Tok::RParen, "`)`", paren)?;
        }
        let end = self.expect_close(&Tok::RBracket, "`]`", open)?;
        Ok(Attr {
            name,
            args,
            span: start.to(end),
        })
    }

    fn attr_arg(&mut self) -> PResult<AttrArg> {
        match self.tok() {
            Some(&Tok::Int(n)) => {
                self.bump();
                Ok(AttrArg::Int(n))
            }
            Some(Tok::Str(s)) => {
                let s = s.clone();
                self.bump();
                Ok(AttrArg::Str(s))
            }
            Some(Tok::Ident(_)) => {
                let first = self.ident("an attribute argument")?;
                if self.eat(&Tok::Eq) {
                    return Ok(AttrArg::Named(first, Box::new(self.attr_arg()?)));
                }
                let mut path = vec![first];
                while self.eat(&Tok::Dot) {
                    path.push(self.ident("a path segment")?);
                }
                Ok(AttrArg::Path(path))
            }
            _ => Err(self.expected("an attribute argument")),
        }
    }

    /// `struct Io { pid: u32, comm: str(16) }`
    fn struct_item(&mut self, attrs: Vec<Attr>, start: Span, is_extern: bool) -> PResult<Struct> {
        self.bump(); // `struct`
        let name = self.ident("a name after `struct`")?;
        let params = self.params()?;
        if is_extern {
            self.expect_semi()?;
            return Ok(Struct {
                attrs,
                name,
                params,
                fields: Vec::new(),
                span: start.to(self.prev_end()),
            });
        }
        let open = self.expect(&Tok::LBrace, "`{`")?;
        let fields = self.comma_separated(&Tok::RBrace, open, "`}`", |p| {
            let f = p.ident("a field name")?;
            p.expect(&Tok::Colon, "`:` and a type")?;
            Ok((f, p.ty()?))
        })?;
        let end = self.expect_close(&Tok::RBrace, "`}`", open)?;
        Ok(Struct {
            attrs,
            name,
            params,
            fields,
            span: start.to(end),
        })
    }

    /// `(K, V, const N: size)`, or nothing at all.
    fn params(&mut self) -> PResult<Vec<Param>> {
        if !self.at(&Tok::LParen) {
            return Ok(Vec::new());
        }
        let open = self.span();
        self.bump();
        let params = self.comma_separated(&Tok::RParen, open, "`)`", |p| p.param())?;
        self.expect_close(&Tok::RParen, "`)`", open)?;
        Ok(params)
    }

    /// A bound type (`K`), a bound index (`const N: size`), or a value (`key: K`).
    fn param(&mut self) -> PResult<Param> {
        let start = self.span();
        let is_const = self.eat(&Tok::Const);
        let name = self.ident("a parameter name")?;
        let ty = if self.eat(&Tok::Colon) {
            Some(self.ty()?)
        } else {
            None
        };
        Ok(Param {
            name,
            is_const,
            ty,
            span: start.to(self.prev_end()),
        })
    }

    /// `#[kprobe(vfs_read)] fn enter() { .. }`
    fn fn_item(&mut self, attrs: Vec<Attr>, start: Span, is_extern: bool) -> PResult<Func> {
        self.bump(); // `fn`
        let first = self.ident("a name after `fn`")?;
        // `HashMap.get`: an operation on a type rather than a free function
        let (recv, name) = if self.eat(&Tok::Dot) {
            (Some(first), self.ident("an operation name")?)
        } else {
            (None, first)
        };
        let open = self.expect(&Tok::LParen, "`(`")?;
        let params = self.comma_separated(&Tok::RParen, open, "`)`", |p| p.param())?;
        self.expect_close(&Tok::RParen, "`)`", open)?;
        let ret = if self.eat(&Tok::Arrow) {
            Some(self.ty()?)
        } else {
            None
        };
        let body = if is_extern {
            self.expect_semi()?;
            None
        } else {
            Some(self.block()?)
        };
        Ok(Func {
            attrs,
            recv,
            name,
            params,
            ret,
            body,
            span: start.to(self.prev_end()),
        })
    }

    /// `lat = hist(64);`, `const MAXARG = 20;`, `#[host] p: u32 = 4;`
    fn global(&mut self, attrs: Vec<Attr>, start: Span) -> PResult<Global> {
        let is_const = self.eat(&Tok::Const);
        let name = self.ident("`const`, `struct`, `fn` or a name")?;
        let ty = if self.eat(&Tok::Colon) {
            Some(self.ty()?)
        } else {
            None
        };
        self.expect(&Tok::Eq, "`=` and a value")?;
        let init = self.expr()?;
        self.expect_semi()?;
        let span = start.to(self.prev_end());
        Ok(Global {
            attrs,
            is_const,
            name,
            ty,
            init,
            span,
        })
    }

    /// To the next place an item can start again.
    fn sync_to_item(&mut self) {
        while let Some(t) = self.tok() {
            if matches!(
                t,
                Tok::Hash | Tok::Const | Tok::Struct | Tok::Fn | Tok::Extern
            ) {
                return;
            }
            self.bump();
        }
    }
}
