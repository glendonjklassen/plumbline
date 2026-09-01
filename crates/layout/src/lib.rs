//! Reader text layout + per-word hit-testing.
//!
//! The algorithm (greedy line-breaking, word-rectangle assembly, hit-testing)
//! lives here and takes text measurements as input: a shell supplies a
//! [`Measure`] backed by its own text stack, gets back a display list of
//! positioned boxes, paints them, and sends taps back for hit-testing. Because
//! the same engine measured and drew the text, hit regions always agree with
//! what was painted.

use plumbline_core::corpus::{Verse, FLAG_PARA, FLAG_TITLE};
use plumbline_core::VRef;

pub mod memo;

pub use memo::{MeasureMemo, Memoized};

/// Measures the advance width of a run of text in the reader's scripture font at
/// the current size. Implemented by each shell over its native text stack; a
/// synthetic monospace impl backs the tests.
///
/// A measurement is a call out of Rust into the shell's text stack, and scripture
/// repeats itself, so callers should wrap their impl in [`Memoized`].
pub trait Measure {
    /// Advance width of `text` in device pixels.
    fn text_width(&self, text: &str) -> f32;
}

/// Layout parameters, all in device pixels.
#[derive(Debug, Clone, Copy)]
pub struct LayoutConfig {
    /// Wrapping width of the reading column.
    pub width: f32,
    /// Baseline-to-baseline line height.
    pub line_height: f32,
    /// Width of an inter-word space.
    pub space_width: f32,
    /// Gap after a verse number before its first word.
    pub verse_num_gap: f32,
    /// Left indent applied to the first line of a new paragraph (¶).
    pub para_indent: f32,
    /// Vertical gap added above a paragraph break.
    pub para_spacing: f32,
    /// Start every verse on a fresh line (verse-per-line reading mode)
    /// instead of flowing verses continuously.
    pub verse_break: bool,
    /// Paint the small leading verse numbers. Decided here rather than in a
    /// shell's paint step: a number a shell declines to draw still holds its
    /// width and gap, so the text would flow around an invisible marker.
    pub verse_numbers: bool,
    /// Lay the line out right to left, for a script that reads that way.
    ///
    /// Set from the reading language's registry row (`i18n::Lang::is_rtl`), not
    /// from the shell's own idea of direction: the open corpus decides, since a
    /// reader whose translation has not downloaded is still reading the KJV.
    pub rtl: bool,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        LayoutConfig {
            width: 640.0,
            line_height: 28.0,
            space_width: 6.0,
            verse_num_gap: 4.0,
            para_indent: 16.0,
            para_spacing: 8.0,
            verse_break: false,
            verse_numbers: true,
            rtl: false,
        }
    }
}

/// What a placed box represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    /// A verse number marker (the small leading number).
    VerseNumber(u16),
    /// A rendered word token, hit-testable back to its verse + token index.
    Word { verse: VRef, token_index: u32 },
}

/// One positioned box in the display list: where to paint it, what text, and
/// (for words) the identity + styling a UI needs to render and hit-test it.
#[derive(Debug, Clone)]
pub struct PlacedItem {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The exact glyphs to paint (`pre + word + post` for a word token).
    pub text: String,
    pub kind: ItemKind,
    /// Token flag bits (added / divine / title / paragraph) for styling; 0 for
    /// verse numbers. See `plumbline_core::corpus::FLAG_*`.
    pub flags: u32,
    /// Strong's refs on this word (empty for untagged words / verse numbers).
    pub strongs: Vec<String>,
}

impl PlacedItem {
    /// Whether a point falls inside this box.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    /// The word identity, if this is a word.
    pub fn word(&self) -> Option<(&VRef, u32)> {
        match &self.kind {
            ItemKind::Word { verse, token_index } => Some((verse, *token_index)),
            ItemKind::VerseNumber(_) => None,
        }
    }
}

/// A laid-out chapter: positioned boxes plus the total painted size.
#[derive(Debug, Clone)]
pub struct DisplayList {
    pub items: Vec<PlacedItem>,
    /// Total height needed (for scrollbar extent).
    pub height: f32,
    /// The column width the layout targeted.
    pub width: f32,
    /// Whether these boxes were mirrored for a right-to-left text. Carried out
    /// with the coordinates so a painter cannot disagree with them: the platform
    /// places a trailing full stop on the side the context's direction says.
    pub rtl: bool,
}

/// What a tap/hover resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub verse: VRef,
    pub token_index: u32,
    pub strongs: Vec<String>,
}

impl DisplayList {
    /// Resolve a point to the word under it, if any (verse numbers and gaps
    /// return `None`). What a shell calls on click/hover.
    pub fn hit_test(&self, px: f32, py: f32) -> Option<Hit> {
        self.items.iter().find_map(|it| {
            if it.contains(px, py) {
                it.word().map(|(v, ti)| Hit { verse: v.clone(), token_index: ti, strongs: it.strongs.clone() })
            } else {
                None
            }
        })
    }
}

/// A tiny cursor tracking the current pen position while placing boxes.
struct Pen {
    x: f32,
    y: f32,
    line_started: bool,
}

impl Pen {
    fn newline(&mut self, cfg: &LayoutConfig) {
        self.x = 0.0;
        self.y += cfg.line_height;
        self.line_started = false;
    }
}

/// Lay out a chapter's verses into a display list for a column of
/// `cfg.width`. Greedy word-wrap; each rendered token becomes one hit-testable
/// box; a paragraph flag (¶) starts a new, indented line.
pub fn layout_chapter<M: Measure>(verses: &[Verse], m: &M, cfg: &LayoutConfig) -> DisplayList {
    let mut items: Vec<PlacedItem> = Vec::new();
    let mut pen = Pen { x: 0.0, y: 0.0, line_started: false };

    for verse in verses {
        let vref = verse.vref();

        // A paragraph flag on the verse's *first* word breaks the line before
        // the verse number is placed — otherwise the number strands at the end
        // of the previous line while its verse starts the next one.
        if pen.line_started && verse.tokens.first().is_some_and(|t| t.has_flag(FLAG_PARA)) {
            pen.y += cfg.para_spacing;
            pen.newline(cfg);
            pen.x = cfg.para_indent;
        } else if cfg.verse_break && pen.line_started {
            // Verse-per-line mode: every verse starts a fresh line.
            pen.newline(cfg);
        }

        // Verse number marker, wrapped together with the first word so a number
        // never orphans at the end of a line. Numbers off: no marker, no gap and
        // no wrap check (a zero `num_w` would still reserve `verse_num_gap`).
        let num = verse.verse.to_string();
        let num_w = if cfg.verse_numbers { m.text_width(&num) } else { 0.0 };
        // Measure the first token once — `text_width` crosses into the shell and
        // `render()` allocates — and reuse it for the wrap check here and for
        // placing the word at ti == 0 below.
        let mut first_measured = verse.tokens.first().map(|t| {
            let text = t.render();
            let w = m.text_width(&text);
            (text, w)
        });
        let first_w = first_measured.as_ref().map_or(0.0, |(_, w)| *w);
        let lead = if cfg.verse_numbers { num_w + cfg.verse_num_gap } else { 0.0 };
        if pen.line_started && pen.x + lead + first_w > cfg.width {
            pen.newline(cfg);
        }
        if cfg.verse_numbers {
            items.push(PlacedItem {
                x: pen.x,
                y: pen.y,
                w: num_w,
                h: cfg.line_height,
                text: num,
                kind: ItemKind::VerseNumber(verse.verse),
                flags: 0,
                strongs: Vec::new(),
            });
            pen.x += lead;
            pen.line_started = true;
        }

        for (ti, token) in verse.tokens.iter().enumerate() {
            // Paragraph break: start a fresh, indented line before this word
            // (the first word's break already happened before the number).
            if ti > 0 && token.has_flag(FLAG_PARA) && pen.line_started {
                pen.y += cfg.para_spacing;
                pen.newline(cfg);
                pen.x = cfg.para_indent;
            }

            let (text, w) = match first_measured.take() {
                Some(fm) if ti == 0 => fm,
                _ => {
                    let text = token.render();
                    let w = m.text_width(&text);
                    (text, w)
                }
            };

            // A token that renders to nothing paints nothing and consumes no
            // space. The AKJV overlay blanks the interior tokens of a re-rendered
            // run rather than removing them, so `ti` stays the corpus token index
            // and Strong's lookups still resolve.
            if text.is_empty() {
                continue;
            }

            if pen.line_started && pen.x + w > cfg.width {
                pen.newline(cfg);
            }

            items.push(PlacedItem {
                x: pen.x,
                y: pen.y,
                w,
                h: cfg.line_height,
                text,
                kind: ItemKind::Word { verse: vref.clone(), token_index: ti as u32 },
                flags: token.flags,
                strongs: token.strongs.clone(),
            });
            pen.x += w + cfg.space_width;
            pen.line_started = true;
        }
    }

    // Right-to-left is one mirror over the finished list. Widths do not depend
    // on direction, so the greedy fill above already put the right words on each
    // line; only each box's origin is wrong, and `width - x - w` reflects it.
    // Hit-testing needs no change — `contains` is pure geometry and the boxes
    // mirror with the text.
    //
    // A mirror suffices only because the RTL corpus carries no Latin letters and
    // no digits (`data/svd1865.jsonl`, verified by `check-svd.py`), so there is
    // no bidirectional run for UAX #9 to resolve. A corpus with a Latin quotation
    // inside an Arabic verse would need UAX #9: a mirror reverses that run's
    // word order.
    if cfg.rtl {
        for it in &mut items {
            it.x = cfg.width - it.x - it.w;
        }
    }

    let height = if items.is_empty() { 0.0 } else { pen.y + cfg.line_height };
    DisplayList { items, height, width: cfg.width, rtl: cfg.rtl }
}

/// Does a token carry a superscription flag (psalm titles, often styled
/// differently)? Here so shells need not reach into `plumbline_core` for it.
pub fn is_title_flag(flags: u32) -> bool {
    flags & FLAG_TITLE != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use plumbline_core::corpus;

    /// Monospace measurement: every character is `char_w` wide, so layout is
    /// exactly predictable.
    struct Mono {
        char_w: f32,
    }
    impl Measure for Mono {
        fn text_width(&self, text: &str) -> f32 {
            text.chars().count() as f32 * self.char_w
        }
    }

    const SAMPLE: &str = concat!(
        r#"{"format":"x","tokenization":"kjv1769-tok2","verses":2}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","In","",[],0],["","the","",[],0],["","beginning","",["H7225"],0],["","God","",["H430"],0]],"v":1}"#,
        "\n",
        r#"{"b":"Gen","c":2,"t":[["","Thus","",[],8],["","the","",[],0],["","heavens",".",["H8064"],0]],"v":1}"#,
    );

    /// The Arabic corpus's own first verse, at its real addresses.
    const ARABIC: &str = concat!(
        r#"{"format":"x","tokenization":"svd1865-tok1","verses":1}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","فِي","",[],8],["","ٱلْبَدْءِ","",[],0],["","خَلَقَ","",[],0],["","ٱللهُ","",[],0],["","ٱلسَّمَاوَاتِ","",[],0],["","وَٱلْأَرْضَ",".",[],0]],"v":1}"#,
    );

    /// The CUV corpus's first verse: per-character tokens, the full stop glued
    /// into the last token's `post` (`build-cuv.py`).
    const CHINESE: &str = concat!(
        r#"{"format":"x","tokenization":"cuv1919t-tok1","verses":1}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","起","",[],0],["","初","",[],0],["","神","",[],0],["","創","",[],0],["","造","",[],0],["","天","",[],0],["","地","。",[],0]],"v":1}"#,
    );

    /// A Han corpus uses the same greedy fill with `space_width` zeroed (the FFI
    /// derives that from the tokenization stamp, as it derives `rtl`). Fails
    /// against a spaced-script default: characters must set snug (each box
    /// starting where the last ended, no gap between every pair) and a narrow
    /// column must break between any two, since break opportunities are token
    /// boundaries here. Kinsoku rides in the tokens — the glued 。 is inside its
    /// character's box and can never open a line.
    #[test]
    fn cjk_sets_snug_and_breaks_between_any_two_characters() {
        let c = corpus::from_str(CHINESE).unwrap();
        let verses = c.chapter_verses("Gen", 1);
        let m = Mono { char_w: 10.0 };

        let wide = LayoutConfig { width: 10_000.0, space_width: 0.0, verse_numbers: false, ..Default::default() };
        let dl = layout_chapter(verses, &m, &wide);
        assert_eq!(dl.items.len(), 7);
        for pair in dl.items.windows(2) {
            assert_eq!(pair[1].x, pair[0].x + pair[0].w, "a gap opened before {:?}", pair[1].text);
        }

        // 45px column, 10px characters: four fit, the fifth wraps — a break
        // inside what a spaced script would call a word. The last token is
        // "地。" (20px), so the second line holds three boxes.
        let narrow = LayoutConfig { width: 45.0, space_width: 0.0, verse_numbers: false, ..Default::default() };
        let dl = layout_chapter(verses, &m, &narrow);
        let first_y = dl.items[0].y;
        let first_line: Vec<&PlacedItem> = dl.items.iter().filter(|it| it.y == first_y).collect();
        assert_eq!(first_line.len(), 4, "expected four characters on the first line");
        let second = &dl.items[4];
        assert_eq!(second.x, 0.0, "the wrapped character must open its line at the margin");
        assert!(second.y > first_y);
    }

    /// Right-to-left is the same layout, reflected. Without the mirror every
    /// assertion here fails: Arabic packs from the left edge rightwards, putting
    /// the verse's first word where a reader looks for its last while each
    /// individual word still renders perfectly. The properties are checked
    /// separately because a mirror that got one wrong would still look plausible.
    #[test]
    fn rtl_mirrors_the_line_without_relaying_it_out() {
        let c = corpus::from_str(ARABIC).unwrap();
        let verses = c.chapter_verses("Gen", 1);
        let m = Mono { char_w: 10.0 };
        // Narrow enough to force several lines, so this tests wrapping and not
        // just one row of boxes.
        let ltr = LayoutConfig { width: 220.0, verse_numbers: true, ..Default::default() };
        let rtl = LayoutConfig { rtl: true, ..ltr };

        let a = layout_chapter(verses, &m, &ltr);
        let b = layout_chapter(verses, &m, &rtl);

        // 1. The line breaks are the same — the claim that lets the greedy fill
        //    stay untouched.
        assert!(a.items.len() > 6, "the sample must wrap for this test to mean anything");
        assert_eq!(a.height, b.height);
        let ys: Vec<f32> = a.items.iter().map(|i| i.y).collect();
        let ys_rtl: Vec<f32> = b.items.iter().map(|i| i.y).collect();
        assert_eq!(ys, ys_rtl, "a word changed line when the direction flipped");

        // 2. Every box is reflected, and stays inside the column.
        for (l, r) in a.items.iter().zip(&b.items) {
            assert_eq!(r.text, l.text, "the mirror reordered the item list");
            assert_eq!(r.x, ltr.width - l.x - l.w);
            assert!(r.x >= -0.01 && r.x + r.w <= ltr.width + 0.01, "{:?} left the column", r.text);
        }

        // 3. The line begins at the right edge — what a reader would notice, and
        //    what no coordinate assertion above states. The verse number leads,
        //    with the verse's first word inboard of it.
        let first = b.items.iter().find(|i| i.text == "فِي").unwrap();
        let last = b.items.iter().find(|i| i.text.starts_with("وَٱلْأَرْضَ")).unwrap();
        assert!(first.x > last.x, "the verse reads left to right");
        let num = b.items.iter().find(|i| matches!(i.kind, ItemKind::VerseNumber(1))).unwrap();
        assert_eq!(num.x + num.w, ltr.width, "the verse number does not sit at the right edge");
        assert!(num.x > first.x, "the verse number is not outboard of the first word");
        let rightmost_word =
            b.items.iter().filter(|i| i.y == first.y && i.word().is_some()).fold(f32::MIN, |acc, i| acc.max(i.x));
        assert_eq!(first.x, rightmost_word, "the verse's first word is not the rightmost word on its line");

        // 4. Hit-testing follows the boxes, with no direction logic of its own.
        let hit = b.hit_test(first.x + 1.0, first.y + 1.0).expect("no word under the first word's box");
        assert_eq!(hit.token_index, 0);
    }

    #[test]
    fn lays_out_and_hit_tests() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let verses = c.chapter_verses("Gen", 1);
        let m = Mono { char_w: 10.0 };
        let cfg = LayoutConfig {
            width: 10_000.0, // wide: everything on one line
            line_height: 20.0,
            space_width: 5.0,
            verse_num_gap: 4.0,
            para_indent: 16.0,
            para_spacing: 8.0,
            verse_break: false,
            verse_numbers: true,
            rtl: false,
        };
        let dl = layout_chapter(verses, &m, &cfg);

        // 1 verse number + 4 words
        assert_eq!(dl.items.len(), 5);

        // Hit-test the word "God" (token index 3): find its box, click its centre.
        let god = dl.items.iter().find(|it| it.word() == Some((&VRef::new("Gen", 1, 1), 3))).unwrap();
        assert_eq!(god.text, "God");
        let hit = dl.hit_test(god.x + 1.0, god.y + 1.0).unwrap();
        assert_eq!(hit.token_index, 3);
        assert_eq!(hit.strongs, vec!["H430".to_string()]);

        // Clicking the verse number resolves to no word.
        let num = &dl.items[0];
        assert!(matches!(num.kind, ItemKind::VerseNumber(1)));
        assert!(dl.hit_test(num.x + 1.0, num.y + 1.0).is_none());
    }

    #[test]
    fn wraps_to_multiple_lines() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let verses = c.chapter_verses("Gen", 1);
        let m = Mono { char_w: 10.0 };
        // Narrow column forces wrapping.
        let cfg = LayoutConfig { width: 60.0, ..Default::default() };
        let dl = layout_chapter(verses, &m, &cfg);
        let distinct_ys: std::collections::BTreeSet<i32> = dl.items.iter().map(|it| it.y as i32).collect();
        assert!(distinct_ys.len() > 1, "expected multiple lines, got {distinct_ys:?}");
        assert!(dl.height >= cfg.line_height);
    }

    #[test]
    fn paragraph_flag_breaks_line() {
        let c = corpus::from_str(SAMPLE).unwrap();
        // Gen 2:1's first token carries FLAG_PARA (8). Lay out both chapters'
        // verses together so the paragraph break has a preceding line.
        let mut verses = c.chapter_verses("Gen", 1).to_vec();
        verses.extend_from_slice(c.chapter_verses("Gen", 2));
        let m = Mono { char_w: 8.0 };
        let cfg = LayoutConfig { width: 10_000.0, ..Default::default() };
        let dl = layout_chapter(&verses, &m, &cfg);
        // The paragraph line starts with Gen 2:1's verse number at the indent,
        // its first word right after: the number must not strand at the end of
        // the previous line.
        let num = dl
            .items
            .iter()
            .find(|it| matches!(it.kind, ItemKind::VerseNumber(1)) && it.y > 0.0)
            .expect("Gen 2:1's number on the paragraph line");
        let thus = dl.items.iter().find(|it| it.text == "Thus").unwrap();
        assert_eq!(num.x, cfg.para_indent);
        assert_eq!(num.y, thus.y);
        assert_eq!(thus.x, cfg.para_indent + num.w + cfg.verse_num_gap);
    }
}

#[cfg(test)]
mod para_tests {
    use super::*;
    use plumbline_core::corpus;

    struct Mono;
    impl Measure for Mono {
        fn text_width(&self, text: &str) -> f32 {
            text.chars().count() as f32 * 10.0
        }
    }

    /// Verse 2's first word carries ¶: the verse number must move to the new
    /// indented line with its verse, not strand at the end of the previous one.
    #[test]
    fn paragraph_break_carries_the_verse_number() {
        let sample = concat!(
            r#"{"format":"x","tokenization":"kjv1769-tok2","verses":2}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","In","",[],0],["","the","",[],0]],"v":1}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","Thus","",[],8],["","ended","",[],0]],"v":2}"#,
        );
        let c = corpus::from_str(sample).unwrap();
        let cfg = LayoutConfig {
            width: 10_000.0,
            line_height: 20.0,
            space_width: 5.0,
            verse_num_gap: 4.0,
            para_indent: 16.0,
            para_spacing: 8.0,
            verse_break: false,
            verse_numbers: true,
            rtl: false,
        };
        let dl = layout_chapter(c.chapter_verses("Gen", 1), &Mono, &cfg);

        let num2 =
            dl.items.iter().find(|it| matches!(it.kind, ItemKind::VerseNumber(2))).expect("verse 2 number placed");
        let thus = dl.items.iter().find(|it| it.text == "Thus").unwrap();
        let v1_word = dl.items.iter().find(|it| it.text == "In").unwrap();

        // The number sits on the same (new) line as its first word…
        assert_eq!(num2.y, thus.y, "number must ride the paragraph break");
        // …below verse 1's line, at the paragraph indent.
        assert!(num2.y > v1_word.y);
        assert_eq!(num2.x, cfg.para_indent);
    }

    /// A verse number never sits alone at the end of a full line — it wraps
    /// together with its first word.
    #[test]
    fn verse_number_wraps_with_its_first_word() {
        let sample = concat!(
            r#"{"format":"x","tokenization":"kjv1769-tok2","verses":2}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","abcdefgh","",[],0]],"v":1}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","wide","",[],0]],"v":2}"#,
        );
        let c = corpus::from_str(sample).unwrap();
        // Width fits "1 abcdefgh" (10+4+80=94) and then the "2" (10) — but not
        // "2" + gap + "wide": the number must wrap, not strand at x=99.
        let cfg = LayoutConfig {
            width: 110.0,
            line_height: 20.0,
            space_width: 5.0,
            verse_num_gap: 4.0,
            para_indent: 16.0,
            para_spacing: 8.0,
            verse_break: false,
            verse_numbers: true,
            rtl: false,
        };
        let dl = layout_chapter(c.chapter_verses("Gen", 1), &Mono, &cfg);
        let num2 = dl.items.iter().find(|it| matches!(it.kind, ItemKind::VerseNumber(2))).unwrap();
        let wide = dl.items.iter().find(|it| it.text == "wide").unwrap();
        assert_eq!(num2.y, wide.y, "the number wraps as a unit with its word");
    }

    /// Numbers off reclaims the space they held: not just the number box but its
    /// gap, or every verse starts behind an invisible marker.
    #[test]
    fn verse_numbers_off_emits_none_and_reclaims_their_width() {
        let sample = concat!(
            r#"{"format":"x","tokenization":"kjv1769-tok2","verses":2}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","alpha","",[],0]],"v":1}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","beta","",[],0]],"v":2}"#,
        );
        let c = corpus::from_str(sample).unwrap();
        let base = LayoutConfig {
            width: 10_000.0,
            line_height: 20.0,
            space_width: 5.0,
            verse_num_gap: 4.0,
            para_indent: 16.0,
            para_spacing: 8.0,
            verse_break: false,
            verse_numbers: true,
            rtl: false,
        };
        let off = LayoutConfig { verse_numbers: false, ..base };

        let with = layout_chapter(c.chapter_verses("Gen", 1), &Mono, &base);
        let without = layout_chapter(c.chapter_verses("Gen", 1), &Mono, &off);

        assert!(with.items.iter().any(|it| matches!(it.kind, ItemKind::VerseNumber(_))));
        assert!(
            !without.items.iter().any(|it| matches!(it.kind, ItemKind::VerseNumber(_))),
            "numbers off must emit no number boxes"
        );

        // The text starts at the margin, not behind a ghost marker…
        let alpha = without.items.iter().find(|it| it.text == "alpha").unwrap();
        assert_eq!(alpha.x, 0.0);
        // …and the saving is cumulative along the line: by verse 2 the text has
        // moved left by both numbers' width plus both gaps (each "N" is 10 wide
        // under Mono, and the gap is 4).
        let beta_with = with.items.iter().find(|it| it.text == "beta").unwrap();
        let beta_without = without.items.iter().find(|it| it.text == "beta").unwrap();
        assert_eq!(beta_with.x - beta_without.x, 2.0 * (10.0 + base.verse_num_gap));
        // Verses still flow with one space between them, not jammed together.
        assert_eq!(beta_without.x, 5.0 * 10.0 + base.space_width);
    }
}
