//! Rust format specs for `--log-format`, parsed once and applied by hand.
//!
//! `format!` requires a literal spec, so a runtime template such as
//! `{mono:>12.6}` cannot be handed to it. [`Spec::parse`] turns the spec into
//! data and [`Spec::render`] applies it.
//!
//! Accepted grammar, a subset of std's:
//!
//! ```text
//! spec  := [[fill] align] [width] ['.' precision]
//! fill  := any char
//! align := '<' | '^' | '>'
//! ```
//!
//! Any character preceding an align char is a fill, so `0>5`, `+>9` and `>>`
//! mean fill-`0`, fill-`+` and fill-`>` — not zero padding, a sign, or a
//! doubled align. That is counterintuitive enough to have caught the author
//! twice, so `agrees_with_std` in the tests compares against `format!`
//! directly. That test is what guarantees fidelity here; keep it.
//!
//! Rejected rather than ignored, so a spec never silently does less than it
//! claims: sign, `#`, zero padding, type chars (`{:?}`, `{:x}`), trailing text.
//!
//! Behaviour follows std: numbers right-align and everything else left-aligns
//! by default; width is a minimum, never a truncation, and counts characters
//! rather than bytes; precision fixes decimal places on numbers and truncates
//! strings.
//!
//! [`Spec::parse`] receives only the text *after* the colon. Finding the braces
//! and handling `{{`/`}}` is the caller's job, in `parse_format`.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spec {
    pub fill: Option<char>,
    pub align: Option<Align>,
    pub width: Option<usize>,
    pub precision: Option<usize>,
}

#[derive(Clone, Copy)]
pub enum Val<'a> {
    Str(&'a str),
    Num(f64),
}

/// The only place that decides what an alignment character is. Returning an
/// Option rather than defaulting lets the callers use this as their guard, so
/// the set is not restated anywhere and an unexpected char cannot silently
/// become right-alignment.
fn align(c: char) -> Option<Align> {
    match c {
        '<' => Some(Align::Left),
        '^' => Some(Align::Center),
        '>' => Some(Align::Right),
        _ => None,
    }
}

impl Spec {
    /// Parses the part after `:` in `{name:>12.6}`.
    pub fn parse(s: &str) -> Result<Spec, String> {
        let mut sp = Spec::default();
        let c: Vec<char> = s.chars().collect();
        let mut i = 0;

        if let Some(a) = c.get(1).copied().and_then(align) {
            sp.fill = Some(c[0]);
            sp.align = Some(a);
            i = 2;
        } else if let Some(a) = c.first().copied().and_then(align) {
            sp.align = Some(a);
            i = 1;
        }

        if let Some(&ch) = c.get(i)
            && matches!(ch, '+' | '-' | '#' | '0')
        {
            return Err(format!(
                "`{s}`: sign, `#` and zero padding are not implemented"
            ));
        }

        let start = i;
        while c.get(i).is_some_and(char::is_ascii_digit) {
            i += 1;
        }
        if i > start {
            sp.width = c[start..i].iter().collect::<String>().parse().ok();
        }

        if c.get(i) == Some(&'.') {
            i += 1;
            let ps = i;
            while c.get(i).is_some_and(char::is_ascii_digit) {
                i += 1;
            }
            if i == ps {
                return Err(format!("`{s}`: `.` with no precision"));
            }
            sp.precision = c[ps..i].iter().collect::<String>().parse().ok();
        }

        if i != c.len() {
            return Err(format!("`{s}`: unsupported format spec"));
        }
        Ok(sp)
    }

    pub fn render(&self, v: Val) -> String {
        let body = match (v, self.precision) {
            (Val::Num(n), Some(p)) => format!("{n:.p$}"),
            (Val::Num(n), None) => format!("{n}"),
            // precision truncates a string, as it does in std
            (Val::Str(s), Some(p)) => s.chars().take(p).collect(),
            (Val::Str(s), None) => s.to_owned(),
        };
        let Some(w) = self.width else {
            return body;
        };
        let len = body.chars().count();
        if len >= w {
            return body;
        }
        let pad = w - len;
        let fill = self.fill.unwrap_or(' ');
        // std's defaults: numbers right, everything else left
        let align = self.align.unwrap_or(match v {
            Val::Num(_) => Align::Right,
            Val::Str(_) => Align::Left,
        });
        let (l, r) = match align {
            Align::Left => (0, pad),
            Align::Right => (pad, 0),
            Align::Center => (pad / 2, pad - pad / 2),
        };
        let mut out = String::with_capacity(w);
        out.extend(std::iter::repeat_n(fill, l));
        out.push_str(&body);
        out.extend(std::iter::repeat_n(fill, r));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(
        fill: Option<char>,
        align: Option<Align>,
        width: Option<usize>,
        precision: Option<usize>,
    ) -> Spec {
        Spec {
            fill,
            align,
            width,
            precision,
        }
    }

    #[test]
    fn parses_empty() {
        assert_eq!(Spec::parse("").unwrap(), Spec::default());
    }

    #[test]
    fn parses_align_alone() {
        use Align::*;
        for (t, a) in [("<", Left), ("^", Center), (">", Right)] {
            assert_eq!(
                Spec::parse(t).unwrap(),
                spec(None, Some(a), None, None),
                "{t}"
            );
        }
    }

    #[test]
    fn parses_fill_and_align() {
        assert_eq!(
            Spec::parse("*^").unwrap(),
            spec(Some('*'), Some(Align::Center), None, None)
        );
    }

    /// A char before an align char is always a fill, so none of these are a
    /// flag: `0>5` is not zero padding, `+>9` is not a sign, `>>` is not a
    /// doubled align.
    #[test]
    fn a_leading_char_is_a_fill_not_a_flag() {
        assert_eq!(
            Spec::parse("0>5").unwrap(),
            spec(Some('0'), Some(Align::Right), Some(5), None)
        );
        assert_eq!(
            Spec::parse("+>9").unwrap(),
            spec(Some('+'), Some(Align::Right), Some(9), None)
        );
        assert_eq!(
            Spec::parse(">>").unwrap(),
            spec(Some('>'), Some(Align::Right), None, None)
        );
    }

    #[test]
    fn parses_width_and_precision() {
        assert_eq!(Spec::parse("12").unwrap(), spec(None, None, Some(12), None));
        assert_eq!(Spec::parse(".6").unwrap(), spec(None, None, None, Some(6)));
        assert_eq!(
            Spec::parse(">12.6").unwrap(),
            spec(None, Some(Align::Right), Some(12), Some(6))
        );
        assert_eq!(
            Spec::parse("*^9.2").unwrap(),
            spec(Some('*'), Some(Align::Center), Some(9), Some(2))
        );
    }

    #[test]
    fn rejects_unimplemented_grammar() {
        for t in ["+9", "#9", "09", "-9"] {
            assert!(Spec::parse(t).is_err(), "{t} should be rejected");
        }
    }

    #[test]
    fn rejects_malformed() {
        for t in [".", "9x", "x", "12.3.4"] {
            assert!(Spec::parse(t).is_err(), "{t} should be rejected");
        }
    }

    /// The point of the module is to agree with std, so compare against it
    /// rather than against anyone's recollection of the grammar. Specs must be
    /// literals here, which is exactly why runtime formatting needs this module.
    #[test]
    fn agrees_with_std() {
        let p = |s: &str| Spec::parse(s).unwrap();
        assert_eq!(p(">>5").render(Val::Str("ab")), format!("{:>>5}", "ab"));
        assert_eq!(p("<7").render(Val::Str("abc")), format!("{:<7}", "abc"));
        assert_eq!(p("^7").render(Val::Str("abc")), format!("{:^7}", "abc"));
        assert_eq!(
            p("*^9").render(Val::Str("ERROR")),
            format!("{:*^9}", "ERROR")
        );
        assert_eq!(
            p(".4").render(Val::Str("traqilet")),
            format!("{:.4}", "traqilet")
        );
        assert_eq!(p("8").render(Val::Str("ab")), format!("{:8}", "ab"));
        assert_eq!(p("").render(Val::Str("abc")), "abc".to_owned());
        assert_eq!(
            p(">12.6").render(Val::Num(0.0034)),
            format!("{:>12.6}", 0.0034)
        );
        assert_eq!(p(".6").render(Val::Num(1.5)), format!("{:.6}", 1.5));
        assert_eq!(
            p("10.2").render(Val::Num(1.23456)),
            format!("{:10.2}", 1.23456)
        );
        assert_eq!(p("0>5").render(Val::Num(42.0)), format!("{:0>5}", 42.0));
        assert_eq!(p("6").render(Val::Num(12.0)), format!("{:6}", 12.0));
    }

    #[test]
    fn render_without_width_is_unchanged() {
        assert_eq!(Spec::default().render(Val::Str("abc")), "abc");
        assert_eq!(Spec::default().render(Val::Num(1.5)), "1.5");
    }

    /// std's defaults: numbers right, everything else left.
    #[test]
    fn render_default_alignment_depends_on_the_value() {
        let s = spec(None, None, Some(6), None);
        assert_eq!(s.render(Val::Str("ab")), "ab    ");
        assert_eq!(s.render(Val::Num(12.0)), "    12");
    }

    #[test]
    fn render_explicit_alignment() {
        let w = |a| spec(None, Some(a), Some(7), None);
        assert_eq!(w(Align::Left).render(Val::Str("abc")), "abc    ");
        assert_eq!(w(Align::Right).render(Val::Str("abc")), "    abc");
        assert_eq!(w(Align::Center).render(Val::Str("abc")), "  abc  ");
    }

    /// An odd amount of padding puts the extra character on the right.
    #[test]
    fn render_centre_with_odd_padding() {
        let s = spec(Some('*'), Some(Align::Center), Some(6), None);
        assert_eq!(s.render(Val::Str("abc")), "*abc**");
    }

    /// Precision truncates a string, as it does in std.
    #[test]
    fn render_precision_truncates_strings() {
        assert_eq!(
            spec(None, None, None, Some(4)).render(Val::Str("traqilet")),
            "traq"
        );
    }

    /// Width is a minimum, never a maximum, and counts characters not bytes.
    #[test]
    fn render_width_is_a_minimum_in_characters() {
        assert_eq!(
            spec(None, None, Some(3), None).render(Val::Str("abcdefg")),
            "abcdefg"
        );
        assert_eq!(
            spec(None, Some(Align::Left), Some(7), None).render(Val::Str("héllo")),
            "héllo  "
        );
    }
}
