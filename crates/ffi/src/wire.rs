//! The wire schemas: the JSON contract every binding decodes.
//!
//! These DTOs are deliberately **separate** from `plumbline_core` / `plumbline_layout`
//! internals. The core's own serde impls serve its frozen on-disk formats (the
//! positional token array, OSIS-keyed verses); this ABI instead speaks a
//! flat, self-describing, `camelCase` JSON that is pleasant to bind from C#,
//! Kotlin, Swift or JS and stable to evolve (new fields are additive). Verse
//! references cross the wire as compact keys (`"John 3:16"`) plus a display
//! form, so a shell needs no canon table of its own.

use serde::{Deserialize, Serialize};

use plumbline_core::config::{Config, PaneRef, StudyMode};
use plumbline_core::panel::{Block, Color, PanelLink, Run};
use plumbline_core::theme::ThemeChoice;
use plumbline_core::corpus::{Corpus, Token, Verse};
use plumbline_core::crossref::CrossRef;
use plumbline_core::memory;
use plumbline_core::reference::VRef;
use plumbline_core::search::{SearchAnswer, SearchHit};
use plumbline_core::strongs::StrongsEntry;
use plumbline_core::tag::{LoadedTag, TagTarget};
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
                            resolved: corpus.verse(&l.a).is_some()
                                && corpus.verse(&l.b).is_some(),
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
    pub label: &'static str,
    pub first: usize,
    pub last: usize,
}

pub fn canon_segments_to_wire() -> WireCanonSegments {
    WireCanonSegments {
        segments: plumbline_core::reference::CANON_SEGMENTS
            .iter()
            .map(|&(label, first, last)| WireCanonSegment { label, first, last })
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

/// Everything the concept-map popup paints for one code: the centre + its
/// spokes (near ∪ community, deduped, labels pre-baked) and the per-book
/// dispersion counts. One producer replaces the shell's assembly + its four
/// lookups (neighbours / concept / gloss / lemma) with a single call. Assembled
/// in `lib.rs` because the labels need the gloss + dictionary.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireConceptMap {
    pub code: String,
    /// The centre node's label (English gloss over lemma, `\n`-separated;
    /// falls back to lemma, then the bare code).
    pub center_label: String,
    pub spokes: Vec<WireConceptSpoke>,
    /// Per-book dispersion counts in **canon order** (length = `book_count`); a
    /// book the concept never occurs in is 0. The strip places cell `i` at
    /// `i / book_count` — no book-id table needed in the shell.
    pub by_book: Vec<u32>,
    pub ot_nt_divide: usize,
    pub book_count: usize,
    /// The cross-testament **bridge row**: the strongest other-testament
    /// equivalents of `code` and their unioned dispersion. Absent when the code
    /// has no cross-testament partner. This is what makes viewing *Christ*
    /// (Greek) light up where *Messiah* (Hebrew) occurs — the OT half of the
    /// strip fills in even though `by_book` (this code) is NT-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge: Option<WireConceptBridge>,
}

/// One spoke of the concept map: a neighbour code, its pre-baked label, and
/// whether it is a **semantic** (embedding) neighbour — gold — or a collocation
/// community member — green.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireConceptSpoke {
    pub code: String,
    pub label: String,
    pub semantic: bool,
    /// Cosine similarity to the centre concept (semantic spokes only) — the
    /// shells scale spoke distance by it, so more-related concepts sit closer.
    /// Absent for community spokes, which draw at the outer ring. Additive
    /// wire evolution (2026-07-26): older shells ignore it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
}

/// The dispersion strip's cross-testament overlay (see [`WireConceptMap::bridge`]):
/// the other-testament partner lemmas plus their unioned per-book dispersion,
/// rendered as a second row beneath the concept's own.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireConceptBridge {
    /// The other-testament partners, strongest-first. `label` is the English
    /// gloss over lemma, exactly like the centre and spoke labels.
    pub partners: Vec<WireBridgeNode>,
    /// The partners' unioned per-book dispersion in **canon order**
    /// (length = `book_count`) — so the shell paints it exactly like `by_book`.
    pub by_book: Vec<u32>,
}

/// One cross-testament partner node in a [`WireConceptBridge`].
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireBridgeNode {
    pub code: String,
    pub label: String,
    /// The fused trust prior of the strongest witness tying this partner (0–1).
    pub prior: f32,
}

// ── study-panel content model (the typed block list) ──────────────────────────

/// A panel view as a list of typed blocks (see `plumbline_core::panel`). The shell
/// walks these with a small per-block renderer; it derives nothing.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePanel {
    pub blocks: Vec<WireBlock>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WireBlock {
    /// A section header, with an optional tier mark (glyph + colour role).
    Section { title: String, mark_glyph: Option<String>, mark_color: Option<&'static str> },
    /// A flowing paragraph of styled runs.
    Para { runs: Vec<WireRun>, indent: bool, top_gap: bool },
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
#[serde(tag = "verb", rename_all = "camelCase")]
pub enum WirePanelLink {
    Go { book: String, chapter: u32, verse: Option<u32> },
    Occurrences { code: String },
    Rendering { code: String, rendering: String },
    CodeStudy { code: String, word: String },
    Thread { index: usize },
    Tag { index: usize },
    Weave { index: usize },
    ConceptMap { code: String },
    AddTag {
        #[serde(rename = "refKey")]
        ref_key: String,
    },
    AddThread {
        #[serde(rename = "refKey")]
        ref_key: String,
    },
    Untag {
        tag: usize,
        #[serde(rename = "refKey")]
        ref_key: String,
    },
    MakeWeave { tag: usize },
    Approve { index: usize },
    Reject { index: usize },
    EditThreadNotes { index: usize },
    EditWeaveNotes { index: usize },
    EditEntryNote { thread: usize, entry: usize },
    EditNote {
        #[serde(rename = "refKey")]
        ref_key: String,
    },
    Guide,
    About,
}

pub fn link_to_wire(l: PanelLink) -> WirePanelLink {
    match l {
        PanelLink::Go { book, chapter, verse } => WirePanelLink::Go { book, chapter, verse },
        PanelLink::Occurrences { code } => WirePanelLink::Occurrences { code },
        PanelLink::Rendering { code, rendering } => WirePanelLink::Rendering { code, rendering },
        PanelLink::CodeStudy { code, word } => WirePanelLink::CodeStudy { code, word },
        PanelLink::Thread { index } => WirePanelLink::Thread { index },
        PanelLink::Tag { index } => WirePanelLink::Tag { index },
        PanelLink::Weave { index } => WirePanelLink::Weave { index },
        PanelLink::ConceptMap { code } => WirePanelLink::ConceptMap { code },
        PanelLink::AddTag { refkey } => WirePanelLink::AddTag { ref_key: refkey },
        PanelLink::AddThread { refkey } => WirePanelLink::AddThread { ref_key: refkey },
        PanelLink::Untag { tag, refkey } => WirePanelLink::Untag { tag, ref_key: refkey },
        PanelLink::MakeWeave { tag } => WirePanelLink::MakeWeave { tag },
        PanelLink::Approve { index } => WirePanelLink::Approve { index },
        PanelLink::Reject { index } => WirePanelLink::Reject { index },
        PanelLink::EditThreadNotes { index } => WirePanelLink::EditThreadNotes { index },
        PanelLink::EditWeaveNotes { index } => WirePanelLink::EditWeaveNotes { index },
        PanelLink::EditEntryNote { thread, entry } => WirePanelLink::EditEntryNote { thread, entry },
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
                Block::Para { runs, indent, top_gap } => WireBlock::Para {
                    runs: runs.into_iter().map(run_to_wire).collect(),
                    indent,
                    top_gap,
                },
                Block::Rule => WireBlock::Rule,
            })
            .collect(),
    }
}

// ── config / session (shared with the GTK shell via core::config) ─────────────

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
    /// Verse-per-line reading mode.
    #[serde(default)]
    pub verse_per_line: bool,
    /// Colour theme choice (`system`/`light`/`dark`/`night`).
    #[serde(default)]
    pub theme: String,
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
    /// Show the curated-scholarship analysis tiers (additive, 2026-07-25;
    /// absent on load → derived from `studyMode`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_analysis: Option<bool>,
    /// Show the learned/statistical analysis tiers (additive, 2026-07-25).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_analysis: Option<bool>,
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
}

pub fn config_to_wire(cfg: &Config, first_run: bool) -> WireConfigState {
    WireConfigState {
        study_mode: cfg.mode.token().to_string(),
        body_size: cfg.body_size,
        open_panes: cfg
            .panes
            .iter()
            .map(|p| WirePaneRef { book: p.book.clone(), chapter: p.chapter, verse: p.verse })
            .collect(),
        active_pane: cfg.active,
        verse_per_line: cfg.verse_per_line,
        theme: cfg.theme.token().to_string(),
        copy_style: cfg.copy_style.clone(),
        side_margin: cfg.side_margin,
        line_spacing: cfg.line_spacing,
        history: cfg
            .history
            .iter()
            .map(|p| WirePaneRef { book: p.book.clone(), chapter: p.chapter, verse: None })
            .collect(),
        human_analysis: Some(cfg.human_analysis),
        machine_analysis: Some(cfg.machine_analysis),
        first_run,
    }
}

pub fn config_from_wire(w: &WireConfigState) -> Config {
    let mode = StudyMode::parse(&w.study_mode).unwrap_or(StudyMode::Simple);
    Config {
        mode,
        body_size: if w.body_size.is_finite() && w.body_size > 0.0 { w.body_size } else { 18.0 },
        panes: w
            .open_panes
            .iter()
            .map(|p| PaneRef {
                book: p.book.clone(),
                chapter: p.chapter.max(1),
                verse: p.verse.filter(|v| *v >= 1),
            })
            .collect(),
        active: w.active_pane,
        verse_per_line: w.verse_per_line,
        theme: ThemeChoice::parse(&w.theme).unwrap_or_default(),
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
            .map(|p| PaneRef { book: p.book.clone(), chapter: p.chapter.max(1), verse: None })
            .collect(),
        human_analysis: w.human_analysis.unwrap_or(true),
        machine_analysis: w.machine_analysis.unwrap_or(true),
    }
}

pub fn search_to_wire(a: &SearchAnswer) -> WireSearch {
    match a {
        SearchAnswer::GoTo { book, chapter, verse } => {
            let display = match verse {
                Some(v) => plumbline_core::VRef::new(book.clone(), *chapter, *v).display(),
                None => format!("{} {}", plumbline_core::canon::display_name(book), chapter),
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

// ── highlight washes (Tier 0 #4) ───────────────────────────────────────────────

/// The highlight colour for one verse in a chapter (member of a colour-bearing
/// tag). `color` is a `#rrggbb` tone the shell washes behind the verse.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireVerseHighlight {
    pub verse: String,
    pub color: String,
}

/// One word-precise wash run within a verse: inclusive token indices `[lo, hi]`
/// plus the tone. Additive companion to the whole-verse `verses` list, carrying
/// cross-verse drag highlights (Tier 0 #4). A range's interior verses arrive as
/// a full run (`lo` 0 … last token); its first/last as partial runs.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireHighlightRun {
    pub verse: String,
    pub lo: u16,
    pub hi: u16,
    pub color: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireChapterHighlights {
    pub book: String,
    pub chapter: u16,
    pub verses: Vec<WireVerseHighlight>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<WireHighlightRun>,
}

/// One selectable highlight tone (`name`, `#rrggbb`) — the shell's swatch menu.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireHighlightTone {
    pub name: &'static str,
    pub hex: &'static str,
}

#[derive(Serialize)]
pub struct WireHighlightTones {
    pub tones: Vec<WireHighlightTone>,
}

pub fn highlight_tones_to_wire() -> WireHighlightTones {
    WireHighlightTones {
        tones: plumbline_core::theme::HIGHLIGHT_TONES
            .iter()
            .map(|&(name, hex)| WireHighlightTone { name, hex })
            .collect(),
    }
}

// ── memorization (Tier 2 #15) — SRS cards, coverage/activity, drills ─────────

/// A verse's SRS card: SM-2 schedule, mastery bucket, and full review log.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireMemoryCard {
    #[serde(rename = "ref")]
    pub reference: String,
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

/// The coverage-map data: per-verse standing plus the 8-section rollup.
#[derive(Serialize)]
pub struct WireMemoryCoverage {
    pub verses: Vec<memory::VerseCoverage>,
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
    pub text: String,
    pub first_letters: String,
    pub blanked: String,
    pub level: u8,
    pub max_level: u8,
}
