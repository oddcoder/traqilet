//! Byte positions and spans, after rustc's `rustc_span`.

use std::fmt;

/// A region of the source: `lo` inclusive, `hi` exclusive.
///
/// The positions are 32 bits, as rustc's are, which caps a script at 4 GiB.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Span {
    lo: u32,
    hi: u32,
}

/// The span of something with no source of its own.
///
/// As in rustc, this is simply the empty span at offset zero, so it is not
/// distinguishable from a real span there. Nothing relies on telling them apart.
pub const DUMMY_SP: Span = Span { lo: 0, hi: 0 };

impl Span {
    pub const fn new(lo: usize, hi: usize) -> Span {
        debug_assert!(hi <= u32::MAX as usize, "byte offset exceeds 32 bits");
        debug_assert!(lo <= u32::MAX as usize, "byte offset exceeds 32 bits");
        // A mistake in ordering hi and low costs a caret in the wrong place instead of taking the
        // tool down.
        if lo <= hi {
            Span {
                lo: lo as u32,
                hi: hi as u32,
            }
        } else {
            Span {
                lo: hi as u32,
                hi: lo as u32,
            }
        }
    }

    pub const fn lo(self) -> usize {
        self.lo as usize
    }

    pub const fn hi(self) -> usize {
        self.hi as usize
    }

    /// The span enclosing both, from the earlier start to the later end.
    ///
    /// Order-insensitive. A parser that has the two halves in hand need not know which came first.
    pub fn to(self, end: Span) -> Span {
        Span::new(self.lo().min(end.lo()), self.hi().max(end.hi()))
    }

    /// Clamped to something safe to slice `src` with: both ends on character
    /// boundaries, and neither past the end.
    pub fn clamp_to(self, src: &str) -> Span {
        let mut lo = self.lo().min(src.len());
        while !src.is_char_boundary(lo) {
            lo -= 1;
        }
        let mut hi = self.hi().clamp(lo, src.len());
        while !src.is_char_boundary(hi) {
            hi += 1;
        }
        Span::new(lo, hi)
    }

    /// The empty span at the start of this one.
    pub const fn shrink_to_lo(self) -> Span {
        Span {
            lo: self.lo,
            hi: self.lo,
        }
    }

    /// The empty span just past this one, where a missing terminator belongs.
    pub const fn shrink_to_hi(self) -> Span {
        Span {
            lo: self.hi,
            hi: self.hi,
        }
    }
}

/// `lo..hi`, so a failing assertion is readable.
impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.lo, self.hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_and_range_agree() {
        let s = Span::new(3, 7);
        assert_eq!((s.lo(), s.hi()), (3, 7));
        assert_eq!(s.lo()..s.hi(), 3..7);
    }

    /// A reversed span is a bug, but one that should cost a misplaced caret
    /// rather than a panic, so it is normalised the way rustc does it.
    #[test]
    fn a_reversed_span_is_swapped_not_rejected() {
        assert_eq!(Span::new(7, 3), Span::new(3, 7));
        // so the invariant holds, and `range` can never hand a slice a
        // backwards range no matter what the caller passed
        let swapped = Span::new(7, 3);
        assert_eq!(&"abcdefghij"[swapped.lo()..swapped.hi()], "defg");
    }

    #[test]
    fn to_encloses_both_in_either_order() {
        let a = Span::new(3, 5);
        let b = Span::new(9, 12);
        assert_eq!(a.to(b), Span::new(3, 12));
        assert_eq!(b.to(a), Span::new(3, 12));
        let outer = Span::new(0, 20);
        assert_eq!(outer.to(a), outer);
    }

    #[test]
    fn an_empty_span_covers_nothing() {
        let s = Span::new(4, 9);
        assert_eq!(s.shrink_to_hi(), Span::new(9, 9));
        assert_eq!(s.shrink_to_hi().lo(), s.shrink_to_hi().hi());
        assert_ne!(s.lo(), s.hi());
        assert_eq!(DUMMY_SP.lo(), DUMMY_SP.hi());
        let src = "abcdefghij";
        let empty = Span::new(4, 4);
        let one = Span::new(4, 5);
        assert_eq!(&src[empty.lo()..empty.hi()], "");
        assert_eq!(&src[one.lo()..one.hi()], "e");
    }

    #[test]
    fn clamping_keeps_a_span_sliceable() {
        let src = "x = \"héllo\";";
        let e = src.find('\u{e9}').unwrap();
        for span in [Span::new(e + 1, src.len()), Span::new(0, e + 1)] {
            let c = span.clamp_to(src);
            assert!(src.get(c.lo()..c.hi()).is_some(), "{span:?} -> {c:?}");
        }
        assert_eq!(Span::new(e + 1, e + 2).clamp_to(src), Span::new(e, e + 2));
        assert_eq!(Span::new(4, 999).clamp_to(src), Span::new(4, src.len()));
        assert_eq!(Span::new(0, 1).clamp_to(src), Span::new(0, 1));
        assert_eq!(Span::new(7, 9).clamp_to(""), Span::new(0, 0));
    }

    #[test]
    fn spans_are_copy_and_small() {
        let s = Span::new(1, 2);
        let a = s;
        let b = s;
        assert_eq!(a, b);
        assert_eq!(size_of::<Span>(), 8);
    }

    #[test]
    fn debug_reads_as_a_range() {
        assert_eq!(format!("{:?}", Span::new(3, 7)), "3..7");
    }
}
