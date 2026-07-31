//! Memorization: spaced-repetition over verses (Tier 2 #15 — flagged top
//! priority). The KJV is *the* memorization text (homeschool, AWANA, Bible
//! bees); this is the core every shell drives.
//!
//! One SRS card per verse (keyed by [`VRef`]), scheduled with **SM-2** (the
//! proven SuperMemo/Anki-classic algorithm). Each card keeps its full **review
//! log** — that log is the data, "by construction", behind both views the
//! product wants: a **coverage map** across the canon (per-verse mastery +
//! recency, in the canon-strip visual language) and an **activity heatmap over
//! time** (reviews per day).
//!
//! Also here: pure text drills over a verse's words — first-letter prompts,
//! progressive blank-out, and typed-recall scoring — shell-agnostic so every
//! shell renders the same drill. Decks are sourced from a tag or thread by the
//! shell (it passes the verse set); this module owns the per-verse cards.
//!
//! Personal study data: one plain JSON file per verse under `home/memory/`,
//! `format` stamped, additive-friendly, refKey inside (the filename is a slug).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::civil::{add_days, date_to_days, days_to_date};
use crate::reference::VRef;
use crate::Error;

const FORMAT: &str = "overlay-memory-v1";

/// The starting ease factor (SM-2 EF); the floor it can never drop below.
const EASE_START: f32 = 2.5;
const EASE_FLOOR: f32 = 1.3;
/// Interval (days) at/above which a card counts as "mature" (Anki's convention).
const MATURE_DAYS: u32 = 21;
/// Blank-out levels: 0 = full text … [`MAX_BLANK_LEVEL`] = every word masked.
pub const MAX_BLANK_LEVEL: u8 = 4;

// ── grades + review log ──────────────────────────────────────────────────────

/// A recall grade (Anki's four buttons), mapped to an SM-2 quality 0–5.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    /// Failed to recall — a lapse; the card resets to relearning.
    Again,
    /// Recalled with serious difficulty.
    Hard,
    /// Recalled correctly.
    Good,
    /// Recalled effortlessly.
    Easy,
}

impl Grade {
    /// Parse a grade token (`again` / `hard` / `good` / `easy`) — the shells'
    /// four review buttons over the ABI.
    pub fn parse(s: &str) -> Option<Grade> {
        match s {
            "again" => Some(Grade::Again),
            "hard" => Some(Grade::Hard),
            "good" => Some(Grade::Good),
            "easy" => Some(Grade::Easy),
            _ => None,
        }
    }

    /// SM-2 quality (q < 3 is a failed recall).
    fn quality(self) -> u8 {
        match self {
            Grade::Again => 1,
            Grade::Hard => 3,
            Grade::Good => 4,
            Grade::Easy => 5,
        }
    }
    fn passed(self) -> bool {
        self.quality() >= 3
    }
}

/// One review event — the audit + heatmap source. `at` is the caller-supplied
/// UTC timestamp; `interval_days` is the interval this review scheduled next.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Review {
    pub at: String,
    pub grade: Grade,
    #[serde(rename = "intervalDays")]
    pub interval_days: u32,
    /// Unknown keys on this review, kept — see [`Card::extra`]. The log is the
    /// data behind the coverage map and the heatmap, and it is never rewritten,
    /// only appended to; a key stripped from one entry is gone as finally as one
    /// stripped from the card.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

// ── the card ─────────────────────────────────────────────────────────────────

/// An SRS card for one verse — or, when `through` is set, for a passage read
/// and recalled as one chunk: its SM-2 schedule plus every review.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub verse: VRef,
    /// Last verse of the passage, inclusive — `None` for a single-verse card.
    /// A card is keyed and filed by its FIRST verse, so this is additive: a
    /// reader on an older build sees the passage card as a card on its opening
    /// verse rather than losing it (`overlay-memory-v1` is frozen, §Data
    /// formats). Always the same book and chapter as `verse` — see
    /// [`Card::new_passage`].
    pub through: Option<VRef>,
    pub tok_version: String,
    pub created: String,
    /// SM-2 ease factor (EF), ≥ 1.3.
    pub ease: f32,
    /// Current inter-review interval, in days (0 for a brand-new card).
    pub interval_days: u32,
    /// Consecutive successful recalls (resets to 0 on a lapse).
    pub reps: u32,
    /// Lifetime count of lapses (`Again`).
    pub lapses: u32,
    /// Next-due date, `YYYY-MM-DD` (day granularity, the SRS norm).
    pub due: String,
    pub reviews: Vec<Review>,
    /// Every key in the file this build has never heard of, carried back out
    /// again on save.
    ///
    /// The on-disk formats evolve **additively** (CLAUDE.md §Data formats), and
    /// a sideloaded APK never auto-updates: a build that drops the fields of a
    /// later one drops them for good on that device. A card is months of the
    /// reader's work, so it comes back whole — review log included
    /// ([`Review::extra`]).
    ///
    /// Serde fills this with the leftovers after the known fields are matched, so
    /// a known key can never be swallowed, and a key a later version promotes to
    /// a real field stops arriving here the moment that field exists — it can
    /// never be written twice. Empty for every card on disk today, and an empty
    /// flattened map writes no key at all, so those files are written exactly as
    /// they were.
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize)]
struct CardRepr {
    format: String,
    #[serde(rename = "ref")]
    ref_key: String,
    /// Additive (2026-07-27): the passage's last verse as a refKey.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    through: Option<String>,
    tokenization: String,
    created: String,
    ease: f32,
    #[serde(rename = "intervalDays")]
    interval_days: u32,
    reps: u32,
    lapses: u32,
    due: String,
    #[serde(default)]
    reviews: Vec<Review>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl Card {
    /// A fresh card for `vref`, due immediately (interval 0), at `created`.
    pub fn new(vref: VRef, tok_version: &str, created: &str) -> Card {
        Card {
            verse: vref,
            through: None,
            tok_version: tok_version.to_string(),
            created: created.to_string(),
            ease: EASE_START,
            interval_days: 0,
            reps: 0,
            lapses: 0,
            due: days_to_date(date_to_days(created).unwrap_or(0)),
            reviews: Vec::new(),
            extra: Map::new(),
        }
    }

    /// A fresh card for the passage `start`…`through` (inclusive), due
    /// immediately. `through` is normalized: a different book, a different
    /// chapter, or an end at/before `start` collapses to a single-verse card, so
    /// a passage card always spans a real, forward, same-chapter run.
    ///
    /// (Same-chapter is the limit today — the field holds a full refKey, so
    /// crossing a chapter boundary can be allowed later without a format
    /// change.)
    pub fn new_passage(start: VRef, through: &VRef, tok_version: &str, created: &str) -> Card {
        let spans = through.book == start.book && through.chapter == start.chapter && through.verse > start.verse;
        Card { through: spans.then(|| through.clone()), ..Card::new(start, tok_version, created) }
    }

    /// Every verse this card covers, in order — one entry for a single-verse
    /// card, the whole inclusive run for a passage.
    pub fn verses(&self) -> Vec<VRef> {
        match &self.through {
            Some(end) => (self.verse.verse..=end.verse)
                .map(|v| VRef::new(self.verse.book.clone(), self.verse.chapter, v))
                .collect(),
            None => vec![self.verse.clone()],
        }
    }

    /// How this card is named to the reader: `"Ps 23:1–6"` for a passage (en
    /// dash, the way a passage is written), else the plain refKey `"Gen 1:7"`.
    pub fn label(&self) -> String {
        match &self.through {
            Some(end) => format!("{}\u{2013}{}", self.verse.ref_key(), end.verse),
            None => self.verse.ref_key(),
        }
    }

    fn to_repr(&self) -> CardRepr {
        CardRepr {
            format: FORMAT.to_string(),
            ref_key: self.verse.ref_key(),
            through: self.through.as_ref().map(VRef::ref_key),
            tokenization: self.tok_version.clone(),
            created: self.created.clone(),
            ease: self.ease,
            interval_days: self.interval_days,
            reps: self.reps,
            lapses: self.lapses,
            due: self.due.clone(),
            reviews: self.reviews.clone(),
            extra: self.extra.clone(),
        }
    }
}

/// The mastery bucket a card falls in — drives the coverage-map shading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mastery {
    /// Never reviewed.
    New,
    /// In the early (sub-day … first steps) phase, or relearning after a lapse.
    Learning,
    /// Scheduled but not yet mature (interval < 21 days).
    Young,
    /// Interval ≥ 21 days — settled into long-term memory.
    Mature,
}

// ── SM-2 scheduling ────────────────────────────────────────────────────────────

/// Apply a review `grade` at `now` (RFC3339 UTC): update the SM-2 schedule and
/// append to the review log. On a lapse the card resets to relearning; on a
/// pass the interval grows (1 → 6 → ×ease). The ease factor moves per SM-2.
pub fn review(card: &mut Card, grade: Grade, now: &str) {
    let q = grade.quality() as f32;
    // SM-2 ease update; never below the floor.
    card.ease = (card.ease + (0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02))).max(EASE_FLOOR);

    if grade.passed() {
        card.reps += 1;
        card.interval_days = match card.reps {
            1 => 1,
            2 => 6,
            _ => ((card.interval_days.max(1) as f32) * card.ease).round().max(1.0) as u32,
        };
    } else {
        card.lapses += 1;
        card.reps = 0;
        card.interval_days = 1; // relearn tomorrow
    }

    card.due = add_days(now, card.interval_days as i64);
    card.reviews.push(Review { at: now.to_string(), grade, interval_days: card.interval_days, extra: Map::new() });
}

/// Whether the card is due for review at `now` (RFC3339 UTC) — `due` on or
/// before today. Unparseable dates are treated as due (fail-safe to surfacing).
pub fn is_due(card: &Card, now: &str) -> bool {
    match (date_to_days(&card.due), date_to_days(now)) {
        (Some(due), Some(today)) => due <= today,
        _ => true,
    }
}

/// The card's mastery bucket (coverage-map shading).
pub fn mastery(card: &Card) -> Mastery {
    if card.reviews.is_empty() {
        Mastery::New
    } else if card.reps == 0 || card.interval_days < 1 {
        Mastery::Learning // brand-new-but-touched, or relearning after a lapse
    } else if card.interval_days < MATURE_DAYS {
        Mastery::Young
    } else {
        Mastery::Mature
    }
}

// ── storage (one file per verse, under home/memory) ──────────────────────────

fn memory_dir(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref().join("memory")
}

/// The file a card for `vref` lives in (`home/memory/<slug>.json`).
pub fn card_file(home: impl AsRef<Path>, vref: &VRef) -> PathBuf {
    memory_dir(home).join(format!("{}.json", crate::store::slug(&vref.ref_key(), "card")))
}

/// Load every `home/memory/*.json` into a map by verse. Returns the cards plus
/// any per-file parse errors (a bad file never sinks the rest).
pub fn load_cards(home: impl AsRef<Path>) -> (HashMap<VRef, Card>, Vec<String>) {
    let dir = memory_dir(&home);
    let mut cards = HashMap::new();
    let mut errors = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return (cards, errors),
    };
    for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")) {
        match std::fs::read(&path) {
            Err(e) => errors.push(format!("{}: {e}", path.display())),
            Ok(bytes) => match parse_card(&path, &bytes) {
                Ok(card) => {
                    cards.insert(card.verse.clone(), card);
                }
                Err(msg) => errors.push(msg),
            },
        }
    }
    (cards, errors)
}

/// Parse one card file's `bytes`; `Err` is the message [`load_cards`] reports.
/// Shared with [`write_card`]'s refuse-to-clobber check so the reader and the
/// writer can never disagree about which files we understand.
fn parse_card(path: &Path, bytes: &[u8]) -> Result<Card, String> {
    let r: CardRepr = serde_json::from_slice(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if r.format != FORMAT {
        return Err(format!("{}: unknown memory format {}", path.display(), r.format));
    }
    let Some(vref) = VRef::parse_ref_key(&r.ref_key) else {
        return Err(format!("{}: bad ref {}", path.display(), r.ref_key));
    };
    // An unparseable or nonsensical `through` degrades to a single-verse card
    // rather than sinking the file.
    let through = r
        .through
        .as_deref()
        .and_then(VRef::parse_ref_key)
        .filter(|t| t.book == vref.book && t.chapter == vref.chapter && t.verse > vref.verse);
    Ok(Card {
        verse: vref,
        through,
        tok_version: r.tokenization,
        created: r.created,
        ease: r.ease,
        interval_days: r.interval_days,
        reps: r.reps,
        lapses: r.lapses,
        due: r.due,
        reviews: r.reviews,
        extra: r.extra,
    })
}

/// Serialize a card to pretty JSON with a trailing newline.
pub fn to_json(card: &Card) -> Result<String, Error> {
    serde_json::to_string_pretty(&card.to_repr()).map(|s| s + "\n").map_err(|e| Error::Parse(e.to_string()))
}

/// Atomically write a card to its file under `home`.
///
/// Refuses when that file is already there and we could not read it — corrupt,
/// or stamped by a build newer than this one. Nothing could have loaded such a
/// file, so every write over one is a clobber of the reader's data in a form we
/// do not understand yet; the same refuse-to-clobber rule as
/// [`crate::thread::add_to_thread`], sited at the writer because grading is not
/// the only way in (the shells also seed cards straight through here). A missing
/// or empty file holds nothing to lose and writes as normal.
pub fn write_card(home: impl AsRef<Path>, card: &Card) -> Result<(), Error> {
    let path = card_file(&home, &card.verse);
    if let Ok(bytes) = std::fs::read(&path) {
        let empty = bytes.iter().all(|b| b.is_ascii_whitespace());
        if !empty && parse_card(&path, &bytes).is_err() {
            return Err(Error::Corpus(format!(
                "{} exists but could not be read — refusing to overwrite",
                path.display()
            )));
        }
    }
    crate::store::write_atomic(path, &to_json(card)?)
}

/// Grade the verse at `now`, creating the card on first review; persists and
/// returns the updated card. `loaded` is the current card set (from
/// [`load_cards`]); a caller reloads after.
///
/// A card file that exists but is absent from `loaded` (i.e. it failed to parse)
/// is refused rather than clobbered — see [`write_card`].
pub fn grade_verse(
    home: impl AsRef<Path>,
    loaded: &HashMap<VRef, Card>,
    vref: &VRef,
    tok_version: &str,
    grade: Grade,
    now: &str,
) -> Result<Card, Error> {
    let mut card = loaded.get(vref).cloned().unwrap_or_else(|| Card::new(vref.clone(), tok_version, now));
    review(&mut card, grade, now);
    write_card(&home, &card)?;
    Ok(card)
}

/// Remove a verse's card (stop memorizing it).
pub fn remove_card(home: impl AsRef<Path>, vref: &VRef) -> Result<(), Error> {
    let path = card_file(&home, vref);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::Corpus(format!("{}: {e}", path.display()))),
    }
}

/// The verses due for review at `now`, in reading order — the study queue.
pub fn due_queue(cards: &HashMap<VRef, Card>, now: &str) -> Vec<VRef> {
    let mut due: Vec<&VRef> = cards.keys().filter(|v| is_due(&cards[*v], now)).collect();
    due.sort_by_key(|v| v.reading_key());
    due.into_iter().cloned().collect()
}

// ── drills (pure text over a verse's words) ──────────────────────────────────

/// First-letter prompt: each word reduced to its first letter (leading/trailing
/// punctuation kept, the rest dropped) — the classic memory scaffold.
/// `"For God so loved the world,"` → `"F G s l t w,"`.
pub fn first_letters(text: &str) -> String {
    text.split_whitespace().map(first_letter_hint).collect::<Vec<_>>().join(" ")
}

fn first_letter_hint(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    let lead: String = chars.iter().take_while(|c| !c.is_alphanumeric()).collect();
    let first = chars.iter().find(|c| c.is_alphanumeric()).copied();
    let trail: String =
        chars.iter().rev().take_while(|c| !c.is_alphanumeric()).collect::<Vec<_>>().into_iter().rev().collect();
    match first {
        Some(c) => format!("{lead}{c}{trail}"),
        None => word.to_string(), // punctuation-only token
    }
}

/// Progressive blank-out: mask a share of the words (their letters → `_`,
/// punctuation kept), rising with `level` (0 = full text … [`MAX_BLANK_LEVEL`] =
/// every word masked). Which words hide is deterministic and spread out, so a
/// given level always yields the same prompt.
pub fn blank_out(text: &str, level: u8) -> String {
    let f = (level.min(MAX_BLANK_LEVEL) as f64) / (MAX_BLANK_LEVEL as f64);
    text.split_whitespace()
        .enumerate()
        .map(|(i, w)| if hide_fraction(i) < f { mask_word(w) } else { w.to_string() })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A stable per-index value in [0,1) that spreads hidden words evenly-ish.
fn hide_fraction(i: usize) -> f64 {
    let h = (i as u64).wrapping_mul(2_654_435_761) ^ 0x9e37_79b9;
    ((h >> 11) & 0xffff) as f64 / 65535.0
}

fn mask_word(w: &str) -> String {
    w.chars().map(|c| if c.is_alphanumeric() { '_' } else { c }).collect()
}

/// The result of scoring a typed recall against the verse.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RecallScore {
    /// Fraction of the verse's words recalled correctly (0.0–1.0).
    pub accuracy: f32,
    /// One entry per expected word (original casing), and whether it was hit.
    pub words: Vec<WordHit>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WordHit {
    pub word: String,
    pub ok: bool,
}

/// Score `typed` against the verse `actual`, tolerant of casing, punctuation,
/// and skipped/extra words (a longest-common-subsequence match, so one missing
/// word doesn't misalign the rest). Marks each expected word hit or missed.
pub fn score_recall(typed: &str, actual: &str) -> RecallScore {
    let expected: Vec<&str> = actual.split_whitespace().collect();
    let exp_norm: Vec<String> = expected.iter().map(|w| normalize(w)).collect();
    let got_norm: Vec<String> = typed.split_whitespace().map(normalize).collect();
    let hits = lcs_hits(&exp_norm, &got_norm);
    let correct = hits.iter().filter(|&&h| h).count();
    let accuracy = if exp_norm.is_empty() { 0.0 } else { correct as f32 / exp_norm.len() as f32 };
    let words = expected.iter().zip(hits.iter()).map(|(w, &ok)| WordHit { word: (*w).to_string(), ok }).collect();
    RecallScore { accuracy, words }
}

fn normalize(w: &str) -> String {
    w.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// LCS over the two token lists; returns, per expected token, whether it is part
/// of the longest common subsequence with the typed tokens (in order).
fn lcs_hits(expected: &[String], got: &[String]) -> Vec<bool> {
    let (n, m) = (expected.len(), got.len());
    let mut dp = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if expected[i] == got[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }
    let mut hits = vec![false; n];
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if expected[i] == got[j] {
            hits[i] = true;
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    hits
}

// ── aggregations: the coverage map + the activity heatmap ────────────────────

/// One verse's standing, for the canon-strip coverage map.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VerseCoverage {
    #[serde(rename = "ref")]
    pub ref_key: String,
    pub mastery: Mastery,
    pub reps: u32,
    pub lapses: u32,
    /// UTC timestamp of the most recent review, if any (drives recency shading).
    #[serde(rename = "lastAt", skip_serializing_if = "Option::is_none")]
    pub last_at: Option<String>,
    pub due: bool,
}

/// Per-verse coverage across every card — the spatial map "shaded by verses
/// memorized and review depth/recency", in reading order.
/// A passage card shades every verse it covers — the map answers "have I
/// memorized this verse", and a verse learned inside a chunk has been.
pub fn coverage(cards: &HashMap<VRef, Card>, now: &str) -> Vec<VerseCoverage> {
    let mut out: Vec<VerseCoverage> = cards
        .values()
        .flat_map(|c| {
            let (m, due) = (mastery(c), is_due(c, now));
            c.verses().into_iter().map(move |v| VerseCoverage {
                ref_key: v.ref_key(),
                mastery: m,
                reps: c.reps,
                lapses: c.lapses,
                last_at: c.reviews.last().map(|r| r.at.clone()),
                due,
            })
        })
        .collect();
    out.sort_by_key(|v| VRef::parse_ref_key(&v.ref_key).map(|r| r.reading_key()));
    out
}

/// One card as the memorize hub lists it — the row a reader drills or removes.
/// Distinct from [`VerseCoverage`], which is per-verse shading: a passage card
/// is ONE row here and every verse it covers there.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CardSummary {
    /// The card's key — its first verse. Every card endpoint takes this.
    #[serde(rename = "ref")]
    pub ref_key: String,
    /// What to show the reader: `"Ps 23:1–6"` or `"Gen 1:7"`.
    pub label: String,
    /// How many verses this card covers (1 unless it's a passage).
    pub verses: u32,
    pub mastery: Mastery,
    pub reps: u32,
    pub lapses: u32,
    pub due: bool,
}

/// Every card, one entry each, in reading order — the memorize hub's list.
pub fn card_list(cards: &HashMap<VRef, Card>, now: &str) -> Vec<CardSummary> {
    let mut out: Vec<(_, CardSummary)> = cards
        .values()
        .map(|c| {
            (
                c.verse.reading_key(),
                CardSummary {
                    ref_key: c.verse.ref_key(),
                    label: c.label(),
                    verses: c.verses().len() as u32,
                    mastery: mastery(c),
                    reps: c.reps,
                    lapses: c.lapses,
                    due: is_due(c, now),
                },
            )
        })
        .collect();
    out.sort_by_key(|a| a.0);
    out.into_iter().map(|(_, s)| s).collect()
}

/// Reviews on one calendar day — the temporal activity heatmap.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DayActivity {
    /// `YYYY-MM-DD` (UTC).
    pub day: String,
    pub reviews: u32,
}

/// Review counts bucketed by calendar day, oldest first — the "how much activity
/// when" heatmap. Built from every card's review log.
pub fn activity_by_day(cards: &HashMap<VRef, Card>) -> Vec<DayActivity> {
    let mut by_day: HashMap<String, u32> = HashMap::new();
    for c in cards.values() {
        for r in &c.reviews {
            if let Some(day) = r.at.get(0..10) {
                *by_day.entry(day.to_string()).or_insert(0) += 1;
            }
        }
    }
    let mut out: Vec<DayActivity> = by_day.into_iter().map(|(day, reviews)| DayActivity { day, reviews }).collect();
    out.sort_by(|a, b| a.day.cmp(&b.day));
    out
}

/// Coverage rolled up to the 8 canon sections (Law … Revelation) — a compact
/// spatial summary beside the per-verse map.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SectionCoverage {
    pub label: &'static str,
    /// Verses being memorized in this section — a passage card counts every
    /// verse it covers, so this stays "how much of the section do I know".
    pub cards: u32,
    /// Of those verses, the ones whose card has reached `Mature`.
    pub mature: u32,
    /// Total reviews logged against this section.
    pub reviews: u32,
}

/// Roll coverage up to [`crate::reference::CANON_SEGMENTS`].
pub fn coverage_by_section(cards: &HashMap<VRef, Card>) -> Vec<SectionCoverage> {
    use crate::reference::CANON_SEGMENTS;
    let mut acc: Vec<SectionCoverage> =
        CANON_SEGMENTS.iter().map(|(label, _, _)| SectionCoverage { label, cards: 0, mature: 0, reviews: 0 }).collect();
    for c in cards.values() {
        let Some(bi) = crate::canon::book_order(&c.verse.book) else { continue };
        if let Some(si) = CANON_SEGMENTS.iter().position(|(_, lo, hi)| bi >= *lo && bi <= *hi) {
            let verses = c.verses().len() as u32;
            acc[si].cards += verses;
            acc[si].reviews += c.reviews.len() as u32;
            if mastery(c) == Mastery::Mature {
                acc[si].mature += verses;
            }
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: &str = "2026-01-01T00:00:00Z";

    #[test]
    fn sm2_interval_progression_on_good() {
        let mut c = Card::new(VRef::new("John", 3, 16), "kjv1769-tok2", T0);
        review(&mut c, Grade::Good, T0); // rep 1 → 1 day
        assert_eq!((c.reps, c.interval_days), (1, 1));
        review(&mut c, Grade::Good, "2026-01-02T00:00:00Z"); // rep 2 → 6 days
        assert_eq!((c.reps, c.interval_days), (2, 6));
        review(&mut c, Grade::Good, "2026-01-08T00:00:00Z"); // rep 3 → 6*ease
        assert!(c.interval_days >= 14, "interval was {}", c.interval_days);
        assert_eq!(c.reviews.len(), 3);
        assert_eq!(c.due, add_days("2026-01-08T00:00:00Z", c.interval_days as i64));
    }

    #[test]
    fn again_lapses_and_resets() {
        let mut c = Card::new(VRef::new("John", 3, 16), "kjv1769-tok2", T0);
        review(&mut c, Grade::Good, T0);
        review(&mut c, Grade::Good, "2026-01-02T00:00:00Z");
        let ease_before = c.ease;
        review(&mut c, Grade::Again, "2026-01-08T00:00:00Z");
        assert_eq!((c.reps, c.interval_days, c.lapses), (0, 1, 1));
        assert!(c.ease < ease_before && c.ease >= EASE_FLOOR);
        assert_eq!(mastery(&c), Mastery::Learning);
    }

    #[test]
    fn mastery_buckets_and_due() {
        let mut c = Card::new(VRef::new("Ps", 23, 1), "kjv1769-tok2", T0);
        assert_eq!(mastery(&c), Mastery::New);
        assert!(is_due(&c, T0)); // new cards are due now
                                 // Grind Good until mature (interval crosses 21 days).
        let mut day = 0i64;
        for _ in 0..6 {
            let now = add_days(T0, day) + "T00:00:00Z";
            review(&mut c, Grade::Good, &now);
            day += c.interval_days as i64;
        }
        assert!(c.interval_days >= MATURE_DAYS, "interval {}", c.interval_days);
        assert_eq!(mastery(&c), Mastery::Mature);
        // Due exactly on its scheduled day, but not the day before (scheduled ahead).
        assert!(is_due(&c, &(add_days(T0, day) + "T00:00:00Z")));
        assert!(!is_due(&c, &(add_days(T0, day - 1) + "T00:00:00Z")));
    }

    #[test]
    fn first_letters_prompt() {
        assert_eq!(first_letters("For God so loved the world,"), "F G s l t w,");
        assert_eq!(first_letters("(the LORD)"), "(t L)");
    }

    #[test]
    fn blank_out_progressive() {
        let v = "For God so loved the world";
        assert_eq!(blank_out(v, 0), v); // nothing hidden at level 0
        assert_eq!(blank_out(v, MAX_BLANK_LEVEL).replace(['_', ' '], ""), ""); // all letters gone
        let mid = blank_out(v, 2);
        assert!(mid.contains('_') && mid.split_whitespace().any(|w| !w.contains('_')));
    }

    #[test]
    fn recall_scoring_is_alignment_tolerant() {
        let actual = "For God so loved the world";
        assert_eq!(score_recall("for god so loved the world", actual).accuracy, 1.0);
        // A skipped word: the rest still align via LCS, not a cascade of misses.
        let s = score_recall("For God loved the world", actual);
        assert_eq!(s.words.iter().filter(|w| w.ok).count(), 5); // all but "so"
        assert!(!s.words[2].ok && s.words[2].word == "so");
        assert!((s.accuracy - 5.0 / 6.0).abs() < 1e-6);
    }

    #[test]
    fn store_roundtrip_and_queue() {
        let home = std::env::temp_dir().join(format!("plumbline-mem-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let (loaded, _) = load_cards(&home);
        assert!(loaded.is_empty());

        let jn = VRef::new("John", 3, 16);
        grade_verse(&home, &loaded, &jn, "kjv1769-tok2", Grade::Good, T0).unwrap();
        let (loaded, errs) = load_cards(&home);
        assert!(errs.is_empty());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[&jn].reps, 1);

        // Due yesterday-scheduled card shows in the queue; a far-future one doesn't.
        let ps = VRef::new("Ps", 119, 11);
        grade_verse(&home, &loaded, &ps, "kjv1769-tok2", Grade::Easy, T0).unwrap();
        let (loaded, _) = load_cards(&home);
        let q = due_queue(&loaded, "2026-01-02T00:00:00Z");
        assert!(q.contains(&jn)); // John 3:16 was due after 1 day
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn coverage_and_activity() {
        let mut cards = HashMap::new();
        let mut c = Card::new(VRef::new("John", 3, 16), "kjv1769-tok2", T0);
        review(&mut c, Grade::Good, T0);
        review(&mut c, Grade::Good, "2026-01-03T00:00:00Z");
        cards.insert(c.verse.clone(), c);

        let cov = coverage(&cards, "2026-01-10T00:00:00Z");
        assert_eq!(cov.len(), 1);
        assert_eq!(cov[0].ref_key, "John 3:16");
        assert_eq!(cov[0].reps, 2);

        let act = activity_by_day(&cards);
        assert_eq!(
            act,
            vec![
                DayActivity { day: "2026-01-01".into(), reviews: 1 },
                DayActivity { day: "2026-01-03".into(), reviews: 1 },
            ]
        );

        let sec = coverage_by_section(&cards);
        let gospels = sec.iter().find(|s| s.label == "Gospels").unwrap();
        assert_eq!((gospels.cards, gospels.reviews), (1, 2));
    }

    #[test]
    fn passage_card_spans_verses_and_normalizes_its_end() {
        let ps1 = VRef::new("Ps", 23, 1);
        let card = Card::new_passage(ps1.clone(), &VRef::new("Ps", 23, 6), "kjv1769-tok2", T0);
        assert_eq!(card.label(), "Ps 23:1\u{2013}6");
        assert_eq!(card.verses().len(), 6);
        assert_eq!(card.verses().first().unwrap().ref_key(), "Ps 23:1");
        assert_eq!(card.verses().last().unwrap().ref_key(), "Ps 23:6");

        // A single verse keeps the plain refKey and one covered verse.
        let one = Card::new(ps1.clone(), "kjv1769-tok2", T0);
        assert_eq!(one.label(), "Ps 23:1");
        assert_eq!(one.verses(), vec![ps1.clone()]);

        // Nonsense ends collapse to a single-verse card rather than spanning
        // backwards, across chapters, or across books.
        for bad in [VRef::new("Ps", 23, 1), VRef::new("Ps", 24, 2), VRef::new("Prov", 23, 6)] {
            let c = Card::new_passage(ps1.clone(), &bad, "kjv1769-tok2", T0);
            assert_eq!(c.through, None, "{} should not span", bad.ref_key());
            assert_eq!(c.verses(), vec![ps1.clone()]);
        }
    }

    #[test]
    fn passage_card_round_trips_and_shades_every_verse_it_covers() {
        let home = std::env::temp_dir().join(format!("plumbline-passage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let start = VRef::new("Ps", 23, 1);
        let card = Card::new_passage(start.clone(), &VRef::new("Ps", 23, 4), "kjv1769-tok2", T0);
        write_card(&home, &card).unwrap();

        // The span survives the on-disk format (additive `through`).
        let (loaded, errs) = load_cards(&home);
        assert!(errs.is_empty());
        assert_eq!(loaded.len(), 1, "one card, filed under its first verse");
        assert_eq!(loaded[&start].through, Some(VRef::new("Ps", 23, 4)));
        assert!(to_json(&card).unwrap().contains("\"through\": \"Ps 23:4\""));

        // A single-verse card writes no `through` at all — old readers see the
        // file they always saw.
        assert!(!to_json(&Card::new(start.clone(), "kjv1769-tok2", T0)).unwrap().contains("through"));

        // Grading the passage keeps it a passage.
        grade_verse(&home, &loaded, &start, "kjv1769-tok2", Grade::Good, T0).unwrap();
        let (loaded, _) = load_cards(&home);
        assert_eq!(loaded[&start].through, Some(VRef::new("Ps", 23, 4)));
        assert_eq!(loaded[&start].reps, 1);

        // The coverage map shades all four verses; the queue still holds ONE card.
        let cov = coverage(&loaded, "2026-01-10T00:00:00Z");
        assert_eq!(
            cov.iter().map(|v| v.ref_key.as_str()).collect::<Vec<_>>(),
            ["Ps 23:1", "Ps 23:2", "Ps 23:3", "Ps 23:4"]
        );
        assert_eq!(due_queue(&loaded, "2026-01-10T00:00:00Z"), vec![start.clone()]);
        let sec = coverage_by_section(&loaded);
        let writings = sec.iter().find(|s| s.cards > 0).unwrap();
        assert_eq!((writings.cards, writings.reviews), (4, 1), "4 verses, one review");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_through_the_reader_cannot_use_degrades_to_one_verse() {
        // Hand-written/older files: `through` pointing outside the card's own
        // chapter must not produce a card that claims verses it cannot render.
        let home = std::env::temp_dir().join(format!("plumbline-badspan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("memory")).unwrap();
        std::fs::write(
            home.join("memory").join("ps-23-1.json"),
            r#"{"format":"overlay-memory-v1","ref":"Ps 23:1","through":"Rev 22:21",
                "tokenization":"kjv1769-tok2","created":"2026-01-01T00:00:00Z",
                "ease":2.5,"intervalDays":0,"reps":0,"lapses":0,"due":"2026-01-01"}"#,
        )
        .unwrap();
        let (loaded, errs) = load_cards(&home);
        assert!(errs.is_empty());
        let c = &loaded[&VRef::new("Ps", 23, 1)];
        assert_eq!(c.through, None);
        assert_eq!(c.verses().len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_card_file_we_cannot_read_is_never_overwritten() {
        let home = std::env::temp_dir().join(format!("plumbline-noclobber-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let jn = VRef::new("John", 3, 16);
        let path = card_file(&home, &jn);

        // Corrupt bytes, and a card stamped by a build newer than this one. Both
        // are the reader's work in a form we cannot use; neither is "no card yet",
        // and a card carries its whole review log, so a clobber loses months.
        let future = r#"{"format":"overlay-memory-v9","ref":"John 3:16","tokenization":"kjv1769-tok2","created":"2026-01-01T00:00:00Z","ease":2.5,"intervalDays":40,"reps":9,"lapses":0,"due":"2026-02-10","reviews":[]}"#;
        for content in ["{ not json".to_string(), future.to_string()] {
            crate::store::write_atomic(&path, &content).unwrap();
            let (cards, errs) = load_cards(&home);
            assert!(!cards.contains_key(&jn), "a file we cannot read must not become a card");
            assert_eq!(errs.len(), 1, "it is reported, not read as absent: {errs:?}");

            for e in [
                grade_verse(&home, &cards, &jn, "kjv1769-tok2", Grade::Good, T0).err(),
                write_card(&home, &Card::new(jn.clone(), "kjv1769-tok2", T0)).err(),
            ] {
                let msg = e.map(|e| e.to_string()).unwrap_or_default();
                assert!(msg.contains("refusing to overwrite"), "want a refusal, got {msg:?}");
            }
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                content,
                "the bytes on disk must be exactly as the reader left them",
            );
        }

        // No file is genuinely no card, and a card we CAN read still updates in
        // place — the guard must not stand between a reader and their own reviews.
        std::fs::remove_file(&path).unwrap();
        let (cards, _) = load_cards(&home);
        grade_verse(&home, &cards, &jn, "kjv1769-tok2", Grade::Good, T0).unwrap();
        let (cards, errs) = load_cards(&home);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(cards[&jn].reps, 1);
        grade_verse(&home, &cards, &jn, "kjv1769-tok2", Grade::Good, "2026-01-02T00:00:00Z").unwrap();
        assert_eq!(load_cards(&home).0[&jn].reps, 2, "a readable card is still graded");

        // An empty file holds nothing to lose, so it does not block a write.
        crate::store::write_atomic(&path, "").unwrap();
        write_card(&home, &Card::new(jn.clone(), "kjv1769-tok2", T0)).unwrap();
        assert_eq!(load_cards(&home).0[&jn].reps, 0);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// AUDIT 2026-07-29 forward compatibility: the on-disk formats evolve
    /// **additively** (CLAUDE.md §Data formats), and a sideloaded APK never
    /// auto-updates — so a key this build drops is dropped for good on that
    /// device. A card written by a later build has to come back out whole, review
    /// log and all: every grade rewrites the whole file.
    #[test]
    fn a_card_keeps_the_keys_of_a_later_build() {
        let home = std::env::temp_dir().join(format!("plumbline-mem-forward-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let jn = VRef::new("John", 3, 16);
        let path = card_file(&home, &jn);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"format":"overlay-memory-v1","ref":"John 3:16","tokenization":"kjv1769-tok2",
                "created":"2026-01-01T00:00:00Z","ease":2.5,"intervalDays":1,"reps":1,
                "lapses":0,"due":"2026-01-02",
                "reviews":[{"at":"2026-01-01T00:00:00Z","grade":"good","intervalDays":1,
                            "seconds":12,"device":{"kind":"phone"},"hints":["first letters"]}],
                "deck":"AWANA","fsrs":{"stability":4.2},"decks":["AWANA","family"]}"#,
        )
        .unwrap();

        // Grading appends a review and rewrites the file.
        let (cards, errs) = load_cards(&home);
        assert!(errs.is_empty(), "{errs:?}");
        grade_verse(&home, &cards, &jn, "kjv1769-tok2", Grade::Good, "2026-01-02T00:00:00Z").unwrap();

        let back: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["reps"], 2, "the grade itself must land");
        assert_eq!(back["reviews"].as_array().unwrap().len(), 2);
        assert_eq!(back["deck"], "AWANA", "an unknown scalar was stripped");
        assert_eq!(back["fsrs"], serde_json::json!({"stability":4.2}), "an unknown object was stripped");
        assert_eq!(back["decks"], serde_json::json!(["AWANA", "family"]), "an unknown array was stripped");
        assert_eq!(back["reviews"][0]["seconds"], 12, "a review's unknown scalar was stripped");
        assert_eq!(
            back["reviews"][0]["device"],
            serde_json::json!({"kind":"phone"}),
            "a review's unknown object was stripped"
        );
        assert_eq!(
            back["reviews"][0]["hints"],
            serde_json::json!(["first letters"]),
            "a review's unknown array was stripped"
        );
        // The review this build logged carries nothing of its own.
        assert!(back["reviews"][1].get("seconds").is_none());

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A card with nothing unknown in it is written byte for byte as it was before
    /// any of that landed — these files already ship inside backup zips.
    #[test]
    fn a_card_with_no_unknown_keys_is_written_exactly_as_before() {
        let card = Card::new(VRef::new("John", 3, 16), "kjv1769-tok2", T0);
        assert_eq!(
            to_json(&card).unwrap(),
            r#"{
  "format": "overlay-memory-v1",
  "ref": "John 3:16",
  "tokenization": "kjv1769-tok2",
  "created": "2026-01-01T00:00:00Z",
  "ease": 2.5,
  "intervalDays": 0,
  "reps": 0,
  "lapses": 0,
  "due": "2026-01-01",
  "reviews": []
}
"#
        );
    }

    #[test]
    fn date_math_handles_month_and_leap_boundaries() {
        assert_eq!(add_days("2026-01-31T00:00:00Z", 1), "2026-02-01");
        assert_eq!(add_days("2026-12-31T12:00:00Z", 1), "2027-01-01");
        assert_eq!(add_days("2024-02-28T00:00:00Z", 1), "2024-02-29"); // 2024 is leap
        assert_eq!(add_days("2026-02-28T00:00:00Z", 1), "2026-03-01"); // 2026 is not
    }
}
