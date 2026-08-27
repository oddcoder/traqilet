use super::parse;
use crate::lang::Span;
use crate::lang::ast::*;

fn ok(src: &str) -> Vec<Item> {
    let p = parse(src);
    assert!(p.errors.is_empty(), "{src}\n{:#?}", p.errors);
    p.script.items
}

fn errs(src: &str) -> Vec<String> {
    parse(src).errors.into_iter().map(|e| e.msg).collect()
}

fn body(src: &str) -> Vec<Stmt> {
    match ok(&format!("fn f() {{ {src} }}")).into_iter().next() {
        Some(Item::Fn(f)) => f.body.expect("a script fn has a body").stmts,
        other => panic!("expected a fn, got {other:?}"),
    }
}

fn stmt(src: &str) -> Statement {
    let mut b = body(src);
    assert_eq!(b.len(), 1, "expected one statement: {b:?}");
    b.pop().unwrap().stmt
}

fn sexp(e: &Expr) -> String {
    match &e.expr {
        Expression::Int(n) => n.to_string(),
        Expression::Str(s) => format!("{s:?}"),
        Expression::Bool(b) => b.to_string(),
        Expression::Ident(s) => s.clone(),
        Expression::Field(b, n) => format!("(. {} {})", sexp(b), n.name),
        Expression::Index(b, i) => format!("([] {} {})", sexp(b), sexp(i)),
        Expression::Call(f, a) => format!("(call {}{})", sexp(f), args(a)),
        Expression::Method(r, n, a) => format!("(.{} {}{})", n.name, sexp(r), args(a)),
        Expression::Unary(op, b) => format!("({op:?} {})", sexp(b)),
        Expression::Binary(op, l, r) => format!("({op:?} {} {})", sexp(l), sexp(r)),
        Expression::StructLit(n, init) => {
            let fs: Vec<String> = match init {
                StructInit::Named(f) => f
                    .iter()
                    .map(|(k, v)| format!(" {}: {}", k.name, sexp(v)))
                    .collect(),
                StructInit::Positional(vs) => vs.iter().map(|v| format!(" {}", sexp(v))).collect(),
            };
            format!("({}{{{}}})", n.name, fs.join(","))
        }
        Expression::List(a) => format!("(list{})", args(a)),
        Expression::Range(l, r) => format!("(.. {} {})", sexp(l), sexp(r)),
        Expression::If(c, t, e) => format!("(if {} {} {})", sexp(c), sexp(t), sexp(e)),
        Expression::Tuple(a) => format!("(tuple{})", args(a)),
    }
}

fn args(es: &[Expr]) -> String {
    es.iter().map(|e| format!(" {}", sexp(e))).collect()
}

fn expr_of(src: &str) -> String {
    match stmt(&format!("x = {src};")) {
        Statement::Assign { value, .. } => sexp(&value),
        other => panic!("expected an assignment, got {other:?}"),
    }
}

#[test]
fn a_missing_semicolon_points_at_the_end_of_the_statement() {
    let src = "fn f() {\n    y = 2\n    info(\"x\");\n}\n";
    let p = parse(src);
    assert_eq!(p.errors.len(), 1, "{:#?}", p.errors);
    assert_eq!(p.errors[0].msg, "expected `;`");
    let just_past_the_2 = src.find("2\n").unwrap() + 1;
    assert_eq!(p.errors[0].span.lo(), p.errors[0].span.hi());
    assert_eq!(
        p.errors[0].span,
        Span::new(just_past_the_2, just_past_the_2)
    );
}

#[test]
fn a_missing_semicolon_before_a_closing_brace_moves_too() {
    let src = "fn f() {\n    y = 2\n}\n";
    let p = parse(src);
    assert_eq!(p.errors.len(), 1, "{:#?}", p.errors);
    assert_eq!(p.errors[0].msg, "expected `;`");
    let just_past_the_2 = src.find("2\n").unwrap() + 1;
    assert_eq!(p.errors[0].span.lo(), p.errors[0].span.hi());
    assert_eq!(
        p.errors[0].span,
        Span::new(just_past_the_2, just_past_the_2)
    );
}

#[test]
fn an_unclosed_delimiter_keeps_its_own_span() {
    let src = "fn f() {\n    info(\"x\";\n}\n";
    let p = parse(src);
    assert_eq!(p.errors.len(), 1, "{:#?}", p.errors);
    assert!(p.errors[0].msg.contains("`)`"), "{}", p.errors[0].msg);
    assert!(p.errors[0].msg.contains("found"), "{}", p.errors[0].msg);
    assert_eq!(p.errors[0].span.lo(), src.find(';').unwrap());
}

#[test]
fn comments_do_not_reach_the_grammar() {
    let src = concat!(
        "// leading\n",
        "x = hash(4); // trailing\n",
        "/* before */ #[exit] /* between the attribute and the fn */\n",
        "fn f() { /* in the body */ info(x); }\n",
        "// at end of file, with no newline"
    );
    let p = parse(src);
    assert!(p.errors.is_empty(), "{:#?}", p.errors);
    assert_eq!(p.script.items.len(), 2);
}

#[test]
fn a_trailing_comment_does_not_move_the_missing_semicolon() {
    let src = "fn f() {\n    y = 2  // why\n    info(\"x\");\n}\n";
    let p = parse(src);
    assert_eq!(p.errors.len(), 1, "{:#?}", p.errors);
    assert_eq!(p.errors[0].msg, "expected `;`");
    let just_past_the_2 = src.find("2 ").unwrap() + 1;
    assert_eq!(
        p.errors[0].span,
        Span::new(just_past_the_2, just_past_the_2)
    );
}

#[test]
fn const_is_a_modifier_on_a_global() {
    let items = ok("const MAXARG = 20;\nlat = hist(64);\n#[host] const N: u32 = 4;");
    let consts: Vec<(&str, bool)> = items
        .iter()
        .map(|i| match i {
            Item::Global(g) => (g.name.name.as_str(), g.is_const),
            other => panic!("expected a global, got {other:?}"),
        })
        .collect();
    assert_eq!(consts, [("MAXARG", true), ("lat", false), ("N", true)]);

    let Item::Global(n) = &items[2] else {
        panic!("expected a global")
    };
    assert_eq!(n.attrs.len(), 1);
    assert!(n.ty.is_some());
}

#[test]
fn const_is_no_longer_available_as_an_identifier() {
    assert!(!parse("const = 1;").errors.is_empty());
}

#[test]
fn every_example_parses() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .expect("examples/ should exist")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "tql"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no examples found in {dir}");
    for path in names {
        let src = std::fs::read_to_string(&path).unwrap();
        let p = parse(&src);
        assert!(
            p.errors.is_empty(),
            "{} should parse, got {:#?}",
            path.display(),
            p.errors
        );
        assert!(
            !p.script.items.is_empty(),
            "{} parsed to nothing",
            path.display()
        );
    }
}

#[test]
fn a_global_is_just_an_assignment() {
    let items = ok("MAXARG = 20;\nlat = hist(64);\n#[host] pending = hash(10);");
    assert_eq!(items.len(), 3);
    let names: Vec<&str> = items
        .iter()
        .map(|i| match i {
            Item::Global(g) => g.name.name.as_str(),
            other => panic!("expected a global, got {other:?}"),
        })
        .collect();
    assert_eq!(names, ["MAXARG", "lat", "pending"]);
    let Item::Global(g) = &items[2] else {
        unreachable!()
    };
    assert_eq!(g.attrs.len(), 1);
    assert_eq!(g.attrs[0].name.name, "host");
    assert!(g.ty.is_none());
}

#[test]
fn a_global_may_carry_a_type() {
    let items = ok("#[param] failed_only: bool = false;");
    let Some(Item::Global(g)) = items.into_iter().next() else {
        panic!("expected a global")
    };
    let Some(Type {
        ty: Ty::Name(t, _), ..
    }) = g.ty
    else {
        panic!("expected a named type")
    };
    assert_eq!(t.name, "bool");
    assert_eq!(g.init.expr, Expression::Bool(false));
}

#[test]
fn a_path_is_separated_by_dots() {
    let found = errs("#[tracepoint(syscalls::sys_enter_execve)] fn f() {}");
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("`)`"), "got {:?}", found[0]);
}

#[test]
fn attribute_arguments() {
    let items =
        ok("#[tracepoint(linux.syscalls.sys_enter_execve)] #[interval(secs = 1)] fn f() {}");
    let Some(Item::Fn(f)) = items.into_iter().next() else {
        panic!("expected a fn")
    };
    assert_eq!(f.attrs.len(), 2);
    assert_eq!(f.attrs[0].name.name, "tracepoint");
    let AttrArg::Path(p) = &f.attrs[0].args[0] else {
        panic!("expected a path")
    };
    let segs: Vec<&str> = p.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(segs, ["linux", "syscalls", "sys_enter_execve"]);
    let AttrArg::Named(k, v) = &f.attrs[1].args[0] else {
        panic!("expected a named argument")
    };
    assert_eq!(k.name, "secs");
    assert_eq!(**v, AttrArg::Int(1));
}

#[test]
fn a_declaration_may_state_what_it_returns() {
    let items = ok("extern fn hashmap(K, V, const N: size) -> HashMap(K, V, N);");
    let Some(Item::Fn(f)) = items.into_iter().next() else {
        panic!("expected a fn")
    };
    let Some(Type {
        ty: Ty::Name(n, args),
        ..
    }) = &f.ret
    else {
        panic!("expected a named return type")
    };
    assert_eq!((n.name.as_str(), args.len()), ("HashMap", 3));

    let states_one = |src: &str| match ok(src).into_iter().next() {
        Some(Item::Fn(f)) => f.ret.is_some(),
        other => panic!("expected a fn, got {other:?}"),
    };
    assert!(states_one("fn f() -> u64 { return 1; }"));
    assert!(!states_one("fn f() { x = 1; }"));
    assert_eq!(errs("extern fn f() ->;"), ["expected a type, found `;`"]);
}

#[test]
fn an_extern_declaration_has_no_body() {
    let items = ok("extern fn hashmap(K, V, const N: size);");
    let Some(Item::Fn(f)) = items.into_iter().next() else {
        panic!("expected a fn")
    };
    assert_eq!(f.params.len(), 3);
    assert!(
        f.body.is_none(),
        "an extern declaration has no body to parse"
    );

    assert_eq!(errs("extern fn f() { x = 1; }"), ["expected `;`"]);
    assert_eq!(
        errs("extern x = 1;"),
        ["expected `fn` or `struct` after `extern`, found `x`"]
    );
}

#[test]
fn an_extern_struct_is_declared_without_a_body() {
    let items = ok("extern struct HashMap(K, V, const N: size);");
    let Some(Item::Struct(r)) = items.into_iter().next() else {
        panic!("expected a struct")
    };
    assert_eq!(r.params.len(), 3);
    assert!(r.fields.is_empty());

    assert_eq!(
        errs("struct HashMap(K, V, const N: size);"),
        ["expected `{`, found `;`"]
    );
    assert_eq!(errs("extern struct Io { pid: u32 }"), ["expected `;`"]);
}

#[test]
fn a_declaration_may_bind_parameters() {
    let items = ok("struct HashMap(K, V, const N: size, key: K) { a: u32 }");
    let Some(Item::Struct(r)) = items.into_iter().next() else {
        panic!("expected a struct")
    };
    let bound: Vec<(&str, bool, bool)> = r
        .params
        .iter()
        .map(|p| (p.name.name.as_str(), p.is_const, p.ty.is_some()))
        .collect();
    assert_eq!(
        bound,
        [
            ("K", false, false),
            ("V", false, false),
            ("N", true, true),
            ("key", false, true),
        ]
    );
    let Some(Type {
        ty: Ty::Name(n, args),
        ..
    }) = &r.params[2].ty
    else {
        panic!("expected a named type")
    };
    assert_eq!((n.name.as_str(), args.len()), ("size", 0));
    assert_eq!(r.fields.len(), 1, "the body is still a body");
}

#[test]
fn parameters_are_optional_at_both_levels() {
    let params = |src: &str| match ok(src).into_iter().next() {
        Some(Item::Fn(f)) => f.params.len(),
        other => panic!("expected a fn, got {other:?}"),
    };
    assert_eq!(params("fn h(value: u64, const B: size) { x = 1; }"), 2);
    assert_eq!(params("fn h(a, b) { x = 1; }"), 2);
    assert_eq!(params("fn h() { x = 1; }"), 0);

    let Some(Item::Struct(r)) = ok("struct Io { pid: u32 }").into_iter().next() else {
        panic!("expected a struct")
    };
    assert!(r.params.is_empty(), "no parentheses means nothing is bound");
    assert_eq!(errs("fn h { x = 1; }").len(), 1);
}

#[test]
fn a_struct_keeps_its_attributes() {
    let items = ok("#[host] #[link(kind = linux.BPF_MAP_TYPE_HASH)] struct Io { pid: u32 }");
    let Some(Item::Struct(r)) = items.into_iter().next() else {
        panic!("expected a struct")
    };
    let names: Vec<&str> = r.attrs.iter().map(|a| a.name.name.as_str()).collect();
    assert_eq!(names, ["host", "link"]);
    assert_eq!(r.span.lo(), 0);
}

#[test]
fn struct_fields_carry_their_own_string_size() {
    let items = ok("struct Ev { pid: u32, comm: str(16), arg: str(ARGSIZE), args: [str(8)] }");
    let Some(Item::Struct(r)) = items.into_iter().next() else {
        panic!("expected a struct")
    };
    assert_eq!(r.fields.len(), 4);
    let Ty::Name(plain, args) = &r.fields[0].1.ty else {
        panic!("expected a named type")
    };
    assert_eq!((plain.name.as_str(), args.len()), ("u32", 0));

    let Ty::Name(s, args) = &r.fields[1].1.ty else {
        panic!("expected a named type")
    };
    assert_eq!(s.name, "str");
    assert_eq!(args, &[TyArg::Int(16)]);

    let Ty::Name(_, args) = &r.fields[2].1.ty else {
        panic!("expected a named type")
    };
    let [TyArg::Type(t)] = &args[..] else {
        panic!("expected a type argument")
    };
    let Ty::Name(n, inner) = &t.ty else {
        panic!("expected a named type")
    };
    assert_eq!((n.name.as_str(), inner.len()), ("ARGSIZE", 0));

    assert!(matches!(&r.fields[3].1.ty, Ty::List(..)));
}

#[test]
fn assignment_unpacks_a_tuple() {
    let Statement::Assign { target, op, .. } = stmt("(dev, rw) = origin[rq];") else {
        panic!("expected an assignment")
    };
    assert!(op.is_none());
    assert_eq!(sexp(&target), "(tuple dev rw)");
}

#[test]
fn for_takes_a_name_or_a_tuple() {
    assert!(matches!(
        stmt("for cpu in refs.keys() { }"),
        Statement::For {
            pat: Pat {
                pat: Pattern::Name(_),
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        stmt("for (dev, rw) in lat.keys() { }"),
        Statement::For {
            pat: Pat {
                pat: Pattern::Tuple(_),
                ..
            },
            ..
        }
    ));
}

#[test]
fn else_if_chains() {
    let Statement::If { els: Some(els), .. } = stmt("if a { } else if b { } else { }") else {
        panic!("expected an if with an else")
    };
    assert_eq!(els.stmts.len(), 1);
    assert!(matches!(
        els.stmts[0].stmt,
        Statement::If { els: Some(_), .. }
    ));
}

#[test]
fn compound_assignment() {
    assert!(matches!(
        stmt("counts[k] += 1;"),
        Statement::Assign {
            op: Some(BinOp::Add),
            ..
        }
    ));
    assert!(matches!(
        stmt("x -= 1;"),
        Statement::Assign {
            op: Some(BinOp::Sub),
            ..
        }
    ));
    assert!(matches!(stmt("x = 1;"), Statement::Assign { op: None, .. }));
}

#[test]
fn precedence_and_associativity() {
    assert_eq!(expr_of("1 + 2 * 3"), "(Add 1 (Mul 2 3))");
    assert_eq!(expr_of("1 * 2 + 3"), "(Add (Mul 1 2) 3)");
    assert_eq!(expr_of("1 - 2 - 3"), "(Sub (Sub 1 2) 3)");
    assert_eq!(expr_of("a || b && c"), "(OrOr a (AndAnd b c))");
    assert_eq!(expr_of("a & b | c"), "(Or (And a b) c)");
    assert_eq!(expr_of("a == b && c != d"), "(AndAnd (Eq a b) (Ne c d))");
    assert_eq!(expr_of("1 << 2 + 3"), "(Shl 1 (Add 2 3))");
    assert_eq!(expr_of("-a + b"), "(Add (Neg a) b)");
    assert_eq!(expr_of("!(k in m)"), "(Not (In k m))");
}

#[test]
fn negative_numbers_are_unary_minus() {
    assert_eq!(expr_of("-5"), "(Neg 5)");
    assert_eq!(expr_of("a-5"), "(Sub a 5)");
    assert_eq!(expr_of("a - 5"), "(Sub a 5)");
    assert_eq!(expr_of("-a - -5"), "(Sub (Neg a) (Neg 5))");
    assert_eq!(expr_of("--5"), "(Neg (Neg 5))");
    assert_eq!(expr_of("-a * b"), "(Mul (Neg a) b)");
    assert_eq!(expr_of("-9223372036854775808"), "(Neg 9223372036854775808)");
    assert_eq!(expr_of("-counts[k]"), "(Neg ([] counts k))");
    assert_eq!(expr_of("-probe.ret"), "(Neg (. probe ret))");
}

#[test]
fn postfix_chains_left_to_right() {
    assert_eq!(
        expr_of("linux.curtask.real_parent.tgid"),
        "(. (. (. linux curtask) real_parent) tgid)"
    );
    assert_eq!(
        expr_of("row.args.join(\" \")"),
        "(.join (. row args) \" \")"
    );
    assert_eq!(expr_of("pending[key].args"), "(. ([] pending key) args)");
    assert_eq!(expr_of("argv[i]"), "([] argv i)");
}

#[test]
fn a_multi_index_is_a_tuple_key() {
    assert_eq!(expr_of("lat[dev, rw]"), "([] lat (tuple dev rw))");
    assert_eq!(expr_of("lat[(dev, rw)]"), expr_of("lat[dev, rw]"));
    assert_eq!(expr_of("lat[dev]"), "([] lat dev)");
}

#[test]
fn parens_group_without_making_a_tuple() {
    assert_eq!(expr_of("(1 + 2) * 3"), "(Mul (Add 1 2) 3)");
    assert_eq!(expr_of("(a)"), "a");
}

#[test]
fn if_as_an_expression() {
    assert_eq!(
        expr_of("if rw == 1 { \"write\" } else { \"read\" }"),
        "(if (Eq rw 1) \"write\" \"read\")"
    );
}

#[test]
fn a_call_stands_as_a_statement() {
    for (src, want) in [
        ("info(x);", "(call info x)"),
        ("delete(start, rq);", "(call delete start rq)"),
        ("m.clear();", "(.clear m)"),
        (
            "events.emit(Io { pid: p });",
            "(.emit events (Io{ pid: p}))",
        ),
    ] {
        match stmt(src) {
            Statement::Call(e) => assert_eq!(sexp(&e), want, "{src}"),
            other => panic!("{src} -> {other:?}"),
        }
    }
}

#[test]
fn an_expression_with_no_effect_is_not_a_statement() {
    for src in [
        "1 + 2;",
        "x;",
        "counts[k];",
        "linux.pid;",
        "-x;",
        "Io { pid: p };",
    ] {
        let msgs = errs(&format!("fn f() {{ {src} }}"));
        assert_eq!(msgs, ["this expression has no effect"], "{src}");
    }
}

#[test]
fn the_keyword_statements_build_their_own_shapes() {
    assert!(matches!(stmt("break;"), Statement::Break));
    assert!(matches!(stmt("continue;"), Statement::Continue));
    assert!(matches!(stmt("return;"), Statement::Return { value: None }));
    match stmt("return 1 + 2;") {
        Statement::Return { value: Some(e) } => assert_eq!(sexp(&e), "(Add 1 2)"),
        other => panic!("{other:?}"),
    }
    match stmt("while a < 3 { x += 1; }") {
        Statement::While { cond, body } => {
            assert_eq!(sexp(&cond), "(Lt a 3)");
            assert_eq!(body.stmts.len(), 1, "{body:?}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_keyword_statements_each_need_a_semicolon() {
    for src in [
        "fn f() { break }",
        "fn f() { continue }",
        "fn f() { return }",
        "fn f() { return 1 }",
    ] {
        let msgs = errs(src);
        assert_eq!(msgs.len(), 1, "{src} -> {msgs:?}");
        assert!(msgs[0].starts_with("expected `;`"), "{src} -> {msgs:?}");
    }
}

#[test]
fn a_literal_gives_its_fields_by_name_or_by_position() {
    assert_eq!(expr_of("KV { key: 1, v: 2 }"), "(KV{ key: 1, v: 2})");
    assert_eq!(expr_of("KV { 1, 2 }"), "(KV{ 1, 2})");
    assert_eq!(expr_of("KV { x, }"), "(KV{ x})");
    assert_eq!(
        expr_of("KV { linux.tid, f(a) + 1 }"),
        "(KV{ (. linux tid), (Add (call f a) 1)})"
    );
    // a trailing comma is allowed in both, as everywhere else
    assert_eq!(expr_of("KV { 1, 2, }"), "(KV{ 1, 2})");
    assert_eq!(expr_of("KV { key: 1, }"), "(KV{ key: 1})");
}

#[test]
fn named_and_positional_fields_cannot_be_mixed() {
    assert_eq!(
        errs("fn f() { x = KV { 1, k: 2 }; }"),
        ["named and positional fields cannot be mixed"]
    );
    // the other order is caught where the name should have been
    let msgs = errs("fn f() { x = KV { k: 1, 2 }; }");
    assert!(msgs[0].starts_with("expected a field name"), "{msgs:?}");
}

#[test]
fn an_if_is_a_value_anywhere_an_expression_is() {
    assert_eq!(expr_of("if c { 1 } else { 2 }"), "(if c 1 2)");
    assert_eq!(
        expr_of("if a < b { f(x) } else { g(y) }"),
        "(if (Lt a b) (call f x) (call g y))"
    );
    assert_eq!(
        expr_of("KV { a: if c { 1 } else { 2 } }"),
        "(KV{ a: (if c 1 2)})"
    );
    assert_eq!(
        expr_of("KV { if c { 1 } else { 2 }, }"),
        "(KV{ (if c 1 2)})"
    );
    assert_eq!(
        expr_of("KV { if c { 1 } else { 2 }, 3 }"),
        "(KV{ (if c 1 2), 3})"
    );
    assert_eq!(errs("fn f() { while m { f() } }"), ["expected `;`"]);
}

#[test]
fn a_brace_holding_statements_is_still_a_block() {
    for src in [
        "while m { x = 1; }",
        "while m { g(); }",
        "if m { return; }",
        "if m { }",
        "while m { if a { g(); } }",
        "while m { while a { g(); } }",
    ] {
        let b = body(src);
        assert_eq!(b.len(), 1, "{src} -> {b:?}");
        assert!(
            matches!(
                b[0].stmt,
                Statement::While { .. } | Statement::If { .. } | Statement::For { .. }
            ),
            "{src} -> {:?}",
            b[0].stmt
        );
    }
    // the body is the block, not a field list
    match stmt("while m { x = 1; }") {
        Statement::While { body, .. } => assert_eq!(body.stmts.len(), 1, "{body:?}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_struct_literal_is_told_from_a_block_by_what_the_group_holds() {
    match stmt("for x in f(Ev { a: 1 }) { }") {
        Statement::For { iter, .. } => assert_eq!(sexp(&iter), "(call f (Ev{ a: 1}))"),
        other => panic!("{other:?}"),
    }
    match stmt("while m[Ev { a: 1 }] { }") {
        Statement::While { cond, .. } => assert_eq!(sexp(&cond), "([] m (Ev{ a: 1}))"),
        other => panic!("{other:?}"),
    }
    let b = body("while a { } x = Ev { a: 1 };");
    assert_eq!(b.len(), 2, "{b:?}");
    match &b[1].stmt {
        Statement::Assign { value, .. } => assert_eq!(sexp(value), "(Ev{ a: 1})"),
        other => panic!("{other:?}"),
    }
    assert_eq!(expr_of("io { a: 1 }"), "(io{ a: 1})");
    assert_eq!(ok("struct io { a: u32 }").len(), 1);
}

#[test]
fn a_scrutinee_brace_opens_a_block_not_a_struct() {
    assert!(matches!(stmt("if m { } "), Statement::If { .. }));
    assert!(matches!(stmt("for x in m { }"), Statement::For { .. }));
    assert!(matches!(stmt("while m { }"), Statement::While { .. }));
    assert_eq!(
        expr_of("Ev { kind: EXEC, pid: linux.pid }"),
        "(Ev{ kind: EXEC, pid: (. linux pid)})"
    );
}

#[test]
fn ranges_and_lists() {
    assert_eq!(expr_of("0..MAXARG"), "(.. 0 MAXARG)");
    assert_eq!(expr_of("[]"), "(list)");
    assert_eq!(expr_of("[\"a\", \"b\"]"), "(list \"a\" \"b\")");
}

#[test]
fn in_is_a_binary_operator_like_any_other() {
    assert_eq!(expr_of("k in m"), "(In k m)");
    assert_eq!(expr_of("a in b && c"), "(AndAnd (In a b) c)");
    assert_eq!(expr_of("inner + 1"), "(Add inner 1)");
}

#[test]
fn recovery_reports_every_error_and_reaches_the_end() {
    let src = "A = ;\nstruct R { a: }\nfn f() {\n    x = 1\n    y = 2;\n}\nstruct S { b: u32 }\n";
    let p = parse(src);
    assert_eq!(p.errors.len(), 3, "{:#?}", p.errors);
    assert!(
        p.script
            .items
            .iter()
            .any(|i| matches!(i, Item::Struct(s) if s.name.name == "S")),
        "recovery did not reach the final item: {:#?}",
        p.script.items
    );
}

#[test]
fn operator_noise_is_collapsed() {
    let msgs = errs("fn f() { x = 1\n y = 2; }");
    assert_eq!(msgs.len(), 1, "{msgs:?}");
    assert!(msgs[0].starts_with("expected `;`"), "{:?}", msgs[0]);
    assert!(
        !msgs[0].contains("`<<`"),
        "operator noise survived: {:?}",
        msgs[0]
    );
}

#[test]
fn a_missing_expression_says_so_without_listing_prefixes() {
    let msgs = errs("A = ;");
    assert_eq!(msgs, ["expected an expression, found `;`"]);
}

#[test]
fn a_lexer_error_is_not_reported_twice() {
    assert_eq!(errs("A = 1.5;"), ["floating point is not supported"]);
    assert_eq!(errs("A = 0xZZ;"), ["hex literal has no digits"]);
    assert_eq!(errs("A = $;"), ["unexpected character `$`"]);
    assert_eq!(errs("A = \"oops"), ["string is not terminated"]);
}

#[test]
fn unbalanced_delimiters_do_not_panic() {
    for src in [
        "fn f() { x = a); }",
        "fn f() { x = a]; }",
        "fn f() { )))",
        "fn f() { {{{ ",
        "fn f() {",
        "fn",
        "#",
        "#[",
        "struct",
        "",
    ] {
        let p = parse(src);
        assert!(p.errors.len() < 30, "{src} produced {:#?}", p.errors);
    }
}

#[test]
fn shebang_reaches_the_script_node() {
    let p = parse("#!/usr/bin/env traqilet\nlat = hist(64);\n");
    assert!(p.errors.is_empty(), "{:#?}", p.errors);
    assert_eq!(p.script.shebang.as_deref(), Some("#!/usr/bin/env traqilet"));
    assert_eq!(p.script.items.len(), 1);

    let p = parse("lat = hist(64);\n");
    assert_eq!(p.script.shebang, None);
}
