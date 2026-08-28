//! The typed AST. Every node carries a byte span.

pub use crate::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// A whole script: the shebang if it had one, then its items.
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    pub shebang: Option<String>,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Global(Global),
    Struct(Struct),
    Fn(Func),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub is_const: bool,
    pub name: Ident,
    pub ty: Option<Type>,
    pub span: Span,
}

/// `lat = hist(64);`, `const MAXARG = 20;`, `#[host] pending = hash(10240, u32, Row);`
#[derive(Clone, Debug, PartialEq)]
pub struct Global {
    pub attrs: Vec<Attr>,
    pub is_const: bool,
    pub name: Ident,
    pub ty: Option<Type>,
    pub init: Expr,
    pub span: Span,
}

/// `struct Io { pid: u32, comm: str(16) }`
#[derive(Clone, Debug, PartialEq)]
pub struct Struct {
    pub attrs: Vec<Attr>,
    pub name: Ident,
    pub params: Vec<Param>,
    pub fields: Vec<(Ident, Type)>,
    pub span: Span,
}

/// `#[kprobe(vfs_read)] fn enter() { .. }`
#[derive(Clone, Debug, PartialEq)]
pub struct Func {
    pub attrs: Vec<Attr>,
    /// The type this is an operation on: `fn hashmap.get(..)`.
    pub recv: Option<Ident>,
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Option<Block>,
    pub span: Span,
}

/// `#[kprobe(vfs_read)]`, `#[interval(secs = 1)]`
#[derive(Clone, Debug, PartialEq)]
pub struct Attr {
    pub name: Ident,
    pub args: Vec<AttrArg>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AttrArg {
    /// `linux.vfs_read`, or `linux.syscalls.sys_enter_execve`
    Path(Vec<Ident>),
    Int(u64),
    Str(String),
    /// `secs = 1`
    Named(Ident, Box<AttrArg>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    pub ty: Ty,
    pub span: Span,
}

/// `Ty` rather than a longer word because rustc names this exact node the same:
/// the syntax of a type, with the position held by [`Type`] around it.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    /// A name and its arguments: `u64` has none, `str(16)` has one.
    Name(Ident, Vec<TyArg>),
    /// `[str(128)]` — a growable list, host side only. Bracketed rather than
    /// named, which is why it is not the case above.
    List(Box<Type>),
}

/// An argument to an applied type.
#[derive(Debug, Clone, PartialEq)]
pub enum TyArg {
    /// `str(16)`, `Slice(u64, B + 5)`
    Index(Expr),
    /// `str(ARGSIZE)`, and any nested type such as the `[str(8)]` in
    /// `map(u32, [str(8)])`.
    Type(Type),
}

/// `{ .. }`. The span covers the braces, which no statement inside it does.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub stmt: Statement,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Assign {
        target: Expr,
        op: Option<BinOp>,
        value: Expr,
    },
    If {
        cond: Expr,
        then: Block,
        els: Option<Block>,
    },
    For {
        pat: Pat,
        iter: Expr,
        body: Block,
    },
    While {
        cond: Expr,
        body: Block,
    },
    Break,
    Continue,
    Return {
        value: Option<Expr>,
    },
    Call(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum StructInit {
    /// `{ kind: EXEC, pid: linux.pid }`
    Named(Vec<(Ident, Expr)>),
    /// `{ EXEC, linux.pid }`, matched against the declaration by position.
    Positional(Vec<Expr>),
}

/// A loop variable: `for cpu in ..`, or `for (dev, rw) in ..` to unpack a tuple
/// key. Assignment needs no pattern — its target is an ordinary expression, so
/// `(dev, rw) = m[k];` unpacks by assigning to a tuple.
#[derive(Clone, Debug, PartialEq)]
pub struct Pat {
    pub pat: Pattern,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Name(Ident),
    Tuple(Vec<Ident>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    /// `key in map`: the presence test. Reading a key that is absent is an
    /// error rather than a zero, so this is how a script asks the question;
    /// silently substituting zero is the bpftrace behaviour we are avoiding.
    In,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Not,
    Neg,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub expr: Expression,
    pub span: Span,
}

/// What an expression *is*, with no position attached.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Int(u64),
    Str(String),
    Bool(bool),
    Ident(String),
    /// `linux.pid`, `row.args`, `rq.q.disk.major`
    Field(Box<Expr>, Ident),
    /// `counts[key]`
    Index(Box<Expr>, Box<Expr>),
    /// `hash(1024)`, `str(p, 128)`
    Call(Box<Expr>, Vec<Expr>),
    /// `lat.add(x)`, `args.join(" ")`
    Method(Box<Expr>, Ident, Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `Ev { kind: EXEC, pid: linux.pid }`, or `Ev { EXEC, linux.pid }`
    StructLit(Ident, StructInit),
    /// `[]`, `["a", "b"]`
    List(Vec<Expr>),
    /// `0..MAXARG`
    Range(Box<Expr>, Box<Expr>),
    /// `if rw == 1 { "write" } else { "read" }` — a value, so both arms are
    /// required and each is a single expression rather than a block.
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `(dev, rw)` — a composite value, used for composite map keys. Distinct
    /// from a list: fixed arity, mixed types, not growable.
    Tuple(Vec<Expr>),
}
