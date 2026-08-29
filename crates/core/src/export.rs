//! Formatting verses and chapters for the clipboard.
//!
//! There is no clipboard logic here — only the *text* a shell copies, produced
//! once so GTK/WinUI/Compose all yield byte-identical output (a verse copied on
//! Windows pastes the same as one copied on Linux). The shell owns the actual
//! `gdk::Clipboard` / `DataPackage` call; it asks this module for the string.
//!
//! Three shapes, matching the context-menu affordances (Tier 0 #1):
//! - **plain** — just the verse/chapter text;
//! - **ref-suffixed** — `…text — John 3:16 (KJV)`;
//! - **markdown** — a blockquote with an attribution line, for pasting into
//!   notes apps and chat.

use crate::corpus::{render_tokens, Corpus, Verse};
use crate::reference::VRef;

/// The bundled edition — the attribution suffix on ref-suffixed / markdown copy.
const EDITION: &str = "KJV";

/// What to copy and in which shape. The shell maps each context-menu item to one
/// variant; [`parse_kind`] turns the FFI token back into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyKind {
    /// The verse text alone.
    Verse,
    /// The verse text followed by ` — <display> (KJV)`.
    VerseRef,
    /// The verse as a markdown blockquote with an attribution line.
    VerseMarkdown,
    /// The whole chapter, one `N text` line per verse, then a trailing ref.
    Chapter,
    /// The whole chapter as a markdown blockquote (bold verse numbers).
    ChapterMarkdown,
}

impl CopyKind {
    /// The frozen wire token (for the FFI copy endpoint).
    pub fn token(self) -> &'static str {
        match self {
            CopyKind::Verse => "verse",
            CopyKind::VerseRef => "verseRef",
            CopyKind::VerseMarkdown => "verseMarkdown",
            CopyKind::Chapter => "chapter",
            CopyKind::ChapterMarkdown => "chapterMarkdown",
        }
    }
}

/// Parse a wire token into a [`CopyKind`]; `None` for an unknown token.
pub fn parse_kind(t: &str) -> Option<CopyKind> {
    Some(match t {
        "verse" => CopyKind::Verse,
        "verseRef" => CopyKind::VerseRef,
        "verseMarkdown" => CopyKind::VerseMarkdown,
        "chapter" => CopyKind::Chapter,
        "chapterMarkdown" => CopyKind::ChapterMarkdown,
        _ => return None,
    })
}

/// The full text of one verse: superscription (psalm title) then body, so
/// nothing is silently dropped when a psalm's v1 carries a title.
fn verse_text(v: &Verse) -> String {
    let title = v.title();
    let body = v.body();
    match (title.is_empty(), body.is_empty()) {
        (false, false) => format!("{title} {body}"),
        (false, true) => title,
        _ => body,
    }
}

/// Produce the clipboard text for `vref` (its chapter, for the `Chapter*` kinds)
/// in the requested shape. `None` when the verse/chapter isn't in the corpus.
/// `lang` IS A PARAMETER, not `i18n::active()` read inside.
///
/// This function renders text for one reader, so the language is an input to
/// it — the same reason [`VRef::display_in`] exists beside `VRef::display`. It
/// was a hidden global read, and the cost showed up the moment a test wanted to
/// check the German shape: `set_active` is process-wide, the test binary runs
/// in parallel, and one test switching the language broke two others that were
/// asserting English in another thread.
pub fn copy_text(corpus: &Corpus, vref: &VRef, kind: CopyKind, lang: crate::i18n::Lang) -> Option<String> {
    match kind {
        CopyKind::Verse | CopyKind::VerseRef | CopyKind::VerseMarkdown => {
            let v = corpus.verse(vref)?;
            let text = verse_text(v);
            Some(match kind {
                CopyKind::Verse => text,
                CopyKind::VerseRef => format!("{text} — {} ({EDITION})", vref.display_in(lang)),
                CopyKind::VerseMarkdown => {
                    format!("> {text}\n>\n> — *{}* ({EDITION})", vref.display_in(lang))
                }
                _ => unreachable!(),
            })
        }
        CopyKind::Chapter | CopyKind::ChapterMarkdown => {
            let verses = corpus.chapter_verses(&vref.book, vref.chapter);
            if verses.is_empty() {
                return None;
            }
            // The SAME bug as the plan card's, and worse for sitting six lines
            // under two `vref.display_in(lang)` calls that get it right: copying a
            // VERSE gave a Hindi reader "यूहन्ना 3:16" and copying the CHAPTER
            // it is in gave them "John 3". `canon::display_name` is the frozen
            // English table that `refKey` is built from, and it is never what
            // reaches a reader.
            let chapter_ref = vref.chapter_display_in(lang);
            let mut out = String::new();
            for v in verses {
                let title = v.title();
                if !title.is_empty() {
                    out.push_str(&title);
                    out.push('\n');
                }
                match kind {
                    CopyKind::Chapter => {
                        out.push_str(&format!("{} {}\n", v.verse, v.body()));
                    }
                    CopyKind::ChapterMarkdown => {
                        out.push_str(&format!("**{}** {}\n", v.verse, v.body()));
                    }
                    _ => unreachable!(),
                }
            }
            Some(match kind {
                CopyKind::Chapter => format!("{}\n— {chapter_ref} ({EDITION})", out.trim_end()),
                CopyKind::ChapterMarkdown => {
                    // A blockquote of the whole chapter.
                    let quoted = out.trim_end().lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n");
                    format!("{quoted}\n>\n> — *{chapter_ref}* ({EDITION})")
                }
                _ => unreachable!(),
            })
        }
    }
}

/// Space-joined body of a run of verses (used by tests / callers wanting the raw
/// text without numbering). Kept small; the shaped variants above are the API.
pub fn plain_join<'a, I: IntoIterator<Item = &'a Verse>>(verses: I) -> String {
    verses.into_iter().map(|v| render_tokens(v.tokens.iter())).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::from_str;

    const SAMPLE: &str = concat!(
        r#"{"format":"overlay-kjv-canonical","tokenization":"kjv1769-tok2","verses":3}"#,
        "\n",
        r#"{"b":"John","c":3,"t":[["","For","",[],0],["","God","",["H430"],0],["","so","",[],0],["","loved","",[],0]],"v":16}"#,
        "\n",
        r#"{"b":"John","c":3,"t":[["","For","",[],0],["","God","",[],0],["","sent","",[],0]],"v":17}"#,
        "\n",
        r#"{"b":"John","c":3,"t":[["","He","",[],0],["","that","",[],0],["","believeth","",[],0]],"v":18}"#,
    );

    #[test]
    fn verse_shapes() {
        let c = from_str(SAMPLE).unwrap();
        let v = VRef::new("John", 3, 16);
        let en = crate::i18n::Lang::En;
        assert_eq!(copy_text(&c, &v, CopyKind::Verse, en).unwrap(), "For God so loved");
        assert_eq!(copy_text(&c, &v, CopyKind::VerseRef, en).unwrap(), "For God so loved — John 3:16 (KJV)");
        assert_eq!(
            copy_text(&c, &v, CopyKind::VerseMarkdown, en).unwrap(),
            "> For God so loved\n>\n> — *John 3:16* (KJV)"
        );
    }

    #[test]
    fn chapter_shapes() {
        let c = from_str(SAMPLE).unwrap();
        let v = VRef::new("John", 3, 16);
        let en = crate::i18n::Lang::En;
        let plain = copy_text(&c, &v, CopyKind::Chapter, en).unwrap();
        assert!(plain.starts_with("16 For God so loved\n17 For God sent\n18 He that believeth"));
        assert!(plain.ends_with("— John 3 (KJV)"));
        let md = copy_text(&c, &v, CopyKind::ChapterMarkdown, en).unwrap();
        assert!(md.starts_with("> **16** For God so loved"));
        assert!(md.ends_with("> — *John 3* (KJV)"));
    }

    /// A CHAPTER HEADER IS NAMED IN THE READER'S LANGUAGE, like the verse
    /// headers six lines above it in the same function.
    ///
    /// FAILS AGAINST THE BUG IT DESCRIBES: `chapter_ref` was built with
    /// `canon::display_name`, the frozen ENGLISH table that `refKey` is made
    /// from, so a German reader who copied John 3:16 got "Johannes 3,16" and
    /// the same reader copying the chapter around it got "John 3". Both
    /// assertions below pass in English, which is why this went unnoticed —
    /// the language has to be switched to see it.
    ///
    /// It takes `lang` rather than switching the process global, which is what
    /// made this testable at all — see `copy_text`.
    #[test]
    fn a_copied_chapter_is_named_in_the_readers_language() {
        let c = from_str(SAMPLE).unwrap();
        let v = VRef::new("John", 3, 16);
        let de = crate::i18n::Lang::De;
        let plain = copy_text(&c, &v, CopyKind::Chapter, de).unwrap();
        // The same book name the VERSE forms use, and the German separator with
        // it — `ref.chapter` is a catalogue template, not a space.
        assert!(plain.ends_with(&format!("— {} (KJV)", v.chapter_display_in(de))), "{plain}");
        assert!(!plain.contains("John 3"), "the English book name survived: {plain}");
        // And the verse forms agree with it, which is the half that was already
        // right and the reason the mismatch was visible on one screen.
        let one = copy_text(&c, &v, CopyKind::VerseRef, de).unwrap();
        assert!(one.contains("Johannes"), "{one}");
    }

    #[test]
    fn unknown_verse_is_none() {
        let c = from_str(SAMPLE).unwrap();
        assert!(copy_text(&c, &VRef::new("John", 99, 1), CopyKind::Verse, crate::i18n::Lang::En).is_none());
        assert!(copy_text(&c, &VRef::new("John", 99, 1), CopyKind::Chapter, crate::i18n::Lang::En).is_none());
    }

    #[test]
    fn kind_tokens_roundtrip() {
        for k in
            [CopyKind::Verse, CopyKind::VerseRef, CopyKind::VerseMarkdown, CopyKind::Chapter, CopyKind::ChapterMarkdown]
        {
            assert_eq!(parse_kind(k.token()), Some(k));
        }
        assert_eq!(parse_kind("nope"), None);
    }
}
