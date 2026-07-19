//! Reader text layout + per-word hit-testing.
//!
//! This is the load-bearing idea from `PLAN.md`: overlay's reader
//! (`ReaderView.hs`) lays out and hit-tests **every word individually** so
//! Strong's clicks, hover cards, and cross-pane weave connectors all ride on
//! one layout. We keep that layout in shared Rust — but instead of forcing a
//! shaping engine (cosmic-text) on every platform, the *algorithm* (greedy
//! line-breaking, word-rectangle assembly, hit-testing) lives here and takes
//! text **measurements as input**. Each native UI supplies a [`Measure`]
//! backed by its own excellent text stack (Pango on GTK, DirectWrite on WinUI,
//! Android's text engine) and paints at the positions this crate computes.
//!
//! The payoff: the hard bookkeeping is written once and unit-tested with
//! synthetic metrics; per-word hit regions are always consistent with what the
//! platform actually painted (same engine measured and drew them); and native
//! text quality is preserved per platform.

use pure_core::corpus::{Verse, FLAG_PARA, FLAG_TITLE};
use pure_core::VRef;

/// Something that can measure the advance width of a run of text in the
/// reader's scripture font at the current size. Implemented by each UI over
/// its native text stack; a synthetic monospace impl backs the tests.
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
    /// verse numbers. See `pure_core::corpus::FLAG_*`.
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
    /// return `None`). This is what a native UI calls on click/hover.
    pub fn hit_test(&self, px: f32, py: f32) -> Option<Hit> {
        self.items.iter().find_map(|it| {
            if it.contains(px, py) {
                it.word().map(|(v, ti)| Hit {
                    verse: v.clone(),
                    token_index: ti,
                    strongs: it.strongs.clone(),
                })
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
        if pen.line_started
            && verse.tokens.first().is_some_and(|t| t.has_flag(FLAG_PARA))
        {
            pen.y += cfg.para_spacing;
            pen.newline(cfg);
            pen.x = cfg.para_indent;
        } else if cfg.verse_break && pen.line_started {
            // Verse-per-line mode: every verse starts a fresh line.
            pen.newline(cfg);
        }

        // Verse number marker. Wrap it together with the first word — a
        // number alone at the end of a line orphans it from its verse.
        let num = verse.verse.to_string();
        let num_w = m.text_width(&num);
        // Measure the first token once: text_width is a native shaping call
        // (Pango/DirectWrite) and render() heap-allocates. The result is reused
        // for both the number/first-word wrap check here and placing the word
        // at ti == 0 below, instead of measuring it twice per verse.
        let mut first_measured = verse.tokens.first().map(|t| {
            let text = t.render();
            let w = m.text_width(&text);
            (text, w)
        });
        let first_w = first_measured.as_ref().map_or(0.0, |(_, w)| *w);
        if pen.line_started && pen.x + num_w + cfg.verse_num_gap + first_w > cfg.width {
            pen.newline(cfg);
        }
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
        pen.x += num_w + cfg.verse_num_gap;
        pen.line_started = true;

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

    let height = if items.is_empty() { 0.0 } else { pen.y + cfg.line_height };
    DisplayList { items, height, width: cfg.width }
}

/// A convenience: does a token carry a superscription flag? (Psalm titles are
/// often styled differently by the UI.) Re-exported so shells don't reach into
/// `pure_core` just for the constant.
pub fn is_title_flag(flags: u32) -> bool {
    flags & FLAG_TITLE != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use pure_core::corpus;

    /// Monospace measurement: every character is `char_w` wide. Deterministic,
    /// so layout is exactly predictable in tests.
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
        };
        let dl = layout_chapter(verses, &m, &cfg);

        // 1 verse number + 4 words
        assert_eq!(dl.items.len(), 5);

        // Hit-test the word "God" (token index 3): find its box, click its centre.
        let god = dl
            .items
            .iter()
            .find(|it| it.word() == Some((&VRef::new("Gen", 1, 1), 3)))
            .unwrap();
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
        let distinct_ys: std::collections::BTreeSet<i32> =
            dl.items.iter().map(|it| it.y as i32).collect();
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
        // The paragraph line starts with Gen 2:1's *verse number* at the
        // indent, its first word right after — the number must not strand at
        // the end of the previous line (the pre-fix behavior).
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
    use pure_core::corpus;

    struct Mono;
    impl Measure for Mono {
        fn text_width(&self, text: &str) -> f32 {
            text.chars().count() as f32 * 10.0
        }
    }

    /// Two verses; verse 2's FIRST word carries ¶ — the verse number must
    /// move to the new indented line with its verse, not strand at the end
    /// of the previous one (REVIEW 2026-07-14 correctness #2).
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
        };
        let dl = layout_chapter(c.chapter_verses("Gen", 1), &Mono, &cfg);

        let num2 = dl
            .items
            .iter()
            .find(|it| matches!(it.kind, ItemKind::VerseNumber(2)))
            .expect("verse 2 number placed");
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
        };
        let dl = layout_chapter(c.chapter_verses("Gen", 1), &Mono, &cfg);
        let num2 = dl
            .items
            .iter()
            .find(|it| matches!(it.kind, ItemKind::VerseNumber(2)))
            .unwrap();
        let wide = dl.items.iter().find(|it| it.text == "wide").unwrap();
        assert_eq!(num2.y, wide.y, "the number wraps as a unit with its word");
    }
}
