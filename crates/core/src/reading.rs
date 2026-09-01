//! Where you've read, and how long ago — the reading map behind the navigator's
//! glow. Not a completion tracker: the grids tint what you have drifted away from.
//!
//! ## What gets measured
//!
//! ```text
//! covered_words = min( words above the furthest point you reached,
//!                      dwell_seconds × READING_WORDS_PER_MINUTE / 60 )
//! pct           = covered_words / chapter words
//! ```
//!
//! Scrolling to the bottom instantly credits nothing; sitting on verse 1 for an
//! hour credits only verse 1. Dwell is aggregate, not per-verse. A pass completes
//! at [`COMPLETE_AT`] (85%) and snaps to a full read — but only if it ALSO reached
//! the chapter's last verse: the snap is a tolerance on the clock, not on the
//! chapter. Falling short of either is a partial pass.
//!
//! ## What gets stored
//!
//! [`ChapterReading`], one file per book under `home/reading/`, plus `_since.json`
//! (see [`ensure_since`]). `reached`/`dwell` describe the pass under way and reset
//! when it completes; `last_read` is the last COMPLETED pass, `touched` the last
//! contact of any kind. Dwell within a verse is not persisted — the shell holds it
//! for the session.
//!
//! ## The glow
//!
//! Read means "you have been away a while"; never read is full glow from the first
//! launch; part-read glows in proportion to what is LEFT. Over all of it,
//! **recency outranks coverage**: the ramp runs from the most recent contact
//! (`touched` or `last_read`, whichever is later), flat zero for [`FRESH_DAYS`] and
//! full at [`STALE_DAYS`], so a chapter you were in this morning says nothing
//! whatever its coverage.
//!
//! Personal study data, so it rides in the backup zip like `memory/` and `notes/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::canon;
use crate::civil::{date_to_days, days_between, days_to_date};
use crate::corpus::Corpus;
use crate::Error;

/// The on-disk stamp — frozen, like every format tag here.
pub const FORMAT: &str = "plumbline-reading-v1";

/// The reading speed dwell is converted at, deliberately fast. The rate is NOT
/// the flip gate — [`GRACE_SECONDS`] plus the high-water mark are, and a flipper
/// banks no seconds at all — so it can afford to sit above a brisk ~600 wpm
/// re-read of familiar text. Slower rates (220, 300, 500) all called real reads
/// of a short chapter "Partial".
pub const READING_WORDS_PER_MINUTE: f32 = 700.0;

/// Coverage at or above which a pass counts as a full read and snaps to 1.0.
pub const COMPLETE_AT: f32 = 0.85;

/// Days after a read during which there is no glow at all.
pub const FRESH_DAYS: i64 = 30;

/// Days after which the glow is at full: a year, matching the cadence of any
/// read-the-Bible-in-a-year plan.
pub const STALE_DAYS: i64 = 365;

/// Seconds a chapter must be on screen before dwell starts accruing at all.
/// This — not the reading rate — is what makes flipping through free.
pub const GRACE_SECONDS: f32 = 3.0;

/// The cadence a shell should report at. Every call writes a file, so this trades
/// losing a session's tail against churning the disk (and IndexedDB) constantly.
pub const TICK_SECONDS: f32 = 30.0;

/// No scroll, tap or keypress for this long and accrual stops — a chapter left
/// open on a table overnight is not reading. It resumes on the next interaction.
pub const IDLE_SECONDS: f32 = 120.0;

/// The tuning a shell reads over the ABI rather than hard-coding, so nothing can
/// drift on what "read" means.
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

/// Where a chapter stands. Drives the hue in the navigator: unread gold, partial
/// copper, read sage. The glow rides on top of all three — see the module docs.
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
    /// counts, a completed one counts, `None` means never. Additive. It is what
    /// lets recency silence the glow on its own, without a completed pass.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub touched: Option<String>,
    /// Every key on this record this build has never heard of, carried back out
    /// again on save. The on-disk formats evolve **additively** (CLAUDE.md
    /// §Data formats), and every write path here rewrites the book's whole chapter
    /// list — so without this an older build would strip a newer one's per-chapter
    /// field from all 150 psalms the first time the reader opened one. An empty
    /// flattened map writes no key, so today's files round-trip byte for byte.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A book's file: `home/reading/<book>.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct BookFile {
    format: String,
    book: String,
    chapters: Vec<ChapterReading>,
    /// The file's own unknown keys, lifted off the file being replaced by
    /// [`write_book`] — a book's reading state is a bare `Vec` with no container
    /// to hang them on.
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// `home/reading/_since.json` — when this reader started. Underscored so it can
/// never be mistaken for a book file. No unknown-key catch-all: [`ensure_since`]
/// writes this file once and never again, so there is no save to lose a key to.
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

/// Load ONE book's chapters — what every write path uses, since dwell recording
/// runs on a timer and must not read all 66 files to touch one.
///
/// A file that is THERE but unparseable — corrupt, or stamped by a newer build —
/// is an **error**, never an empty history: every caller writes the list back out,
/// so "nothing read yet" would blank the reader's history. Same refuse-to-clobber
/// rule as [`crate::thread::add_to_thread`]. A missing or empty file still means
/// nothing read yet.
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
        _ => Err(Error::Corpus(format!("{} exists but could not be read — refusing to overwrite", path.display()))),
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
    // Top-level keys we do not understand go back out with the file; the chapters
    // carry their own. Unparseable bytes yield nothing — `load_book` refuses such
    // a file before any caller gets here.
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
/// It no longer feeds the glow, but it stays: it already ships inside backup zips
/// and `since` is part of the wire payload, and these contracts evolve additively.
///
/// An unparseable file is refused rather than overwritten: [`since`] reads it as
/// `None`, and the anchor is written once and never again, so stamping a fresh
/// date over it is the one chance to lose it. Callers treat a failure as "no
/// anchor yet" and carry on.
pub fn ensure_since(home: impl AsRef<Path>, now: &str) -> Result<String, Error> {
    if let Some(s) = since(&home) {
        return Ok(s);
    }
    let path = since_file(&home);
    // `since` could not use the file, so anything in it is content we do not
    // understand rather than an absence.
    if std::fs::read(&path).is_ok_and(|b| !b.iter().all(|b| b.is_ascii_whitespace())) {
        return Err(Error::Corpus(format!("{} exists but could not be read — refusing to overwrite", path.display())));
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
/// Public because it is the store's date grain: anything comparing against a
/// `last_read`/`touched` day (the plans' done-today check) must round the same way.
pub fn day_of(stamp: &str) -> String {
    match date_to_days(stamp) {
        Some(d) => days_to_date(d),
        None => stamp.to_string(),
    }
}

// ── word counts ──────────────────────────────────────────────────────────────

/// Words per chapter for the whole canon — the denominator of every percentage,
/// and the weight that makes a book's coverage the sum of its chapters'. Built
/// once (~5 KB retained): the navigator asks for all 1189 chapters every open,
/// and re-walking 31,102 verses each time buys nothing.
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
        self.by_book.get(book).and_then(|v| v.get(chapter.checked_sub(1)? as usize)).copied().unwrap_or(0)
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

/// The last verse of a chapter — the bottom a pass has to reach. Zero for a
/// chapter the corpus does not have.
fn last_verse(corpus: &Corpus, book: &str, chapter: u16) -> u16 {
    corpus.chapter_verses(book, chapter).last().map_or(0, |v| v.verse)
}

/// Whether a pass got to the END of the chapter, which no percentage can say on
/// its own: [`COMPLETE_AT`] snaps at 85%, so without this a reader who stopped
/// three verses short with time to spare is told they read the whole thing. A
/// shell reports the high-water verse generously (a verse counts once its line has
/// cleared the fold), so scrolling to the bottom does reach it.
fn reached_end(corpus: &Corpus, book: &str, chapter: u16, reached: u16) -> bool {
    let end = last_verse(corpus, book, chapter);
    end > 0 && reached >= end
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
    /// The book's name in the reader's language (`i18n::active`). Owned, not a
    /// slice of the canon table, because it is a translation now.
    pub name: String,
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
fn heat_of(corpus: &Corpus, book: &str, r: Option<&ChapterReading>, words: u32, now: &str) -> Heat {
    let last_read = r.and_then(|r| r.last_read.clone());
    let touched = r.and_then(|r| r.touched.clone());

    // Recency outranks coverage: the most recent contact of any kind anchors the
    // ramp, which is flat zero inside FRESH_DAYS.
    let contact = [last_read.as_deref(), touched.as_deref()].into_iter().flatten().filter_map(date_to_days).max();
    // Upgrade amnesty: `touched` is additive, so a pass begun before it existed has
    // progress and no date. Dwell banked with no contact date reads as contact NOW,
    // rather than glowing as if the reader had never been there.
    let has_progress = r.is_some_and(|r| r.dwell > 0.0);
    let ramp = match contact {
        Some(d) => glow_for_days((date_to_days(now).unwrap_or(d) - d).max(0)),
        None if has_progress => 0.0,
        None => 1.0,
    };

    match &last_read {
        // Read through at least once: full coverage, glow from the last contact.
        Some(from) => {
            let elapsed = days_between(from, now).unwrap_or(0).max(0);
            Heat { pct: 1.0, standing: Standing::Read, glow: ramp, days: Some(elapsed), last_read }
        }
        None => {
            let pct = r.map_or(0.0, |r| pass_pct(corpus, book, r, words));
            let standing = if pct > 0.0 { Standing::Partial } else { Standing::Unread };
            // Never opened: fully lit, nothing to be recent about. Part-read: what
            // is LEFT, faded by how recently the reader was in it.
            let glow = if pct > 0.0 { (1.0 - pct).clamp(0.0, 1.0) * ramp } else { 1.0 };
            Heat { pct, standing, glow, days: None, last_read: None }
        }
    }
}

/// Every chapter of `book`, in order — the chapter grid's data.
pub fn book_chapters(corpus: &Corpus, words: &ChapterWords, store: &Store, book: &str, now: &str) -> Vec<ChapterHeat> {
    let recs = store.get(book);
    (1..=words.chapters(book))
        .map(|c| {
            let r = recs.and_then(|v| v.iter().find(|r| r.chapter == c));
            let w = words.words(book, c);
            ChapterHeat { chapter: c, words: w, heat: heat_of(corpus, book, r, w, now) }
        })
        .collect()
}

/// Every book in canon order — the book grid's data. `pct` and `glow` are the
/// **word-weighted means** of the book's chapters, so a book is 40% read when 40%
/// of its words are. `days` is the exception: it reports the most recent full read
/// anywhere in the book, since a mean over a half-unread book says nothing.
pub fn books(corpus: &Corpus, words: &ChapterWords, store: &Store, now: &str) -> Vec<BookHeat> {
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
            // Only chapters the corpus has words for count toward "all of it read":
            // a wordless chapter must not hold a finished book at "partial".
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
                name: crate::i18n::book_name(crate::i18n::active(), book),
                chapters: chapters.len() as u16,
                words: total,
                read,
                heat: Heat { pct, standing, glow, days, last_read },
            }
        })
        .collect()
}

// ── the dwell tracker ────────────────────────────────────────────────────────

/// The most a single sample may credit. A tick that arrives very late means the
/// shell was not running, not that somebody read for that long; without this the
/// first sample after a stall could bank an hour of "reading".
pub const MAX_STEP_SECONDS: f32 = 5.0;

/// A tick's verdict: hand these seconds to [`record`] for this chapter.
#[derive(Clone, Debug, PartialEq)]
pub struct DwellReport {
    pub book: String,
    pub chapter: u16,
    /// The high-water verse of the chapter the seconds were earned in — NOT
    /// whatever is on screen now. See [`DwellTracker::tick`].
    pub reached: u16,
    pub seconds: f32,
}

/// How long a chapter was really read. The counting lives here, not in a shell:
/// a shell owns only what the core cannot know, having no clock and no window —
/// that another second passed with a chapter in front of somebody.
///
/// Three refusals, and they are the whole design:
///
/// * a GRACE window before anything accrues, so paging through a book never
///   credits the chapters it flew past. It is a THRESHOLD, not a tax: once a pass
///   survives it the withheld seconds are credited back, or every dialog and
///   word-study tap re-serves the window and silently docks 3s from the pass;
/// * an IDLE cutoff, so a phone left face-up on a table does not read Leviticus
///   overnight;
/// * nothing on screen stops the clock and banks the tail — a backgrounded app is
///   not being read, and locking a phone is how a reading session usually ends.
#[derive(Clone, Debug, Default)]
pub struct DwellTracker {
    /// The chapter the banked seconds belong to — the reader may have moved on by
    /// the time they are handed over.
    owner: Option<(String, u16)>,
    reached: u16,
    on_screen: f32,
    since_input: f32,
    pending: f32,
    /// Whether the grace window has been refunded — once per pass, on the first
    /// sample to survive it.
    graced: bool,
}

impl DwellTracker {
    /// One sample.
    ///
    /// `target` is what is being read right now, `None` when nothing is (a
    /// dialog is up, the app is backgrounded, the reader is in Present). `reached`
    /// is the deepest verse of `target` the reader has scrolled to, `interacted`
    /// whether anything happened since the last sample, and `step` the seconds
    /// that sample covers (clamped to [`MAX_STEP_SECONDS`]).
    ///
    /// Returns seconds to credit, on the [`TICK_SECONDS`] cadence and whenever
    /// the target changes — the tail of a reading session is real reading and
    /// must not be lost because it fell between two ticks.
    pub fn tick(
        &mut self,
        target: Option<(&str, u16)>,
        reached: u16,
        interacted: bool,
        step: f32,
    ) -> Option<DwellReport> {
        let step = if step.is_finite() { step.clamp(0.0, MAX_STEP_SECONDS) } else { 0.0 };
        let same = match (self.owner.as_ref(), target) {
            (Some((b, c)), Some((nb, nc))) => b == nb && *c == nc,
            (None, None) => true,
            _ => false,
        };
        if !same {
            // Leaving or arriving: bank the tail against the chapter that earned
            // it, then start clean — coming back is not continuing, so the grace
            // period is served again.
            let out = self.bank();
            self.owner = target.map(|(b, c)| (b.to_string(), c));
            self.reached = reached;
            self.on_screen = 0.0;
            self.since_input = 0.0;
            self.graced = false;
            return out;
        }
        self.owner.as_ref()?;
        if interacted {
            self.since_input = 0.0;
        }
        // Monotonic within the pass: scrolling back up never surrenders ground.
        self.reached = self.reached.max(reached);
        self.on_screen += step;
        self.since_input += step;
        // Grace first, then presence.
        if self.on_screen < GRACE_SECONDS || self.since_input > IDLE_SECONDS {
            return None;
        }
        // Survived: this was a stay, not a flip, so the withheld seconds were real
        // reading. Credit them back, once per pass.
        if !self.graced {
            self.graced = true;
            self.pending += self.on_screen - step;
        }
        self.pending += step;
        (self.pending >= TICK_SECONDS).then(|| self.bank()).flatten()
    }

    /// Nothing is on screen any more (the app is going to the background, the
    /// tracker is being torn down). Banks the tail and re-serves the grace
    /// period for whatever comes next.
    pub fn stop(&mut self) -> Option<DwellReport> {
        self.tick(None, 0, false, 0.0)
    }

    /// Take the banked seconds, attributed to the chapter that earned them.
    fn bank(&mut self) -> Option<DwellReport> {
        let seconds = std::mem::take(&mut self.pending);
        let (book, chapter) = self.owner.clone()?;
        (seconds > 0.0).then_some(DwellReport { book, chapter, reached: self.reached, seconds })
    }
}

// ── recording ────────────────────────────────────────────────────────────────

/// What a [`record`] call did, so a shell can react to a chapter completing.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Recorded {
    pub book: String,
    pub chapter: u16,
    /// Coverage after this call.
    pub pct: f32,
    /// True when this call carried the pass over [`COMPLETE_AT`] *with* the
    /// chapter's last verse reached.
    pub completed: bool,
    #[serde(rename = "lastRead", skip_serializing_if = "Option::is_none")]
    pub last_read: Option<String>,
}

/// Credit reading time to a chapter and persist it.
///
/// `reached` is the furthest verse number on screen and `seconds` the dwell
/// **since the last call**. Calling more often is correct but writes a file each
/// time; [`TICK_SECONDS`] is the intended cadence.
///
/// Crossing [`COMPLETE_AT`] **with the chapter's last verse reached** snaps
/// coverage to a full read at `now` and clears the pass. Time without the bottom
/// of the chapter, or the bottom without the time, stays a partial pass.
// Eight arguments: three are the loaded core the shell already holds, five are the
// tick arriving as separate scalars off the C ABI. A params struct would be built
// and destructured again at the single call site.
#[allow(clippy::too_many_arguments)]
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
    // Any credited second is contact, finished pass or not — what lets a part-read
    // chapter go quiet.
    if seconds > 0.0 {
        rec.touched = Some(day_of(now));
    }

    let pct = pass_pct(corpus, book, rec, total);
    // TWO gates: enough time AND the bottom of the chapter. The 85% snap alone
    // credits a full read to someone who stopped short.
    let completed = pct >= COMPLETE_AT && total > 0 && reached_end(corpus, book, chapter, rec.reached);
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

/// Log a chapter as read on `date` by hand — for reading done in a paper Bible.
/// Full credit. `date` may be `YYYY-MM-DD` or RFC3339; only its day is kept.
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

    fn verse(b: &str, c: u16, v: u16, words: usize) -> String {
        let toks: Vec<String> = (0..words).map(|i| format!(r#"["","w{i}","",[],0]"#)).collect();
        format!(r#"{{"b":"{b}","c":{c},"v":{v},"t":[{}]}}"#, toks.join(","))
    }

    /// A three-chapter toy corpus: 10, 20 and 5 words, spread over verses so the
    /// high-water arithmetic has something to bite on.
    fn toy() -> Corpus {
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
        // Gen 1 is 10 words. Verse 4 = 8 words = 80%, under COMPLETE_AT.
        let r = record(&home, &c, &w, "Gen", 1, 4, 60.0, NOW).unwrap();
        assert!((r.pct - 0.8).abs() < 1e-6);
        assert!(!r.completed, "80% is short of the bar");
        // One more verse clears the bar and snaps to a full read.
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
    fn the_snap_is_generous_about_time_not_about_scroll() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("generous");
        // The 85% bar is a tolerance on the CLOCK: COMPLETE_AT of Gen 1's 10 words
        // is 8.5, so 9 words of dwell over a fully-scrolled chapter must complete it.
        let r = record(&home, &c, &w, "Gen", 1, 5, seconds_for_words(9), NOW).unwrap();
        assert!(r.completed, "clearing COMPLETE_AT at the bottom is a full read");
        assert_eq!(r.pct, 1.0);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn time_enough_is_not_a_read_without_the_bottom_of_the_chapter() {
        // A chapter whose words are front-loaded, so the WORD bar clears while
        // verses still remain: v1=17, v2=2, v3=1 — verse 2 is 95% of 20 words.
        let lines = [
            serde_json::to_string(&corpus::corpus_header(canon::TOKENIZATION_VERSION, 3)).unwrap(),
            verse("Gen", 1, 1, 17),
            verse("Gen", 1, 2, 2),
            verse("Gen", 1, 3, 1),
        ];
        let c = corpus::from_str(&lines.join("\n")).unwrap();
        let w = ChapterWords::build(&c);
        let home = scratch("no-bottom");

        // 95% of the words, ample time — but verse 3 never came into view.
        let r = record(&home, &c, &w, "Gen", 1, 2, 3600.0, NOW).unwrap();
        assert!(!r.completed, "the last verse was never reached");
        assert!((r.pct - 0.95).abs() < 1e-6, "still an honest partial, got {}", r.pct);

        // The last verse in view finishes it — dwell is already banked on the pass.
        let r = record(&home, &c, &w, "Gen", 1, 3, 0.0, NOW).unwrap();
        assert!(r.completed, "reaching the bottom completes the banked pass");
        assert_eq!(r.pct, 1.0);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A real read of a short chapter, finished to the last verse, must not land
    /// at Partial. Dwell is stated in ABSOLUTE seconds here, not in the rate's own
    /// terms as every other test does, because this is the test of whether the rate
    /// is fast enough: 295 words read at a familiar ~600 wpm has to clear the bar.
    #[test]
    fn a_fast_read_of_a_short_chapter_is_a_read() {
        let lines = [
            serde_json::to_string(&corpus::corpus_header(canon::TOKENIZATION_VERSION, 5)).unwrap(),
            verse("Gen", 1, 1, 59),
            verse("Gen", 1, 2, 59),
            verse("Gen", 1, 3, 59),
            verse("Gen", 1, 4, 59),
            verse("Gen", 1, 5, 59),
        ];
        let c = corpus::from_str(&lines.join("\n")).unwrap();
        let w = ChapterWords::build(&c);
        let home = scratch("fast-read");
        let wall = 295.0 * 60.0 / 600.0; // 29.5s: the read really happened
        let r = record(&home, &c, &w, "Gen", 1, 5, wall, NOW).unwrap();
        assert!(r.completed, "a 600 wpm read that reached the end is a read, got pct {}", r.pct);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn dwell_accumulates_across_calls() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("accumulate");
        // Gen 2 is 20 words. Sipped in thirds of what those words cost, so the test
        // survives a change to READING_WORDS_PER_MINUTE.
        let third = seconds_for_words(20) / 3.0;
        for (reached, secs) in [(1u16, third), (2, third)] {
            let r = record(&home, &c, &w, "Gen", 2, reached, secs, NOW).unwrap();
            assert!(!r.completed, "two thirds of the words is short of the bar");
        }
        let (store, _) = load(&home);
        let rec = &store["Gen"][0];
        assert!((rec.dwell - third * 2.0).abs() < 1e-3, "seconds carry over between calls");
        assert_eq!(rec.reached, 2);
        // The third sip covers the chapter, with the end already reached.
        let r = record(&home, &c, &w, "Gen", 2, 2, third, NOW).unwrap();
        assert!(r.completed);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn scrolling_back_up_does_not_surrender_ground() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let home = scratch("high-water");
        // Well short of completing (which would legitimately reset `reached`).
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

        // Read most of a chapter but short of the completion bar: Partial, and it
        // must NOT glow — the reader was in it moments ago.
        let r = record(&home, &c, &w, "Gen", 2, 1, seconds_for_words(10), NOW).unwrap();
        assert!(!r.completed, "half of Gen 2 is short of the bar");
        let (store, _) = load(&home);
        let ch = &book_chapters(&c, &w, &store, "Gen", NOW)[1];
        assert_eq!(ch.heat.standing, Standing::Partial);
        assert!(ch.heat.pct > 0.0 && ch.heat.pct < 1.0);
        assert_eq!(ch.heat.glow, 0.0, "just read, so the map says nothing about it");

        // A year later the unfinished half is an invitation again — faded by how
        // much of it is already behind you.
        let later = "2027-07-28T12:00:00Z";
        let ch = &book_chapters(&c, &w, &store, "Gen", later)[1];
        assert_eq!(ch.heat.standing, Standing::Partial);
        let expected = 1.0 - ch.heat.pct;
        assert!(
            (ch.heat.glow - expected).abs() < 1e-6,
            "a long-abandoned partial glows by what is left: want {expected}, got {}",
            ch.heat.glow,
        );

        // A chapter never opened stays lit — nothing to be recent about.
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
        // A record written before `touched` existed: progress banked, no contact
        // date. It must NOT glow — see the upgrade amnesty in `heat_of`.
        store.insert("Gen".into(), vec![ChapterReading { chapter: 1, reached: 4, dwell: 600.0, ..Default::default() }]);
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

        // A little time in it today: the map has nothing to say, even though the
        // last full pass is old.
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
        // Day one and every day after: what you have never read is fully lit.
        for now in ["2026-07-28T12:00:00Z", "2030-01-01T00:00:00Z"] {
            let fresh = book_chapters(&c, &w, &store, "Gen", now);
            assert!(fresh.iter().all(|ch| ch.heat.standing == Standing::Unread));
            assert!(fresh.iter().all(|ch| ch.heat.glow == 1.0), "unread glows at once, at {now}");
            assert!(fresh.iter().all(|ch| ch.heat.days.is_none()), "never read has no last-read day");
        }

        // Part-read: the glow shrinks in proportion to what is LEFT.
        let mut part = Store::new();
        part.insert(
            "Gen".into(),
            // Gen 1 is 10 words over 5 verses; verse 4 (8 words) with ample dwell
            // is 80% covered, so 20% remains. Touched long ago and abandoned, so
            // recency damps nothing.
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
                .map(|ch| ChapterReading { chapter: ch, last_read: Some("2026-07-20".into()), ..Default::default() })
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

        // Corrupt bytes and a file from a newer build. Neither is "nothing read
        // yet" — both are the reader's history, and every write path rewrites the
        // whole file.
        let future = r#"{"format":"plumbline-reading-v9","book":"Gen","chapters":[{"c":1,"lastRead":"2019-01-01"}]}"#;
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
            // `unwrap_or_default`, not `unwrap`: a refused `forget` that in fact
            // went through deletes the file outright, and that must read as a diff
            // rather than a panic about i/o.
            assert_eq!(
                std::fs::read_to_string(&path).unwrap_or_default(),
                content,
                "the bytes on disk must be exactly as the reader left them",
            );
        }

        // Absent still means nothing read yet: a first-ever read must not trip the
        // guard.
        std::fs::remove_file(&path).unwrap();
        assert!(load_book(&home, "Gen").unwrap().is_empty());
        mark_read(&home, "Gen", 1, "2026-05-04").unwrap();
        assert_eq!(load_book(&home, "Gen").unwrap().len(), 1);

        // A file with no chapters, and an empty file, hold nothing to lose.
        for content in [format!(r#"{{"format":"{FORMAT}","book":"Gen","chapters":[]}}"#), String::new()] {
            crate::store::write_atomic(&path, &content).unwrap();
            assert!(load_book(&home, "Gen").unwrap().is_empty(), "empty: {content:?}");
            mark_read(&home, "Gen", 3, "2026-05-04").unwrap();
            assert_eq!(load_book(&home, "Gen").unwrap().len(), 1);
        }

        // `_since.json` is the same story: written once and never again, so
        // stamping a new anchor over an unparseable one is the only chance to lose it.
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

    /// Forward compatibility: the on-disk formats evolve **additively** (CLAUDE.md
    /// §Data formats). Reading one chapter rewrites that book's whole file, so a
    /// dropped key would go from all 150 psalms at once.
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

        // The start anchor is written once and never again, so it has no save to
        // lose a key to — checked anyway, because that is the promise.
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

    // ── the dwell tracker ────────────────────────────────────────────────────

    /// Drive the tracker at one sample a second, the shell's cadence.
    fn seconds(t: &mut DwellTracker, book: &str, chapter: u16, reached: u16, n: usize) -> Vec<DwellReport> {
        (0..n).filter_map(|_| t.tick(Some((book, chapter)), reached, false, 1.0)).collect()
    }

    #[test]
    fn flipping_through_a_book_credits_nothing() {
        let mut t = DwellTracker::default();
        // Two seconds each in ten chapters: inside the grace period every time.
        for c in 1..=10u16 {
            assert!(seconds(&mut t, "Ps", c, 1, 2).is_empty());
        }
        assert_eq!(t.stop(), None, "a flip-through must bank no tail either");
    }

    #[test]
    fn dwell_is_reported_on_the_cores_cadence() {
        let mut t = DwellTracker::default();
        // One sample to arrive (establishes the target, credits nothing), then
        // TICK_SECONDS on screen. Grace is refunded once survived, so 30 seconds in
        // front of the reader is 30 credited and the report lands on the tick.
        let tick = TICK_SECONDS as usize;
        let out = seconds(&mut t, "Gen", 1, 12, 1 + tick);
        assert_eq!(out.len(), 1, "exactly one report per {TICK_SECONDS}s of credited reading");
        assert_eq!(out[0], DwellReport { book: "Gen".into(), chapter: 1, reached: 12, seconds: TICK_SECONDS });
        // And it starts over, rather than reporting every second from here on.
        assert!(seconds(&mut t, "Gen", 1, 12, tick - 1).is_empty());
    }

    #[test]
    fn a_phone_left_on_a_table_stops_reading() {
        let mut t = DwellTracker::default();
        // Ten tick-lengths past the idle cutoff, with nothing touched.
        let n = IDLE_SECONDS as usize + 10 * TICK_SECONDS as usize;
        let mut credited: f32 = seconds(&mut t, "Lev", 11, 4, n).iter().map(|r| r.seconds).sum();
        credited += t.stop().map_or(0.0, |r| r.seconds);
        assert!(credited <= IDLE_SECONDS, "{n}s of staring credited {credited}s");
        assert!(credited > IDLE_SECONDS - GRACE_SECONDS - 2.0, "the reader really was there at first");
    }

    #[test]
    fn a_touch_starts_the_clock_again_without_leaving_the_chapter() {
        let mut t = DwellTracker::default();
        let tick = TICK_SECONDS as usize;
        let banked: f32 =
            seconds(&mut t, "Lev", 11, 4, IDLE_SECONDS as usize + 4 * tick).iter().map(|r| r.seconds).sum();
        assert!(banked > 0.0, "the reader really was there at first");
        // More of the same silence adds nothing at all.
        assert!(seconds(&mut t, "Lev", 11, 4, 2 * tick).is_empty());
        // One touch and the chapter counts again. Grace is NOT re-served — the
        // reader never left.
        t.tick(Some(("Lev", 11)), 4, true, 1.0);
        assert_eq!(seconds(&mut t, "Lev", 11, 4, tick).len(), 1, "a touch must start the clock again");
    }

    /// Fails against: the tail of a chapter handed over with whatever verse the
    /// reader had reached in the chapter they just moved TO, because `reached` was
    /// read at flush time off the live pane.
    #[test]
    fn seconds_are_credited_to_the_chapter_that_earned_them() {
        let mut t = DwellTracker::default();
        seconds(&mut t, "Gen", 1, 5, 13); // arrival + 12s on screen, grace refunded
        let out = t.tick(Some(("Gen", 2)), 40, false, 1.0).expect("the tail of Gen 1");
        assert_eq!(out.book, "Gen");
        assert_eq!(out.chapter, 1);
        assert_eq!(out.reached, 5, "Gen 1 must not be credited with how far Gen 2 got");
        assert_eq!(out.seconds, 12.0);

        // Within a pass it is the HIGH-WATER mark, not wherever the reader happens
        // to be sitting when the seconds are handed over.
        let mut t = DwellTracker::default();
        seconds(&mut t, "Gen", 1, 12, 8);
        seconds(&mut t, "Gen", 1, 3, 4);
        let back = t.stop().expect("the tail");
        assert_eq!(back.reached, 12, "scrolling back up surrendered ground already covered");
    }

    /// Coming back is not continuing: the grace window is re-served on every
    /// re-entry, a dialog opening and closing in the same chapter included.
    #[test]
    fn coming_back_serves_the_grace_window_again() {
        let mut t = DwellTracker::default();
        seconds(&mut t, "Gen", 1, 5, 13);
        assert!(t.stop().is_some(), "the tail is banked on the way out");
        // A bounce back in, inside the re-served window: nothing may be credited,
        // not even on the way back out. The refund must not undermine this refusal.
        assert!(seconds(&mut t, "Gen", 1, 5, 3).is_empty());
        assert_eq!(t.stop(), None, "a bounce credits nothing");
    }

    /// Fails against: the grace window charged as a tax rather than a threshold.
    /// Every dialog and word-study tap re-serves the window, so without the refund
    /// a study-heavy chapter is docked 3s five times over and a finished chapter
    /// lands at Partial.
    #[test]
    fn a_pass_that_survives_grace_is_credited_the_graced_seconds() {
        let mut t = DwellTracker::default();
        // Arrival plus ten seconds. A flip banks nothing; a tax would bank seven.
        seconds(&mut t, "Gen", 1, 5, 11);
        let out = t.stop().expect("ten real seconds");
        assert_eq!(out.seconds, 10.0, "all ten seconds count, not ten minus the window");
        // Interrupted and back: served again (a bounce still credits nothing) and
        // refunded again.
        seconds(&mut t, "Gen", 1, 5, 11);
        let out = t.stop().expect("the second stay");
        assert_eq!(out.seconds, 10.0, "an interruption must not tax the pass");
    }

    #[test]
    fn the_tail_of_a_session_is_banked_not_lost() {
        let mut t = DwellTracker::default();
        seconds(&mut t, "John", 3, 16, 8);
        let out = t.stop().expect("seven credited seconds must survive being stopped");
        assert_eq!((out.book.as_str(), out.chapter, out.seconds), ("John", 3, 7.0));
        assert_eq!(t.stop(), None, "and only once");
    }

    #[test]
    fn a_late_sample_cannot_credit_an_hour() {
        let mut t = DwellTracker::default();
        // Arrive, then get past grace. `interacted` throughout, so it is the clamp
        // under test and not the idle cutoff.
        t.tick(Some(("Gen", 1)), 1, true, MAX_STEP_SECONDS);
        t.tick(Some(("Gen", 1)), 1, true, MAX_STEP_SECONDS);
        // A whole minute in one sample: enough to bank a report on its own, short
        // enough that IDLE_SECONDS is not what refuses it.
        assert_eq!(
            t.tick(Some(("Gen", 1)), 1, true, TICK_SECONDS * 2.0),
            None,
            "one sample can never carry a whole tick's worth",
        );
        // An hour between samples means the shell was not running at all.
        assert_eq!(t.tick(Some(("Gen", 1)), 1, true, 3600.0), None);
        let tail = t.stop().expect("what it did carry").seconds;
        assert!(tail <= 4.0 * MAX_STEP_SECONDS, "two absurd samples banked {tail}s");
    }

    /// NaN out of a shell's own clock arithmetic must credit nothing AND leave
    /// the counters usable — a NaN that reaches `on_screen` compares false
    /// against every threshold, so the tracker would never report again.
    #[test]
    fn a_nan_sample_neither_credits_nor_poisons() {
        let mut t = DwellTracker::default();
        t.tick(Some(("Gen", 1)), 1, true, 1.0);
        assert_eq!(t.tick(Some(("Gen", 1)), 1, true, f32::NAN), None);
        let out = seconds(&mut t, "Gen", 1, 1, GRACE_SECONDS as usize + TICK_SECONDS as usize);
        assert_eq!(out.len(), 1, "reading after a bad sample still counts");
    }

    #[test]
    fn nothing_on_screen_accrues_nothing() {
        let mut t = DwellTracker::default();
        for _ in 0..600 {
            assert_eq!(t.tick(None, 0, true, 1.0), None);
        }
    }

    /// A book file with no unknown keys round-trips byte for byte — these files
    /// already ship inside backup zips.
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
