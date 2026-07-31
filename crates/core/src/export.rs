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
pub fn copy_text(corpus: &Corpus, vref: &VRef, kind: CopyKind) -> Option<String> {
    match kind {
        CopyKind::Verse | CopyKind::VerseRef | CopyKind::VerseMarkdown => {
            let v = corpus.verse(vref)?;
            let text = verse_text(v);
            Some(match kind {
                CopyKind::Verse => text,
                CopyKind::VerseRef => format!("{text} — {} ({EDITION})", vref.display()),
                CopyKind::VerseMarkdown => {
                    format!("> {text}\n>\n> — *{}* ({EDITION})", vref.display())
                }
                _ => unreachable!(),
            })
        }
        CopyKind::Chapter | CopyKind::ChapterMarkdown => {
            let verses = corpus.chapter_verses(&vref.book, vref.chapter);
            if verses.is_empty() {
                return None;
            }
            let chapter_ref = format!("{} {}", crate::canon::display_name(&vref.book), vref.chapter);
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
        assert_eq!(copy_text(&c, &v, CopyKind::Verse).unwrap(), "For God so loved");
        assert_eq!(copy_text(&c, &v, CopyKind::VerseRef).unwrap(), "For God so loved — John 3:16 (KJV)");
        assert_eq!(copy_text(&c, &v, CopyKind::VerseMarkdown).unwrap(), "> For God so loved\n>\n> — *John 3:16* (KJV)");
    }

    #[test]
    fn chapter_shapes() {
        let c = from_str(SAMPLE).unwrap();
        let v = VRef::new("John", 3, 16);
        let plain = copy_text(&c, &v, CopyKind::Chapter).unwrap();
        assert!(plain.starts_with("16 For God so loved\n17 For God sent\n18 He that believeth"));
        assert!(plain.ends_with("— John 3 (KJV)"));
        let md = copy_text(&c, &v, CopyKind::ChapterMarkdown).unwrap();
        assert!(md.starts_with("> **16** For God so loved"));
        assert!(md.ends_with("> — *John 3* (KJV)"));
    }

    #[test]
    fn unknown_verse_is_none() {
        let c = from_str(SAMPLE).unwrap();
        assert!(copy_text(&c, &VRef::new("John", 99, 1), CopyKind::Verse).is_none());
        assert!(copy_text(&c, &VRef::new("John", 99, 1), CopyKind::Chapter).is_none());
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
