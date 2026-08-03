//! The study-panel **content model**: one Rust producer builds a typed list of
//! blocks for every panel view (word study, code study, concordance, …), and
//! each shell walks the blocks with a *small* per-block renderer (GTK Pango,
//! WinUI `Inlines`, Compose `AnnotatedString`). This retires the ~700-line
//! hand-duplicated panel builders the shells carried (architecture-review P0.1).
//!
//! The producer knows nothing about pixels, colours, or fonts. A [`Run`] carries
//! a **semantic** colour role + a logical point size + bold/italic + an optional
//! `uri`; the shell maps the role to its palette and paints. Data reaches the
//! producer through the [`PanelSource`] trait, which both the GTK `State` and
//! the FFI `PlumblineEngine` implement — so the derivation lives here **once**.
//!
//! The block vocabulary is deliberately tiny and generic (unknown kinds render
//! as nothing on the wire), so the core can add kinds without breaking older
//! shells.

use crate::renderings::normalize as render_key;

// ── the block model ───────────────────────────────────────────────────────────

/// A semantic colour role. The shell owns the actual colours; every shell maps
/// these identically so the panel reads the same on each platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// Primary body text.
    Ink,
    /// Muted / secondary text.
    Faded,
    /// A link or accent (gold).
    Gold,
    /// A section header (spaced muted gold).
    Section,
    /// Authority tiers: the text itself / curated scholarship / machine-derived
    /// / still-research-grade.
    TierGod,
    TierHuman,
    TierMachine,
    TierResearch,
    /// A neutral grey (#888-ish): pronunciations, kind labels, entry notes.
    Mono,
    /// The morphology gloss tint.
    Morph,
    /// A lemma shown small beside a chip.
    Lemma,
}

/// One styled span of text. `uri`, when present, makes the run a link the shell
/// routes back through the panel dispatcher.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    /// Logical point size (DPI-independent). The shell scales to device pixels.
    pub size: f32,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
    pub uri: Option<String>,
}

impl Run {
    pub fn new(text: impl Into<String>, size: f32, color: Color) -> Run {
        Run { text: text.into(), size, color, bold: false, italic: false, uri: None }
    }
    pub fn bold(mut self) -> Run {
        self.bold = true;
        self
    }
    pub fn italic(mut self) -> Run {
        self.italic = true;
        self
    }
    pub fn link(mut self, uri: impl Into<String>) -> Run {
        self.uri = Some(uri.into());
        self
    }
}

/// A panel block. Kept tiny and generic; a shell that doesn't know a kind
/// renders nothing (forward-compatible).
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// A section header (spaced, muted gold), with an optional tier mark glyph
    /// whose colour is the paired role.
    Section { title: String, mark: Option<(String, Color)> },
    /// A flowing paragraph of styled runs. `indent` insets it (compare-card
    /// verse text, snippets); `top_gap` adds a little space above (action rows).
    Para { runs: Vec<Run>, indent: bool, top_gap: bool },
    /// A horizontal rule.
    Rule,
}

/// A catalogue string, in the reader's language.
///
/// This module generates PROSE the reader reads, so it is as much a part of the
/// catalogue as any shell — the labels here were English on a German screen until
/// UAT caught it (2026-08-03). Shorthand because there are twenty call sites and
/// `crate::i18n::t(crate::i18n::active(), …)` at each of them would be noise.
fn s(id: &str) -> String {
    crate::i18n::t(crate::i18n::active(), id, &[])
}

/// The same, with `{n}` filled.
fn sn(id: &str, n: usize) -> String {
    crate::i18n::t(crate::i18n::active(), id, &[("n", &n.to_string())])
}

impl Block {
    fn para(runs: Vec<Run>) -> Block {
        Block::Para { runs, indent: false, top_gap: false }
    }
    fn section(title: impl Into<String>) -> Block {
        Block::Section { title: title.into(), mark: None }
    }
    fn section_marked(title: impl Into<String>, glyph: &str, color: Color) -> Block {
        Block::Section { title: title.into(), mark: Some((glyph.to_string(), color)) }
    }
}

// ── logical sizes (one place, so the hierarchy is consistent) ─────────────────

mod sz {
    pub const WORD: f32 = 26.0;
    pub const LEMMA: f32 = 22.0;
    pub const TITLE: f32 = 18.0;
    pub const SEARCH_GOTO: f32 = 17.0;
    pub const SUGGEST_HEAD: f32 = 15.0;
    pub const LABEL: f32 = 14.5;
    pub const BODY: f32 = 14.0;
    pub const LIST: f32 = 13.5;
    pub const SMALL: f32 = 13.0;
    pub const NOTE: f32 = 12.5;
    pub const CAPTION: f32 = 12.0;
    pub const FINE: f32 = 11.5;
    pub const MARK: f32 = 11.0;
}

// ── projected data the producer renders (the trait fills these in) ────────────

/// A Strong's dictionary entry, projected to plain strings.
#[derive(Debug, Clone, Default)]
pub struct StrongsView {
    pub lemma: Option<String>,
    pub xlit: Option<String>,
    pub pron: Option<String>,
    pub deriv: Option<String>,
    pub def: Option<String>,
    pub kjv: Option<String>,
}

/// A concept chip: the code plus its English gloss and original lemma.
#[derive(Debug, Clone, Default)]
pub struct ChipView {
    pub code: String,
    pub gloss: Option<String>,
    pub lemma: Option<String>,
}

/// One English rendering of a code and how often it occurs.
#[derive(Debug, Clone)]
pub struct RenderingView {
    pub rendering: String,
    pub total: u32,
}

/// The verses where a code is rendered exactly one way (filtered concordance).
#[derive(Debug, Clone)]
pub struct RenderingRefsView {
    pub rendering: String,
    pub total: u32,
    /// `(ref key, display)`, already capped by the source.
    pub refs: Vec<(String, String)>,
}

/// A code's full concordance: total + `(ref key, display)` verses (capped).
#[derive(Debug, Clone, Default)]
pub struct OccurrencesView {
    pub total: u32,
    pub verses: Vec<(String, String)>,
}

/// An OT↔NT bridge partner: the code, its humanized witness sources, and the
/// authority tiers / research-grade flag driving its provenance marks.
#[derive(Debug, Clone)]
pub struct BridgePartnerView {
    pub code: String,
    /// Already lay-humanized (`bridge::source_label`), ready to join with " + ".
    pub sources: Vec<String>,
    pub tiers: Vec<String>,
    pub research_grade: bool,
}

/// The symbolic concept engine's view of a code.
#[derive(Debug, Clone, Default)]
pub struct ConceptView {
    pub community: Vec<String>,
    /// `(display name, count)` for the top concentrating books.
    pub top_books: Vec<(String, u32)>,
    pub ot: u32,
    pub nt: u32,
    pub leitwort: Option<LeitwortView>,
}

#[derive(Debug, Clone)]
pub struct LeitwortView {
    pub n: usize,
    pub win_count: usize,
    pub score: f64,
    pub label: String,
}

/// A weave cross-reference partner of a verse.
#[derive(Debug, Clone)]
pub struct XrefView {
    pub verse: String,
    pub display: String,
    pub weave: String,
    /// Library index of the weave, when it resolves (→ a compare-card link).
    pub weave_index: Option<usize>,
}

/// A TSK study cross-reference (optionally a range).
#[derive(Debug, Clone)]
pub struct StudyXrefView {
    pub to: String,
    pub to_display: String,
    pub end: Option<String>,
    pub end_display: Option<String>,
}

/// A thread (an ordered passage trail).
#[derive(Debug, Clone, Default)]
pub struct ThreadView {
    pub name: String,
    pub notes: String,
    pub entries: Vec<ThreadEntryView>,
}

#[derive(Debug, Clone)]
pub struct ThreadEntryView {
    pub verse: String,
    pub display: String,
    /// The passage's word snapshot (joined + truncated for the trail preview).
    pub text: Vec<String>,
    pub note: Option<String>,
}

/// A tag (a labelled set of verses / codes).
#[derive(Debug, Clone, Default)]
pub struct TagView {
    pub name: String,
    pub members: Vec<TagMemberView>,
}

#[derive(Debug, Clone)]
pub struct TagMemberView {
    pub kind: String,
    pub verse: Option<String>,
    pub display: Option<String>,
    pub strongs: Option<String>,
    pub note: Option<String>,
}

/// A weave in the library (compare card + weaves list).
#[derive(Debug, Clone, Default)]
pub struct WeaveView {
    pub index: usize,
    pub name: String,
    pub kind_label: String,
    pub notes: String,
    pub suggested: bool,
    pub links: Vec<WeaveLinkView>,
}

#[derive(Debug, Clone)]
pub struct WeaveLinkView {
    pub a: String,
    pub a_display: String,
    pub b: String,
    pub b_display: String,
    pub label: String,
    pub span_a: Option<[u16; 2]>,
    pub span_b: Option<[u16; 2]>,
}

/// A weave awaiting review (the suggested queue).
#[derive(Debug, Clone, Default)]
pub struct SuggestedView {
    /// Ordinal within the suggested subset — the approve/reject handle.
    pub index: usize,
    pub name: String,
    pub kind: String,
    pub notes: String,
    /// Library index, when it resolves (→ compare + edit-notes links).
    pub lib_index: Option<usize>,
    pub links: Vec<SuggestedLinkView>,
}

#[derive(Debug, Clone)]
pub struct SuggestedLinkView {
    pub a: String,
    pub a_display: String,
    pub b: String,
    pub b_display: String,
    pub label: String,
}

/// One token for a compare-card verse: its rendered form + whether the KJV
/// translators supplied it (italic grey).
#[derive(Debug, Clone)]
pub struct TokenView {
    pub render: String,
    pub added: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VerseTokensView {
    pub tokens: Vec<TokenView>,
}

/// A search result: either a direct navigation, or ranked hits.
#[derive(Debug, Clone)]
pub enum SearchView {
    Goto { book: String, chapter: u32, verse: Option<u32>, display: String },
    Hits { how: String, total: usize, capped: bool, hits: Vec<SearchHitView> },
}

#[derive(Debug, Clone)]
pub struct SearchHitView {
    pub verse: String,
    pub display: String,
    pub note: bool,
    pub why: String,
}

// ── the data source ───────────────────────────────────────────────────────────

/// Everything the panel producer reads. Both the GTK `State` and the FFI
/// `PlumblineEngine` implement this over the same underlying indices, so the panel
/// derivation is written once. Every method is a thin projection; the R&D tiers
/// (bridge / concept / morphology / similar verses) return empty when their
/// artifact is absent, so a simple-reader source needs only the base methods.
pub trait PanelSource {
    /// The surface English word at a token, if the verse/token resolve.
    fn token_word(&self, verse: &str, token: u32) -> Option<String>;
    /// Whether the open corpus is the KJV — the text every analytic in this
    /// module is tagged against. False for a translation with its own
    /// tokenization (the German Luther 1912), where the study card says so
    /// instead of showing evidence about a different set of words.
    fn is_kjv_text(&self) -> bool {
        true
    }
    /// A verse's display form (`"John 3:16"`), if it resolves.
    fn verse_display(&self, refkey: &str) -> Option<String>;
    /// The morphology gloss for a token (Full study), when the sidecar has it.
    fn morph_gloss(&self, verse: &str, token: u32) -> Option<String>;

    fn occurrence_count(&self, code: &str) -> usize;
    fn strongs(&self, code: &str) -> Option<StrongsView>;
    /// The modal English gloss for a code (recognisable rendering), when any.
    fn gloss(&self, code: &str) -> Option<String>;
    /// A chip projection (gloss + lemma) for a code.
    fn chip(&self, code: &str) -> ChipView;

    /// The rendering lens for a code (Full study): every English rendering with
    /// counts, most frequent first. Empty for an untagged code.
    fn renderings(&self, code: &str) -> Vec<RenderingView>;
    /// The verses where a code is rendered exactly `rendering` (filtered
    /// concordance); `None` when the code has no such rendering.
    fn rendering_refs(&self, code: &str, rendering: &str) -> Option<RenderingRefsView>;
    /// The codes a surface English word translates (the reverse lens).
    fn word_codes(&self, word: &str) -> Vec<String>;

    /// A code's full concordance (capped by the source).
    fn occurrences(&self, code: &str) -> OccurrencesView;

    fn bridge_partners(&self, code: &str) -> Vec<BridgePartnerView>;
    /// `(near, cross)` embedding neighbour codes (Full study; empty w/o an
    /// embedding).
    fn concept(&self, code: &str) -> Option<ConceptView>;

    fn verse_xrefs(&self, verse: &str) -> Vec<XrefView>;
    fn study_xrefs(&self, verse: &str) -> Vec<StudyXrefView>;
    /// The tags holding a verse, as `(tag index, name)`.
    fn verse_tags(&self, verse: &str) -> Vec<(usize, String)>;
    fn verse_notes(&self, verse: &str) -> Vec<String>;
    /// The reader's own personal note on a verse (Tier 0 #3), if any.
    fn user_note(&self, verse: &str) -> Option<String>;

    fn threads(&self) -> Vec<ThreadView>;
    fn tags(&self) -> Vec<TagView>;
    /// The full weave library (compare card + weaves list).
    fn weaves(&self) -> Vec<WeaveView>;
    /// The suggested-weave review queue.
    fn suggested(&self) -> Vec<SuggestedView>;
    /// A verse's tokens (compare-card side rendering); `None` if it doesn't resolve.
    fn verse_tokens(&self, refkey: &str) -> Option<VerseTokensView>;
    /// A verse's plain body text (search snippet windowing).
    fn verse_body(&self, refkey: &str) -> Option<String>;
    /// Run a search query.
    fn search(&self, query: &str) -> SearchView;
}

// ── the link router (one verb vocabulary, shared by every shell) ──────────────
//
// The producer above bakes every interactive `uri`; [`parse_link`] turns one
// back into a typed verb. Emit and parse live side by side here, so the verb
// vocabulary is a single source both shells route through (GTK calls it
// directly; the non-Rust shells via `plumbline_route_link_json`) — a verb can't
// drift between what the panel emits and what a shell handles. Navigation and
// native prompts stay shell-side; the write verbs still call the author
// endpoints (which need shell-gathered input).

/// A parsed panel link. Read verbs re-fetch a view or navigate; write verbs
/// (the `Edit*` / `Add*` / `Untag` / `Approve` / `Reject` family) drive an
/// author endpoint after the shell gathers any input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelLink {
    /// `go:BOOK:CH[:V]` — navigate the active pane (V bands the verse).
    Go {
        book: String,
        chapter: u32,
        verse: Option<u32>,
    },
    /// `occ:CODE` — the code's full concordance.
    Occurrences {
        code: String,
    },
    /// `rend:CODE:RENDERING` — the concordance filtered to one rendering.
    Rendering {
        code: String,
        rendering: String,
    },
    /// `code:CODE[:WORD]` — the standalone code-study card.
    CodeStudy {
        code: String,
        word: String,
    },
    /// `thread:I` / `tag:I` / `weave:I` — open a detail / compare view.
    Thread {
        index: usize,
    },
    Tag {
        index: usize,
    },
    Weave {
        index: usize,
    },
    /// `addtag:REF` / `addthread:REF` — prompt, then author onto REF.
    AddTag {
        refkey: String,
    },
    AddThread {
        refkey: String,
    },
    /// `untag:I:REF` — remove REF from tag I.
    Untag {
        tag: usize,
        refkey: String,
    },
    /// `makeweave:I` — weave tag I's passages (the shell may offer a subset)
    /// into a canon-ordered chain via `weave_from_tag`.
    MakeWeave {
        tag: usize,
    },
    /// `approve:I` / `reject:I` — resolve a suggested weave (suggested ordinal).
    Approve {
        index: usize,
    },
    Reject {
        index: usize,
    },
    /// `editthreadnotes:I` / `editweavenotes:I` — prompt, then set notes.
    EditThreadNotes {
        index: usize,
    },
    EditWeaveNotes {
        index: usize,
    },
    /// `editentrynote:T:E` — prompt, then set thread T's entry E note.
    EditEntryNote {
        thread: usize,
        entry: usize,
    },
    /// `editnote:REF` — prompt, then set the reader's personal note on REF.
    EditNote {
        refkey: String,
    },
    /// `guide` / `about` — open the in-app guide / about card in the panel.
    Guide,
    About,
}

/// Parse a panel `uri` into a typed [`PanelLink`]; `None` for an unknown verb or
/// malformed payload (a shell then ignores the click, as both do today).
pub fn parse_link(uri: &str) -> Option<PanelLink> {
    let (verb, rest) = uri.split_once(':').unwrap_or((uri, ""));
    Some(match verb {
        "go" => {
            // `BOOK:CH[:V]` — the book may contain spaces ("1 John") but never a
            // ':', so split from the left into at most three parts.
            let segs: Vec<&str> = rest.splitn(3, ':').collect();
            match segs.as_slice() {
                [book, ch] => PanelLink::Go { book: (*book).to_string(), chapter: ch.parse().ok()?, verse: None },
                [book, ch, v] => {
                    PanelLink::Go { book: (*book).to_string(), chapter: ch.parse().ok()?, verse: Some(v.parse().ok()?) }
                }
                _ => return None,
            }
        }
        "occ" => PanelLink::Occurrences { code: rest.to_string() },
        "rend" => {
            let (code, rendering) = rest.split_once(':')?;
            PanelLink::Rendering { code: code.to_string(), rendering: rendering.to_string() }
        }
        "code" => {
            let (code, word) = rest.split_once(':').unwrap_or((rest, ""));
            PanelLink::CodeStudy { code: code.to_string(), word: word.to_string() }
        }
        "thread" => PanelLink::Thread { index: rest.parse().ok()? },
        "tag" => PanelLink::Tag { index: rest.parse().ok()? },
        "weave" => PanelLink::Weave { index: rest.parse().ok()? },
        "addtag" => PanelLink::AddTag { refkey: rest.to_string() },
        "addthread" => PanelLink::AddThread { refkey: rest.to_string() },
        "untag" => {
            let (i, refkey) = rest.split_once(':')?;
            PanelLink::Untag { tag: i.parse().ok()?, refkey: refkey.to_string() }
        }
        "makeweave" => PanelLink::MakeWeave { tag: rest.parse().ok()? },
        "approve" => PanelLink::Approve { index: rest.parse().ok()? },
        "reject" => PanelLink::Reject { index: rest.parse().ok()? },
        "editthreadnotes" => PanelLink::EditThreadNotes { index: rest.parse().ok()? },
        "editweavenotes" => PanelLink::EditWeaveNotes { index: rest.parse().ok()? },
        "editentrynote" => {
            let (t, e) = rest.split_once(':')?;
            PanelLink::EditEntryNote { thread: t.parse().ok()?, entry: e.parse().ok()? }
        }
        "editnote" => PanelLink::EditNote { refkey: rest.to_string() },
        "guide" => PanelLink::Guide,
        "about" => PanelLink::About,
        _ => return None,
    })
}

// ── shared helpers ────────────────────────────────────────────────────────────

/// `"Gen 1:7"` → `"go:Gen:1:7"` (the navigate verb the shell routes).
pub fn go_uri(refkey: &str) -> String {
    match refkey.rfind(' ') {
        Some(sp) => format!("go:{}:{}", &refkey[..sp], &refkey[sp + 1..]),
        None => format!("go:{refkey}"),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        one.to_string()
    } else {
        many.to_string()
    }
}

/// A `go:` link run for a verse.
fn go(refkey: &str, display: &str, size: f32) -> Run {
    Run::new(display, size, Color::Gold).link(go_uri(refkey))
}

/// The occurrence chips ("gloss lemma" joined by middots, each → `occ:CODE`).
fn concept_chips(src: &dyn PanelSource, size: f32, codes: &[String]) -> Vec<Run> {
    let mut runs = Vec::new();
    for (i, code) in codes.iter().enumerate() {
        if i > 0 {
            runs.push(Run::new("  ·  ", size, Color::Ink));
        }
        let c = src.chip(code);
        let label = c.gloss.clone().or_else(|| c.lemma.clone()).unwrap_or_else(|| code.clone());
        runs.push(Run::new(label, size, Color::Gold).link(format!("occ:{code}")));
        if let (Some(_), Some(lemma)) = (&c.gloss, &c.lemma) {
            runs.push(Run::new(format!(" {lemma}"), size - 1.0, Color::Lemma));
        }
    }
    runs
}

/// The additive tier-mark glyphs (never one "winning" tier) + a research flask.
fn tier_marks(runs: &mut Vec<Run>, tiers: &[String], research: bool) {
    let has = |t: &str| tiers.iter().any(|x| x == t);
    if has("god") {
        runs.push(Run::new(" ✝", sz::MARK, Color::TierGod));
    }
    if has("human") {
        runs.push(Run::new(" †", sz::MARK, Color::TierHuman));
    }
    if has("machine") {
        runs.push(Run::new(" ≈", sz::MARK, Color::TierMachine));
    }
    if research {
        runs.push(Run::new(" ⚗", sz::MARK, Color::TierResearch));
    }
}

/// The provenance legend, shown once at the foot of a Full-study card.
fn legend() -> Block {
    Block::para(vec![
        Run::new("where this comes from:  ", sz::MARK, Color::Faded),
        Run::new("✝", sz::MARK, Color::TierGod),
        Run::new(" the text  ·  ", sz::MARK, Color::Faded),
        Run::new("†", sz::MARK, Color::TierHuman),
        Run::new(" curated scholarship  ·  ", sz::MARK, Color::Faded),
        Run::new("≈", sz::MARK, Color::TierMachine),
        Run::new(" machine-derived, weigh it  ·  ", sz::MARK, Color::Faded),
        Run::new("⚗", sz::MARK, Color::TierResearch),
        Run::new(" research-grade", sz::MARK, Color::Faded),
    ])
}

// ── word study ────────────────────────────────────────────────────────────────

/// Which analysis tiers the reader has switched on. The text (and the reader's
/// own data — tags, notes, author actions) is always on; **human** gates the
/// curated-scholarship tiers (renderings, morphology, same-root, TSK) and
/// **machine** the learned/statistical ones (embeddings, concept, SIF,
/// leitwort). Replaces the old all-or-nothing Simple/Full request flag — the
/// reader accumulates tags in any mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gates {
    pub human: bool,
    pub machine: bool,
}

impl Gates {
    pub const ALL: Gates = Gates { human: true, machine: true };
    pub const TEXT_ONLY: Gates = Gates { human: false, machine: false };

    /// The legacy Simple/Full flag as gates (Full = everything on).
    pub fn from_full(full: bool) -> Gates {
        if full {
            Gates::ALL
        } else {
            Gates::TEXT_ONLY
        }
    }

    /// The wire form: bit 0 = human, bit 1 = machine.
    pub fn from_bits(bits: u32) -> Gates {
        Gates { human: bits & 1 != 0, machine: bits & 2 != 0 }
    }

    pub fn any(self) -> bool {
        self.human || self.machine
    }
}

/// The study of a clicked word: its Strong's entries (dictionary + gated
/// analysis tiers), this verse's cross-references, and its margin notes.
/// `verse`/`token` locate the tap; `codes` are its Strong's codes. The
/// legacy `full: bool` shape lives on as [`word_study`].
pub fn word_study_gated(src: &dyn PanelSource, gates: Gates, verse: &str, token: u32, codes: &[String]) -> Vec<Block> {
    let mut out = Vec::new();
    let display = src.verse_display(verse).unwrap_or_else(|| verse.to_string());
    let word = src.token_word(verse, token).unwrap_or_default();

    out.push(Block::para(vec![Run::new(display, sz::BODY, Color::Ink).bold()]));
    if !word.is_empty() {
        out.push(Block::para(vec![Run::new(&word, sz::WORD, Color::Ink)]));
    }
    // WHAT A SECOND TRANSLATION CAN AND CANNOT HAVE, and the line between them
    // is exactly whether a thing is keyed by TOKEN INDEX or by refKey.
    //
    // Token-keyed, so meaningless here: Strong's, the morphology gloss,
    // renderings, same-root, the concept lens. The German corpus tokenizes the
    // same verse into different words at different indices, so this evidence
    // would describe a different set of words while appearing to describe these.
    //
    // refKey-keyed, so perfectly valid here: the reader's own note, their tags
    // and threads, and the Treasury's CROSS-REFERENCES — which is the one that
    // matters, because it is a lot of real study value and my first pass threw it
    // away by returning early (caught by reading the test's own output).
    let kjv = src.is_kjv_text();

    if kjv && gates.human {
        if let Some(g) = src.morph_gloss(verse, token) {
            out.push(Block::para(vec![Run::new(g, sz::NOTE, Color::Morph).italic()]));
        }
    }
    // The reader's own note rides near the top — it's what they wrote, not
    // evidence to scroll for (product feedback 2026-07-25).
    user_note_block(src, verse, &mut out);

    if !kjv {
        // Said once, plainly, in the reader's language. Showing the English
        // evidence instead read as an app that had half-forgotten which language
        // it was in (UAT, 2026-08-03).
        out.push(Block::para(vec![
            Run::new(crate::i18n::t(crate::i18n::active(), "study.onlyKjv", &[]), sz::NOTE, Color::Faded).italic(),
        ]));
    } else {
        if codes.is_empty() {
            out.push(Block::para(vec![
                Run::new(crate::i18n::t(crate::i18n::active(), "study.noStrongs", &[]), sz::BODY, Color::Faded)
                    .italic(),
            ]));
        }
        for code in codes {
            code_study(src, code, &word, gates, &mut out);
        }
    }
    verse_extras(src, verse, gates, &mut out);
    if kjv && gates.any() && !codes.is_empty() {
        out.push(legend());
    }
    out
}

/// Legacy Simple/Full entry point (the GTK shell + the v1 ABI endpoints call
/// this shape); Full switches every tier on.
pub fn word_study(src: &dyn PanelSource, full: bool, verse: &str, token: u32, codes: &[String]) -> Vec<Block> {
    word_study_gated(src, Gates::from_full(full), verse, token, codes)
}

/// The study of one Strong's code, appended to `out`: dictionary entry plus
/// the gated tiers — the rendering lens + same-root under `human`, the
/// analytics under `machine`. Rendered inline for each of a tapped word's
/// codes and standalone by [`code_study_card`].
fn code_study(src: &dyn PanelSource, code: &str, word: &str, gates: Gates, out: &mut Vec<Block>) {
    out.push(Block::Rule);

    let n = src.occurrence_count(code);
    out.push(Block::para(vec![
        Run::new(code, sz::BODY, Color::Ink).bold(),
        Run::new("   ", sz::BODY, Color::Ink),
        Run::new(format!("{n} occurrence{} ▸", plural(n, "", "s")), sz::BODY, Color::Gold).link(format!("occ:{code}")),
    ]));

    match src.strongs(code) {
        Some(e) => {
            if let Some(l) = &e.lemma {
                out.push(Block::para(vec![Run::new(l, sz::LEMMA, Color::Ink)]));
            }
            if let Some(x) = &e.xlit {
                out.push(Block::para(vec![Run::new(x, sz::SMALL, Color::Ink).italic()]));
            }
            if let Some(p) = &e.pron {
                out.push(Block::para(vec![Run::new(format!("/{p}/"), sz::SMALL, Color::Mono)]));
            }
            if let Some(d) = &e.deriv {
                out.push(Block::para(vec![Run::new(d, sz::SMALL, Color::Faded).italic()]));
            }
            if let Some(d) = &e.def {
                out.push(Block::para(vec![Run::new(d, sz::BODY, Color::Ink)]));
            }
            if let Some(k) = &e.kjv {
                out.push(Block::para(vec![Run::new(format!("KJV: {k}"), sz::NOTE, Color::Faded)]));
            }
        }
        None => out.push(Block::para(vec![Run::new("(not in the dictionary)", sz::BODY, Color::Faded).italic()])),
    }

    if !gates.any() {
        return;
    }

    // RENDERINGS: the English words this code is translated as, most frequent
    // first; the tapped word's own rendering is bold.
    let rends = if gates.human { src.renderings(code) } else { Vec::new() };
    if !rends.is_empty() {
        out.push(Block::section_marked("RENDERINGS", "†", Color::TierHuman));
        let wkey = render_key(word);
        let mut runs = Vec::new();
        for (i, r) in rends.iter().enumerate() {
            if i > 0 {
                runs.push(Run::new("  ·  ", sz::LIST, Color::Ink));
            }
            let tapped = !wkey.is_empty() && render_key(&r.rendering) == wkey;
            let mut link = Run::new(&r.rendering, sz::LIST, Color::Gold).link(format!("rend:{code}:{}", r.rendering));
            link.bold = tapped;
            runs.push(link);
            runs.push(Run::new(format!(" ×{}", r.total), sz::FINE, Color::Faded));
        }
        out.push(Block::para(runs));

        // Reverse lens: if the tapped word also stands for other codes.
        if !word.is_empty() {
            let others: Vec<String> = src.word_codes(word).into_iter().filter(|c| c != code).collect();
            if !others.is_empty() {
                let mut runs = vec![Run::new(format!("“{word}” also translates "), sz::FINE, Color::Faded)];
                for (i, o) in others.iter().enumerate() {
                    if i > 0 {
                        runs.push(Run::new(", ", sz::FINE, Color::Faded));
                    }
                    let label = match src.gloss(o) {
                        Some(g) => format!("{o} ({g})"),
                        None => o.clone(),
                    };
                    runs.push(Run::new(label, sz::FINE, Color::Gold).link(format!("code:{o}:{word}")));
                }
                out.push(Block::Para { runs, indent: false, top_gap: false });
            }
        }
    }

    let partners = if gates.human { src.bridge_partners(code) } else { Vec::new() };
    if !partners.is_empty() {
        out.push(Block::section("SAME ROOT ACROSS TESTAMENTS"));
        for p in partners.iter().take(6) {
            let mut runs = concept_chips(src, sz::LIST, std::slice::from_ref(&p.code));
            runs.push(Run::new(format!("   {}", p.sources.join(" + ")), sz::FINE, Color::Faded));
            tier_marks(&mut runs, &p.tiers, p.research_grade);
            out.push(Block::para(runs));
        }
    }

    if !gates.machine {
        return;
    }

    if let Some(c) = src.concept(code) {
        if !c.community.is_empty() {
            out.push(Block::section_marked("APPEARS ALONGSIDE", "≈", Color::TierMachine));
            let take: Vec<String> = c.community.iter().take(8).cloned().collect();
            out.push(Block::para(concept_chips(src, sz::LIST, &take)));
        }
        if !c.top_books.is_empty() {
            out.push(Block::section_marked("MOST USED IN", "≈", Color::TierMachine));
            let joined = c.top_books.iter().map(|(b, n)| format!("{b} ×{n}")).collect::<Vec<_>>().join(" · ");
            out.push(Block::para(vec![
                Run::new(joined, sz::SMALL, Color::Ink),
                Run::new(format!("   (OT {} · NT {})", c.ot, c.nt), sz::CAPTION, Color::Faded),
            ]));
        }
        if let Some(lw) = &c.leitwort {
            out.push(Block::section_marked("LEITWORT", "≈", Color::TierMachine));
            out.push(Block::para(vec![
                Run::new(
                    format!("{} of its {} uses cluster in {} ", lw.win_count, lw.n, lw.label),
                    sz::SMALL,
                    Color::Ink,
                ),
                Run::new(format!("(p ≈ 10^−{:.1})", lw.score), sz::CAPTION, Color::Faded),
            ]));
        }
    }
}

/// The per-verse extras after a word's code blocks: author actions, weave + TSK
/// cross-references, tags, and margin notes. Author
/// actions and the verse's tags are the reader's own data — never gated
/// (tags accumulate in any mode; the weave comes later).
fn verse_extras(src: &dyn PanelSource, verse: &str, gates: Gates, out: &mut Vec<Block>) {
    out.push(Block::Para {
        runs: vec![
            Run::new(s("panel.tagVerse"), sz::LIST, Color::Gold).link(format!("addtag:{verse}")),
            Run::new("     ", sz::LIST, Color::Ink),
            Run::new(s("panel.addToThread"), sz::LIST, Color::Gold).link(format!("addthread:{verse}")),
        ],
        indent: false,
        top_gap: true,
    });

    let xrefs = src.verse_xrefs(verse);
    if !xrefs.is_empty() {
        out.push(Block::para(vec![
            Run::new(sn("panel.xrefs", xrefs.len()), sz::LABEL, Color::Ink).bold()
        ]));
        for p in xrefs.iter().take(40) {
            let weave = match p.weave_index {
                Some(wi) => Run::new(&p.weave, sz::CAPTION, Color::Faded).link(format!("weave:{wi}")),
                None => Run::new(&p.weave, sz::CAPTION, Color::Faded),
            };
            out.push(Block::para(vec![
                go(&p.verse, &p.display, sz::LIST),
                Run::new("   ", sz::LIST, Color::Ink),
                weave,
            ]));
        }
    }

    if gates.human {
        let sx = src.study_xrefs(verse);
        if !sx.is_empty() {
            out.push(Block::para(vec![
                Run::new(sn("panel.studyXrefs", sx.len()), sz::LABEL, Color::Ink).bold(),
                Run::new("  TSK", sz::FINE, Color::Mono),
                Run::new("  †", sz::MARK, Color::TierHuman),
            ]));
            for r in sx.iter().take(40) {
                let mut runs = vec![go(&r.to, &r.to_display, sz::LIST)];
                if let (Some(end), Some(ed)) = (&r.end, &r.end_display) {
                    runs.push(Run::new("–", sz::LIST, Color::Ink));
                    runs.push(go(end, ed, sz::LIST));
                }
                out.push(Block::para(runs));
            }
            if sx.len() > 40 {
                out.push(Block::para(vec![
                    Run::new(format!("… {} more", sx.len() - 40), sz::CAPTION, Color::Faded).italic()
                ]));
            }
        }
    }

    let tags = src.verse_tags(verse);
    if !tags.is_empty() {
        out.push(Block::para(vec![Run::new(s("panel.tags"), sz::LABEL, Color::Ink).bold()]));
        for (i, name) in &tags {
            out.push(Block::para(vec![
                Run::new(name, sz::LIST, Color::Gold).link(format!("tag:{i}")),
                Run::new("  ", sz::LIST, Color::Ink),
                Run::new("✕", sz::LIST, Color::Faded).link(format!("untag:{i}:{verse}")),
            ]));
        }
    }

    let notes = src.verse_notes(verse);
    if !notes.is_empty() {
        out.push(Block::para(vec![
            Run::new(s("panel.marginNotes"), sz::LABEL, Color::Ink).bold(),
            Run::new("  †", sz::MARK, Color::TierHuman),
        ]));
        for n in &notes {
            out.push(Block::para(vec![Run::new(n, sz::NOTE, Color::Faded)]));
        }
    }
}

/// The reader's own note for a verse — emitted near the **top** of the word
/// study (their words come before the evidence; product feedback 2026-07-25).
/// Never gated. The edit link prompts; empty text clears.
fn user_note_block(src: &dyn PanelSource, verse: &str, out: &mut Vec<Block>) {
    let mine = src.user_note(verse);
    out.push(Block::Para {
        runs: vec![
            Run::new(s("panel.yourNote"), sz::LABEL, Color::Ink).bold(),
            Run::new("   ", sz::LABEL, Color::Ink),
            Run::new(
                if mine.as_deref().is_some_and(|t| !t.is_empty()) { s("panel.editNote") } else { s("panel.addNote") },
                sz::CAPTION,
                Color::Gold,
            )
            .link(format!("editnote:{verse}")),
        ],
        indent: false,
        top_gap: true,
    });
    if let Some(text) = mine {
        if !text.is_empty() {
            out.push(Block::para(vec![Run::new(text, sz::NOTE, Color::Ink)]));
        }
    }
}

/// The standalone `code:CODE[:word]` study card (the reverse rendering-lens
/// target): the code's own entry, so "'love' also translates G5368" lands on
/// G5368 rather than a bare concordance. `word` is the surface that led here.
pub fn code_study_card(src: &dyn PanelSource, full: bool, code: &str, word: &str) -> Vec<Block> {
    code_study_card_gated(src, Gates::from_full(full), code, word)
}

/// [`code_study_card`] with per-tier gates (the v2 endpoints' shape).
pub fn code_study_card_gated(src: &dyn PanelSource, gates: Gates, code: &str, word: &str) -> Vec<Block> {
    let mut out = Vec::new();
    code_study(src, code, word, gates, &mut out);
    if gates.any() {
        out.push(legend());
    }
    out
}

// ── concordance ───────────────────────────────────────────────────────────────

/// The full concordance for a code: header + every occurrence verse (capped).
pub fn concordance(src: &dyn PanelSource, code: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let occ = src.occurrences(code);
    if occ.total == 0 && occ.verses.is_empty() {
        return vec![Block::para(vec![Run::new(format!("no occurrences of {code}"), sz::BODY, Color::Faded).italic()])];
    }
    let lemma = src.strongs(code).and_then(|e| e.lemma);
    let mut head = vec![Run::new(code, sz::TITLE, Color::Ink).bold()];
    if let Some(l) = lemma {
        head.push(Run::new(format!("  {l}"), sz::TITLE, Color::Ink));
    }
    out.push(Block::para(head));
    out.push(Block::para(vec![Run::new(
        format!("{} occurrence{}", occ.total, if occ.total == 1 { "" } else { "s" }),
        sz::SMALL,
        Color::Gold,
    )]));
    for (refkey, display) in &occ.verses {
        out.push(Block::para(vec![go(refkey, display, sz::LIST)]));
    }
    let shown = occ.verses.len() as u32;
    if occ.total > shown {
        out.push(Block::para(vec![
            Run::new(format!("… {} more", occ.total - shown), sz::CAPTION, Color::Faded).italic()
        ]));
    }
    out
}

/// The concordance filtered to one rendering of a code (a RENDERINGS chip).
pub fn rendering_concordance(src: &dyn PanelSource, code: &str, rendering: &str) -> Vec<Block> {
    let Some(m) = src.rendering_refs(code, rendering) else {
        return vec![Block::para(vec![Run::new(
            format!("no “{rendering}” rendering of {code}"),
            sz::BODY,
            Color::Faded,
        )
        .italic()])];
    };
    let mut out = vec![
        Block::para(vec![
            Run::new(code, sz::TITLE, Color::Ink).bold(),
            Run::new(format!("  “{}”", m.rendering), sz::TITLE, Color::Ink),
        ]),
        Block::para(vec![Run::new(
            format!("{} verse{} rendered “{}”", m.total, if m.total == 1 { "" } else { "s" }, m.rendering),
            sz::SMALL,
            Color::Gold,
        )]),
    ];
    for (refkey, display) in &m.refs {
        out.push(Block::para(vec![go(refkey, display, sz::LIST)]));
    }
    let shown = m.refs.len() as u32;
    if m.total > shown {
        out.push(Block::para(
            vec![Run::new(format!("… {} more", m.total - shown), sz::CAPTION, Color::Faded).italic()],
        ));
    }
    out
}

// ── threads / tags / weaves lists ─────────────────────────────────────────────

/// How many refs a card shows before an "… N more" tail.
const LIST_CAP: usize = 40;

pub fn threads_list(src: &dyn PanelSource) -> Vec<Block> {
    let threads = src.threads();
    let mut out =
        vec![Block::para(vec![Run::new(format!("Threads ({})", threads.len()), sz::TITLE, Color::Ink).bold()])];
    if threads.is_empty() {
        out.push(Block::para(vec![Run::new(
            "No threads yet — open a word study and “＋ add to thread”.",
            sz::SMALL,
            Color::Faded,
        )
        .italic()]));
    }
    for (i, t) in threads.iter().enumerate() {
        out.push(Block::para(vec![
            Run::new(&t.name, sz::BODY, Color::Gold).link(format!("thread:{i}")),
            Run::new(
                format!("   {} passage{}", t.entries.len(), plural(t.entries.len(), "", "s")),
                sz::CAPTION,
                Color::Faded,
            ),
        ]));
    }
    out
}

pub fn thread_detail(src: &dyn PanelSource, index: usize) -> Vec<Block> {
    let threads = src.threads();
    let Some(t) = threads.get(index) else { return threads_list(src) };
    let mut out = vec![
        Block::para(vec![Run::new(&t.name, sz::TITLE, Color::Ink).bold()]),
        Block::para(vec![
            Run::new(
                format!("{} passage{}", t.entries.len(), plural(t.entries.len(), "", "s")),
                sz::SMALL,
                Color::Faded,
            ),
            Run::new("   ", sz::SMALL, Color::Ink),
            Run::new(s("panel.notes"), sz::CAPTION, Color::Faded).link(format!("editthreadnotes:{index}")),
        ]),
    ];
    if !t.notes.is_empty() {
        out.push(Block::para(vec![Run::new(&t.notes, sz::NOTE, Color::Faded)]));
    }
    for (e, en) in t.entries.iter().enumerate() {
        out.push(Block::Rule);
        out.push(Block::para(vec![
            go(&en.verse, &en.display, sz::LIST),
            Run::new("   ", sz::LIST, Color::Ink),
            Run::new(s("panel.note"), sz::CAPTION, Color::Faded).link(format!("editentrynote:{index}:{e}")),
        ]));
        let joined = en.text.join(" ");
        let snap = if joined.chars().count() > 70 {
            format!("{}…", joined.chars().take(70).collect::<String>().trim_end())
        } else {
            joined
        };
        if !snap.is_empty() {
            out.push(Block::para(vec![Run::new(format!("“{snap}”"), sz::NOTE, Color::Faded).italic()]));
        }
        if let Some(note) = &en.note {
            if !note.is_empty() {
                out.push(Block::para(vec![Run::new(format!("— {note}"), sz::NOTE, Color::Mono)]));
            }
        }
    }
    out
}

pub fn tags_list(src: &dyn PanelSource) -> Vec<Block> {
    let tags = src.tags();
    let mut out = vec![Block::para(vec![Run::new(format!("Tags ({})", tags.len()), sz::TITLE, Color::Ink).bold()])];
    if tags.is_empty() {
        out.push(Block::para(vec![Run::new(
            "No tags yet — open a word study and “＋ tag verse”.",
            sz::SMALL,
            Color::Faded,
        )
        .italic()]));
    }
    for (i, t) in tags.iter().enumerate() {
        out.push(Block::para(vec![
            Run::new(&t.name, sz::BODY, Color::Gold).link(format!("tag:{i}")),
            Run::new(
                format!("   {} member{}", t.members.len(), plural(t.members.len(), "", "s")),
                sz::CAPTION,
                Color::Faded,
            ),
        ]));
    }
    out
}

pub fn tag_detail(src: &dyn PanelSource, index: usize) -> Vec<Block> {
    let tags = src.tags();
    let Some(t) = tags.get(index) else { return tags_list(src) };
    let mut out = vec![Block::para(vec![Run::new(&t.name, sz::TITLE, Color::Ink).bold()])];
    // Tags accumulate over time; the weave comes later — offer the conversion
    // whenever a chain is possible (≥2 verse members).
    let verse_members = t.members.iter().filter(|m| m.kind == "verse").count();
    if verse_members >= 2 {
        out.push(Block::para(vec![
            Run::new(s("panel.makeWeave"), sz::LIST, Color::Gold).link(format!("makeweave:{index}")),
            Run::new(format!("   {}", s("panel.makeWeaveHint")), sz::CAPTION, Color::Faded),
        ]));
    }
    for m in &t.members {
        let mut runs = if let Some(v) = m.verse.as_deref().filter(|_| m.kind == "verse") {
            vec![go(v, m.display.as_deref().unwrap_or(v), sz::LIST)]
        } else {
            let s = m.strongs.clone().unwrap_or_default();
            vec![Run::new(format!("≈ {s}"), sz::LIST, Color::Gold).link(format!("occ:{s}"))]
        };
        if let Some(note) = &m.note {
            if !note.is_empty() {
                runs.push(Run::new(format!("   {note}"), sz::CAPTION, Color::Mono));
            }
        }
        out.push(Block::para(runs));
    }
    out
}

/// The whole weave library, flat: name → compare card. (The constellation is
/// the graphical view of the same list.)
pub fn weaves_list(src: &dyn PanelSource) -> Vec<Block> {
    let mut ws = src.weaves();
    ws.sort_by_key(|w| std::cmp::Reverse(w.links.len()));
    let mut out = vec![Block::para(vec![Run::new(format!("Weaves ({})", ws.len()), sz::TITLE, Color::Ink).bold()])];
    if ws.is_empty() {
        // A heading over nothing reads as a broken panel. Threads, tags and the
        // review queue all say what to do here; this one said nothing.
        out.push(Block::para(vec![Run::new(
            s("panel.noWeaves"),
            sz::SMALL,
            Color::Faded,
        )
        .italic()]));
    }
    for w in &ws {
        let suffix = if w.suggested { " · suggested" } else { "" };
        out.push(Block::para(vec![
            Run::new(&w.name, sz::BODY, Color::Gold).link(format!("weave:{}", w.index)),
            Run::new(
                format!("   {} · {} link{}{suffix}", w.kind_label, w.links.len(), plural(w.links.len(), "", "s")),
                sz::CAPTION,
                Color::Faded,
            ),
        ]));
    }
    out
}

// ── suggested review queue ────────────────────────────────────────────────────

pub fn suggested(src: &dyn PanelSource) -> Vec<Block> {
    let items = src.suggested();
    let mut out =
        vec![Block::para(vec![Run::new(format!("Suggested weaves ({})", items.len()), sz::TITLE, Color::Ink).bold()])];
    if items.is_empty() {
        out.push(Block::para(vec![
            Run::new(s("panel.emptyQueue"), sz::SMALL, Color::Faded).italic()
        ]));
    }
    for w in &items {
        out.push(Block::Rule);
        out.push(Block::para(vec![
            Run::new(&w.name, sz::SUGGEST_HEAD, Color::Ink).bold(),
            Run::new(format!("   {}", w.kind), sz::CAPTION, Color::Mono),
        ]));
        if !w.notes.is_empty() {
            out.push(Block::para(vec![Run::new(&w.notes, sz::NOTE, Color::Faded)]));
        }
        for l in w.links.iter().take(LIST_CAP) {
            let mut runs = vec![
                go(&l.a, &l.a_display, sz::LIST),
                Run::new("  ↔  ", sz::LIST, Color::Ink),
                go(&l.b, &l.b_display, sz::LIST),
            ];
            if !l.label.is_empty() {
                runs.push(Run::new(format!("   {}", l.label), sz::CAPTION, Color::Faded));
            }
            out.push(Block::para(runs));
        }
        if w.links.len() > LIST_CAP {
            out.push(Block::para(vec![Run::new(
                format!("… {} more", w.links.len() - LIST_CAP),
                sz::CAPTION,
                Color::Faded,
            )
            .italic()]));
        }
        let mut actions = Vec::new();
        if let Some(li) = w.lib_index {
            actions.push(Run::new(s("panel.compare"), sz::LIST, Color::Gold).link(format!("weave:{li}")));
            actions.push(Run::new("   ", sz::LIST, Color::Ink));
        }
        actions.push(Run::new(s("panel.approve"), sz::LIST, Color::Gold).link(format!("approve:{}", w.index)));
        actions.push(Run::new("   ", sz::LIST, Color::Ink));
        actions.push(Run::new(s("panel.reject"), sz::LIST, Color::Gold).link(format!("reject:{}", w.index)));
        if let Some(li) = w.lib_index {
            actions.push(Run::new("   ", sz::LIST, Color::Ink));
            actions.push(Run::new(s("panel.note"), sz::CAPTION, Color::Faded).link(format!("editweavenotes:{li}")));
        }
        out.push(Block::para(actions));
    }
    out
}

// ── weave compare card ────────────────────────────────────────────────────────

pub fn compare_card(src: &dyn PanelSource, full: bool, index: usize) -> Vec<Block> {
    let weaves = src.weaves();
    let Some(w) = weaves.get(index) else { return Vec::new() };
    let suffix = if w.suggested { "  (suggested)" } else { "" };
    let mut out = vec![Block::para(vec![
        Run::new(&w.name, sz::TITLE, Color::Ink).bold(),
        Run::new(format!("   {}{suffix}", w.kind_label), sz::CAPTION, Color::Mono),
    ])];
    // ✎ note is the reader's own annotation — always available (author
    // actions left the Simple/Full gate with the 2026-07-25 product change).
    let _ = full;
    let mut head =
        vec![Run::new(format!("{} link{}", w.links.len(), plural(w.links.len(), "", "s")), sz::SMALL, Color::Faded)];
    head.push(Run::new("   ", sz::SMALL, Color::Ink));
    head.push(Run::new(s("panel.note"), sz::CAPTION, Color::Faded).link(format!("editweavenotes:{index}")));
    out.push(Block::para(head));
    if !w.notes.is_empty() {
        out.push(Block::para(vec![Run::new(&w.notes, sz::NOTE, Color::Faded)]));
    }
    for l in w.links.iter().take(LIST_CAP) {
        out.push(Block::Rule);
        if !l.label.is_empty() {
            out.push(Block::para(vec![Run::new(format!("“{}”", l.label), sz::NOTE, Color::Gold)]));
        }
        compare_side(src, &l.a, &l.a_display, l.span_a, &mut out);
        compare_side(src, &l.b, &l.b_display, l.span_b, &mut out);
    }
    if w.links.len() > LIST_CAP {
        out.push(Block::para(vec![Run::new(
            format!("… {} more", w.links.len() - LIST_CAP),
            sz::CAPTION,
            Color::Faded,
        )
        .italic()]));
    }
    out
}

/// One side of a compare card: the verse link, then its text small with span
/// words bold and translator-supplied words italic grey.
fn compare_side(src: &dyn PanelSource, refkey: &str, display: &str, span: Option<[u16; 2]>, out: &mut Vec<Block>) {
    out.push(Block::para(vec![go(refkey, display, sz::LIST)]));
    let Some(vt) = src.verse_tokens(refkey) else { return };
    let mut runs = Vec::new();
    for (ti, t) in vt.tokens.iter().enumerate() {
        let in_span = span.is_some_and(|[lo, hi]| ti as u16 >= lo && ti as u16 <= hi);
        let mut r = Run::new(format!("{} ", t.render), sz::NOTE, if t.added { Color::Faded } else { Color::Ink });
        r.bold = in_span;
        r.italic = t.added;
        runs.push(r);
    }
    out.push(Block::Para { runs, indent: true, top_gap: false });
}

// ── search results ────────────────────────────────────────────────────────────

pub fn search(src: &dyn PanelSource, query: &str) -> Vec<Block> {
    match src.search(query) {
        SearchView::Goto { book, chapter, verse, display } => {
            let uri = match verse {
                Some(v) => format!("go:{book}:{chapter}:{v}"),
                None => format!("go:{book}:{chapter}"),
            };
            vec![Block::para(vec![Run::new(format!("go to {display}"), sz::SEARCH_GOTO, Color::Gold).link(uri)])]
        }
        SearchView::Hits { how, total, capped, hits } => {
            let mut out = vec![Block::para(vec![Run::new(
                format!("{total} result{}", plural(total, "", "s")),
                sz::SUGGEST_HEAD,
                Color::Ink,
            )
            .bold()])];
            if !how.is_empty() {
                out.push(Block::para(vec![Run::new(how, sz::CAPTION, Color::Faded).italic()]));
            }
            if total == 0 {
                // "0 results" alone leaves the reader guessing whether they typed
                // the wrong thing or the search is broken. Say what this search
                // takes — the same three shapes the guide names.
                out.push(Block::para(vec![Run::new(
                    "Nothing matched. Try a single word (its other forms are found too), a reference like John 3:16 or 1 Cor 13, or a Strong's code like H430.",
                    sz::SMALL,
                    Color::Faded,
                )
                .italic()]));
            }
            for h in &hits {
                let mut runs = vec![go(&h.verse, &h.display, sz::LIST)];
                if !h.why.is_empty() {
                    runs.push(Run::new(format!("   {}", h.why), sz::CAPTION, Color::Mono));
                }
                if h.note {
                    runs.push(Run::new(format!("   {}", s("panel.hasNote")), sz::CAPTION, Color::Gold));
                }
                out.push(Block::para(runs));
                if let Some(snip) = snippet(src, &h.verse, query) {
                    out.push(snip);
                }
            }
            if capped {
                out.push(Block::para(vec![Run::new(
                    format!("… {} more", total.saturating_sub(hits.len())),
                    sz::CAPTION,
                    Color::Faded,
                )
                .italic()]));
            }
            out
        }
    }
}

/// A one-line context snippet for a search hit: the verse body windowed around
/// the first match of the query's first word, the match emboldened. Windowing
/// (review item 8) lives here, not in a shell. Char-indexed so a non-ASCII
/// verse never panics on a byte-boundary slice.
fn snippet(src: &dyn PanelSource, refkey: &str, query: &str) -> Option<Block> {
    let body = src.verse_body(refkey)?;
    let chars: Vec<char> = body.chars().collect();
    if chars.is_empty() {
        return None;
    }
    let needle: Vec<char> = query.split_whitespace().next().unwrap_or("").chars().collect();
    let at = find_ci(&chars, &needle);

    const WINDOW: usize = 46;
    let (mut start, mut end) = match at {
        None => (0usize, chars.len().min(2 * WINDOW)),
        Some(a) => (a.saturating_sub(WINDOW), (a + needle.len() + WINDOW).min(chars.len())),
    };
    // Snap to word boundaries so the window doesn't split a word.
    while start > 0 && chars[start - 1] != ' ' {
        start -= 1;
    }
    while end < chars.len() && chars[end] != ' ' {
        end += 1;
    }
    let slice = |lo: usize, hi: usize| chars[lo..hi].iter().collect::<String>();

    let mut runs = Vec::new();
    if start > 0 {
        runs.push(Run::new("…", sz::CAPTION, Color::Faded));
    }
    match at {
        Some(a) if a >= start => {
            runs.push(Run::new(slice(start, a), sz::CAPTION, Color::Faded));
            runs.push(Run::new(slice(a, a + needle.len()), sz::CAPTION, Color::Ink).bold());
            runs.push(Run::new(slice(a + needle.len(), end), sz::CAPTION, Color::Faded));
        }
        _ => runs.push(Run::new(slice(start, end), sz::CAPTION, Color::Faded)),
    }
    if end < chars.len() {
        runs.push(Run::new("…", sz::CAPTION, Color::Faded));
    }
    Some(Block::Para { runs, indent: true, top_gap: false })
}

// ── in-app guide + about (Tier 0 #7) ──────────────────────────────────────────
//
// Static content, produced as blocks so both shells render it with the same
// per-block renderer they already have (no shell-side guide layout). The header
// Help affordance opens `guide`; the `?`/F1 shortcuts overlay is shell-native
// (keybindings differ per shell), but the guide and about card are shared here.

/// A guide section heading + its body paragraphs.
fn guide_section(out: &mut Vec<Block>, title: &str, paras: &[&str]) {
    out.push(Block::section(title));
    for p in paras {
        out.push(Block::para(vec![Run::new(*p, sz::BODY, Color::Ink)]));
    }
}

/// A sub-heading inside a guide section, and its paragraphs. Bold rather than a
/// second `Block::section`, so the task/step hierarchy is visible at a glance
/// without inventing a heading level every shell would have to style.
fn guide_step(out: &mut Vec<Block>, title: &str, paras: &[&str]) {
    out.push(Block::para(vec![Run::new(title, sz::LABEL, Color::Ink).bold()]));
    for p in paras {
        out.push(Block::para(vec![Run::new(*p, sz::BODY, Color::Ink)]));
    }
}

/// The in-app guide: a concise tour of the reader. (The full manual lives in
/// docs/GUIDE.md; this is the on-screen version.)
///
/// Organised by **what you came to do**, not by what the app has (product call,
/// 2026-07-28). Nobody opens a Bible wanting "the study panel"; they want to
/// share the gospel, get ready to teach on Sunday, understand a passage, or keep
/// reading faithfully. Every feature below earns its place under one of those.
pub fn guide_blocks() -> Vec<Block> {
    let mut out = vec![Block::para(vec![Run::new("Using Plumbline", sz::TITLE, Color::Ink).bold()])];
    out.push(Block::para(vec![Run::new(
        "Whatever you came here to do, start with the thing you want — the rest of the app arranges itself around it.",
        sz::BODY,
        Color::Faded,
    )]));

    guide_section(
        &mut out,
        "READ THE BIBLE",
        &[
            "One to three panes sit side by side; the ＋ / ✕ in the pane strip add and remove them. Each pane has its own book and chapter and scrolls on its own, so you can hold two passages open together.",
            "Go to… walks Testament, then book, then chapter — every step a big target. Left / Right (or [ / ]) step chapters and roll across book boundaries; Alt+Left / Alt+Right walk back and forth through where you have been.",
            "Hold Shift while scrolling to move every pane together, which is how you read two accounts side by side without losing your place in either.",
        ],
    );
    guide_step(
        &mut out,
        "If the older English is hard going",
        &[
            "Turn on the Plain-English overlay in Settings. It marks the words the American King James Version puts differently — tap one to see what it replaced. The text stays the King James: nothing you memorize, present, copy or share is changed by it.",
        ],
    );
    guide_step(
        &mut out,
        "Keeping track of where you have read",
        &[
            "The book and chapter grids in Go to… tint themselves by how you have read: slate for a chapter you have never been through, amber while you are partway, sage once you have read it through. A book takes the blend of its chapters, weighted by length, so its chapters always add up to it.",
            "On top of the colour, a chapter glows the longer it has been since you read it — nothing for the first month, building to a full glow at a year. That is the whole point of the map: it shows you where you have not been lately. It is not a score, and nobody sees it but you.",
            "Reading is counted generously. A chapter fills as you move through it at a natural pace — flipping past credits nothing, and neither does leaving it open — and it counts as read through at 90%, so there is never a last verse to chase.",
            "Read a chapter in a paper Bible? Long-press its first verse and choose Mark chapter read… to set the date yourself.",
        ],
    );

    guide_section(
        &mut out,
        "SHARE THE GOSPEL",
        &[
            "Present is the mode for handing your phone to somebody, or putting the text on a screen: fullscreen, high contrast, one passage at a time, no chrome to explain. It opens on the Romans Road, ready to walk through.",
        ],
    );
    guide_step(
        &mut out,
        "Share a thread with someone",
        &[
            "Any thread can be handed over as a link or a QR code — from Present, or from the thread itself. Whoever opens it lands in the first passage with the whole trail in front of them, in their own copy of the app, offline from then on.",
            "This is the way to leave someone with something after a conversation: they keep the passages you walked through, in order, instead of a photo of a page.",
        ],
    );
    guide_step(
        &mut out,
        "Set your church",
        &[
            "Settings ▸ Church holds your congregation's name, a line about it, and a link. It travels in the links and QR codes you share, and nowhere else — no account, and nothing is sent anywhere.",
            "It matters because a stranger who reads the gospel on their phone still needs somebody to sit next to on Sunday. Fill it in and every Bible you hand out says where to find you.",
        ],
    );

    guide_section(
        &mut out,
        "PREPARE TO TEACH",
        &[
            "A thread is a trail you make: passages in your order, for your reason. It is the tool for a sermon, a lesson, a Bible study, or an argument you want to be able to walk someone through twice.",
            "Right-click (or long-press) a verse and choose Add to thread… — pick a thread you have already started, or name a new one. Each entry keeps the words you selected and takes a note of its own, and the thread itself has a running notes document for the shape of the whole thing.",
        ],
    );
    guide_step(
        &mut out,
        "Then preach it",
        &[
            "Open the thread in Present and it becomes your outline: the passages advance one at a time, big enough to read from across a room, in the order you set. The same thread you built studying is the one you teach from and the one you hand out afterwards.",
        ],
    );

    guide_section(
        &mut out,
        "STUDY A PASSAGE",
        &[
            "Ctrl+click (or double-click) a word for what is underneath it: its Strong's entry, every way the King James renders that same word elsewhere, cross-references, and — with the analysis tiers on — morphology, same-root words and the concept map.",
            "Search takes a word, a phrase, or a reference. A reference (John 3:16, 1 Cor 13, psalms) jumps there; a word or phrase lists ranked hits, and every hit in the chapter you are reading is banded as you go. A bare Strong's code (H430, G26) lists every verse tagged with it.",
        ],
    );
    guide_step(
        &mut out,
        "Follow what the text says about itself",
        &[
            "A weave is a connection you FIND — the same event in three Gospels, a prophecy and its fulfilment, an Old Testament verse and the New that quotes it. Point two panes at linked passages and the connector lines draw themselves; no mode to enter.",
            "That is the difference from a thread. A weave is in the text whether or not you noticed it, and you will stumble back into it for years. A thread is yours, and it is in the order you chose. If the connection is in the text, weave it; if the connection is your point, thread it.",
            "The Map and Constellation views show the whole weave library at once. Weaves that shipped with the app began as AI-generated study aids and say so — approving one, from its compare card, is how it graduates to something you have checked against the text yourself.",
        ],
    );
    guide_step(
        &mut out,
        "Collect a topic over time",
        &[
            "Tags are labelled sets of verses and concepts — the slow way a topic accumulates as you keep running into it. Tag a verse whenever you run into the idea again, and the tag becomes the list you wished you had kept.",
        ],
    );

    guide_section(
        &mut out,
        "HIDE IT IN YOUR HEART",
        &[
            "Any verse (or a passage, as one chunk) can become a memorization card. The drills are first-letter prompts, progressive blank-out, and typed recall; the schedule is the proven spaced-repetition algorithm, so review comes back exactly when you are about to forget.",
            "The memorize hub shows what is due, a map of the canon shaded by what you know, and a heatmap of the days you turned up.",
        ],
    );

    guide_section(
        &mut out,
        "MAKE IT YOURS",
        &[
            "Right-click a verse to copy it, copy the chapter, tag it, add it to a thread, or write a note in your own words. Your notes get a gutter mark and a “your note” line in the study panel.",
            "The theme button cycles light, a candlelight-warm dark, a true-black night, and follow-system. Your theme, text size, margins and last reading position are all remembered.",
            "Everything you write is yours and stays on your device. Settings ▸ Back up (.zip) exports all of it — notes, tags, threads, weaves, memory work and your reading history — and the same archive restores on a phone or in a browser.",
        ],
    );

    out.push(Block::Rule);
    out.push(Block::para(vec![Run::new("Press ? for keyboard shortcuts.", sz::SMALL, Color::Faded)]));
    // Guide & About are one combined card — inline the About content here.
    out.push(Block::Rule);
    about_body(&mut out);
    out
}

/// The About content (edition, provenance, covenant), pushed onto `out` so both
/// the standalone About card and the combined guide can reuse it verbatim.
fn about_body(out: &mut Vec<Block>) {
    out.push(Block::para(vec![Run::new("About Plumbline", sz::TITLE, Color::Ink).bold()]));
    out.push(Block::para(vec![Run::new(
        "A KJV-only Bible-study tool: a parallel-passage reader with an optional Full-study tier of Strong's, morphology, cross-references, and corpus analytics. Everything runs locally and offline.",
        sz::BODY,
        Color::Ink,
    )]));
    guide_section(
        out,
        "THE TEXT",
        &[
            "The Holy Bible — the traditional English text, in the public domain.",
            "The text is the KJV, always. An optional plain-English overlay marks where the American King James Version (1999, public domain) words a verse differently — a dotted underline; tap one to see the KJV word it replaced. It is a reading aid over the same text, not a second translation: what you memorize, present, copy and share is the KJV whether the overlay is on or off.",
        ],
    );
    guide_section(
        out,
        "PROVENANCE",
        &[
            "KJV text via eBible.org; Strong's via Open Scriptures (CC-BY-SA); morphology from OSHB (CC-BY 4.0) and Robinson's public-domain Textus Receptus tagging; cross-references from the Treasury of Scripture Knowledge via openbible.info. Full credits are in BIBLIOGRAPHY.md.",
        ],
    );
    guide_section(
        out,
        "THE COVENANT",
        &[
            "Yours forever: no account, no ads, no tracking, nothing sent anywhere. Your library — notes, tags, threads — is saved as ordinary files on your device that you can back up or move. The app is free.",
        ],
    );
}

/// The About card: edition, provenance, and the covenant (also inlined at the
/// end of [`guide_blocks`] so Guide & About read as one card).
pub fn about_blocks() -> Vec<Block> {
    let mut out = Vec::new();
    about_body(&mut out);
    out
}

/// Case-insensitive (ASCII-fold) substring search over char slices.
fn find_ci(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    let fold = |c: char| c.to_ascii_lowercase();
    'outer: for i in 0..=haystack.len() - needle.len() {
        for j in 0..needle.len() {
            if fold(haystack[i + j]) != fold(needle[j]) {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

#[cfg(test)]
mod tests;
