//! The wire schemas: the JSON contract every binding decodes.
//!
//! These DTOs are deliberately **separate** from `pure_core` / `pure_layout`
//! internals. The core's own serde impls serve its frozen on-disk formats (the
//! positional token array, OSIS-keyed verses); this ABI instead speaks a
//! flat, self-describing, `camelCase` JSON that is pleasant to bind from C#,
//! Kotlin, Swift or JS and stable to evolve (new fields are additive). Verse
//! references cross the wire as compact keys (`"John 3:16"`) plus a display
//! form, so a shell needs no canon table of its own.

use serde::Serialize;

use pure_core::corpus::{Token, Verse};
use pure_core::reference::VRef;
use pure_core::search::{SearchAnswer, SearchHit};
use pure_core::strongs::StrongsEntry;
use pure_core::tag::{LoadedTag, TagTarget};
use pure_core::thread::LoadedThread;
use pure_core::weave::LoadedWeave;
use pure_layout::{DisplayList, Hit, ItemKind};

// ── table of contents ──────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct Toc {
    pub books: Vec<TocBook>,
}

#[derive(Serialize)]
pub struct TocBook {
    /// OSIS id, e.g. `"John"`.
    pub id: &'static str,
    /// Display name, e.g. `"John"` / `"1 Corinthians"`.
    pub name: &'static str,
    /// Chapters in the loaded corpus (floored at 1 for a book it lacks).
    pub chapters: u16,
}

// ── verse / token ────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireToken {
    pub pre: String,
    pub word: String,
    pub post: String,
    /// `pre + word + post`, exactly the glyphs a reader sees.
    pub render: String,
    pub flags: u32,
    pub strongs: Vec<String>,
}

pub fn token_to_wire(t: &Token) -> WireToken {
    WireToken {
        pre: t.pre.clone(),
        word: t.word.clone(),
        post: t.post.clone(),
        render: t.render(),
        flags: t.flags,
        strongs: t.strongs.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireVerse {
    /// Compact key, e.g. `"John 3:16"`.
    pub reference: String,
    /// Display form, e.g. `"John 3:16"`.
    pub display: String,
    /// Verse text without any superscription.
    pub body: String,
    /// Superscription text (psalm titles); empty for most verses.
    pub title: String,
    pub tokens: Vec<WireToken>,
}

pub fn verse_to_wire(v: &Verse) -> WireVerse {
    let vref = v.vref();
    WireVerse {
        reference: vref.ref_key(),
        display: vref.display(),
        body: v.body(),
        title: v.title(),
        tokens: v.tokens.iter().map(token_to_wire).collect(),
    }
}

// ── display list ────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireDisplayList {
    pub width: f32,
    pub height: f32,
    pub items: Vec<WireItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireItem {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The glyphs to paint (`pre + word + post` for a word).
    pub text: String,
    /// `"word"` or `"verseNumber"`.
    pub kind: &'static str,
    /// For words: the verse this token belongs to (compact key); explicit
    /// `null` for verse numbers. (Every field is always present — a strict
    /// statically-typed decoder can rely on that.)
    pub verse: Option<String>,
    /// For words: display form of `verse`; `null` for verse numbers.
    pub verse_display: Option<String>,
    /// For words: token index within the verse; `null` for verse numbers.
    pub token_index: Option<u32>,
    /// For verse numbers: the verse number; `null` for words.
    pub verse_number: Option<u16>,
    /// Token flag bits (see the `PURE_FLAG_*` constants); 0 for verse numbers.
    pub flags: u32,
    /// Strong's codes on this word (empty otherwise).
    pub strongs: Vec<String>,
}

pub fn display_list_to_wire(dl: &DisplayList) -> WireDisplayList {
    let items = dl
        .items
        .iter()
        .map(|it| {
            let (kind, verse, verse_display, token_index, verse_number) = match &it.kind {
                ItemKind::VerseNumber(n) => ("verseNumber", None, None, None, Some(*n)),
                ItemKind::Word { verse, token_index } => (
                    "word",
                    Some(verse.ref_key()),
                    Some(verse.display()),
                    Some(*token_index),
                    None,
                ),
            };
            WireItem {
                x: it.x,
                y: it.y,
                w: it.w,
                h: it.h,
                text: it.text.clone(),
                kind,
                verse,
                verse_display,
                token_index,
                verse_number,
                flags: it.flags,
                strongs: it.strongs.clone(),
            }
        })
        .collect();
    WireDisplayList { width: dl.width, height: dl.height, items }
}

// ── hit ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireHit {
    pub verse: String,
    pub display: String,
    pub token_index: u32,
    pub strongs: Vec<String>,
}

pub fn hit_to_wire(h: &Hit) -> WireHit {
    WireHit {
        verse: h.verse.ref_key(),
        display: h.verse.display(),
        token_index: h.token_index,
        strongs: h.strongs.clone(),
    }
}

// ── Strong's ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireStrongs {
    pub code: String,
    pub lemma: Option<String>,
    pub xlit: Option<String>,
    pub pron: Option<String>,
    pub deriv: Option<String>,
    pub def: Option<String>,
    pub kjv: Option<String>,
}

pub fn strongs_to_wire(code: &str, e: &StrongsEntry) -> WireStrongs {
    WireStrongs {
        code: code.to_string(),
        lemma: e.lemma.clone(),
        xlit: e.xlit.clone(),
        pron: e.pron.clone(),
        deriv: e.deriv.clone(),
        def: e.def.clone(),
        kjv: e.kjv.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Occurrences {
    pub code: String,
    pub total: usize,
    pub capped: bool,
    pub verses: Vec<String>,
}

// ── search ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WireSearch {
    /// The query resolved to a reference.
    Goto {
        book: String,
        chapter: u16,
        verse: Option<u16>,
        display: String,
    },
    /// The query matched verses.
    Hits {
        /// Human phrase describing how the hits were found.
        how: String,
        /// Honest total match count (may exceed the returned `hits`).
        total: usize,
        /// Whether `hits` was capped below `total`.
        capped: bool,
        hits: Vec<WireSearchHit>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSearchHit {
    pub verse: String,
    pub display: String,
    /// Matched a 1769 margin note rather than the verse text.
    pub note: bool,
    /// Why the hit widened past an exact match (`""` for exact/phrase).
    pub why: String,
}

fn search_hit_to_wire(h: &SearchHit) -> WireSearchHit {
    WireSearchHit {
        verse: h.vref.ref_key(),
        display: h.vref.display(),
        note: h.note,
        why: h.why.clone(),
    }
}

// ── threads ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct WireThreads {
    pub threads: Vec<WireThread>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireThread {
    pub name: String,
    pub notes: String,
    pub created: String,
    pub entries: Vec<WireThreadEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireThreadEntry {
    pub verse: String,
    pub display: String,
    pub span: [u16; 2],
    /// Snapshot of the words this entry covered when added.
    pub text: Vec<String>,
    pub note: Option<String>,
    pub added: String,
}

pub fn threads_to_wire(loaded: &[LoadedThread]) -> WireThreads {
    WireThreads {
        threads: loaded
            .iter()
            .map(|lt| {
                let t = &lt.thread;
                WireThread {
                    name: t.name.clone(),
                    notes: t.notes.clone(),
                    created: t.created.clone(),
                    entries: t
                        .entries
                        .iter()
                        .map(|e| WireThreadEntry {
                            verse: e.vref.ref_key(),
                            display: e.vref.display(),
                            span: [e.span.0, e.span.1],
                            text: e.text.clone(),
                            note: e.note.clone(),
                            added: e.added.clone(),
                        })
                        .collect(),
                }
            })
            .collect(),
    }
}

// ── tags ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct WireTags {
    pub tags: Vec<WireTag>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireTag {
    pub name: String,
    pub color: Option<String>,
    pub created: String,
    pub members: Vec<WireTagMember>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireTagMember {
    /// `"verse"` or `"concept"`.
    pub kind: &'static str,
    /// Set for a verse member (compact key); null for a concept.
    pub verse: Option<String>,
    /// Display form of a verse member; null for a concept.
    pub display: Option<String>,
    /// Set for a concept member (Strong's code); null for a verse.
    pub strongs: Option<String>,
    pub note: Option<String>,
    pub added: String,
}

pub fn tags_to_wire(loaded: &[LoadedTag]) -> WireTags {
    WireTags {
        tags: loaded
            .iter()
            .map(|lt| {
                let t = &lt.tag;
                WireTag {
                    name: t.name.clone(),
                    color: t.color.clone(),
                    created: t.created.clone(),
                    members: t
                        .members
                        .iter()
                        .map(|m| {
                            let (kind, verse, display, strongs) = match &m.target {
                                TagTarget::Verse(v) => {
                                    ("verse", Some(v.ref_key()), Some(v.display()), None)
                                }
                                TagTarget::Concept(c) => ("concept", None, None, Some(c.clone())),
                            };
                            WireTagMember {
                                kind,
                                verse,
                                display,
                                strongs,
                                note: m.note.clone(),
                                added: m.added.clone(),
                            }
                        })
                        .collect(),
                }
            })
            .collect(),
    }
}

// ── verse cross-references ────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireXrefs {
    pub verse: String,
    pub partners: Vec<WireXrefPartner>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireXrefPartner {
    pub verse: String,
    pub display: String,
    pub weave: String,
}

/// A verse's weave partners across all loaded weaves (both link directions),
/// deduped by partner in first-seen order.
pub fn verse_xrefs_to_wire(loaded: &[LoadedWeave], vref: &VRef) -> WireXrefs {
    let mut partners = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for lw in loaded {
        for l in &lw.weave.links {
            let other = if &l.a == vref {
                Some(&l.b)
            } else if &l.b == vref {
                Some(&l.a)
            } else {
                None
            };
            if let Some(p) = other {
                if seen.insert(p.clone()) {
                    partners.push(WireXrefPartner {
                        verse: p.ref_key(),
                        display: p.display(),
                        weave: lw.weave.name.clone(),
                    });
                }
            }
        }
    }
    WireXrefs { verse: vref.ref_key(), partners }
}

// ── suggested weaves (review queue) ────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSuggestedWeaves {
    pub suggested: Vec<WireSuggestedWeave>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSuggestedWeave {
    /// Ordinal within the suggested subset — the handle the approve/reject
    /// calls take. Stable only until the next authoring write (which reloads).
    pub index: usize,
    pub name: String,
    /// Weave kind token (`retelling`/`type`/`prophecy`/`quotation`).
    pub kind: &'static str,
    pub notes: String,
    pub links: Vec<WireSuggestedLink>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSuggestedLink {
    pub a: String,
    pub a_display: String,
    pub b: String,
    pub b_display: String,
    pub label: String,
}

/// The suggested weaves (those under `home/weaves/suggested`), in load order,
/// each carrying its ordinal within that subset — the handle for approve/reject.
pub fn suggested_weaves_to_wire(loaded: &[LoadedWeave]) -> WireSuggestedWeaves {
    let suggested = loaded
        .iter()
        .filter(|lw| pure_core::weave::is_suggested(lw))
        .enumerate()
        .map(|(index, lw)| WireSuggestedWeave {
            index,
            name: lw.weave.name.clone(),
            kind: lw.weave.kind.token(),
            notes: lw.weave.notes.clone(),
            links: lw
                .weave
                .links
                .iter()
                .map(|l| WireSuggestedLink {
                    a: l.a.ref_key(),
                    a_display: l.a.display(),
                    b: l.b.ref_key(),
                    b_display: l.b.display(),
                    label: l.label.clone(),
                })
                .collect(),
        })
        .collect();
    WireSuggestedWeaves { suggested }
}

// ── R&D tier ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireScored {
    pub code: String,
    pub score: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireConceptNeighbours {
    pub code: String,
    /// Same-testament distributional neighbours.
    pub near: Vec<WireScored>,
    /// Cross-testament neighbours (empty unless the embedding is aligned).
    pub cross: Vec<WireScored>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireMorph {
    pub verse: String,
    pub token_index: u32,
    /// The raw parsing code, e.g. `"HVqp3ms"` / `"V-AAI-3S"`.
    pub code: String,
    /// The rendered study-panel phrase, e.g. `"Qal perfect, 3rd masculine singular"`.
    pub gloss: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireBridgePartner {
    pub code: String,
    /// The witnesses that assert this link (`etymology`, `lxx`, `abbott-smith`, …).
    pub sources: Vec<String>,
    /// The best trust prior across those witnesses.
    pub prior: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireBridgePartners {
    pub code: String,
    pub partners: Vec<WireBridgePartner>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSimilarVerse {
    pub verse: String,
    pub display: String,
    pub score: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSimilarVerses {
    pub verse: String,
    /// Same-testament thematic neighbours.
    #[serde(rename = "in")]
    pub within: Vec<WireSimilarVerse>,
    /// Cross-testament neighbours (empty unless the embedding is aligned).
    pub cross: Vec<WireSimilarVerse>,
}

pub fn scored_to_wire(items: Vec<(String, f32)>) -> Vec<WireScored> {
    items.into_iter().map(|(code, score)| WireScored { code, score }).collect()
}

pub fn similar_to_wire(items: Vec<(VRef, f32)>) -> Vec<WireSimilarVerse> {
    items
        .into_iter()
        .map(|(v, score)| WireSimilarVerse { verse: v.ref_key(), display: v.display(), score })
        .collect()
}

pub fn search_to_wire(a: &SearchAnswer) -> WireSearch {
    match a {
        SearchAnswer::GoTo { book, chapter, verse } => {
            let display = match verse {
                Some(v) => pure_core::VRef::new(book.clone(), *chapter, *v).display(),
                None => format!("{} {}", pure_core::canon::display_name(book), chapter),
            };
            WireSearch::Goto {
                book: book.clone(),
                chapter: *chapter,
                verse: *verse,
                display,
            }
        }
        SearchAnswer::Hits { how, total, hits } => WireSearch::Hits {
            how: how.clone(),
            total: *total,
            capped: *total > hits.len(),
            hits: hits.iter().map(search_hit_to_wire).collect(),
        },
    }
}
