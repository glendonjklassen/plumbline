//! A bounded memo in front of a [`Measure`]: the same run of text is measured
//! once per font, not once per token per layout.
//!
//! Every run [`crate::layout_chapter`] places crosses out of Rust into the
//! shell's text stack, and scripture repeats itself: laying out the whole KJV
//! measures 822,057 runs, only 348,027 of them distinct, and re-laying out a
//! chapter already measured (a rotation, a margin change, a revisit) measures
//! nothing. Living below the ABI, one implementation serves every shell.
//!
//! A remembered width is valid only for the font and size it was measured in, and
//! only the caller knows which that is, so every entry belongs to a
//! caller-supplied font identity ([`Memoized::new`]). Pointing the memo at a new
//! identity drops what the old one remembered, and the identity is checked on
//! every read, not only on retune: two threads laying out in different fonts will
//! thrash, but neither can be handed the other's widths. A wrong width is a
//! mis-laid-out chapter, far worse than a slow one.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::Measure;

/// Remembered advance widths for one font identity. Shared across layouts by the
/// caller (the FFI layer keeps one per engine); drive it through [`Memoized`].
pub struct MeasureMemo {
    /// The font identity every entry below was measured in.
    font: u64,
    widths: HashMap<String, f32>,
}

impl MeasureMemo {
    /// Entries kept before the memo starts over.
    ///
    /// A bound is mandatory — the whole KJV has 29,158 distinct rendered runs, so
    /// an unbounded map is a slow leak a read-through fills. 4,096 is sized
    /// against the unit of work, not the corpus: the longest chapter (Ps 119) has
    /// 866 distinct runs, so the chapter on screen fits nearly five times over and
    /// its re-layout stays free even right after an overflow. Overflow clears the
    /// map rather than evicting one entry, because LRU bookkeeping would sit in
    /// the measure hot path; a whole-Bible read-through still avoids 87% of the
    /// crossings at ~230 KB resident, against 96% at ~1.4 MB unbounded.
    pub const CAP: usize = 4096;

    pub fn new() -> MeasureMemo {
        MeasureMemo { font: 0, widths: HashMap::new() }
    }

    /// How many widths are remembered (diagnostics + tests).
    pub fn entries(&self) -> usize {
        self.widths.len()
    }

    /// Adopt `font` as the identity the memo holds, dropping every width measured
    /// in a different one.
    fn retune(&mut self, font: u64) {
        if self.font != font {
            self.widths.clear();
            self.font = font;
        }
    }

    /// The width of `text`, if it was measured in `font`.
    fn get(&self, font: u64, text: &str) -> Option<f32> {
        if self.font != font {
            return None;
        }
        self.widths.get(text).copied()
    }

    fn insert(&mut self, font: u64, text: &str, width: f32) {
        // Another identity took the memo over while this width was being measured
        // (two panes, two fonts): drop it rather than file it under the wrong
        // identity.
        if self.font != font {
            return;
        }
        if self.widths.len() >= Self::CAP {
            self.widths.clear();
        }
        self.widths.insert(text.to_string(), width);
    }
}

impl Default for MeasureMemo {
    fn default() -> Self {
        MeasureMemo::new()
    }
}

/// A [`Measure`] that answers from a shared [`MeasureMemo`] and only asks `inner`
/// for text it has not already measured at this font identity.
pub struct Memoized<'a, M: Measure> {
    inner: &'a M,
    memo: &'a Mutex<MeasureMemo>,
    font: u64,
}

impl<'a, M: Measure> Memoized<'a, M> {
    /// Wrap `inner`, pointing `memo` at `font` — the caller's identity for the
    /// font and size `inner` measures with right now. Constructing this is what
    /// invalidates: widths remembered for any other identity are dropped here, so
    /// a caller that folds every measurement-affecting input into `font` cannot
    /// be served a stale width.
    pub fn new(memo: &'a Mutex<MeasureMemo>, font: u64, inner: &'a M) -> Memoized<'a, M> {
        lock(memo).retune(font);
        Memoized { inner, memo, font }
    }

    /// Widths currently remembered (diagnostics + tests).
    pub fn entries(&self) -> usize {
        lock(self.memo).entries()
    }
}

impl<M: Measure> Measure for Memoized<'_, M> {
    fn text_width(&self, text: &str) -> f32 {
        if let Some(w) = lock(self.memo).get(self.font, text) {
            return w;
        }
        // The guard is dropped before the inner call: `inner` is a foreign upcall
        // into the shell's text stack, so holding the lock across it would make
        // two panes' layouts wait on each other, and would deadlock outright if
        // the upcall ever re-entered layout on this thread.
        let w = self.inner.text_width(text);
        lock(self.memo).insert(self.font, text, w);
        w
    }
}

/// Take the memo, ignoring poisoning. A memo is a pure cache: a panic on another
/// thread must not turn every later layout into a panic, since the ABI's firewall
/// would then answer null and the reader would get a blank chapter.
fn lock(memo: &Mutex<MeasureMemo>) -> MutexGuard<'_, MeasureMemo> {
    memo.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{layout_chapter, LayoutConfig};
    use plumbline_core::corpus;
    use std::cell::Cell;

    /// Monospace measurement that counts its calls — one call is one crossing of
    /// the shell boundary in the product.
    struct Counting {
        char_w: f32,
        calls: Cell<usize>,
    }
    impl Counting {
        fn new(char_w: f32) -> Counting {
            Counting { char_w, calls: Cell::new(0) }
        }
    }
    impl Measure for Counting {
        fn text_width(&self, text: &str) -> f32 {
            self.calls.set(self.calls.get() + 1);
            text.chars().count() as f32 * self.char_w
        }
    }

    /// Two verses that repeat "the" and "God" — the redundancy the memo removes.
    const SAMPLE: &str = concat!(
        r#"{"format":"x","tokenization":"kjv1769-tok2","verses":2}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","In","",[],0],["","the","",[],0],["","beginning","",["H7225"],0],["","God","",["H430"],0]],"v":1}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","And","",[],0],["","the","",[],0],["","Spirit","",[],0],["","of","",[],0],["","God","",["H430"],0],["","moved",".",[],0]],"v":2}"#,
    );

    fn wide() -> LayoutConfig {
        LayoutConfig { width: 10_000.0, ..Default::default() }
    }

    #[test]
    fn a_repeated_run_of_text_is_measured_once() {
        let inner = Counting::new(10.0);
        let memo = Mutex::new(MeasureMemo::new());
        let m = Memoized::new(&memo, 1, &inner);

        assert_eq!(m.text_width("God"), 30.0);
        assert_eq!(m.text_width("God"), 30.0);
        assert_eq!(m.text_width("the"), 30.0);
        assert_eq!(m.text_width("God"), 30.0);

        assert_eq!(inner.calls.get(), 2, "only the two DISTINCT runs may cross");
        assert_eq!(m.entries(), 2);
    }

    #[test]
    fn a_second_layout_of_the_same_chapter_measures_nothing() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let verses = c.chapter_verses("Gen", 1);
        let inner = Counting::new(10.0);
        let memo = Mutex::new(MeasureMemo::new());
        let cfg = wide();

        let first = layout_chapter(verses, &Memoized::new(&memo, 7, &inner), &cfg);
        let cold = inner.calls.get();
        // 12 runs are laid out (2 verse numbers + 10 tokens); "the" and "God"
        // each recur, so only 10 distinct ones cross.
        assert_eq!(cold, 10, "cold layout crosses once per DISTINCT run");

        let second = layout_chapter(verses, &Memoized::new(&memo, 7, &inner), &cfg);
        assert_eq!(inner.calls.get(), cold, "a re-layout must cross zero times");

        // …and it is the same layout, not merely a cheaper one.
        let boxes = |dl: &crate::DisplayList| -> Vec<(String, i32, i32, i32)> {
            dl.items.iter().map(|i| (i.text.clone(), i.x as i32, i.y as i32, i.w as i32)).collect()
        };
        assert_eq!(boxes(&first), boxes(&second));
        assert_eq!(first.height, second.height);
    }

    #[test]
    fn a_new_font_identity_never_reuses_an_old_width() {
        let memo = Mutex::new(MeasureMemo::new());

        let small = Counting::new(10.0);
        let m = Memoized::new(&memo, 1, &small);
        assert_eq!(m.text_width("God"), 30.0);
        assert_eq!(small.calls.get(), 1);

        // Same text, same shared memo, a different font identity: the width must
        // be re-measured, and the old entries must not still be sitting there.
        let big = Counting::new(20.0);
        let m = Memoized::new(&memo, 2, &big);
        assert_eq!(m.entries(), 0, "retuning drops the old font's widths");
        assert_eq!(m.text_width("God"), 60.0, "the new font's width, not the old one");
        assert_eq!(big.calls.get(), 1);

        // Back to the first identity: nothing of it survived, so it re-measures
        // rather than serving the big font's width.
        let m = Memoized::new(&memo, 1, &small);
        assert_eq!(m.text_width("God"), 30.0);
        assert_eq!(small.calls.get(), 2);
    }

    /// Two layouts overlapping in different fonts — a caller may lay out on a
    /// thread pool with two panes in flight. Retuning alone is not enough to keep
    /// them apart: the one already running would read the other's widths.
    #[test]
    fn two_layouts_in_different_fonts_never_read_each_others_widths() {
        let memo = Mutex::new(MeasureMemo::new());
        let small = Counting::new(10.0);
        let big = Counting::new(20.0);

        let a = Memoized::new(&memo, 1, &small);
        assert_eq!(a.text_width("God"), 30.0);

        // The second layout takes the memo over mid-flight.
        let b = Memoized::new(&memo, 2, &big);
        assert_eq!(b.text_width("God"), 60.0);

        // `a` is still running and asks for the same word again: it must be told
        // its own font's width, not the one `b` just filed.
        assert_eq!(a.text_width("God"), 30.0, "a must not be handed b's width");
        assert_eq!(small.calls.get(), 2, "so it re-measures instead of hitting");

        // …and what `a` re-measured must not have been filed under `b`'s identity.
        assert_eq!(b.text_width("God"), 60.0, "b must not be handed a's width");
        assert_eq!(big.calls.get(), 1);
    }

    #[test]
    fn a_layout_longer_than_the_bound_stays_within_it() {
        let inner = Counting::new(3.0);
        let memo = Mutex::new(MeasureMemo::new());
        let m = Memoized::new(&memo, 1, &inner);
        for i in 0..(MeasureMemo::CAP + 500) {
            m.text_width(&format!("w{i}"));
        }
        assert!(m.entries() <= MeasureMemo::CAP, "the memo must stay bounded");
        // Everything after the overflow is still remembered, so the chapter being
        // laid out when the bound is reached re-lays out free.
        assert!(m.entries() >= 500);
    }
}
