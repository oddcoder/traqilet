//! Tokens, with byte spans.

use logos::{FilterResult, Logos};
use std::{fmt, str::Chars};
use traqilet_lang::{Error, Span};

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(error = String)]
pub enum Tok {
    #[regex(r"\s+")]
    Whitespace,
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*", |lex| lex.slice().to_owned())]
    Ident(String),
    #[regex(r"[0-9][0-9_]*", decimal)]
    #[regex(r"0[xX][0-9a-fA-F_]*", hex)]
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*", float)]
    Int(u64),
    #[regex(r#""([^"\\\n]|\\.)*""#, string)]
    #[regex(r#""([^"\\\n]|\\.)*\\?"#, unterminated)]
    Str(String),
    /// `#!/usr/bin/env traqilet`.
    #[regex(r"#![^\n]*", |lex| lex.slice().to_owned(), allow_greedy = true)]
    Shebang(String),
    #[regex(r"//[^\n]*", allow_greedy = true)]
    LineComment,
    #[token("/*", block_comment)]
    BlockComment,
    /// `key in map`, and the separator in `for x in xs`.
    #[token("in")]
    In,
    /// Marks a declaration whose implementation is elsewhere.
    #[token("extern")]
    Extern,
    #[token("const")]
    Const,
    #[token("struct")]
    Struct,
    #[token("fn")]
    Fn,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("for")]
    For,
    #[token("while")]
    While,
    #[token("return")]
    Return,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token(",")]
    Comma,
    #[token(";")]
    Semi,
    #[token(":")]
    Colon,
    #[token(".")]
    Dot,
    #[token("..")]
    DotDot,
    #[token("#")]
    Hash,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("!")]
    Bang,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("<=")]
    Le,
    #[token(">=")]
    Ge,
    #[token("=")]
    Eq,
    #[token("==")]
    EqEq,
    #[token("!=")]
    Ne,
    #[token("+=")]
    PlusEq,
    /// The return type of a declaration: `fn hashmap.get(key: K) -> V;`
    #[token("->")]
    Arrow,
    #[token("-=")]
    MinusEq,
    #[token("<<")]
    Shl,
    #[token(">>")]
    Shr,
    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,

    /// A lexing failure, kept in the stream so the parser can continue past one
    /// and can tell that this position was already reported.
    Error,
}

impl Tok {
    /// Comments and white spaces
    pub fn is_trivia(&self) -> bool {
        matches!(self, Tok::Whitespace | Tok::LineComment | Tok::BlockComment)
    }
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Tok::Ident(s) => return write!(f, "{s}"),
            Tok::Int(n) => return write!(f, "{n}"),
            Tok::Str(s) => return write!(f, "{s:?}"),
            Tok::Whitespace => " ",
            Tok::Shebang(s) => return write!(f, "{s}"),
            Tok::LineComment => "//",
            Tok::BlockComment => "/*",
            Tok::In => "in",
            Tok::Extern => "extern",
            Tok::Const => "const",
            Tok::Struct => "struct",
            Tok::Fn => "fn",
            Tok::If => "if",
            Tok::Else => "else",
            Tok::For => "for",
            Tok::While => "while",
            Tok::Return => "return",
            Tok::Break => "break",
            Tok::Continue => "continue",
            Tok::True => "true",
            Tok::False => "false",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::LBracket => "[",
            Tok::RBracket => "]",
            Tok::LBrace => "{",
            Tok::RBrace => "}",
            Tok::Comma => ",",
            Tok::Semi => ";",
            Tok::Colon => ":",
            Tok::Dot => ".",
            Tok::DotDot => "..",
            Tok::Hash => "#",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::Percent => "%",
            Tok::Amp => "&",
            Tok::Pipe => "|",
            Tok::Caret => "^",
            Tok::Bang => "!",
            Tok::Lt => "<",
            Tok::Gt => ">",
            Tok::Le => "<=",
            Tok::Ge => ">=",
            Tok::Eq => "=",
            Tok::EqEq => "==",
            Tok::Ne => "!=",
            Tok::PlusEq => "+=",
            Tok::Arrow => "->",
            Tok::MinusEq => "-=",
            Tok::Shl => "<<",
            Tok::Shr => ">>",
            Tok::AndAnd => "&&",
            Tok::OrOr => "||",
            Tok::Error => "<error>",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

type Lexer<'a> = logos::Lexer<'a, Tok>;

fn decimal(lex: &Lexer) -> Result<u64, String> {
    lex.slice()
        .replace('_', "")
        .parse()
        .map_err(|_| "integer literal does not fit in 64 bits".to_owned())
}

fn hex(lex: &Lexer) -> Result<u64, String> {
    let digits = lex.slice()[2..].replace('_', "");
    if digits.is_empty() {
        return Err("hex literal has no digits".to_owned());
    }
    u64::from_str_radix(&digits, 16).map_err(|_| "hex literal does not fit in 64 bits".to_owned())
}

fn float(_: &Lexer) -> Result<u64, String> {
    Err("floating point is not supported".to_owned())
}

/// `\xNN`: exactly two hex digits.
///
/// The value is taken as a byte, so this reaches `\xFF` and lands in Latin-1.
/// Rust itself stops at `\x7F` and makes you write `\u{..}` above that; tql is
/// deliberately more permissive, because a traced byte is often not text.
fn hex_escape(chars: &mut Chars<'_>) -> Result<char, String> {
    let mut n = 0u32;
    for _ in 0..2 {
        let h = chars
            .next()
            .and_then(|c| c.to_digit(16))
            .ok_or_else(|| "`\\x` needs two hex digits".to_owned())?;
        n = n * 16 + h;
    }
    Ok(n as u8 as char)
}

/// `\u{..}`: braces around at least one hex digit, naming a character.
fn unicode_escape(chars: &mut Chars<'_>) -> Result<char, String> {
    if chars.next() != Some('{') {
        return Err("`\\u` needs braces, as in `\\u{1f600}`".to_owned());
    }
    let mut n = 0u32;
    let mut any = false;
    let mut closed = false;
    for c in chars.by_ref() {
        if c == '}' {
            closed = true;
            break;
        }
        let h = c
            .to_digit(16)
            .ok_or_else(|| "malformed `\\u{..}` escape".to_owned())?;
        any = true;
        n = n * 16 + h;
    }
    if !any || !closed {
        return Err("malformed `\\u{..}` escape".to_owned());
    }
    char::from_u32(n).ok_or_else(|| "`\\u{..}` is not a character".to_owned())
}

/// Unescapes the body of a closed string literal.
fn string(lex: &Lexer) -> Result<String, String> {
    let raw = lex.slice();
    let body = &raw[1..raw.len() - 1];
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        // the regex guarantees a character follows the backslash
        let esc = chars.next().expect("regex admits no trailing backslash");
        let decoded = match esc {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '0' => '\0',
            '\\' => '\\',
            '"' => '"',
            '\'' => '\'',
            'x' => hex_escape(&mut chars)?,
            'u' => unicode_escape(&mut chars)?,
            other => return Err(format!("unknown escape `\\{other}`")),
        };
        out.push(decoded);
    }
    Ok(out)
}

/// A quote with no closing quote before the end of the line.
fn unterminated(lex: &Lexer) -> FilterResult<String, String> {
    let msg = if lex.slice().ends_with('\\') {
        "string ends after a backslash"
    } else {
        "string is not terminated"
    };
    FilterResult::Error(msg.to_owned())
}

/// Consumes a `/* .. */` comment, honouring nesting.
fn block_comment(lex: &mut Lexer) -> FilterResult<(), String> {
    let rest = lex.remainder().as_bytes();
    let mut depth = 1usize;
    let mut i = 0usize;
    while i + 1 < rest.len() {
        match (rest[i], rest[i + 1]) {
            (b'/', b'*') => {
                depth += 1;
                i += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    // always just past `*/`, so never inside a character
                    lex.bump(i);
                    return FilterResult::Emit(());
                }
            }
            _ => i += 1,
        }
    }
    lex.bump(rest.len());
    FilterResult::Error("block comment is not terminated".to_owned())
}

/// Lexes the whole source. Errors are reported *and* represented in the stream
/// as [`Tok::Error`], so a caller can keep parsing past one.
pub fn lex(src: &str) -> (Vec<Token>, Vec<Error>) {
    let mut toks = Vec::new();
    let mut errs = Vec::new();

    for (result, range) in Tok::lexer(src).spanned() {
        let span = Span::new(range.start, range.end);
        let tok = match result {
            Ok(tok) => tok,
            Err(e) => {
                let msg = if e.is_empty() {
                    let c = src[range].chars().next().unwrap_or('\0');
                    format!("unexpected character `{c}`")
                } else {
                    e
                };
                errs.push(Error::new(msg, span));
                Tok::Error
            }
        };
        toks.push(Token { tok, span });
    }
    (toks, errs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tokens the grammar sees: trivia filtered, as [`super::parse`] does.
    fn toks(src: &str) -> Vec<Tok> {
        all_toks(src)
            .into_iter()
            .filter(|t| !t.is_trivia())
            .collect()
    }

    /// Every token, trivia included, for the tests that are about fidelity.
    fn all_toks(src: &str) -> Vec<Tok> {
        let (toks, errs) = lex(src);
        assert!(errs.is_empty(), "{errs:?}");
        toks.into_iter().map(|t| t.tok).collect()
    }

    fn errs(src: &str) -> Vec<String> {
        let (toks, errs) = lex(src);
        assert_eq!(
            toks.iter().filter(|t| t.tok == Tok::Error).count(),
            errs.len(),
            "every error should leave one Error token: {toks:?}"
        );
        errs.into_iter().map(|e| e.msg).collect()
    }

    fn ident(s: &str) -> Tok {
        Tok::Ident(s.to_owned())
    }

    #[test]
    fn keywords_are_not_idents() {
        assert_eq!(toks("struct Ev"), [Tok::Struct, ident("Ev")]);
        assert_eq!(toks("fn f"), [Tok::Fn, ident("f")]);
        assert_eq!(toks("delete"), [ident("delete")]);
    }

    #[test]
    fn the_trimmed_words_are_ordinary_names() {
        for word in ["static", "param", "let", "record"] {
            assert_eq!(toks(word), [ident(word)], "{word}");
        }
    }

    #[test]
    fn the_reserved_set_is_closed() {
        let reserved = [
            ("extern", Tok::Extern),
            ("const", Tok::Const),
            ("in", Tok::In),
            ("struct", Tok::Struct),
            ("fn", Tok::Fn),
            ("if", Tok::If),
            ("else", Tok::Else),
            ("for", Tok::For),
            ("while", Tok::While),
            ("return", Tok::Return),
            ("break", Tok::Break),
            ("continue", Tok::Continue),
            ("true", Tok::True),
            ("false", Tok::False),
        ];
        for (word, tok) in reserved {
            assert_eq!(toks(word), [tok], "{word}");
        }
    }

    #[test]
    fn a_name_that_begins_with_a_keyword_is_one_name() {
        for name in [
            "staticx",
            "constant",
            "fnord",
            "iff",
            "elsewhere",
            "informal",
            "returns",
            "inner",
            "index",
            "forall",
            "whilst",
            "records",
        ] {
            assert_eq!(toks(name), [ident(name)], "{name}");
        }
        assert_eq!(
            toks("_if if_ if2"),
            [ident("_if"), ident("if_"), ident("if2")]
        );
    }

    #[test]
    fn block_comment_edges() {
        // `/*/` is not a closed comment: the `/` cannot serve as both
        assert_eq!(errs("/*/"), ["block comment is not terminated"]);
        assert_eq!(toks("/**/ a"), [ident("a")]);
        assert_eq!(toks("/* * / ** */ a"), [ident("a")]);
        // an inner opener must be matched before the outer one closes
        assert_eq!(errs("/* /* */"), ["block comment is not terminated"]);
        // multi-byte content inside a comment must not desynchronise the scan
        assert_eq!(toks("/* héllo ünicode */ a"), [ident("a")]);
    }

    #[test]
    fn a_string_ending_in_a_backslash_says_so() {
        assert_eq!(errs(r#""a\"#), ["string ends after a backslash"]);
    }

    #[test]
    fn comment_spans_cover_exactly_the_comment() {
        for (src, kind, text) in [
            ("a // to the end", Tok::LineComment, "// to the end"),
            ("a //", Tok::LineComment, "//"),
            ("a /* inner */ b", Tok::BlockComment, "/* inner */"),
            (
                "a /* /* nested */ */ b",
                Tok::BlockComment,
                "/* /* nested */ */",
            ),
            ("a /* ünicode */ b", Tok::BlockComment, "/* ünicode */"),
        ] {
            let (toks, errs) = lex(src);
            assert!(errs.is_empty(), "{src}: {errs:?}");
            let t = toks
                .iter()
                .find(|t| t.tok == kind)
                .unwrap_or_else(|| panic!("{src}: no {kind:?} in {toks:?}"));
            assert_eq!(&src[t.span.lo()..t.span.hi()], text, "{src}");
        }
    }

    #[test]
    fn only_spacing_and_comments_are_trivia() {
        for t in [Tok::Whitespace, Tok::LineComment, Tok::BlockComment] {
            assert!(t.is_trivia(), "{t:?}");
        }
        for t in [Tok::Fn, Tok::Semi, ident("x"), Tok::Int(1), Tok::Error] {
            assert!(!t.is_trivia(), "{t:?}");
        }
    }

    #[test]
    fn a_shebang_is_line_one_only() {
        let src = "#!/usr/bin/env traqilet\nx = 1;\n";
        let (t, errs) = lex(src);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(t[0].tok, Tok::Shebang("#!/usr/bin/env traqilet".to_owned()));
        assert_eq!(
            &src[t[0].span.lo()..t[0].span.hi()],
            "#!/usr/bin/env traqilet"
        );
        assert!(!t[0].tok.is_trivia());

        assert_eq!(
            all_toks("x\n#!/oops"),
            [
                ident("x"),
                Tok::Whitespace,
                Tok::Shebang("#!/oops".to_owned())
            ]
        );
    }

    #[test]
    fn a_shebang_alone_is_not_an_error() {
        assert_eq!(
            all_toks("#!/bin/traqilet"),
            [Tok::Shebang("#!/bin/traqilet".to_owned())]
        );
        assert_eq!(all_toks("#!"), [Tok::Shebang("#!".to_owned())]);
    }

    #[test]
    fn tokens_tile_the_whole_source() {
        let corpus: Vec<String> = std::fs::read_dir(crate::lang::EXAMPLES_DIR)
            .unwrap()
            .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
            .collect();
        let tricky = [
            String::new(),
            "   ".to_owned(),
            "// only a comment".to_owned(),
            "/* just a block */".to_owned(),
            "a/*b*/c // d\n\n\ne".to_owned(),
            "\u{3000}x\u{a0}=\u{2000}1;".to_owned(),
        ];
        for src in corpus.iter().chain(tricky.iter()) {
            let (toks, errs) = lex(src);
            assert!(errs.is_empty(), "{src:?}: {errs:?}");
            let mut at = 0;
            for t in &toks {
                assert_eq!(t.span.lo(), at, "gap or overlap before {t:?} in {src:?}");
                at = t.span.hi();
            }
            assert_eq!(at, src.len(), "stream stops short in {src:?}");
        }
    }

    #[test]
    fn spans_are_byte_ranges_not_char_offsets() {
        // "é" is two bytes, so a char-counting lexer would report 8 here
        let src = "\"é\" x";
        let (toks, _) = lex(src);
        let x = toks
            .iter()
            .find(|t| t.tok == ident("x"))
            .expect("no `x` in the stream");
        assert_eq!(x.span, Span::new(5, 6));
        // and the span must slice the source without panicking
        assert_eq!(&src[x.span.lo()..x.span.hi()], "x");
    }

    #[test]
    fn integers() {
        assert_eq!(
            toks("0 7 1_000_000"),
            [Tok::Int(0), Tok::Int(7), Tok::Int(1_000_000)]
        );
        assert_eq!(toks("0xff 0xFF_FF"), [Tok::Int(255), Tok::Int(65535)]);
    }

    #[test]
    fn a_minus_is_always_an_operator() {
        assert_eq!(toks("-5"), [Tok::Minus, Tok::Int(5)]);
        assert_eq!(toks("a-5"), [ident("a"), Tok::Minus, Tok::Int(5)]);
        assert_eq!(toks("a - 5"), [ident("a"), Tok::Minus, Tok::Int(5)]);
        assert_eq!(
            toks("-9223372036854775808"),
            [Tok::Minus, Tok::Int(9_223_372_036_854_775_808)]
        );
    }

    #[test]
    fn rejects_floats() {
        assert_eq!(errs("let x = 1.5;"), ["floating point is not supported"]);
        assert_eq!(toks("0..20"), [Tok::Int(0), Tok::DotDot, Tok::Int(20)]);
        assert_eq!(
            toks("1.max(2)"),
            [
                Tok::Int(1),
                Tok::Dot,
                ident("max"),
                Tok::LParen,
                Tok::Int(2),
                Tok::RParen
            ]
        );
    }

    #[test]
    fn rejects_out_of_range_integers() {
        assert_eq!(
            errs("18446744073709551616"),
            ["integer literal does not fit in 64 bits"]
        );
        assert_eq!(
            errs("0x1_0000_0000_0000_0000"),
            ["hex literal does not fit in 64 bits"]
        );
        assert_eq!(errs("0x"), ["hex literal has no digits"]);
    }

    #[test]
    fn escapes() {
        assert_eq!(
            toks(r#""a\nb\t\0\\\"\x41\u{1f600}""#),
            [Tok::Str("a\nb\t\0\\\"A\u{1f600}".to_owned())]
        );
    }

    #[test]
    fn rejects_bad_escapes() {
        assert_eq!(errs(r#""\q""#), ["unknown escape `\\q`"]);
        assert_eq!(errs(r#""\x4""#), ["`\\x` needs two hex digits"]);
        assert_eq!(
            errs(r#""\u41""#),
            ["`\\u` needs braces, as in `\\u{1f600}`"]
        );
        assert_eq!(errs(r#""\u{}""#), ["malformed `\\u{..}` escape"]);
        assert_eq!(errs(r#""\u{d800}""#), ["`\\u{..}` is not a character"]);
    }

    #[test]
    fn unterminated_string_stops_at_the_newline() {
        let (toks, errs) = lex("let a = \"oops\nlet b = 2;");
        assert_eq!(
            errs.iter().map(|e| e.msg.as_str()).collect::<Vec<_>>(),
            ["string is not terminated"]
        );
        assert!(toks.contains(&Token {
            tok: ident("b"),
            span: Span::new(18, 19)
        }));
    }

    #[test]
    fn comments() {
        assert_eq!(toks("a // b\nc"), [ident("a"), ident("c")]);
        assert_eq!(
            all_toks("a // b\nc"),
            [
                ident("a"),
                Tok::Whitespace,
                Tok::LineComment,
                Tok::Whitespace,
                ident("c")
            ]
        );
        assert_eq!(toks("a /* b /* c */ d */ e"), [ident("a"), ident("e")]);
        assert_eq!(
            all_toks("a /* b /* c */ d */ e"),
            [
                ident("a"),
                Tok::Whitespace,
                Tok::BlockComment,
                Tok::Whitespace,
                ident("e")
            ]
        );
        assert_eq!(errs("a /* b"), ["block comment is not terminated"]);
    }

    /// Longest match wins, so a two-character operator never lexes as two
    /// one-character ones.
    #[test]
    fn operators_prefer_the_longer_match() {
        assert_eq!(toks("= =="), [Tok::Eq, Tok::EqEq]);
        assert_eq!(toks(". .."), [Tok::Dot, Tok::DotDot]);
        assert_eq!(toks("< << <="), [Tok::Lt, Tok::Shl, Tok::Le]);
        assert_eq!(toks("& &&"), [Tok::Amp, Tok::AndAnd]);
        assert_eq!(
            toks("+ += - -> -="),
            [Tok::Plus, Tok::PlusEq, Tok::Minus, Tok::Arrow, Tok::MinusEq]
        );
    }

    #[test]
    fn a_stray_multi_byte_character_yields_a_boundary_span() {
        for bad in ["\u{20ac}", "\u{ab}", "\u{1f600}", "\u{e9}"] {
            let src = format!("let a = {bad} b;");
            let (toks, errs) = lex(&src);
            assert_eq!(errs.len(), 1, "{src}: {errs:?}");
            let span = errs[0].span;
            assert!(
                src.is_char_boundary(span.lo()) && src.is_char_boundary(span.hi()),
                "{src}: span {span:?} is not on character boundaries"
            );
            assert_eq!(
                &src[span.lo()..span.hi()],
                bad,
                "{src}: span covered the wrong text"
            );
            assert_eq!(errs[0].msg, format!("unexpected character `{bad}`"));
            assert!(toks.iter().any(|t| t.tok == ident("b")), "{src}: {toks:?}");
        }
    }

    #[test]
    fn unexpected_character_is_reported_once_and_scanning_continues() {
        let (toks, errs) = lex("a $ b");
        assert_eq!(
            errs.iter().map(|e| e.msg.as_str()).collect::<Vec<_>>(),
            ["unexpected character `$`"]
        );
        let code: Vec<Tok> = toks
            .into_iter()
            .map(|t| t.tok)
            .filter(|t| !t.is_trivia())
            .collect();
        assert_eq!(code, [ident("a"), Tok::Error, ident("b")]);
    }
}
