//! Diagnostics wrapper around [`log`] and [`ariadne`].

use ariadne::{CharSet, Color, Config, IndexType, Label, Report, ReportKind, Source};
use log::error;
use traqilet_lang::{Error, Span};

const WRONG: Color = Color::Red;
const SECOND_LOOK: Color = Color::Fixed(81);

/// One log record per line.
fn emit(text: &str) {
    for line in text.lines() {
        error!("{line}");
    }
}

/// Whether a locale string promises UTF-8.
/// Setting `LANG=C` is therefore also the way to ask for the ASCII output.
fn utf8_locale(locale: Option<&str>) -> bool {
    match locale {
        None => true,
        Some(l) => {
            let l = l.to_ascii_lowercase();
            l.contains("utf-8") || l.contains("utf8")
        }
    }
}

/// The character set for this process, from the first locale variable set.
fn char_set() -> CharSet {
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()));
    if utf8_locale(locale.as_deref()) {
        CharSet::Unicode
    } else {
        CharSet::Ascii
    }
}

pub struct Diags<'a> {
    path: &'a str,
    src: &'a str,
    config: Config,
    errors: usize,
}

impl<'a> Diags<'a> {
    pub fn new(path: &'a str, src: &'a str, color: bool) -> Self {
        Diags {
            path,
            src,
            config: Config::default()
                .with_cross_gap(true)
                .with_index_type(IndexType::Byte)
                .with_char_set(char_set())
                .with_color(color),
            errors: 0,
        }
    }

    pub fn errors(&self) -> usize {
        self.errors
    }

    /// For failures with no position to point at.
    pub fn plain(&mut self, msg: impl std::fmt::Display) {
        self.errors += 1;
        let err_line = format!("{}: {msg}", self.path);
        emit(&err_line);
    }

    /// Reports with the offending range underlined, plus the error's note if it
    /// has one.
    pub fn error(&mut self, e: &Error) {
        self.errors += 1;
        let note = e.note.as_ref().map(|(msg, at)| (msg.as_str(), *at));
        let err_line = self.render(e.span, &e.msg, note);
        emit(&err_line);
    }

    fn render(&self, span: Span, msg: &str, note: Option<(&str, Span)>) -> String {
        let span = span.clamp_to(self.src);
        let range = span.lo()..span.hi();
        let mut out = Vec::new();
        let mut report = Report::build(ReportKind::Error, (self.path, range.clone()))
            .with_config(self.config)
            .with_label(
                Label::new((self.path, range))
                    .with_message(msg)
                    .with_color(WRONG),
            );
        if let Some((note, at)) = note {
            let at = at.clamp_to(self.src);
            report = report.with_label(
                Label::new((self.path, at.lo()..at.hi()))
                    .with_message(note)
                    .with_color(SECOND_LOOK),
            );
        }
        let report = report.finish();
        if report
            .write((self.path, Source::from(self.src)), &mut out)
            .is_err()
        {
            return msg.to_owned();
        }
        let text = String::from_utf8_lossy(&out);
        let body = text
            .split_once('\n')
            .map_or(text.as_ref(), |(_, rest)| rest);
        body.trim_end().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diags(src: &str) -> Diags<'_> {
        Diags::new("t.tql", src, false)
    }

    /// ariadne marks a span with a stem and an elbow rather than a row of
    /// carets, and the glyphs differ between its two character sets, so tests
    /// ask only whether *something* was marked.
    fn marked(out: &str) -> bool {
        out.chars()
            .any(|c| "^`\u{252c}\u{2500}\u{2570}".contains(c))
    }

    #[test]
    fn points_at_a_span() {
        let src = "x = 1;\ny = 2;\n";
        let out = diags(src).render(Span::new(11, 12), "expected `;`", None);
        assert!(out.contains("t.tql:2:5"), "{out}");
        assert!(out.contains("y = 2;"), "{out}");
        assert!(marked(&out), "{out}");
        assert!(out.contains("expected `;`"), "{out}");
        assert!(!out.contains("Error:"), "header survived:\n{out}");
    }

    #[test]
    fn spans_are_read_as_bytes_not_characters() {
        let src = "s = \"héllo wörld\";";
        let start = src.find("wörld").unwrap();
        assert_eq!(start, 12, "byte offset assumption changed");
        assert_eq!(
            src[..start].chars().count(),
            11,
            "char offset assumption changed"
        );
        let out = diags(src).render(Span::new(start, start + "wörld".len()), "here", None);
        assert!(
            out.contains("t.tql:1:12"),
            "wrong column for a byte span:\n{out}"
        );
        assert!(out.contains("héllo wörld"), "{out}");
    }

    #[test]
    fn an_empty_span_at_end_of_input_still_marks_something() {
        let src = "fn f() {";
        let out = diags(src).render(Span::new(src.len(), src.len()), "expected `}`", None);
        assert!(out.contains("expected `}`"), "{out}");
        assert!(out.contains("fn f() {"), "{out}");
        assert!(marked(&out), "nothing marked:\n{out}");
    }

    #[test]
    fn an_empty_span_in_an_empty_source_does_not_panic() {
        let out = diags("").render(Span::new(0, 0), "nothing here", None);
        assert!(out.contains("nothing here"), "{out}");
    }

    #[test]
    fn a_span_starting_inside_a_character_does_not_panic() {
        let src = "x = \"é\";";
        let mid = src.find('é').unwrap() + 1; // second byte of the two
        let out = diags(src).render(Span::new(mid, src.len()), "mid-character", None);
        assert!(out.contains("mid-character"), "{out}");
    }

    #[test]
    fn a_span_past_the_end_keeps_its_snippet() {
        let src = "x = 1;";
        let out = diags(src).render(Span::new(4, 999), "too wide", None);
        assert!(out.contains("too wide"), "{out}");
        assert!(out.contains("x = 1;"), "snippet was dropped:\n{out}");
        assert!(marked(&out), "{out}");
    }

    #[test]
    fn a_note_renders_as_a_second_label() {
        let src = "f(\"x\";";
        let open = src.find('(').unwrap();
        let semi = src.find(';').unwrap();
        let out = diags(src).render(
            Span::new(semi, semi + 1),
            "expected `)`, found `;`",
            Some(("unclosed `(`", Span::new(open, open + 1))),
        );
        assert!(out.contains("expected `)`"), "{out}");
        assert!(out.contains("unclosed `(`"), "missing the note:\n{out}");
        assert!(
            out.matches('\u{2570}').count() >= 2,
            "only one label:\n{out}"
        );
    }

    #[test]
    fn no_note_means_one_label() {
        let out = diags("f(\"x\";").render(Span::new(0, 1), "m", None);
        assert!(!out.contains("unclosed"), "{out}");
        assert_eq!(out.matches('\u{2570}').count(), 1, "{out}");
    }

    #[test]
    fn the_marks_carry_their_label_colour() {
        const WRONG_SEQ: &str = "\x1b[31m";
        const SECOND_SEQ: &str = "\x1b[38;5;81m";
        let src = "f(\"x\";";
        let open = src.find('(').unwrap();
        let semi = src.find(';').unwrap();
        let out = Diags::new("t.tql", src, true).render(
            Span::new(semi, semi + 1),
            "expected `)`, found `;`",
            Some(("unclosed `(`", Span::new(open, open + 1))),
        );

        fn line_with<'o>(out: &'o str, needle: &str) -> &'o str {
            out.lines()
                .find(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("no line with {needle:?}:\n{out}"))
        }

        assert!(
            out.contains(&format!("{WRONG_SEQ};")),
            "the `;` is not marked wrong:\n{out:?}"
        );
        assert!(
            out.contains(&format!("{SECOND_SEQ}(")),
            "the `(` is not marked as the second place:\n{out:?}"
        );
        assert!(
            line_with(&out, "unclosed `(`").contains(SECOND_SEQ),
            "the note's underline is unstyled"
        );
        assert!(
            line_with(&out, "expected `)`").contains(WRONG_SEQ),
            "the error's underline is unstyled"
        );
    }

    #[test]
    fn colour_is_forced_in_both_directions() {
        let src = "x = 1;";
        let plain = Diags::new("t.tql", src, false).render(Span::new(4, 5), "m", None);
        let styled = Diags::new("t.tql", src, true).render(Span::new(4, 5), "m", None);
        assert!(!plain.contains('\x1b'), "plain had styles: {plain:?}");
        assert!(styled.contains('\x1b'), "styled had none: {styled:?}");
    }

    #[test]
    fn output_uses_box_drawing() {
        let mut d = diags("x = 1;");
        d.config = d.config.with_char_set(CharSet::Unicode);
        let out = d.render(Span::new(0, 1), "m", None);
        assert!(!out.is_ascii(), "expected box drawing:\n{out}");
        assert!(marked(&out), "{out}");
    }

    #[test]
    fn the_ascii_fallback_still_marks_the_span() {
        let mut d = diags("x = 1;");
        d.config = d.config.with_char_set(CharSet::Ascii);
        let out = d.render(Span::new(0, 1), "m", None);
        assert!(out.is_ascii(), "non-ascii in the ascii set:\n{out}");
        assert!(marked(&out), "{out}");
    }

    #[test]
    fn the_locale_decides_the_character_set() {
        for yes in [
            None,
            Some("en_US.UTF-8"),
            Some("C.utf8"),
            Some("en_GB.utf-8"),
            Some("ja_JP.UTF-8"),
        ] {
            assert!(utf8_locale(yes), "{yes:?} should keep box drawing");
        }
        for no in [
            Some("C"),
            Some("POSIX"),
            Some("en_US"),
            Some("en_US.ISO-8859-1"),
            Some("ru_RU.KOI8-R"),
        ] {
            assert!(!utf8_locale(no), "{no:?} should fall back to ascii");
        }
    }

    #[test]
    fn no_rendered_line_is_blank() {
        let src = "x = 1;\ny = 2;\n";
        let mut d = diags(src);
        for set in [CharSet::Unicode, CharSet::Ascii] {
            d.config = d.config.with_char_set(set);
            let out = d.render(Span::new(11, 12), "expected `;`", None);
            for (i, line) in out.lines().enumerate() {
                assert!(!line.trim().is_empty(), "line {i} is blank:\n{out}");
            }
        }
    }

    #[test]
    fn errors_are_counted() {
        let src = "x = 1;";
        let mut d = Diags::new("t.tql", src, false);
        assert_eq!(d.errors(), 0);
        d.error(&Error::new("one", Span::new(0, 3)));
        d.plain("two");
        assert_eq!(d.errors(), 2);
    }
}
