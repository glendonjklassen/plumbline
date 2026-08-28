//! The wire schemas: the JSON contract every binding decodes.
//!
//! These DTOs are deliberately **separate** from `plumbline_core` / `plumbline_layout`
//! internals. The core's own serde impls serve its frozen on-disk formats (the
//! positional token array, OSIS-keyed verses); this ABI instead speaks a
//! flat, self-describing, `camelCase` JSON that is pleasant to bind from C#,
//! Kotlin, Swift or JS and stable to evolve (new fields are additive). Verse
//! references cross the wire as compact keys (`"John 3:16"`) plus a display
//! form, so a shell needs no canon table of its own.
//!
//! **On an enum, `rename_all` renames the VARIANTS, not the fields inside
//! them** — a tagged union needs `rename_all_fields = "camelCase"` as well, or
//! its struct-variant fields go out in Rust's snake_case. That mistake shipped:
//! `WireBlock` emitted `mark_glyph` / `top_gap`, so Android's decoder (which
//! ignores unknown keys) read nothing and the tier marks and paragraph gaps
//! never rendered. The golden key-set tests in `tests.rs` now pin the complete
//! set of keys each variant emits, so the next one fails at the test.

use serde::{Deserialize, Serialize};

use plumbline_core::church;
use plumbline_core::config::{Config, PaneRef, StudyMode};
use plumbline_core::corpus::{Corpus, Token, Verse};
use plumbline_core::crossref::CrossRef;
use plumbline_core::font::Font;
use plumbline_core::hymnal;
use plumbline_core::i18n;
use plumbline_core::memory;
use plumbline_core::panel::{Block, Color, PanelLink, Run};
use plumbline_core::reading;
use plumbline_core::reference::VRef;
use plumbline_core::search::{SearchAnswer, SearchHit};
use plumbline_core::strongs::StrongsEntry;
use plumbline_core::tag::{LoadedTag, TagTarget};
use plumbline_core::theme::ThemeChoice;
use plumbline_core::thread::LoadedThread;
use plumbline_core::weave::LoadedWeave;
use plumbline_layout::{DisplayList, Hit, ItemKind};

// ── table of contents ──────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct Toc {
    pub books: Vec<TocBook>,
}

#[derive(Serialize)]
pub struct TocBook {
    /// OSIS id, e.g. `"John"`.
    pub id: &'static str,
    /// Display name in the reader's language, e.g. `"John"` / `"1. Johannes"`.
    /// Owned rather than `&'static str`: it is a translation now, not a slice of
    /// the compiled-in canon table.
    pub name: String,
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
    /// Whether these boxes are laid out right to left. Additive (CLAUDE.md
    /// §Frozen contracts). The shell needs it to set its canvas `direction`, so
    /// that a trailing full stop lands at the visual END of an Arabic word
    /// rather than leading it — and taking it from the display list is what
    /// keeps that answer from ever disagreeing with the coordinates it came
    /// with.
    pub rtl: bool,
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
    /// Token flag bits (see the `PLUMBLINE_FLAG_*` constants); 0 for verse numbers.
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
                ItemKind::Word { verse, token_index } => {
                    ("word", Some(verse.ref_key()), Some(verse.display()), Some(*token_index), None)
                }
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
    WireDisplayList { width: dl.width, height: dl.height, rtl: dl.rtl, items }
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

// ── rendering lens ────────────────────────────────────────────────────────────

/// One occurrence of a rendering: the verse plus the inclusive token span of
/// the contiguous same-code run.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireRenderingRef {
    pub verse: String,
    pub display: String,
    pub span: [u16; 2],
}

/// One English rendering of a code, with its occurrence count and (capped)
/// refs.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireRendering {
    pub rendering: String,
    pub total: usize,
    pub capped: bool,
    pub refs: Vec<WireRenderingRef>,
}

/// The forward lens payload: a code and all the ways it is rendered.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireRenderings {
    pub code: String,
    pub renderings: Vec<WireRendering>,
}

/// One code a surface word translates, with how many tagged tokens carry it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireWordCode {
    pub code: String,
    pub count: usize,
}

/// The reverse lens payload: a surface word and the codes it translates.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireWordCodes {
    pub word: String,
    pub codes: Vec<WireWordCode>,
}

// ── search ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WireSearch {
    /// The query resolved to a reference.
    Goto { book: String, chapter: u16, verse: Option<u16>, display: String },
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
    WireSearchHit { verse: h.vref.ref_key(), display: h.vref.display(), note: h.note, why: h.why.clone() }
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
    pub created: String,
    /// Grouping heading for the tag lists; null when the tag has none. Additive.
    pub category: Option<String>,
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
                    created: t.created.clone(),
                    category: t.category.clone(),
                    members: t
                        .members
                        .iter()
                        .map(|m| {
                            let (kind, verse, display, strongs) = match &m.target {
                                TagTarget::Verse(v) => ("verse", Some(v.ref_key()), Some(v.display()), None),
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
        .filter(|lw| plumbline_core::weave::is_suggested(lw))
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
    /// The authority tiers those witnesses attest, deduped and ordered
    /// God→Human→Machine (`"god"`/`"human"`/`"machine"`); e.g. a scripture
    /// quotation is `["god","machine"]`. Additive field — a consumer that
    /// ignores it sees the pre-tier behaviour.
    pub tiers: Vec<String>,
    /// True when any witness's method is still research-grade (has not passed
    /// its held-out grading) — a lead, not a result.
    pub research_grade: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireBridgePartners {
    pub code: String,
    pub partners: Vec<WireBridgePartner>,
}

pub fn scored_to_wire(items: Vec<(String, f32)>) -> Vec<WireScored> {
    items.into_iter().map(|(code, score)| WireScored { code, score }).collect()
}

// ── margin notes ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireVerseNotes {
    pub verse: String,
    /// The verse's 1769 translators' margin notes, in file order.
    pub notes: Vec<String>,
}

// ── TSK study cross-references ────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireStudyXrefs {
    pub verse: String,
    /// Best-voted first (the index is pre-sorted).
    pub refs: Vec<WireStudyXref>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireStudyXref {
    pub to: String,
    pub to_display: String,
    /// Range end when the pointer spans verses ("Gen 1:1–1:3").
    pub end: Option<String>,
    pub end_display: Option<String>,
    pub votes: i32,
}

pub fn study_xrefs_to_wire(vref: &VRef, refs: &[CrossRef]) -> WireStudyXrefs {
    WireStudyXrefs {
        verse: vref.ref_key(),
        refs: refs
            .iter()
            .map(|r| WireStudyXref {
                to: r.to.ref_key(),
                to_display: r.to.display(),
                end: r.end.as_ref().map(|e| e.ref_key()),
                end_display: r.end.as_ref().map(|e| e.display()),
                votes: r.votes,
            })
            .collect(),
    }
}

// ── the weave library ─────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireWeaves {
    pub weaves: Vec<WireWeave>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireWeave {
    /// Ordinal in the loaded library — the handle a shell echoes back to a
    /// compare card. Stable only until the next authoring write.
    pub index: usize,
    pub name: String,
    /// Kind token (`retelling`/`type`/`prophecy`/`quotation`) + display label.
    pub kind: &'static str,
    pub kind_label: &'static str,
    pub notes: String,
    pub notes_source: &'static str,
    pub created: String,
    /// Weave-level approval (every link approved).
    pub approved: bool,
    /// True when the file lives under `weaves/suggested` (review queue).
    pub suggested: bool,
    pub links: Vec<WireWeaveLink>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireWeaveLink {
    pub a: String,
    pub a_display: String,
    pub b: String,
    pub b_display: String,
    pub label: String,
    pub approved: bool,
    pub span_a: Option<[u16; 2]>,
    pub span_b: Option<[u16; 2]>,
    /// Both endpoints resolve in the loaded corpus (drawable / navigable).
    pub resolved: bool,
}

pub fn weaves_to_wire(loaded: &[LoadedWeave], corpus: &Corpus) -> WireWeaves {
    WireWeaves {
        weaves: loaded
            .iter()
            .enumerate()
            .map(|(index, lw)| {
                let w = &lw.weave;
                WireWeave {
                    index,
                    name: w.name.clone(),
                    kind: w.kind.token(),
                    kind_label: w.kind.label(),
                    notes: w.notes.clone(),
                    notes_source: w.notes_source.token(),
                    created: w.created.clone(),
                    approved: w.approved,
                    suggested: plumbline_core::weave::is_suggested(lw),
                    links: w
                        .links
                        .iter()
                        .map(|l| WireWeaveLink {
                            a: l.a.ref_key(),
                            a_display: l.a.display(),
                            b: l.b.ref_key(),
                            b_display: l.b.display(),
                            label: l.label.clone(),
                            approved: l.approved,
                            span_a: l.span_a.map(|(lo, hi)| [lo, hi]),
                            span_b: l.span_b.map(|(lo, hi)| [lo, hi]),
                            resolved: corpus.verse(&l.a).is_some() && corpus.verse(&l.b).is_some(),
                        })
                        .collect(),
                }
            })
            .collect(),
    }
}

// ── connector link pairs (the ambient cross-reference lines + chord map) ──────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireLinkPairs {
    pub pairs: Vec<WireLinkPair>,
}

/// One deduped canonical weave pair, each endpoint spelled out (ref key +
/// located book/chapter/verse) so a shell draws connectors and lays out the
/// chord map without parsing ref keys or re-deriving the dedup.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireLinkPair {
    pub a: String,
    pub a_book: String,
    pub a_chapter: u16,
    pub a_verse: u16,
    pub b: String,
    pub b_book: String,
    pub b_chapter: u16,
    pub b_verse: u16,
    /// Both endpoints resolve in the loaded corpus (drawable / navigable). The
    /// same calc `weaves_to_wire` applies per link — an unresolved pair has an
    /// endpoint the reader can't reach, so a shell skips it when drawing.
    pub resolved: bool,
}

pub fn link_pairs_to_wire(loaded: &[LoadedWeave], corpus: &Corpus) -> WireLinkPairs {
    WireLinkPairs {
        pairs: plumbline_core::weave::link_pairs(loaded)
            .into_iter()
            .map(|(a, b)| {
                let resolved = corpus.verse(&a).is_some() && corpus.verse(&b).is_some();
                WireLinkPair {
                    a: a.ref_key(),
                    a_book: a.book,
                    a_chapter: a.chapter,
                    a_verse: a.verse,
                    b: b.ref_key(),
                    b_book: b.book,
                    b_chapter: b.chapter,
                    b_verse: b.verse,
                    resolved,
                }
            })
            .collect(),
    }
}

// ── canon overview segments (the 66-book strip + OT/NT seam) ───────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireCanonSegments {
    pub segments: Vec<WireCanonSegment>,
    /// Book index (39) at which the New Testament begins — the OT/NT seam.
    pub ot_nt_divide: usize,
}

/// One canon section as `(label, first book index, last book index)` over the
/// 66 books in OSIS order. The single source is `core::reference` — a shell
/// consumes this instead of hardcoding the bands (they were drifting).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireCanonSegment {
    /// The section's name IN THE READER'S LANGUAGE (`reference::segment_label`).
    /// Owned rather than `&'static` because it is translated, not a constant.
    pub label: String,
    pub first: usize,
    pub last: usize,
}

pub fn canon_segments_to_wire() -> WireCanonSegments {
    WireCanonSegments {
        segments: plumbline_core::reference::CANON_SEGMENTS
            .iter()
            .map(|&(label, first, last)| WireCanonSegment {
                label: plumbline_core::reference::segment_label(label, plumbline_core::i18n::active()),
                first,
                last,
            })
            .collect(),
        ot_nt_divide: plumbline_core::reference::OT_NT_DIVIDE,
    }
}

// ── chord / arc map (book-to-book weave density) ──────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireChordMap {
    /// Canon-ordered book-pair counts over the deduped link pairs.
    pub pairs: Vec<WireChordPair>,
    /// The heaviest pair count, for normalising ribbon weight/alpha.
    pub max: u32,
    /// Book index (39) at which the New Testament begins — ribbon colour marks
    /// OT-internal / NT-internal / cross-testament off this seam.
    pub ot_nt_divide: usize,
    /// Book count (66) — the axis the shell lays the ribbon feet over.
    pub book_count: usize,
}

/// One book-pair ribbon: two **canon book indices** (`a <= b`, so `a == b` is a
/// self-pair) and how many deduped verse links weave those books together. The
/// shell maps an index to a foot position and a name off its own book list —
/// it neither folds the pairs nor re-derives the max.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireChordPair {
    pub a: usize,
    pub b: usize,
    pub count: u32,
}

pub fn chord_map_to_wire(loaded: &[LoadedWeave]) -> WireChordMap {
    let (pairs, max) = plumbline_core::weave::chord_pairs(loaded);
    WireChordMap {
        pairs: pairs.into_iter().map(|(a, b, count)| WireChordPair { a, b, count }).collect(),
        max,
        ot_nt_divide: plumbline_core::reference::OT_NT_DIVIDE,
        book_count: plumbline_core::canon::BOOKS.len(),
    }
}

// ── constellation (the weave-library overview) ────────────────────────────────

/// A laid-out page of the constellation (review item 3). Positions are
/// **fractions / logical units** — `x` a canon fraction 0..1, `laneFrac` a
/// 0..1 within a lane's band, `size` a 0..1 witness degree. The shell holds the
/// transient `page`/`pins` (the endpoint's inputs), maps fractions to pixels,
/// picks colours, and paints; it derives nothing.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireConstellation {
    pub lanes: Vec<WireConstellationLane>,
    pub n_pins: usize,
    pub free_total: usize,
    /// The page actually shown (the requested page clamped into range).
    pub page: usize,
    pub max_page: usize,
    /// The fully-composed paging caption.
    pub caption: String,
    /// The fixed lane capacity — the shell's lane-height denominator, so it
    /// can't drift from the paging arithmetic.
    pub lane_capacity: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireConstellationLane {
    /// The weave's library index — the compare-card handle and the pin key.
    pub weave_index: usize,
    pub name: String,
    pub pinned: bool,
    pub nodes: Vec<WireConstellationNode>,
    pub edges: Vec<WireConstellationEdge>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireConstellationNode {
    /// Canon fraction 0..1 (plot x); within-lane fraction 0..1 (lane y).
    pub x: f32,
    pub lane_frac: f32,
    /// Witness degree ÷ the library max (0..1) — the shell picks the radius.
    pub size: f32,
    pub ref_key: String,
    pub book: String,
    pub chapter: u16,
    pub verse: u16,
    pub display: String,
}

/// A link's two endpoints (same lane) as `(x, laneFrac)` — the drawn curve.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireConstellationEdge {
    pub a_x: f32,
    pub a_lane_frac: f32,
    pub b_x: f32,
    pub b_lane_frac: f32,
}

pub fn constellation_to_wire(
    loaded: &[LoadedWeave],
    corpus: &Corpus,
    page: usize,
    pins: &[usize],
) -> WireConstellation {
    let c = plumbline_core::weave::constellation(loaded, corpus, page, pins);
    WireConstellation {
        lanes: c
            .lanes
            .into_iter()
            .map(|l| WireConstellationLane {
                weave_index: l.weave_index,
                name: l.name,
                pinned: l.pinned,
                nodes: l
                    .nodes
                    .into_iter()
                    .map(|n| WireConstellationNode {
                        x: n.x,
                        lane_frac: n.lane_frac,
                        size: n.size,
                        ref_key: n.ref_key,
                        book: n.book,
                        chapter: n.chapter,
                        verse: n.verse,
                        display: n.display,
                    })
                    .collect(),
                edges: l
                    .edges
                    .into_iter()
                    .map(|e| WireConstellationEdge {
                        a_x: e.a_x,
                        a_lane_frac: e.a_lane_frac,
                        b_x: e.b_x,
                        b_lane_frac: e.b_lane_frac,
                    })
                    .collect(),
            })
            .collect(),
        n_pins: c.n_pins,
        free_total: c.free_total,
        page: c.page,
        max_page: c.max_page,
        caption: c.caption,
        lane_capacity: c.lane_capacity,
    }
}

// ── the symbolic concept engine (collocations, distribution, leitwort) ────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireConcept {
    pub code: String,
    /// Total occurrences across the corpus.
    pub total: u32,
    pub ot: u32,
    pub nt: u32,
    /// The top-5 concentrating books, biggest first.
    pub top_books: Vec<WireBookCount>,
    /// Per-book counts for the dispersion strip (OSIS id → count).
    pub by_book: std::collections::HashMap<String, u32>,
    /// Mutual-kNN collocates by PPMI, strongest first.
    pub collocates: Vec<WireScored>,
    /// The code's co-occurrence community (excluding itself).
    pub community: Vec<String>,
    pub leitwort: Option<WireLeitwort>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireBookCount {
    pub book: String,
    pub display: String,
    pub count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireLeitwort {
    /// Total uses, and how many of them cluster in the discovered window.
    pub n: usize,
    pub win_count: usize,
    /// −log10 p — display as "p ≈ 10^−score".
    pub score: f64,
    /// Human window label, e.g. "Genesis 37–50".
    pub label: String,
}

// ── concept map (radial neighbourhood + dispersion strip) ─────────────────────

/// A panel view as a list of typed blocks (see `plumbline_core::panel`). The shell
/// walks these with a small per-block renderer; it derives nothing.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePanel {
    pub blocks: Vec<WireBlock>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WireBlock {
    /// A section header, with an optional tier mark (glyph + colour role).
    Section { title: String, mark_glyph: Option<String>, mark_color: Option<&'static str> },
    /// A flowing paragraph of styled runs. `drag` (additive, absent when not a
    /// drag row) marks a row the shell may reorder by dragging — see
    /// `core::panel::Block::Para`.
    Para {
        runs: Vec<WireRun>,
        indent: bool,
        top_gap: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        drag: Option<String>,
    },
    /// A horizontal rule.
    Rule,
}

/// One styled span: text + a **semantic** colour role + a logical point size +
/// bold/italic, and an optional `uri` that makes it a link.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireRun {
    pub text: String,
    pub size: f32,
    pub color: &'static str,
    pub bold: bool,
    pub italic: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

/// The camelCase token for a colour role (the shell maps it to its palette).
fn color_token(c: Color) -> &'static str {
    match c {
        Color::Ink => "ink",
        Color::Faded => "faded",
        Color::Gold => "gold",
        Color::Section => "section",
        Color::TierGod => "tierGod",
        Color::TierHuman => "tierHuman",
        Color::TierMachine => "tierMachine",
        Color::TierResearch => "tierResearch",
        Color::Mono => "mono",
        Color::Morph => "morph",
        Color::Lemma => "lemma",
    }
}

fn run_to_wire(r: Run) -> WireRun {
    WireRun { text: r.text, size: r.size, color: color_token(r.color), bold: r.bold, italic: r.italic, uri: r.uri }
}

/// A parsed panel link (`plumbline_core::panel::parse_link`) — the one verb
/// vocabulary, tagged by `verb` so a shell dispatches on the typed shape
/// instead of re-splitting the URI string.
#[derive(Serialize)]
#[serde(tag = "verb", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WirePanelLink {
    Go { book: String, chapter: u32, verse: Option<u32> },
    External { url: String },
    Occurrences { code: String },
    Rendering { code: String, rendering: String },
    CodeStudy { code: String, word: String },
    Thread { index: usize },
    Tag { index: usize },
    Weave { index: usize },
    AddTag { ref_key: String },
    AddThread { ref_key: String },
    Untag { tag: usize, ref_key: String },
    MakeWeave { tag: usize },
    Approve { index: usize },
    Reject { index: usize },
    DeleteThread { index: usize },
    DeleteTag { index: usize },
    DeleteWeave { index: usize },
    EditThreadNotes { index: usize },
    EditWeaveNotes { index: usize },
    EditEntryNote { thread: usize, entry: usize },
    MoveEntry { thread: usize, entry: usize, delta: i32 },
    RemoveEntry { thread: usize, entry: usize },
    EditNote { ref_key: String },
    Guide,
    About,
}

pub fn link_to_wire(l: PanelLink) -> WirePanelLink {
    match l {
        PanelLink::Go { book, chapter, verse } => WirePanelLink::Go { book, chapter, verse },
        PanelLink::External { url } => WirePanelLink::External { url },
        PanelLink::Occurrences { code } => WirePanelLink::Occurrences { code },
        PanelLink::Rendering { code, rendering } => WirePanelLink::Rendering { code, rendering },
        PanelLink::CodeStudy { code, word } => WirePanelLink::CodeStudy { code, word },
        PanelLink::Thread { index } => WirePanelLink::Thread { index },
        PanelLink::Tag { index } => WirePanelLink::Tag { index },
        PanelLink::Weave { index } => WirePanelLink::Weave { index },
        PanelLink::AddTag { refkey } => WirePanelLink::AddTag { ref_key: refkey },
        PanelLink::AddThread { refkey } => WirePanelLink::AddThread { ref_key: refkey },
        PanelLink::Untag { tag, refkey } => WirePanelLink::Untag { tag, ref_key: refkey },
        PanelLink::MakeWeave { tag } => WirePanelLink::MakeWeave { tag },
        PanelLink::Approve { index } => WirePanelLink::Approve { index },
        PanelLink::Reject { index } => WirePanelLink::Reject { index },
        PanelLink::DeleteThread { index } => WirePanelLink::DeleteThread { index },
        PanelLink::DeleteTag { index } => WirePanelLink::DeleteTag { index },
        PanelLink::DeleteWeave { index } => WirePanelLink::DeleteWeave { index },
        PanelLink::EditThreadNotes { index } => WirePanelLink::EditThreadNotes { index },
        PanelLink::EditWeaveNotes { index } => WirePanelLink::EditWeaveNotes { index },
        PanelLink::EditEntryNote { thread, entry } => WirePanelLink::EditEntryNote { thread, entry },
        PanelLink::MoveEntry { thread, entry, delta } => WirePanelLink::MoveEntry { thread, entry, delta },
        PanelLink::RemoveEntry { thread, entry } => WirePanelLink::RemoveEntry { thread, entry },
        PanelLink::EditNote { refkey } => WirePanelLink::EditNote { ref_key: refkey },
        PanelLink::Guide => WirePanelLink::Guide,
        PanelLink::About => WirePanelLink::About,
    }
}

pub fn blocks_to_wire(blocks: Vec<Block>) -> WirePanel {
    WirePanel {
        blocks: blocks
            .into_iter()
            .map(|b| match b {
                Block::Section { title, mark } => WireBlock::Section {
                    title,
                    mark_glyph: mark.as_ref().map(|(g, _)| g.clone()),
                    mark_color: mark.map(|(_, c)| color_token(c)),
                },
                Block::Para { runs, indent, top_gap, drag } => {
                    WireBlock::Para { runs: runs.into_iter().map(run_to_wire).collect(), indent, top_gap, drag }
                }
                Block::Rule => WireBlock::Rule,
            })
            .collect(),
    }
}

// ── config / session (shared with the GTK shell via core::config) ─────────────

/// For an ON-by-default switch, so an absent key reads as the shipped default
/// rather than as the reader having turned it off.
fn default_true() -> bool {
    true
}

/// "Never said" for the lifetime counter — an absent key must not read as a
/// reader who answered nought.
fn minus_one() -> i64 {
    -1
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WireConfigState {
    #[serde(default)]
    pub study_mode: String,
    #[serde(default)]
    pub body_size: f64,
    #[serde(default)]
    pub open_panes: Vec<WirePaneRef>,
    #[serde(default)]
    pub active_pane: usize,
    /// Where the reader was PER SEATING, keyed by slot token (additive; see
    /// `core::session_slot`). Absent for a reader who has never been anywhere
    /// in a given slot, which is not the same as an empty position.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub slots: std::collections::BTreeMap<String, WirePaneRef>,
    /// Lifetime reads through the Bible, or -1 for "never said" — which is NOT
    /// the same as a reader who answered "none". Seeded once by hand and earned
    /// thereafter; see `core::config::Config::bible_reads`.
    #[serde(default = "minus_one")]
    pub bible_reads: i64,
    /// Whether the current full-canon state has already been counted.
    #[serde(default)]
    pub bible_reads_credited: bool,
    /// Verse-per-line reading mode.
    #[serde(default)]
    pub verse_per_line: bool,
    /// Page-turn mode (additive): tap margins either side of the text that
    /// scroll most of a screen. Absent → off.
    #[serde(default)]
    pub page_turn: bool,
    /// Sunday service start, minutes since local midnight (additive). Absent =
    /// never set — the Sunday seating keeps its before-noon rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunday_service: Option<u32>,
    /// The two reader-typography switches: paint the leading verse numbers,
    /// and italicize the KJV's supplied words. Both ON by default, so they
    /// default to TRUE when absent rather than to serde's `false` — a shell
    /// built before these existed sends neither, and reading that as "the
    /// reader turned them off" would strip a chapter of its numbers on the
    /// first save an older shell made.
    #[serde(default = "default_true")]
    pub verse_numbers: bool,
    #[serde(default = "default_true")]
    pub added_italics: bool,
    /// Colour theme choice (`system`/`light`/`dark`/`night`).
    #[serde(default)]
    pub theme: String,
    /// The face scripture is painted in, and the face the chrome is painted in
    /// (additive; `plumbline_core::font::Font` tokens). Two axes, independent of
    /// each other and of `theme`. Absent → the shipped default face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_font: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chrome_font: Option<String>,
    /// Default one-tap copy shape (`verse`/`verseRef`/`verseMarkdown`).
    #[serde(default)]
    pub copy_style: String,
    /// Reader horizontal margin in px (space either side of the text column).
    #[serde(default)]
    pub side_margin: f64,
    /// Reader line-height as a multiple of the text height.
    #[serde(default)]
    pub line_spacing: f64,
    /// Reading history, most-recent-first (capped by the core).
    #[serde(default)]
    pub history: Vec<WirePaneRef>,
    /// Show the curated-scholarship analysis tiers (additive; absent on load →
    /// derived from `studyMode`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_analysis: Option<bool>,
    /// Show the learned/statistical analysis tiers (additive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_analysis: Option<bool>,
    /// The reader's home church (additive): shown in the welcome
    /// when a shared link carried one, and attached to the links this reader
    /// shares. Absent when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub church: Option<WireChurch>,
    /// Present-screen shares open as a new believer (additive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub present_shares_as_new: Option<bool>,
    /// The plain-English overlay (the AKJV delta) on the reader (additive).
    /// Absent → off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub akjv_overlay: Option<bool>,
    /// The welcome this reader was given, "new" | "curious" (additive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intro: Option<String>,
    /// Whether the bundled devotional has already been offered (additive).
    /// Absent reads as false, so a config written before devotionals existed
    /// gets the offer once — and a reader who STOPPED the booklet is not
    /// offered it again, because by then this is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devotional_seeded: Option<bool>,
    /// The reader's chosen language (additive). ABSENT means "follow the
    /// device" — the shell then passes its locale to
    /// `plumbline_i18n_catalog_json` and the core resolves it, so a German
    /// phone opens in German without anyone visiting Settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The active concept study's plan id (additive). ABSENT means normal
    /// reading mode; the shell suspends its reading tracker and turns verse
    /// taps into tag-with-confirm while this is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concept_study: Option<String>,
    /// The thread "share the gospel" opens; absent = the stock Romans Road.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gospel_thread: Option<String>,
    /// English definitions preferred over this language's own Strong's
    /// dictionary (additive). Absent = off: the localized one serves when the
    /// pack ships it. `alias` keeps a shell or a stored payload still saying
    /// `strongsDeOff` — the name it carried while German was the only
    /// translation — from silently losing the reader's choice.
    #[serde(default, alias = "strongsDeOff", skip_serializing_if = "Option::is_none")]
    pub localized_lexicon_off: Option<bool>,
    /// Load-only: true when no config file existed yet (guided first run).
    #[serde(default)]
    pub first_run: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePaneRef {
    pub book: String,
    #[serde(default)]
    pub chapter: u16,
    /// First visible verse (additive; absent = top of the chapter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verse: Option<u16>,
    /// This pane's TEXT language (additive; absent/empty = the reader's own).
    /// A pane's text language is not the UI's — see `config::PaneRef::lang`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lang: String,
}

/// A home church on the wire (see [`WireConfigState::church`]).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireChurch {
    #[serde(default)]
    pub name: String,
    /// When the church meets, minutes since local midnight; absent when it
    /// never said. Replaced a free "when and where" line — see
    /// `core::config::Church::service`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<u16>,
    #[serde(default)]
    pub url: String,
}

impl WireChurch {
    pub fn to_core(&self) -> plumbline_core::config::Church {
        plumbline_core::config::Church { name: self.name.clone(), service: self.service, url: self.url.clone() }
    }

    pub fn from_core(c: &plumbline_core::config::Church) -> WireChurch {
        WireChurch { name: c.name.clone(), service: c.service, url: c.url.clone() }
    }
}

// ── the share link (core::church) ────────────────────────────────────────────

/// What a shell asks for when it needs the link it hands over. Every field is
/// optional: `{}` is "the plain app link".
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireShareRequest {
    /// The link to build on. Absent → `core::church::PWA_URL`, the hosted PWA.
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub church: Option<WireChurch>,
    /// Present only: the recipient's welcome opens on the new-believer path.
    #[serde(default)]
    pub start_as_new_believer: bool,
    /// A refKey the recipient opens at (`"Ps 23:1"`).
    #[serde(default)]
    pub at: Option<String>,
}

/// Everything a share surface needs, from one call: the link for the QR and the
/// share sheet, the cleaned church to echo back, and the two derived strings a
/// Church button needs.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireShare {
    /// The link itself — what the QR encodes and the share sheet sends.
    pub url: String,
    /// The base it was built on, so a shell can show the bare host without
    /// hard-coding it.
    pub base: String,
    /// The church as the core normalized it (trimmed, capped).
    pub church: WireChurch,
    /// False when no church is set — the link is then the plain app link.
    pub has_church: bool,
    /// What to show when there is no site to open: who and when.
    pub title: String,
    /// The church's own site, or null when it is not one we will open.
    pub site_url: Option<String>,
}

pub fn config_to_wire(cfg: &Config, first_run: bool) -> WireConfigState {
    WireConfigState {
        study_mode: cfg.mode.token().to_string(),
        body_size: cfg.body_size,
        open_panes: cfg
            .panes
            .iter()
            .map(|p| WirePaneRef { book: p.book.clone(), chapter: p.chapter, verse: p.verse, lang: p.lang.clone() })
            .collect(),
        active_pane: cfg.active,
        bible_reads: cfg.bible_reads,
        bible_reads_credited: cfg.bible_reads_credited,
        slots: cfg
            .slots
            .iter()
            .map(|(k, p)| {
                (
                    k.clone(),
                    WirePaneRef { book: p.book.clone(), chapter: p.chapter, verse: p.verse, lang: p.lang.clone() },
                )
            })
            .collect(),
        verse_per_line: cfg.verse_per_line,
        page_turn: cfg.page_turn,
        sunday_service: cfg.sunday_service,
        verse_numbers: cfg.verse_numbers,
        added_italics: cfg.added_italics,
        theme: cfg.theme.token().to_string(),
        text_font: Some(cfg.text_font.token().to_string()),
        chrome_font: Some(cfg.chrome_font.token().to_string()),
        copy_style: cfg.copy_style.clone(),
        side_margin: cfg.side_margin,
        line_spacing: cfg.line_spacing,
        history: cfg
            .history
            .iter()
            .map(|p| WirePaneRef { book: p.book.clone(), chapter: p.chapter, verse: None, lang: String::new() })
            .collect(),
        human_analysis: Some(cfg.human_analysis),
        machine_analysis: Some(cfg.machine_analysis),
        present_shares_as_new: Some(cfg.present_shares_as_new),
        akjv_overlay: Some(cfg.akjv_overlay),
        intro: (!cfg.intro.is_empty()).then(|| cfg.intro.clone()),
        devotional_seeded: cfg.devotional_seeded.then_some(true),
        language: (!cfg.language.is_empty()).then(|| cfg.language.clone()),
        concept_study: (!cfg.concept_study.is_empty()).then(|| cfg.concept_study.clone()),
        gospel_thread: (!cfg.gospel_thread.is_empty()).then(|| cfg.gospel_thread.clone()),
        localized_lexicon_off: Some(cfg.localized_lexicon_off),
        church: (!cfg.church.is_empty()).then(|| WireChurch {
            name: cfg.church.name.clone(),
            service: cfg.church.service,
            url: cfg.church.url.clone(),
        }),
        first_run,
    }
}

pub fn config_from_wire(w: &WireConfigState) -> Config {
    let mode = StudyMode::parse(&w.study_mode).unwrap_or(StudyMode::Simple);
    Config {
        mode,
        body_size: if w.body_size.is_finite() && w.body_size > 0.0 { w.body_size } else { Config::default().body_size },
        panes: w
            .open_panes
            .iter()
            .map(|p| PaneRef {
                book: p.book.clone(),
                chapter: p.chapter.max(1),
                verse: p.verse.filter(|v| *v >= 1),
                lang: p.lang.clone(),
            })
            .collect(),
        active: w.active_pane,
        bible_reads: w.bible_reads,
        bible_reads_credited: w.bible_reads_credited,
        slots: w
            .slots
            .iter()
            .map(|(k, p)| {
                (
                    k.clone(),
                    PaneRef {
                        book: p.book.clone(),
                        chapter: p.chapter.max(1),
                        verse: p.verse.filter(|v| *v >= 1),
                        lang: p.lang.clone(),
                    },
                )
            })
            .collect(),
        verse_per_line: w.verse_per_line,
        page_turn: w.page_turn,
        // Same guard as `core::config::from_wire`: a minute outside the day
        // reads as never-set.
        sunday_service: w.sunday_service.filter(|m| *m < 24 * 60),
        verse_numbers: w.verse_numbers,
        added_italics: w.added_italics,
        theme: ThemeChoice::parse(&w.theme).unwrap_or_default(),
        // Absent, or a face this build does not ship → the default face. Same
        // stance as `core::config::from_wire`, and for the same reason.
        text_font: w.text_font.as_deref().and_then(Font::parse).unwrap_or_default(),
        chrome_font: w.chrome_font.as_deref().and_then(Font::parse).unwrap_or_default(),
        copy_style: match w.copy_style.as_str() {
            "verse" | "verseRef" | "verseMarkdown" => w.copy_style.clone(),
            _ => Config::default().copy_style,
        },
        side_margin: if w.side_margin.is_finite() && (0.0..=160.0).contains(&w.side_margin) {
            w.side_margin
        } else {
            Config::default().side_margin
        },
        line_spacing: if w.line_spacing.is_finite() && (1.0..=3.0).contains(&w.line_spacing) {
            w.line_spacing
        } else {
            Config::default().line_spacing
        },
        history: w
            .history
            .iter()
            .map(|p| PaneRef { book: p.book.clone(), chapter: p.chapter.max(1), verse: None, lang: String::new() })
            .collect(),
        // Absent = off; the tiers are opt-in (core::config::from_wire).
        human_analysis: w.human_analysis.unwrap_or(false),
        machine_analysis: w.machine_analysis.unwrap_or(false),
        present_shares_as_new: w.present_shares_as_new.unwrap_or(true),
        // Absent = off (core::config::from_wire): the KJV is the text.
        akjv_overlay: w.akjv_overlay.unwrap_or(false),
        devotional_seeded: w.devotional_seeded.unwrap_or(false),
        intro: match w.intro.as_deref() {
            Some("new") => "new".to_string(),
            Some("curious") => "curious".to_string(),
            _ => String::new(),
        },
        // Empty means "follow the device", and a language this build does not
        // ship means the same — never English by fiat. Validated here rather
        // than trusted, since this is where a shell's value becomes the core's.
        language: match w.language.as_deref() {
            Some(code) if i18n::Lang::ALL.iter().any(|l| l.code() == code) => code.to_string(),
            _ => String::new(),
        },
        // An id the plan store answers for at use — a stale one reads as
        // normal mode there, so nothing validates it away here.
        concept_study: w.concept_study.as_deref().map(|s| s.trim().to_string()).unwrap_or_default(),
        gospel_thread: w.gospel_thread.as_deref().map(|s| s.trim().to_string()).unwrap_or_default(),
        localized_lexicon_off: w.localized_lexicon_off.unwrap_or(false),
        // Through the core's clamps, not a local trim: this is the one place a
        // shell's church becomes the core's, so it is where the caps stop being
        // something each shell has to remember.
        church: w.church.as_ref().map(|c| church::clean(&c.to_core())).unwrap_or_default(),
    }
}

pub fn search_to_wire(a: &SearchAnswer) -> WireSearch {
    match a {
        SearchAnswer::GoTo { book, chapter, verse } => {
            let display = match verse {
                Some(v) => plumbline_core::VRef::new(book.clone(), *chapter, *v).display(),
                None => plumbline_core::VRef::new(book.clone(), *chapter, 1)
                    .chapter_display_in(plumbline_core::i18n::active()),
            };
            WireSearch::Goto { book: book.clone(), chapter: *chapter, verse: *verse, display }
        }
        SearchAnswer::Hits { how, total, hits } => WireSearch::Hits {
            how: how.clone(),
            total: *total,
            capped: *total > hits.len(),
            hits: hits.iter().map(search_hit_to_wire).collect(),
        },
    }
}

// ── personal notes (Tier 0 #3) ─────────────────────────────────────────────────

/// One personal note on a verse.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireUserNote {
    pub verse: String,
    pub display: String,
    pub text: String,
    pub created: String,
    pub updated: String,
}

/// All the reader's personal notes (for the gutter marks + a browser).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireUserNotes {
    pub notes: Vec<WireUserNote>,
}

// ── memorization (Tier 2 #15) — SRS cards, coverage/activity, drills ─────────

/// A verse's SRS card: SM-2 schedule, mastery bucket, and full review log.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireMemoryCard {
    #[serde(rename = "ref")]
    pub reference: String,
    /// Reader-facing name: `"Ps 23:1–6"` for a passage card (additive).
    pub label: String,
    /// The passage's last verse, when this card is a passage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub through: Option<String>,
    pub ease: f32,
    pub interval_days: u32,
    pub reps: u32,
    pub lapses: u32,
    pub due: String,
    pub mastery: memory::Mastery,
    pub reviews: Vec<memory::Review>,
}

/// Build the wire card from a core card (folds in the derived mastery bucket).
pub fn memory_card_to_wire(c: &memory::Card) -> WireMemoryCard {
    WireMemoryCard {
        reference: c.verse.ref_key(),
        label: c.label(),
        through: c.through.as_ref().map(plumbline_core::VRef::ref_key),
        ease: c.ease,
        interval_days: c.interval_days,
        reps: c.reps,
        lapses: c.lapses,
        due: c.due.clone(),
        mastery: memory::mastery(c),
        reviews: c.reviews.clone(),
    }
}

/// The study queue: verses due for review now, in reading order.
#[derive(Serialize)]
pub struct WireMemoryDue {
    pub refs: Vec<String>,
}

// ── the reading map (the navigator's glow) ───────────────────────────────────

/// Every book's standing, plus the tuning and the reader's start date, so one
/// call gives a shell everything the book grid needs.
#[derive(Serialize)]
pub struct WireReadingBooks {
    pub books: Vec<reading::BookHeat>,
    /// The date this reader started — the glow anchor for anything unread.
    pub since: String,
    pub spec: reading::Spec,
}

/// One book's chapters, for the chapter grid.
#[derive(Serialize)]
pub struct WireReadingChapters {
    pub book: String,
    pub chapters: Vec<reading::ChapterHeat>,
    pub since: String,
    pub spec: reading::Spec,
}

/// What a dwell report did — a shell repaints the chapter's tile off this, and
/// `completed` is its cue to say so.
#[derive(Serialize)]
pub struct WireReadingRecorded {
    #[serde(flatten)]
    pub recorded: reading::Recorded,
}

/// The coverage-map data: per-verse standing plus the 8-section rollup.
#[derive(Serialize)]
pub struct WireMemoryCoverage {
    /// Per-verse shading for the coverage map — a passage card contributes
    /// every verse it covers.
    pub verses: Vec<memory::VerseCoverage>,
    /// One row per card, for the hub's list (additive).
    pub cards: Vec<memory::CardSummary>,
    pub sections: Vec<memory::SectionCoverage>,
}

/// The activity heatmap: reviews per calendar day, oldest first.
#[derive(Serialize)]
pub struct WireMemoryActivity {
    pub days: Vec<memory::DayActivity>,
}

/// A drill prompt for a verse at a blank-out level: the plain text, its
/// first-letter skeleton, and the progressively-blanked form.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireMemoryDrill {
    #[serde(rename = "ref")]
    pub reference: String,
    /// What the drill is called on screen — `"Ps 23:1–6"` for a passage card
    /// (additive).
    pub label: String,
    /// Verses in the drill (1 unless this card is a passage).
    pub verses: u32,
    pub text: String,
    pub first_letters: String,
    pub blanked: String,
    pub level: u8,
    pub max_level: u8,
}

/// What the AKJV does to one token (`plumbline_engine_akjv_token_json`).
#[derive(serde::Serialize)]
pub struct AkjvTokenWire {
    /// The AKJV's wording for the run this token belongs to.
    pub akjv: String,
    /// The KJV words it replaced — what the reader tapped to see.
    pub kjv: String,
}

// ── hymnal ──────────────────────────────────────────────────────────────────

/// The hymnal's table of contents (`plumbline_engine_hymnal_json`).
#[derive(Serialize)]
pub struct WireHymnal {
    pub hymns: Vec<WireHymnalEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireHymnalEntry {
    pub id: String,
    /// The hymn's stable book number.
    pub number: u32,
    /// Language code → title, every language the hymn ships in. A shell shows
    /// its preferred language and falls back to whatever the hymn has.
    pub titles: std::collections::BTreeMap<String, String>,
    /// Language code → first line of stanza 1, chords stripped (index search).
    pub first_lines: std::collections::BTreeMap<String, String>,
    pub tune: String,
    pub meter: String,
}

/// One hymn, chords transposed and split for painting
/// (`plumbline_engine_hymn_json`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireHymn {
    pub id: String,
    pub number: u32,
    pub tune: String,
    pub meter: String,
    /// The written key of the charts.
    pub key: String,
    /// The transposition applied, in semitones (echo of the request).
    pub transpose: i32,
    /// The key the chords are NOW in — what a transpose control displays.
    pub transposed_key: String,
    pub texts: std::collections::BTreeMap<String, WireHymnText>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireHymnText {
    pub title: String,
    pub author: String,
    pub translator: Option<String>,
    pub year: Option<u32>,
    pub stanzas: Vec<WireHymnStanza>,
    /// Sung after every stanza; charts live on stanza 1 and here.
    pub chorus: Option<WireHymnStanza>,
}

#[derive(Serialize)]
pub struct WireHymnStanza {
    pub lines: Vec<WireHymnLine>,
}

#[derive(Serialize)]
pub struct WireHymnLine {
    pub parts: Vec<WireHymnPart>,
}

/// A run of lyric text with the chord (if any) struck at its first syllable.
#[derive(Serialize)]
pub struct WireHymnPart {
    pub chord: Option<String>,
    pub text: String,
}

/// First line of a stanza string, chords stripped — the index's search text.
fn first_line_plain(stanza: &str) -> String {
    let line = stanza.split('\n').next().unwrap_or("");
    hymnal::parse_line(line).into_iter().map(|s| s.text).collect()
}

pub fn hymnal_to_wire(book: &hymnal::Hymnal) -> WireHymnal {
    WireHymnal {
        hymns: book
            .hymns
            .iter()
            .map(|h| WireHymnalEntry {
                id: h.id.clone(),
                number: h.number,
                titles: h.texts.iter().map(|(lang, t)| (lang.clone(), t.title.clone())).collect(),
                first_lines: h
                    .texts
                    .iter()
                    .filter_map(|(lang, t)| Some((lang.clone(), first_line_plain(t.stanzas.first()?))))
                    .collect(),
                tune: h.tune.clone(),
                meter: h.meter.clone(),
            })
            .collect(),
    }
}

fn stanza_to_wire(stanza: &str, semis: i32, flats: bool) -> WireHymnStanza {
    WireHymnStanza {
        lines: hymnal::stanza_lines(stanza, semis, flats)
            .into_iter()
            .map(|segs| WireHymnLine {
                parts: segs.into_iter().map(|s| WireHymnPart { chord: s.chord, text: s.text }).collect(),
            })
            .collect(),
    }
}

pub fn hymn_to_wire(h: &hymnal::Hymn, semis: i32) -> WireHymn {
    let transposed_key = hymnal::transpose_key(&h.key, semis);
    // Spell every transposed chord for the key it lands in, not the one it left.
    let flats = hymnal::key_uses_flats(&transposed_key);
    WireHymn {
        id: h.id.clone(),
        number: h.number,
        tune: h.tune.clone(),
        meter: h.meter.clone(),
        key: h.key.clone(),
        transpose: semis,
        transposed_key,
        texts: h
            .texts
            .iter()
            .map(|(lang, t)| {
                (
                    lang.clone(),
                    WireHymnText {
                        title: t.title.clone(),
                        author: t.author.clone(),
                        translator: t.translator.clone(),
                        year: t.year,
                        stanzas: t.stanzas.iter().map(|s| stanza_to_wire(s, semis, flats)).collect(),
                        chorus: t.chorus.as_ref().map(|c| stanza_to_wire(c, semis, flats)),
                    },
                )
            })
            .collect(),
    }
}

// ── i18n ────────────────────────────────────────────────────────────────────

/// One language's whole catalogue, plus the list a picker needs
/// (`plumbline_i18n_catalog_json`).
///
/// `camelCase` like every other wire type. It went without for as long as every
/// field here was one word — and then the first two-word field crossed as
/// `native_intros`, which both shells read as absent and quietly answered "no"
/// to. Nothing renames under this: `lang`, `strings` and `languages` are their
/// own camelCase.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireCatalog {
    /// The language actually resolved — not necessarily the code asked for, if
    /// that carried a region tag or named a language this build does not ship.
    pub lang: String,
    /// id → text, English laid under the requested language so every id
    /// resolves even where the translation has not been written.
    pub strings: std::collections::BTreeMap<String, String>,
    /// Every language on offer, each labelled in itself.
    pub languages: Vec<WireLanguage>,
    /// Whether THIS language may be offered the first-run welcome and the
    /// curious path — see `i18n::Lang::has_native_intros`. Those two screens are
    /// somebody speaking to a reader about their own life, and a shell must not
    /// lead anyone into them in a language nobody has written them in. Sent with
    /// the catalogue rather than asked for separately because first run happens
    /// before there is an engine to ask.
    pub native_intros: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireLanguage {
    pub code: String,
    /// What this language calls itself — "Deutsch", not "German".
    pub endonym: String,
    /// Its English name — "German", not "Deutsch". The hymnal finder matches
    /// this, the endonym or the code, so a reader narrows the book by any of
    /// "de", "German" or "Deutsch".
    pub name: String,
    /// The Bible a reader of this language gets, by the name they would know it
    /// by: "KJV", "Luther", "Reina-Valera".
    pub bible: String,
    /// Whether it is written right to left, for the shell's own chrome — the
    /// document's `dir`, and which faces the font picker may offer.
    ///
    /// THE SECOND PLACE THIS HAD TO GO. `i18n::registry_json` carries the same
    /// column for the pack BUILD (a Node script reads it through
    /// `plumbline-hydrate languages`); this is what the running shell reads, and
    /// adding it to only one of them shipped an Arabic app with `dir="ltr"` —
    /// the exact shape of the problem the registry exists to end. If a third
    /// consumer appears, it reads a row; it does not get its own list.
    pub rtl: bool,
    /// The manifest role its corpus cache is filed under, and the role its own
    /// Strong's dictionary is filed under (absent when it has none).
    ///
    /// THE SHELLS ASK RATHER THAN KNOW. The web had `corpusRoleFor` returning a
    /// literal `"germanCorpus"`, a `GERMAN_CACHE` constant and an
    /// `if (code === "de")` in Settings — three places that each had to be found
    /// and edited to add a language. They read these now.
    pub corpus_role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lexicon_role: Option<String>,
    /// Files this language needs that the base pack does not carry, as home
    /// paths (`data/…`). Empty for English. This is the whole answer to "is
    /// there anything to download when the reader picks this language".
    pub pack_files: Vec<String>,
}

pub fn catalog_to_wire(lang: i18n::Lang) -> WireCatalog {
    WireCatalog {
        lang: lang.code().to_string(),
        strings: i18n::resolved(lang),
        languages: i18n::Lang::ALL.iter().map(|l| language_to_wire(*l)).collect(),
        native_intros: lang.has_native_intros(),
    }
}

pub(crate) fn language_to_wire(l: i18n::Lang) -> WireLanguage {
    // English's corpus cache and dictionary ARE the base pack, so it has nothing
    // extra to fetch; every other language's text and dictionary are optional
    // downloads (`docs/I18N.md` — nothing is bundled on the web, and an English
    // reader must not fetch a German Bible to read Genesis).
    let mut pack_files = Vec::new();
    if l != i18n::Lang::En {
        if l.spec().corpus.is_some() {
            pack_files.push(format!("data/{}", l.corpus().cache_file()));
        }
        if let Some(lex) = l.spec().lexicon {
            pack_files.push(format!("data/{}", lex.file));
        }
    }
    WireLanguage {
        code: l.code().to_string(),
        endonym: l.endonym().to_string(),
        name: l.exonym().to_string(),
        bible: l.corpus().label.to_string(),
        rtl: l.is_rtl(),
        corpus_role: l.corpus_role(),
        lexicon_role: (l != i18n::Lang::En && l.spec().lexicon.is_some()).then(|| l.lexicon_role()),
        pack_files,
    }
}
