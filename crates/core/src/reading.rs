//! Where you've read, and how long ago — the reading map behind the navigator's
//! glow (product request, 2026-07-28).
//!
//! The point of the feature is **attention**: the book and chapter grids tint
//! themselves so the parts of the canon you have drifted away from stand out
//! from the parts you were in last week. It is not a completion tracker and
//! deliberately keeps no leaderboard.
//!
//! ## What gets measured
//!
//! Coverage of a chapter is a **percentage**, and it is gated two ways at once:
//!
//! ```text
//! covered_words = min( words above the furthest point you reached,
//!                      dwell_seconds × READING_WORDS_PER_MINUTE / 60 )
//! pct           = covered_words / chapter words
//! ```
//!
//! Scrolling to the bottom instantly credits nothing (no time has passed);
//! sitting on verse 1 for an hour credits only verse 1 (you never went further).
//! Only doing both — moving through the chapter at something like a human
//! reading speed — fills it. The dwell side is deliberately **aggregate rather
//! than per-verse**: time spent lingering over verse 3 pays for verse 30 once
//! you get there. That is the generous reading, and generous is the brief.
//!
//! Generous in one more place: a pass completes at [`COMPLETE_AT`] (90%), not
//! 100%, and **snaps** to a full read. Nobody should be hunting a trailing verse
//! to make a glow go away.
//!
//! ## What gets stored
//!
//! Two numbers and two dates per chapter — [`ChapterReading`]. `reached` and
//! `dwell` describe the pass **currently under way** and are reset when it
//! completes; `last_read` is the last COMPLETED pass and `touched` the last
//! contact of any kind. Partial dwell *within* a verse is not persisted at all:
//! the shell holds it for the session and it is no loss if it evaporates.
//!
//! One file per book under `home/reading/`, plus `_since.json` holding the date
//! the reader started (see [`ensure_since`]).
//!
//! ## The glow, and what silences it
//!
//! The glow does not mean one thing. For a chapter you have READ it means *you
//! have been away a while*. For one you have NEVER read it means *there is
//! something here you have not seen*, and it is full from the first launch
//! (revised 2026-07-29: it used to ramp from the reader's start date, which made
//! the map calm on precisely the day a reader most wants showing where to go, and
//! dressed "you have never opened this" up as "not due yet"). A part-read chapter
//! glows in proportion to what is LEFT.
//!
//! Over all of that sits one rule: **recency outranks coverage.** The glow ramps
//! from the most recent CONTACT — `touched` or `last_read`, whichever is later —
//! and that ramp is flat zero for [`FRESH_DAYS`], reaching full at
//! [`STALE_DAYS`]. So a chapter you were in this morning says nothing, whether you
//! finished it or stopped halfway, and a chapter you finished last year but dipped
//! into today says nothing either. Without this rule, reading a chapter and not
//! quite crossing the 90% bar left it glowing at you the moment you closed it,
//! which is a map arguing with the person holding it.
//!
//! Personal study data, so it rides in the backup zip like `memory/` and
//! `notes/` do.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::canon;
use crate::civil::{date_to_days, days_between, days_to_date};
use crate::corpus::Corpus;
use crate::Error;

/// The on-disk stamp. A new format, so it is named for the product rather than
/// inheriting the `overlay-` prefix its siblings are frozen into.
pub const FORMAT: &str = "plumbline-reading-v1";

/// The reading speed dwell is converted at. Set **generously**, and raised from
/// 220 on 2026-07-29 after a real read of Jude came out `Partial`: at 220 its 613
/// words wanted 2.8 minutes of credited dwell, which a brisk reader beats, so
/// "I just read this" showed as "you are partway through".
///
/// The dwell gate is not a pace to hold a reader to — its ONLY job is to refuse
/// credit to someone flipping through, and [`GRACE_SECONDS`] plus the high-water
/// mark already do that work: a flip banks no seconds at all. So this can afford
/// to be fast, and being fast is the difference between a map that agrees with the
/// reader and one that argues with them.
pub const READING_WORDS_PER_MINUTE: f32 = 300.0;

/// Coverage at or above which a pass counts as a full read and snaps to 1.0.
pub const COMPLETE_AT: f32 = 0.90;

/// Days after a read during which there is no glow at all — recently read is
/// recently read, and the map should be quiet about it.
pub const FRESH_DAYS: i64 = 30;

/// Days after which the glow is at full: a year, so "fully due" lines up with
/// the cadence of any read-the-Bible-in-a-year plan.
pub const STALE_DAYS: i64 = 365;

/// Seconds a chapter must be on screen before dwell starts accruing at all.
/// This — not the reading rate — is what makes flipping through free.
pub const GRACE_SECONDS: f32 = 3.0;

/// The cadence a shell should report at. Every call writes a file, so this is a
/// deliberate compromise between losing a session's tail and churning the disk
/// (and, on the web, IndexedDB) every few seconds.
pub const TICK_SECONDS: f32 = 30.0;

/// No scroll, tap or keypress for this long and accrual stops — a chapter left
/// open on a table overnight is not reading. It resumes on the next interaction.
pub const IDLE_SECONDS: f32 = 120.0;

/// The tuning both shells read rather than each hard-coding. Handed over the ABI
/// so the phone and the browser cannot drift on what "read" means.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Spec {
    #[serde(rename = "wordsPerMinute")]
    pub words_per_minute: f32,
    #[serde(rename = "completeAt")]
    pub complete_at: f32,
    #[serde(rename = "freshDays")]
    pub fresh_days: i64,
    #[serde(rename = "staleDays")]
    pub stale_days: i64,
    #[serde(rename = "graceSeconds")]
    pub grace_seconds: f32,
    #[serde(rename = "tickSeconds")]
    pub tick_seconds: f32,
    #[serde(rename = "idleSeconds")]
    pub idle_seconds: f32,
}

/// The constants above, as one value for the wire.
pub fn spec() -> Spec {
    Spec {
        words_per_minute: READING_WORDS_PER_MINUTE,
        complete_at: COMPLETE_AT,
        fresh_days: FRESH_DAYS,
        stale_days: STALE_DAYS,
        grace_seconds: GRACE_SECONDS,
        tick_seconds: TICK_SECONDS,
        idle_seconds: IDLE_SECONDS,
    }
}

/// Where a chapter stands. Drives the **hue** in the navigator: unread gold
/// (unopened treasure), partial copper (under way), read sage (settled). The glow
/// rides on top of all three, but means different things — see the module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Standing {
    /// No credited reading at all.
    Unread,
    /// Some credited reading, never yet a full pass.
    Partial,
    /// Read through at least once (or logged by hand — see [`mark_read`]).
    Read,
}

// ── the stored record ────────────────────────────────────────────────────────

/// One chapter's reading state. `reached`/`dwell` belong to the pass under way
/// and go back to zero when it completes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChapterReading {
    #[serde(rename = "c")]
    pub chapter: u16,
    /// Furthest verse number whose text has been on screen in this pass; 0 for
    /// "nowhere yet". Monotonic within a pass — scrolling back up never
    /// surrenders ground already covered.
    #[serde(default)]
    pub reached: u16,
    /// Credited dwell seconds accumulated in this pass.
    #[serde(default)]
    pub dwell: f32,
    /// When the chapter was last read through, `YYYY-MM-DD`. `None` = never.
    #[serde(rename = "lastRead", skip_serializing_if = "Option::is_none", default)]
    pub last_read: Option<String>,
    /// When this chapter last had ANY of the reader's attention — a partial pass
    /// counts, a completed one counts, `None` means never. Additive (2026-07-29).
    ///
    /// It exists because recency has to be able to silence the glow on its own.
    /// Before it, only a COMPLETED chapter had an anchor, so a chapter you read
    /// most of an hour ago glowed at you like one you had never opened: "I just
    /// read the book of Jude and it now shows a bronze glow — bit of a false
    /// positive." Quite so. Being in a chapter recently is the whole thing the map
    /// is supposed to notice.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub touched: Option<String>,
    /// Every key on this record this build has never heard of, carried back out
    /// again on save.
    ///
    /// The on-disk formats evolve **additively** (CLAUDE.md §Data formats), and
    /// a sideloaded APK never auto-updates: a build that drops the fields of a
    /// later one drops them for good on that device. Every write path here reads
    /// the book's whole chapter list and writes it back, so without this a v1.0
    /// would strip a v1.1's per-chapter field from all 150 psalms the first time
    /// the reader opened one of them.
    ///
    /// Serde fills this with the leftovers after the known fields are matched, so
    /// a known key can never be swallowed, and a key a later version promotes to
    /// a real field stops arriving here the moment that field exists — it can
    /// never be written twice. Empty for every record on disk today, and an empty
    /// flattened map writes no key at all, so those files are written exactly as
    /// they were.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A book's file: `home/reading/<book>.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct BookFile {
    format: String,
    book: String,
    chapters: Vec<ChapterReading>,
    /// The file's own unknown keys. These ride on the *file* rather than on a
    /// loaded value, because a book's reading state is a bare `Vec` of chapters
    /// with no container to hang them on: [`write_book`] lifts them off the file
    /// it is replacing.
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// `home/reading/_since.json` — when this reader started. Underscored so it can
/// never be mistaken for a book file.
///
/// No unknown-key catch-all here, unlike its siblings: [`ensure_since`] writes
/// this file once and never again — a readable one is returned untouched — so
/// there is no save for a later version's field to be stripped by.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SinceFile {
    format: String,
    since: String,
}

/// Every book's chapters, keyed by OSIS book id. The whole reading store.
pub type Store = HashMap<String, Vec<ChapterReading>>;

fn reading_dir(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref().join("reading")
}

/// A book's file path. The stem is the OSIS id lowercased (`Gen` → `gen.json`),
/// which is already filename-safe for all 66 — no slugging needed.
fn book_file(home: impl AsRef<Path>, book: &str) -> PathBuf {
    reading_dir(home).join(format!("{}.json", book.to_lowercase()))
}

fn since_file(home: impl AsRef<Path>) -> PathBuf {
    reading_dir(home).join("_since.json")
}

/// Load the whole reading store. Unreadable or foreign-format files are skipped
/// with a message rather than sinking the load — the reading map is a nicety and
/// must never be able to stop the reader getting to the text.
pub fn load(home: impl AsRef<Path>) -> (Store, Vec<String>) {
    let mut store = Store::new();
    let mut errors = Vec::new();
    let dir = reading_dir(&home);
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return (store, errors), // no dir yet = nothing read yet
    };
    for path in entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('_')))
    {
        match std::fs::read(&path) {
            Err(e) => errors.push(format!("{}: {e}", path.display())),
            Ok(bytes) => match serde_json::from_slice::<BookFile>(&bytes) {
                Ok(f) if f.format == FORMAT => {
                    if canon::book_by_id(&f.book).is_some() {
                        store.insert(f.book, f.chapters);
                    } else {
                        errors.push(format!("{}: unknown book {}", path.display(), f.book));
                    }
                }
                Ok(f) => errors.push(format!("{}: unknown reading format {}", path.display(), f.format)),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            },
        }
    }
    (store, errors)
}

/// Load ONE book's chapters. What every write path uses: recording dwell runs on
/// a timer while someone reads, and reading all 66 files to touch one of them
/// would put the whole store on that timer for no reason.
///
/// A file that is THERE but that we cannot understand — corrupt, or stamped by a
/// build newer than this one — is an **error**, never an empty history. Every
/// caller writes the list it gets back out again, so answering "nothing read
/// yet" for a file we merely failed to parse would overwrite the reader's
/// history with a blank one. Same refuse-to-clobber rule as
/// [`crate::thread::add_to_thread`]: the reader's data outlives our ability to
/// read it. A missing file — or an empty one, which holds nothing to lose —
/// still means nothing read yet.
pub fn load_book(home: impl AsRef<Path>, book: &str) -> Result<Vec<ChapterReading>, Error> {
    let path = book_file(&home, book);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::Io { path: path.display().to_string(), source: e }),
    };
    if bytes.iter().all(|b| b.is_ascii_whitespace()) {
        return Ok(Vec::new());
    }
    match serde_json::from_slice::<BookFile>(&bytes) {
        Ok(f) if f.format == FORMAT && f.book == book => Ok(f.chapters),
        _ => Err(Error::Corpus(format!(
            "{} exists but could not be read — refusing to overwrite",
            path.display()
        ))),
    }
}

/// Atomically write one book's chapters. An empty list removes the file, so
/// clearing a book's history doesn't leave a husk behind.
pub fn write_book(home: impl AsRef<Path>, book: &str, chapters: &[ChapterReading]) -> Result<(), Error> {
    let path = book_file(&home, book);
    if chapters.is_empty() {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Io { path: path.display().to_string(), source: e }),
        };
    }
    let mut chapters = chapters.to_vec();
    chapters.sort_by_key(|c| c.chapter);
    // Whatever the file we are replacing carries at its top level and we do not
    // understand goes back out with it; the chapters carry their own (see
    // [`ChapterReading::extra`]). Bytes we cannot parse yield nothing — such a
    // file is refused by [`load_book`] before any caller gets here.
    let extra = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<BookFile>(&b).ok())
        .map(|f| f.extra)
        .unwrap_or_default();
    let f = BookFile { format: FORMAT.to_string(), book: book.to_string(), chapters, extra };
    let json = serde_json::to_string_pretty(&f).map_err(|e| Error::Parse(e.to_string()))?;
    crate::store::write_atomic(path, &(json + "\n"))
}

/// The date this reader started, creating it at `now` on first call.
///
/// No longer feeds the glow — unread chapters glow at once (2026-07-29) — but it
/// is kept: it is a true fact about this reader, it already ships inside v0.31.0
/// backup zips, and `since` is part of the wire payload. Deleting it would strip
/// a field from a contract that only evolves additively, to save one small file.
///
/// A file we cannot parse is refused here for the same reason [`load_book`]
/// refuses one: [`since`] reads it as `None`, and the anchor is written once and
/// never again, so stamping a fresh date over it is the one chance to lose it.
/// Callers already treat a failure as "no anchor yet" and carry on, so the
/// reading map keeps working either way — the file just survives.
pub fn ensure_since(home: impl AsRef<Path>, now: &str) -> Result<String, Error> {
    if let Some(s) = since(&home) {
        return Ok(s);
    }
    let path = since_file(&home);
    // Getting here means `since` could not use the file, so anything in it is
    // content we do not understand rather than an absence.
    if std::fs::read(&path).is_ok_and(|b| !b.iter().all(|b| b.is_ascii_whitespace())) {
        return Err(Error::Corpus(format!(
            "{} exists but could not be read — refusing to overwrite",
            path.display()
        )));
    }
    let date = day_of(now);
    let f = SinceFile { format: FORMAT.to_string(), since: date.clone() };
    let json = serde_json::to_string_pretty(&f).map_err(|e| Error::Parse(e.to_string()))?;
    crate::store::write_atomic(path, &(json + "\n"))?;
    Ok(date)
}

/// The stored start date, or `None` if [`ensure_since`] has never run.
pub fn since(home: impl AsRef<Path>) -> Option<String> {
    let bytes = std::fs::read(since_file(&home)).ok()?;
    let f: SinceFile = serde_json::from_slice(&bytes).ok()?;
    (f.format == FORMAT).then_some(f.since)
}

/// The `YYYY-MM-DD` day of an RFC3339 stamp (or of a bare date, unchanged).
fn day_of(stamp: &str) -> String {
    match date_to_days(stamp) {
        Some(d) => days_to_date(d),
        None => stamp.to_string(),
    }
}

// ── word counts ──────────────────────────────────────────────────────────────

/// Words per chapter for the whole canon — the denominator of every percentage,
/// and the weight that makes a book's coverage the sum of its chapters'.
///
/// Built once from the corpus (one pass, ~5 KB retained) because the navigator
/// asks for all 1189 chapters every time it opens, and re-walking 31,102 verses
/// per open is a cost with nothing to show for it.
#[derive(Clone, Debug, Default)]
pub struct ChapterWords {
    /// Book id → words in chapter 1, 2, … (index = chapter - 1).
    by_book: HashMap<String, Vec<u32>>,
}

impl ChapterWords {
    /// Tally every chapter in `corpus`.
    pub fn build(corpus: &Corpus) -> Self {
        let mut by_book: HashMap<String, Vec<u32>> = HashMap::new();
        for v in corpus.verses_iter() {
            let slot = by_book.entry(v.book.clone()).or_default();
            let idx = v.chapter as usize;
            if slot.len() < idx {
                slot.resize(idx, 0);
            }
            slot[idx - 1] += verse_words(v);
        }
        ChapterWords { by_book }
    }

    /// Words in one chapter; 0 if the chapter isn't in the corpus.
    pub fn words(&self, book: &str, chapter: u16) -> u32 {
        self.by_book
            .get(book)
            .and_then(|v| v.get(chapter.checked_sub(1)? as usize))
            .copied()
            .unwrap_or(0)
    }

    /// Chapter count for a book as the corpus has it.
    pub fn chapters(&self, book: &str) -> u16 {
        self.by_book.get(book).map_or(0, |v| v.len() as u16)
    }

    /// Total words in a book.
    pub fn book_words(&self, book: &str) -> u32 {
        self.by_book.get(book).map_or(0, |v| v.iter().sum())
    }
}

/// Words in a verse — tokens carrying an actual word. Punctuation-only tokens
/// don't count toward a reading time nobody spends on them.
fn verse_words(v: &crate::corpus::Verse) -> u32 {
    v.tokens.iter().filter(|t| !t.word.is_empty()).count() as u32
}

/// Cumulative words from the top of the chapter through each verse, as
/// `(verse, words_through_this_verse)` in verse order. What turns a high-water
/// verse number into "words above the point you reached".
pub fn cumulative_words(corpus: &Corpus, book: &str, chapter: u16) -> Vec<(u16, u32)> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    for v in corpus.chapter_verses(book, chapter) {
        acc += verse_words(v);
        out.push((v.verse, acc));
    }
    out
}

/// Words at or above verse `reached` in the chapter. `reached` beyond the last
/// verse counts the whole chapter, which is what a reader who scrolled off the
/// bottom has in fact seen.
fn words_through(corpus: &Corpus, book: &str, chapter: u16, reached: u16) -> u32 {
    if reached == 0 {
        return 0;
    }
    let cum = cumulative_words(corpus, book, chapter);
    let mut words = 0;
    for (verse, acc) in &cum {
        if *verse <= reached {
            words = *acc;
        }
    }
    // Past the end (or a verse number the corpus doesn't have): the whole thing.
    if cum.last().is_some_and(|(v, _)| reached >= *v) {
        words = cum.last().map_or(0, |(_, a)| *a);
    }
    words
}

/// The dwell needed to credit `words` at the reading rate, in seconds.
pub fn seconds_for_words(words: u32) -> f32 {
    words as f32 * 60.0 / READING_WORDS_PER_MINUTE
}

// ── the numbers a shell paints ───────────────────────────────────────────────

/// One chapter's (or book's) standing, as the navigator needs it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Heat {
    /// Coverage, 0.0–1.0. Snapped to 1.0 once a full pass has happened.
    pub pct: f32,
    pub standing: Standing,
    /// Attention, 0.0–1.0. Zero for [`FRESH_DAYS`] after ANY contact with the
    /// chapter, then ramping to full at [`STALE_DAYS`]; a chapter never opened is
    /// 1 from the start, and a part-read one tops out at what is left of it.
    pub glow: f32,
    /// Days since the last full read — `None` if it has never had one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<i64>,
    #[serde(rename = "lastRead", skip_serializing_if = "Option::is_none")]
    pub last_read: Option<String>,
}

/// A chapter row in the chapter grid.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChapterHeat {
    pub chapter: u16,
    pub words: u32,
    #[serde(flatten)]
    pub heat: Heat,
}

/// A book row in the book grid — the word-weighted roll-up of its chapters, so
/// the chapters visibly sum to the book.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BookHeat {
    pub book: String,
    pub name: &'static str,
    pub chapters: u16,
    pub words: u32,
    /// Chapters that have had a full read.
    pub read: u16,
    #[serde(flatten)]
    pub heat: Heat,
}

/// The glow for something last read `days` ago: flat zero through
/// [`FRESH_DAYS`], then linear to full at [`STALE_DAYS`].
pub fn glow_for_days(days: i64) -> f32 {
    if days <= FRESH_DAYS {
        return 0.0;
    }
    let span = (STALE_DAYS - FRESH_DAYS) as f32;
    (((days - FRESH_DAYS) as f32) / span).clamp(0.0, 1.0)
}

/// Coverage of the pass described by `r`, against a chapter of `words` words.
/// The min of "how far you got" and "how long you spent" — see the module docs.
fn pass_pct(corpus: &Corpus, book: &str, r: &ChapterReading, words: u32) -> f32 {
    if words == 0 {
        return 0.0;
    }
    let reached_words = words_through(corpus, book, r.chapter, r.reached);
    let dwell_words = (r.dwell * READING_WORDS_PER_MINUTE / 60.0).max(0.0);
    let covered = (reached_words as f32).min(dwell_words);
    (covered / words as f32).clamp(0.0, 1.0)
}

/// Turn one stored chapter record into paintable numbers.
fn heat_of(
    corpus: &Corpus,
    book: &str,
    r: Option<&ChapterReading>,
    words: u32,
    now: &str,
) -> Heat {
    let last_read = r.and_then(|r| r.last_read.clone());
    let touched = r.and_then(|r| r.touched.clone());

    // RECENCY OUTRANKS EVERYTHING. The most recent contact of any kind — a
    // completed pass, or just time spent in the chapter — anchors the ramp, and
    // inside FRESH_DAYS that ramp is flat zero. So a chapter you were in this
    // morning is silent whatever its coverage says, which is the only honest
    // answer: the map's question is "where have you not been lately", and you
    // were just there. (2026-07-29: reading Jude and being shown a bronze glow
    // for it was this rule missing.)
    let contact = [last_read.as_deref(), touched.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(date_to_days)
        .max();
    // UPGRADE AMNESTY. `touched` is additive, so a pass that was under way before
    // it existed has progress and no date — and would glow as if the reader had
    // never been there, which is the very complaint this rule answers. A record
    // with dwell banked and no contact date is read as contact NOW.
    //
    // Charitable rather than precise, and deliberately so: every such record was
    // written within a day of the field landing, so "now" is very nearly true for
    // all of them, and any that really were abandoned go quiet for a month and then
    // come back. The alternative — leaving them lit — makes the fix look like it
    // did not work on exactly the chapters that prompted it.
    let has_progress = r.is_some_and(|r| r.dwell > 0.0);
    let ramp = match contact {
        Some(d) => glow_for_days((date_to_days(now).unwrap_or(d) - d).max(0)),
        None if has_progress => 0.0,
        None => 1.0,
    };

    match &last_read {
        // Read through at least once: full coverage, and the glow is the ramp from
        // the last time the reader was here at all.
        Some(from) => {
            let elapsed = days_between(from, now).unwrap_or(0).max(0);
            Heat { pct: 1.0, standing: Standing::Read, glow: ramp, days: Some(elapsed), last_read }
        }
        None => {
            let pct = r.map_or(0.0, |r| pass_pct(corpus, book, r, words));
            let standing = if pct > 0.0 { Standing::Partial } else { Standing::Unread };
            // Never opened: lit at once, and fully — nothing to be recent about.
            // Part-read: the invitation is what is LEFT, faded by how recently you
            // were in it, so it goes quiet when you put it down and comes back if
            // you never pick it up again.
            let glow = if pct > 0.0 { (1.0 - pct).clamp(0.0, 1.0) * ramp } else { 1.0 };
            Heat { pct, standing, glow, days: None, last_read: None }
        }
    }
}

/// Every chapter of `book`, in order — the chapter grid's data.
pub fn book_chapters(
    corpus: &Corpus,
    words: &ChapterWords,
    store: &Store,
    book: &str,
    now: &str,
) -> Vec<ChapterHeat> {
    let recs = store.get(book);
    (1..=words.chapters(book))
        .map(|c| {
            let r = recs.and_then(|v| v.iter().find(|r| r.chapter == c));
            let w = words.words(book, c);
            ChapterHeat { chapter: c, words: w, heat: heat_of(corpus, book, r, w, now) }
        })
        .collect()
}

/// Every book in canon order — the book grid's data. Each book's `pct` and
/// `glow` are the **word-weighted means** of its chapters', which is what makes
/// the chapters add up to the book: a book is 40% read when 40% of its words
/// are, not when 40% of its chapter tiles are.
///
/// `days` is the exception, and deliberately so: a mean of "days since read"
/// over a book that is half unread would be a number about nothing. It reports
/// the **most recent** full read anywhere in the book — the answer to "when was
/// I last in Judges".
pub fn books(
    corpus: &Corpus,
    words: &ChapterWords,
    store: &Store,
    now: &str,
) -> Vec<BookHeat> {
    canon::book_ids()
        .map(|book| {
            let chapters = book_chapters(corpus, words, store, book, now);
            let total: u32 = chapters.iter().map(|c| c.words).sum();
            let weight = |f: fn(&ChapterHeat) -> f32| -> f32 {
                if total == 0 {
                    return 0.0;
                }
                let sum: f32 = chapters.iter().map(|c| f(c) * c.words as f32).sum();
                (sum / total as f32).clamp(0.0, 1.0)
            };
            let pct = weight(|c| c.heat.pct);
            let glow = weight(|c| c.heat.glow);
            // Only chapters the corpus actually has words for count toward "all
            // of it read" — a wordless chapter is one the corpus doesn't carry,
            // and it must not be able to hold a finished book at "partial".
            let real: Vec<&ChapterHeat> = chapters.iter().filter(|c| c.words > 0).collect();
            let read = real.iter().filter(|c| c.heat.standing == Standing::Read).count() as u16;
            let days = chapters.iter().filter_map(|c| c.heat.days).min();
            let last_read = chapters
                .iter()
                .filter_map(|c| c.heat.last_read.clone())
                .max_by_key(|d| date_to_days(d).unwrap_or(i64::MIN));
            let standing = match () {
                _ if pct <= 0.0 => Standing::Unread,
                _ if !real.is_empty() && read == real.len() as u16 => Standing::Read,
                _ => Standing::Partial,
            };
            BookHeat {
                book: book.to_string(),
                name: canon::display_name(book),
                chapters: chapters.len() as u16,
                words: total,
                read,
                heat: Heat { pct, standing, glow, days, last_read },
            }
        })
        .collect()
}

// ── recording ────────────────────────────────────────────────────────────────

/// What a [`record`] call did, so a shell can react to a chapter completing
/// (the navigator's tile changing colour under the reader's thumb).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Recorded {
    pub book: String,
    pub chapter: u16,
    /// Coverage after this call.
    pub pct: f32,
    /// True when this call carried the pass over [`COMPLETE_AT`].
    pub completed: bool,
    #[serde(rename = "lastRead", skip_serializing_if = "Option::is_none")]
    pub last_read: Option<String>,
}

/// Credit reading time to a chapter and persist it.
///
/// `reached` is the furthest verse number the reader has had on screen and
/// `seconds` the dwell **since the last call** — shells accumulate both while a
/// chapter is on screen and report on a slow tick, on leaving the chapter, and
/// when the app goes to the background. Calling more often is correct but
/// writes a file each time; roughly every 30 seconds is the intended cadence.
///
/// Crossing [`COMPLETE_AT`] snaps coverage to a full read at `now` and clears
/// the pass, so the next time through starts clean.
pub fn record(
    home: impl AsRef<Path>,
    corpus: &Corpus,
    words: &ChapterWords,
    book: &str,
    chapter: u16,
    reached: u16,
    seconds: f32,
    now: &str,
) -> Result<Recorded, Error> {
    let mut list = load_book(&home, book)?;
    let idx = match list.iter().position(|r| r.chapter == chapter) {
        Some(i) => i,
        None => {
            list.push(ChapterReading { chapter, ..Default::default() });
            list.len() - 1
        }
    };
    let total = words.words(book, chapter);
    let rec = &mut list[idx];
    rec.reached = rec.reached.max(reached);
    rec.dwell += seconds.max(0.0);
    // Any credited second is contact with the chapter, whether or not the pass
    // ever finishes. This is what lets a part-read chapter go quiet.
    if seconds > 0.0 {
        rec.touched = Some(day_of(now));
    }

    let pct = pass_pct(corpus, book, rec, total);
    let completed = pct >= COMPLETE_AT && total > 0;
    if completed {
        rec.last_read = Some(day_of(now));
        rec.touched = Some(day_of(now));
        rec.reached = 0;
        rec.dwell = 0.0;
    }
    let out = Recorded {
        book: book.to_string(),
        chapter,
        pct: if completed { 1.0 } else { pct },
        completed,
        last_read: rec.last_read.clone(),
    };
    write_book(&home, book, &list)?;
    Ok(out)
}

/// Log a chapter as read on `date` by hand — the long-press affordance on a
/// chapter's opening verse, for reading done in a paper Bible. Full credit:
/// you read it, the app simply wasn't there.
///
/// `date` may be any `YYYY-MM-DD` (or RFC3339 stamp); only its day is kept.
pub fn mark_read(home: impl AsRef<Path>, book: &str, chapter: u16, date: &str) -> Result<(), Error> {
    let mut list = load_book(&home, book)?;
    let day = day_of(date);
    match list.iter_mut().find(|r| r.chapter == chapter) {
        Some(r) => {
            r.last_read = Some(day.clone());
            r.touched = Some(day);
            r.reached = 0;
            r.dwell = 0.0;
        }
        None => list.push(ChapterReading {
            chapter,
            reached: 0,
            dwell: 0.0,
            last_read: Some(day.clone()),
            touched: Some(day),
            extra: Map::new(),
        }),
    }
    write_book(&home, book, &list)
}

/// Drop a chapter's reading record entirely — the way back out of a date set by
/// mistake. The chapter returns to unread.
pub fn forget(home: impl AsRef<Path>, book: &str, chapter: u16) -> Result<(), Error> {
    let mut list = load_book(&home, book)?;
    list.retain(|r| r.chapter != chapter);
    write_book(&home, book, &list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;

    const NOW: &str = "2026-07-28T12:00:00Z";

    /// A three-chapter toy corpus: 10, 20 and 5 words respectively, spread over
    /// verses so the high-water arithmetic has something to bite on.
    fn toy() -> Corpus {
        fn verse(b: &str, c: u16, v: u16, words: usize) -> String {
            let toks: Vec<String> = (0..words).map(|i| format!(r#"["","w{i}","",[],0]"#)).collect();
            format!(r#"{{"b":"{b}","c":{c},"v":{v},"t":[{}]}}"#, toks.join(","))
        }
        let mut lines = vec![serde_json::to_string(&corpus::corpus_header(
            canon::TOKENIZATION_VERSION,
            8, // 5 verses in Gen 1, 2 in Gen 2, 1 in Gen 3
        ))
        .unwrap()];
        // Gen 1: 5 verses × 2 words = 10. Gen 2: 2 verses × 10 = 20. Gen 3: 1 × 5.
        for v in 1..=5 {
            lines.push(verse("Gen", 1, v, 2));
        }
        for v in 1..=2 {
            lines.push(verse("Gen", 2, v, 10));
        }
        lines.push(verse("Gen", 3, 1, 5));
        corpus::from_str(&lines.join("\n")).unwrap()
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("plumbline-reading-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn counts_words_per_chapter_and_cumulatively() {
        let c = toy();
        let w = ChapterWords::build(&c);
        assert_eq!(w.words("Gen", 1), 10);
        assert_eq!(w.words("Gen", 2), 20);
        assert_eq!(w.words("Gen", 3), 5);
        assert_eq!(w.chapters("Gen"), 3);
        assert_eq!(w.book_words("Gen"), 35);
        assert_eq!(cumulative_words(&c, "Gen", 1), vec![(1, 2), (2, 4), (3, 6), (4, 8), (5, 10)]);
        // Past the end credits the whole chapter, not nothing.
        assert_eq!(words_through(&c, "Gen", 1, 99), 10);
        assert_eq!(words_through(&c, "Gen", 1, 0), 0);
        assert_eq!(words_through(&c, "Gen", 1, 3), 6);
    }

    #[test]
    fn scrolling_without_time_credits_nothing() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("scroll-only");
        // Straight to the bottom of Gen 1, no time at all.
        let r = record(&home, &c, &w, "Gen", 1, 5, 0.0, NOW).unwrap();
        assert_eq!(r.pct, 0.0, "flipping through must credit nothing");
        assert!(!r.completed);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn time_without_scrolling_credits_only_what_was_reached() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("dwell-only");
        // An hour parked on verse 1 of Gen 1 (2 of its 10 words).
        let r = record(&home, &c, &w, "Gen", 1, 1, 3600.0, NOW).unwrap();
        assert!((r.pct - 0.2).abs() < 1e-6, "capped by how far you got, got {}", r.pct);
        assert!(!r.completed);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn reading_through_completes_generously_and_snaps() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("complete");
        // Gen 1 is 10 words ≈ 2.7s at 220 wpm. Reach verse 4 (8 words = 80%,
        // under the 90% bar) with ample time.
        let r = record(&home, &c, &w, "Gen", 1, 4, 60.0, NOW).unwrap();
        assert!((r.pct - 0.8).abs() < 1e-6);
        assert!(!r.completed, "80% is short of the bar");
        // One more verse clears 90% and snaps to a full read.
        let r = record(&home, &c, &w, "Gen", 1, 5, 1.0, NOW).unwrap();
        assert!(r.completed);
        assert_eq!(r.pct, 1.0);
        assert_eq!(r.last_read.as_deref(), Some("2026-07-28"));

        // The pass is cleared, so the record is just the date now.
        let (store, errs) = load(&home);
        assert!(errs.is_empty(), "{errs:?}");
        let rec = &store["Gen"][0];
        assert_eq!((rec.reached, rec.dwell), (0, 0.0));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn ninety_percent_is_enough_no_chasing_the_last_verse() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("generous");
        // Gen 2 is 2 verses of 10 words. Reaching verse 2 is 100%, but check the
        // bar itself: a chapter where 90% is reachable without the final verse.
        // Gen 1: verses 1–5 at 2 words each; 90% of 10 words = 9, so verse 5 is
        // needed there. Use dwell as the binding constraint instead: 9 words of
        // dwell over a fully-scrolled chapter must complete it.
        let r = record(&home, &c, &w, "Gen", 1, 5, seconds_for_words(9), NOW).unwrap();
        assert!(r.completed, "90% of the words is a full read");
        assert_eq!(r.pct, 1.0);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn dwell_accumulates_across_calls() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("accumulate");
        // Gen 2 is 20 words. Sip time in thirds of what those words cost, so this
        // test states its intent in the rate's own terms and survives a change to
        // READING_WORDS_PER_MINUTE (which caught it once, at 220 → 300).
        let third = seconds_for_words(20) / 3.0;
        for (reached, secs) in [(1u16, third), (2, third)] {
            let r = record(&home, &c, &w, "Gen", 2, reached, secs, NOW).unwrap();
            assert!(!r.completed, "two thirds of the words is short of the 90% bar");
        }
        let (store, _) = load(&home);
        let rec = &store["Gen"][0];
        assert!((rec.dwell - third * 2.0).abs() < 1e-3, "seconds carry over between calls");
        assert_eq!(rec.reached, 2);
        // The third sip covers the chapter, and the scroll had already reached the
        // end — so this is the call that lands it.
        let r = record(&home, &c, &w, "Gen", 2, 2, third, NOW).unwrap();
        assert!(r.completed);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn scrolling_back_up_does_not_surrender_ground() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("high-water");
        // Well short of completing (which would legitimately reset `reached`):
        // a fifth of Gen 1's ten words, twice.
        let sip = seconds_for_words(2);
        let a = record(&home, &c, &w, "Gen", 1, 5, sip, NOW).unwrap();
        let b = record(&home, &c, &w, "Gen", 1, 1, sip, NOW).unwrap();
        assert!(!a.completed && !b.completed, "the pass must stay open for this to mean anything");
        let (store, _) = load(&home);
        assert_eq!(store["Gen"][0].reached, 5, "scrolling back up keeps the ground gained");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_chapter_you_just_read_is_quiet_even_when_it_did_not_complete() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("just-read");

        // The Jude case (2026-07-29). Read most of a chapter, but not past the 90%
        // bar, so it stands as Partial — and it must NOT glow, because the reader
        // was in it moments ago. "If I just read a chapter it doesn't need to show
        // glowing… bit of a false positive."
        let r = record(&home, &c, &w, "Gen", 2, 1, seconds_for_words(10), NOW).unwrap();
        assert!(!r.completed, "half of Gen 2 is short of the bar");
        let (store, _) = load(&home);
        let ch = &book_chapters(&c, &w, &store, "Gen", NOW)[1];
        assert_eq!(ch.heat.standing, Standing::Partial);
        assert!(ch.heat.pct > 0.0 && ch.heat.pct < 1.0);
        assert_eq!(ch.heat.glow, 0.0, "just read, so the map says nothing about it");

        // Put it down and leave it. A year later the unfinished half is an
        // invitation again — faded by how much of it is already behind you.
        let later = "2027-07-28T12:00:00Z";
        let ch = &book_chapters(&c, &w, &store, "Gen", later)[1];
        assert_eq!(ch.heat.standing, Standing::Partial);
        let expected = 1.0 - ch.heat.pct;
        assert!(
            (ch.heat.glow - expected).abs() < 1e-6,
            "a long-abandoned partial glows by what is left: want {expected}, got {}",
            ch.heat.glow,
        );

        // A chapter never opened is unaffected by any of this — nothing to be
        // recent about, so it stays lit.
        let ch1 = &book_chapters(&c, &w, &store, "Gen", NOW)[2];
        assert_eq!(ch1.heat.standing, Standing::Unread);
        assert_eq!(ch1.heat.glow, 1.0);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_pass_from_before_touched_existed_is_treated_as_recent() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let mut store = Store::new();
        // What v0.33.0 wrote: progress banked, no contact date, because the field
        // did not exist yet. It must NOT glow — these records are a day old, and
        // reading the amnesty the other way would leave the glow on precisely the
        // chapters whose glow was reported as a false positive.
        store.insert(
            "Gen".into(),
            vec![ChapterReading { chapter: 1, reached: 4, dwell: 600.0, ..Default::default() }],
        );
        let ch = &book_chapters(&c, &w, &store, "Gen", NOW)[0];
        assert_eq!(ch.heat.standing, Standing::Partial);
        assert_eq!(ch.heat.glow, 0.0, "an undated pass is read as recent, not as never");

        // A record with no progress AND no date is simply unread, and still lit.
        store.insert("Gen".into(), vec![ChapterReading { chapter: 1, ..Default::default() }]);
        let ch = &book_chapters(&c, &w, &store, "Gen", NOW)[0];
        assert_eq!(ch.heat.standing, Standing::Unread);
        assert_eq!(ch.heat.glow, 1.0);
    }

    #[test]
    fn a_completed_chapter_dipped_into_again_goes_quiet() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let mut store = Store::new();
        // Read through a year ago — so, on its own, fully stale and glowing.
        store.insert(
            "Gen".into(),
            vec![ChapterReading { chapter: 1, last_read: Some("2025-07-28".into()), ..Default::default() }],
        );
        assert_eq!(book_chapters(&c, &w, &store, "Gen", NOW)[0].heat.glow, 1.0);

        // Then the reader spends a little time in it today. They have BEEN here;
        // the map has nothing to tell them, even though the last full pass is old.
        store.get_mut("Gen").unwrap()[0].touched = Some("2026-07-28".into());
        let ch = &book_chapters(&c, &w, &store, "Gen", NOW)[0];
        assert_eq!(ch.heat.standing, Standing::Read, "a dip does not undo a full read");
        assert_eq!(ch.heat.glow, 0.0);
        assert_eq!(ch.heat.days, Some(365), "`days` still reports the last full read");
    }

    #[test]
    fn glow_is_flat_then_ramps_to_full_at_a_year() {
        assert_eq!(glow_for_days(0), 0.0);
        assert_eq!(glow_for_days(FRESH_DAYS), 0.0, "a month is still fresh");
        assert_eq!(glow_for_days(STALE_DAYS), 1.0);
        assert_eq!(glow_for_days(10_000), 1.0, "clamped");
        let mid = glow_for_days((FRESH_DAYS + STALE_DAYS) / 2);
        assert!((mid - 0.5).abs() < 0.01, "halfway is half glow, got {mid}");
        // Monotonic across the ramp.
        let mut prev = -1.0;
        for d in 0..400 {
            let g = glow_for_days(d);
            assert!(g >= prev, "glow must never fall as days rise");
            prev = g;
        }
    }

    #[test]
    fn unread_chapters_glow_at_once_and_dim_as_they_fill() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let store = Store::new();
        // Day one, and every day after: what you have never read is fully lit.
        // This is the invitation, not a nag — the reader asked to be shown where
        // the treasure is, and a map that starts dark shows nothing.
        for now in ["2026-07-28T12:00:00Z", "2030-01-01T00:00:00Z"] {
            let fresh = book_chapters(&c, &w, &store, "Gen", now);
            assert!(fresh.iter().all(|ch| ch.heat.standing == Standing::Unread));
            assert!(fresh.iter().all(|ch| ch.heat.glow == 1.0), "unread glows at once, at {now}");
            assert!(fresh.iter().all(|ch| ch.heat.days.is_none()), "never read has no last-read day");
        }

        // Part-read: the invitation shrinks in proportion to what is LEFT, so a
        // chapter you are most of the way through stops shouting.
        let mut part = Store::new();
        part.insert(
            "Gen".into(),
            // Gen 1 is 10 words over 5 verses; reached verse 4 (8 words) with
            // ample dwell = 80% covered, so 20% of the invitation remains.
            // Touched long ago and abandoned: fully stale, so nothing damps the
            // invitation. (An abandoned pass with NO date at all is the upgrade
            // amnesty — covered separately below.)
            vec![ChapterReading {
                chapter: 1,
                reached: 4,
                dwell: 600.0,
                last_read: None,
                touched: Some("2020-01-01".into()),
                ..Default::default()
            }],
        );
        let ch = &book_chapters(&c, &w, &part, "Gen", NOW)[0];
        assert_eq!(ch.heat.standing, Standing::Partial);
        assert!((ch.heat.pct - 0.8).abs() < 1e-6, "got {}", ch.heat.pct);
        assert!((ch.heat.glow - 0.2).abs() < 1e-6, "glow tracks what is left, got {}", ch.heat.glow);
    }

    #[test]
    fn a_read_chapter_is_quiet_then_glows_again() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let mut store = Store::new();
        store.insert(
            "Gen".into(),
            vec![ChapterReading { chapter: 1, last_read: Some("2026-07-01".into()), ..Default::default() }],
        );
        let ch = &book_chapters(&c, &w, &store, "Gen", NOW)[0];
        assert_eq!(ch.heat.standing, Standing::Read);
        assert_eq!(ch.heat.pct, 1.0);
        assert_eq!(ch.heat.days, Some(27));
        assert_eq!(ch.heat.glow, 0.0, "read last month, so quiet");
        // Same record, a year later.
        let ch = &book_chapters(&c, &w, &store, "Gen", "2027-07-28T12:00:00Z")[0];
        assert_eq!(ch.heat.glow, 1.0);
        // Being read must beat the start-date ramp, not be averaged with it.
        assert_eq!(ch.heat.standing, Standing::Read);
    }

    #[test]
    fn chapters_sum_to_the_book_by_words() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let mut store = Store::new();
        // Gen 2 read (20 of the book's 35 words); 1 and 3 untouched.
        store.insert(
            "Gen".into(),
            vec![ChapterReading { chapter: 2, last_read: Some("2026-07-20".into()), ..Default::default() }],
        );
        let gen = books(&c, &w, &store, NOW).into_iter().find(|b| b.book == "Gen").unwrap();
        assert_eq!(gen.words, 35);
        assert_eq!(gen.read, 1);
        assert_eq!(gen.chapters, 3);
        assert!((gen.heat.pct - 20.0 / 35.0).abs() < 1e-6, "word-weighted, got {}", gen.heat.pct);
        assert_eq!(gen.heat.standing, Standing::Partial);
        assert_eq!(gen.heat.days, Some(8), "most recent read in the book");
        // Every book is present, in canon order, even with no data at all.
        let all = books(&c, &w, &Store::new(), NOW);
        assert_eq!(all.len(), 66);
        assert_eq!(all[0].book, "Gen");
        assert_eq!(all[65].book, "Rev");
    }

    #[test]
    fn a_fully_read_book_reads_as_read() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let mut store = Store::new();
        store.insert(
            "Gen".into(),
            (1..=3)
                .map(|ch| ChapterReading {
                    chapter: ch,
                    last_read: Some("2026-07-20".into()),
                    ..Default::default()
                })
                .collect(),
        );
        let gen = books(&c, &w, &store, NOW).into_iter().find(|b| b.book == "Gen").unwrap();
        assert_eq!(gen.heat.standing, Standing::Read);
        assert!((gen.heat.pct - 1.0).abs() < 1e-6);
        assert_eq!(gen.read, 3);
    }

    #[test]
    fn manual_mark_gives_full_credit_and_forget_undoes_it() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("manual");
        mark_read(&home, "Gen", 3, "2026-05-04T00:00:00Z").unwrap();
        let (store, errs) = load(&home);
        assert!(errs.is_empty(), "{errs:?}");
        let ch = &book_chapters(&c, &w, &store, "Gen", NOW)[2];
        assert_eq!(ch.heat.standing, Standing::Read);
        assert_eq!(ch.heat.pct, 1.0);
        assert_eq!(ch.heat.last_read.as_deref(), Some("2026-05-04"), "day only, no clock");

        // A part-read pass is discarded by a manual mark, not left to double-count.
        record(&home, &c, &w, "Gen", 1, 2, 1.0, NOW).unwrap();
        mark_read(&home, "Gen", 1, "2026-06-01").unwrap();
        let (store, _) = load(&home);
        let rec = store["Gen"].iter().find(|r| r.chapter == 1).unwrap();
        assert_eq!((rec.reached, rec.dwell), (0, 0.0));

        forget(&home, "Gen", 1).unwrap();
        let (store, _) = load(&home);
        assert!(store["Gen"].iter().all(|r| r.chapter != 1));
        let ch = &book_chapters(&c, &w, &store, "Gen", NOW)[0];
        assert_eq!(ch.heat.standing, Standing::Unread);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn emptying_a_book_removes_its_file() {
        let home = scratch("empty");
        mark_read(&home, "Gen", 1, "2026-05-04").unwrap();
        assert!(book_file(&home, "Gen").exists());
        forget(&home, "Gen", 1).unwrap();
        assert!(!book_file(&home, "Gen").exists(), "no husk left behind");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn since_is_written_once_and_then_honoured() {
        let home = scratch("since");
        assert_eq!(since(&home), None);
        let first = ensure_since(&home, "2026-03-09T22:00:00Z").unwrap();
        assert_eq!(first, "2026-03-09");
        // A later launch must not move the anchor.
        let again = ensure_since(&home, NOW).unwrap();
        assert_eq!(again, "2026-03-09");
        assert_eq!(since(&home).as_deref(), Some("2026-03-09"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_foreign_or_broken_file_is_skipped_not_fatal() {
        let home = scratch("junk");
        mark_read(&home, "Gen", 1, "2026-05-04").unwrap();
        crate::store::write_atomic(reading_dir(&home).join("exod.json"), "{ not json").unwrap();
        crate::store::write_atomic(
            reading_dir(&home).join("lev.json"),
            r#"{"format":"something-else","book":"Lev","chapters":[]}"#,
        )
        .unwrap();
        crate::store::write_atomic(
            reading_dir(&home).join("nope.json"),
            &format!(r#"{{"format":"{FORMAT}","book":"Nope","chapters":[]}}"#),
        )
        .unwrap();
        let (store, errors) = load(&home);
        assert!(store.contains_key("Gen"), "the good file still loads");
        assert_eq!(errors.len(), 3, "each bad file reported: {errors:?}");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_file_we_cannot_read_is_never_overwritten() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("no-clobber");
        let path = book_file(&home, "Gen");

        // Two ways a book file can be sitting there and mean nothing to us:
        // corrupt bytes, and a well-formed file from a build newer than this one.
        // Neither is "nothing read yet" — both are the reader's history, and
        // every write path here reads the whole file and writes it back.
        let future =
            r#"{"format":"plumbline-reading-v9","book":"Gen","chapters":[{"c":1,"lastRead":"2019-01-01"}]}"#;
        for content in ["{ not json".to_string(), future.to_string()] {
            crate::store::write_atomic(&path, &content).unwrap();
            assert!(load_book(&home, "Gen").is_err(), "must not read as no history: {content}");

            let refusals = [
                record(&home, &c, &w, "Gen", 1, 5, 60.0, NOW).err(),
                mark_read(&home, "Gen", 1, "2026-05-04").err(),
                forget(&home, "Gen", 1).err(),
            ];
            for e in refusals {
                let msg = e.map(|e| e.to_string()).unwrap_or_default();
                assert!(msg.contains("refusing to overwrite"), "want a refusal, got {msg:?}");
            }
            // `unwrap_or_default` and not `unwrap`: a refused `forget` that in
            // fact went through deletes the file outright (an emptied book leaves
            // no husk), and that must read as a diff, not as a panic about i/o.
            assert_eq!(
                std::fs::read_to_string(&path).unwrap_or_default(),
                content,
                "the bytes on disk must be exactly as the reader left them",
            );
        }

        // Absent still means nothing read yet — a first-ever read must not trip
        // over the guard.
        std::fs::remove_file(&path).unwrap();
        assert!(load_book(&home, "Gen").unwrap().is_empty());
        mark_read(&home, "Gen", 1, "2026-05-04").unwrap();
        assert_eq!(load_book(&home, "Gen").unwrap().len(), 1);

        // A file with no chapters in it, and an empty file, are empty too: there
        // is nothing in either to lose.
        for content in [format!(r#"{{"format":"{FORMAT}","book":"Gen","chapters":[]}}"#), String::new()] {
            crate::store::write_atomic(&path, &content).unwrap();
            assert!(load_book(&home, "Gen").unwrap().is_empty(), "empty: {content:?}");
            mark_read(&home, "Gen", 3, "2026-05-04").unwrap();
            assert_eq!(load_book(&home, "Gen").unwrap().len(), 1);
        }

        // `_since.json` is the same story: `since` reads a file it cannot parse
        // as "no anchor yet", and the anchor is written once and never again, so
        // stamping a new one over it is the only chance to lose it.
        for content in ["not json at all", r#"{"format":"plumbline-reading-v9","since":"2019-01-01"}"#] {
            crate::store::write_atomic(since_file(&home), content).unwrap();
            assert_eq!(since(&home), None, "unusable: {content}");
            let msg = ensure_since(&home, NOW).err().map(|e| e.to_string()).unwrap_or_default();
            assert!(msg.contains("refusing to overwrite"), "want a refusal, got {msg:?}");
            assert_eq!(std::fs::read_to_string(since_file(&home)).unwrap(), content);
        }
        std::fs::remove_file(since_file(&home)).unwrap();
        assert_eq!(ensure_since(&home, NOW).unwrap(), "2026-07-28", "absent still writes the anchor");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_stored_shape_is_the_documented_one() {
        let home = scratch("shape");
        mark_read(&home, "Gen", 2, "2026-05-04").unwrap();
        let raw = std::fs::read_to_string(book_file(&home, "Gen")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["format"], FORMAT);
        assert_eq!(v["book"], "Gen");
        assert_eq!(v["chapters"][0]["c"], 2);
        assert_eq!(v["chapters"][0]["lastRead"], "2026-05-04");
        // An untouched pass writes nothing about itself.
        assert!(v["chapters"][0].get("reached").is_none() || v["chapters"][0]["reached"] == 0);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// AUDIT 2026-07-29 forward compatibility: the on-disk formats evolve
    /// **additively** (CLAUDE.md §Data formats), and a sideloaded APK never
    /// auto-updates — so a key this build drops is dropped for good on that
    /// device. Reading one chapter of a book rewrites that book's whole file, so
    /// a v1.1 field would go from all 150 psalms at once.
    #[test]
    fn a_book_file_keeps_the_keys_of_a_later_build() {
        let home = scratch("forward");
        let path = book_file(&home, "Gen");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"format":"plumbline-reading-v1","book":"Gen",
                "chapters":[
                  {"c":1,"reached":0,"dwell":0.0,"lastRead":"2026-05-04","touched":"2026-05-04",
                   "passes":3,"aloud":{"minutes":4},"plans":["chronological"]},
                  {"c":2,"reached":0,"dwell":0.0}
                ],
                "plan":"chronological","streak":{"days":9},"devices":["phone","laptop"]}"#,
        )
        .unwrap();

        // Marking another chapter read rewrites the file, chapter 1 with it.
        mark_read(&home, "Gen", 2, "2026-07-28").unwrap();
        let back: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["chapters"][1]["lastRead"], "2026-07-28", "the mark itself must land");
        assert_eq!(back["plan"], "chronological", "an unknown scalar was stripped");
        assert_eq!(back["streak"], serde_json::json!({"days":9}), "an unknown object was stripped");
        assert_eq!(back["devices"], serde_json::json!(["phone", "laptop"]), "an unknown array was stripped");
        assert_eq!(back["chapters"][0]["passes"], 3, "a chapter's unknown scalar was stripped");
        assert_eq!(
            back["chapters"][0]["aloud"],
            serde_json::json!({"minutes":4}),
            "a chapter's unknown object was stripped"
        );
        assert_eq!(
            back["chapters"][0]["plans"],
            serde_json::json!(["chronological"]),
            "a chapter's unknown array was stripped"
        );

        // The start anchor is written once and never again, so there is no save
        // for it to lose a key to — but check, because that is the whole promise.
        std::fs::write(
            since_file(&home),
            r#"{"format":"plumbline-reading-v1","since":"2026-01-01","timezone":"Africa/Johannesburg"}"#,
        )
        .unwrap();
        assert_eq!(ensure_since(&home, NOW).unwrap(), "2026-01-01");
        let back: Value = serde_json::from_str(&std::fs::read_to_string(since_file(&home)).unwrap()).unwrap();
        assert_eq!(back["timezone"], "Africa/Johannesburg");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A book file with nothing unknown in it is written byte for byte as it was
    /// before any of that landed — these files already ship inside backup zips.
    #[test]
    fn a_book_file_with_no_unknown_keys_is_written_exactly_as_before() {
        let home = scratch("golden");
        mark_read(&home, "Gen", 1, "2026-01-01").unwrap();
        assert_eq!(
            std::fs::read_to_string(book_file(&home, "Gen")).unwrap(),
            r#"{
  "format": "plumbline-reading-v1",
  "book": "Gen",
  "chapters": [
    {
      "c": 1,
      "reached": 0,
      "dwell": 0.0,
      "lastRead": "2026-01-01",
      "touched": "2026-01-01"
    }
  ]
}
"#
        );
        let _ = std::fs::remove_dir_all(&home);
    }
}
