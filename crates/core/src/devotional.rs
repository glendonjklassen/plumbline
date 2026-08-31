//! Devotionals: a bundled booklet of dated readings, one entry a day.
//!
//! **The catalogue** (`data/devotional.json`, format `devotional-v1`) is authored
//! content that ships with the app — a numbered run of entries, each with a
//! scripture passage, a reflection, and an activity. `scripts/build-devotional.mjs`
//! compiles and validates it, so everything here may assume a well-formed file.
//!
//! References are stored STRUCTURED (`{book:"John", chapter:14, verse:15, end:18}`),
//! never as the string "John 14:15–18", so the shell can render the passage from
//! whichever corpus the reader is in and label it in their own language. An entry
//! may carry more than one range.
//!
//! Text is keyed by language (`texts: {"en": …}`): a translation is a second text on
//! the SAME entry, not a second booklet, so adding one is additive against a frozen
//! format.
//!
//! **A run** (`home/devotionals/<id>.json`, format `plumbline-devotional-run-v1`) is
//! the reader's own data: which booklet, when started, which days finished. One file
//! per running booklet, beside `plans/` and `reading/` in the backup zip.
//!
//! The pacing is **sequence-anchored, one entry a day** — a different model from
//! `plan.rs`:
//!
//!   - "Today's entry" is the lowest day not yet marked done; missing a week skips
//!     no content.
//!   - Completion is DECLARED, not derived. A day is a reflection and an activity,
//!     and nothing observable says those were done — the reader presses Done.
//!   - Having banked a day, the next waits for the next local midnight. This is the
//!     one place the model consults a calendar, and why [`Run::last_done`] exists.
//!
//! The core has no clock: the *reader's own local* `YYYY-MM-DD` arrives from the
//! shell. A UTC date would roll the entry over at the wrong hour for most of the
//! world.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store;
use crate::Error;

/// The catalogue's format tag. Frozen, like every on-disk format here.
pub const FORMAT: &str = "devotional-v1";

/// A run file's format tag. Frozen for the same reason `pure-note-v1` is: these
/// files sit inside shipped backup zips.
pub const RUN_FORMAT: &str = "plumbline-devotional-run-v1";

/// The language every booklet ships in, and the fallback when a reader's
/// language has not been translated yet.
pub const BASE_LANG: &str = "en";

// ── the catalogue ─────────────────────────────────────────────────────────────
//
// camelCase throughout, and load-bearing: a `#[serde(default)]` field whose rename
// does not match the data never errors, it silently reads false — `newBeliever`
// would become a booklet quietly declining to be the new-believer one. Only a test
// over the SHIPPED file catches that (`the_shipped_catalogue_loads`).

/// One passage an entry sits on. `end` absent is a single verse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptureRef {
    /// OSIS book id — `"John"`, `"1Cor"` (canon.rs).
    pub book: String,
    pub chapter: u16,
    pub verse: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<u16>,
}

/// One entry's words, in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryText {
    pub title: String,
    /// The reflection, one string per paragraph.
    pub reflection: Vec<String>,
    pub activity: String,
}

/// One day of a booklet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// 1-based, contiguous across the booklet.
    pub day: u32,
    pub scripture: Vec<ScriptureRef>,
    /// Language code → words. A BTreeMap so iteration order is stable.
    pub texts: BTreeMap<String, EntryText>,
}

/// A titled run of days. The booklet calls these weeks; the format does not, since
/// `from`/`to` are day numbers and a booklet need not be week-shaped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub from: u32,
    pub to: u32,
    pub title: String,
}

/// A booklet's own words, in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookletText {
    /// What every list shows the booklet under.
    pub name: String,
    /// The send-off, shown at the foot of the last day.
    pub closing: Vec<String>,
}

/// One devotional booklet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Devotional {
    pub id: String,
    pub days: u32,
    /// Whether the new-believer welcome starts this booklet automatically. On the
    /// DATA rather than an id in a shell, so shipping a second booklet cannot
    /// change which one a new believer is handed. Absent reads as false — a booklet
    /// opts in, never defaults in.
    #[serde(default)]
    pub new_believer: bool,
    #[serde(default)]
    pub sections: Vec<Section>,
    pub entries: Vec<Entry>,
    pub texts: BTreeMap<String, BookletText>,
}

impl Devotional {
    /// Day `day`'s entry (1-based), if the booklet has one.
    pub fn entry(&self, day: u32) -> Option<&Entry> {
        self.entries.iter().find(|e| e.day == day)
    }

    /// The section `day` falls in, if the booklet is sectioned.
    pub fn section(&self, day: u32) -> Option<&Section> {
        self.sections.iter().find(|s| day >= s.from && day <= s.to)
    }

    /// The booklet's words in `lang`, falling back to the base language.
    pub fn text(&self, lang: &str) -> Option<&BookletText> {
        self.texts.get(lang).or_else(|| self.texts.get(BASE_LANG))
    }

    /// Whether this booklet has been translated into `lang` — an exact hit, not
    /// the fallback. What a language picker would ask.
    pub fn has_lang(&self, lang: &str) -> bool {
        self.texts.contains_key(lang) && self.entries.iter().all(|e| e.texts.contains_key(lang))
    }
}

impl Entry {
    /// This entry's words in `lang`, falling back to the base language, so a
    /// half-translated language reads English rather than a blank page.
    pub fn text(&self, lang: &str) -> Option<&EntryText> {
        self.texts.get(lang).or_else(|| self.texts.get(BASE_LANG))
    }
}

#[derive(Deserialize)]
struct CatalogueDoc {
    format: String,
    devotionals: Vec<Devotional>,
}

/// Load `data/devotional.json`. A missing file is an EMPTY catalogue, not an error
/// (a trimmed home simply offers no devotionals), but one that exists and will not
/// parse — or carries the wrong format tag — is a real error. The `hymnal::load`
/// stance.
pub fn load(path: impl AsRef<Path>) -> Result<Vec<Devotional>, Error> {
    let path = path.as_ref();
    match std::fs::read_to_string(path) {
        Ok(raw) => from_str(&raw),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(Error::Io { path: path.display().to_string(), source: e }),
    }
}

/// Parse a catalogue. Refuses a format-tag mismatch (frozen contract).
pub fn from_str(raw: &str) -> Result<Vec<Devotional>, Error> {
    let doc: CatalogueDoc = serde_json::from_str(raw).map_err(|e| Error::Parse(format!("devotional: {e}")))?;
    if doc.format != FORMAT {
        return Err(Error::Parse(format!("devotional: format {:?}, expected {FORMAT:?}", doc.format)));
    }
    Ok(doc.devotionals)
}

// ── a reader's run ────────────────────────────────────────────────────────────

/// One running booklet. Unknown fields are dropped on the next write, the
/// `overlay-tag-v1` stance. `done` holds 1-based day numbers, sorted, deduped.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub format: String,
    /// The catalogue id this run follows.
    pub id: String,
    pub started: String,
    /// Provenance (I18N.md): stamped at CREATE, never on re-save; absent means
    /// unknown, not English.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub done: Vec<u32>,
    /// The reader's LOCAL date (`YYYY-MM-DD`) a day was last banked — the whole of
    /// the calendar this model keeps, and what holds tomorrow's entry until
    /// tomorrow. `None` until the first Done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_done: Option<String>,
    /// Set aside, kept whole: holds its place and its days, asks nothing — no
    /// chip. Additive, and absent again the moment a run resumes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub paused: bool,
}

impl Run {
    /// A fresh run of `id`, started now.
    pub fn new(id: &str, started: &str, lang: Option<&str>) -> Self {
        Run {
            format: RUN_FORMAT.to_string(),
            id: id.to_string(),
            started: started.to_string(),
            lang: lang.map(str::to_string),
            done: Vec::new(),
            last_done: None,
            paused: false,
        }
    }
}

/// Where a run stands today.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Today {
    /// The 1-based day now open — the lowest not yet banked.
    pub day: u32,
    pub days_done: u32,
    pub days_total: u32,
    /// Whether this entry is offered TODAY. False for the rest of the day after
    /// a Done: the entry is still readable, but nothing invites it (no chip).
    pub available: bool,
}

/// Where `run` stands on the reader's local `today` (`YYYY-MM-DD`), or `None`
/// when every day has been banked — a finished booklet asks nothing.
///
/// `days_total` comes from the catalogue rather than the run file: the booklet
/// is the authority on its own length, and a run outlives any one build of it.
pub fn next_day(run: &Run, days_total: u32, today: &str) -> Option<Today> {
    let mut days_done = 0;
    let mut next: Option<u32> = None;
    for day in 1..=days_total {
        if run.done.contains(&day) {
            days_done += 1;
        } else if next.is_none() {
            next = Some(day);
        }
    }
    next.map(|day| Today {
        day,
        days_done,
        days_total,
        // One entry a day: having banked one today, the next waits for midnight. A
        // clock that moved BACKWARDS reads as "not today" and unlocks — permissive
        // on purpose, since locking a reader out of their booklet is the failure
        // that matters.
        available: run.last_done.as_deref() != Some(today),
    })
}

/// Bank day `day` (1-based) on the reader's local `today`. Returns whether it
/// was new — a second Done on a day already banked changes nothing, including
/// the date, so it cannot be used to push tomorrow's entry further away.
pub fn mark_done(run: &mut Run, day: u32, today: &str) -> bool {
    match run.done.binary_search(&day) {
        Ok(_) => false,
        Err(at) => {
            run.done.insert(at, day);
            run.last_done = Some(today.to_string());
            true
        }
    }
}

// ── the store ─────────────────────────────────────────────────────────────────

fn runs_dir(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref().join("devotionals")
}

fn run_path(home: impl AsRef<Path>, id: &str) -> PathBuf {
    runs_dir(home).join(format!("{}.json", store::slug(id, "devotional")))
}

/// Every running booklet, plus one message per file that would not load. A damaged
/// file is reported and left where it lies, never overwritten (the `reading` stance).
pub fn load_runs(home: impl AsRef<Path>) -> (Vec<Run>, Vec<String>) {
    let mut runs = Vec::new();
    let mut errs = Vec::new();
    let entries = match std::fs::read_dir(runs_dir(&home)) {
        Ok(e) => e,
        Err(_) => return (runs, errs), // no dir yet: nothing running, not an error
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "json")
                && !p.file_name().and_then(|n| n.to_str()).is_some_and(store::is_temp_name)
        })
        .collect();
    files.sort(); // deterministic order, whatever the filesystem says
    for path in files {
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Run>(&s).map_err(|e| e.to_string()))
        {
            Ok(r) if r.format == RUN_FORMAT => runs.push(r),
            Ok(r) => errs.push(format!("{}: unknown format {:?}", path.display(), r.format)),
            Err(e) => errs.push(format!("{}: {e}", path.display())),
        }
    }
    (runs, errs)
}

/// Write (or rewrite) a run file atomically.
pub fn write_run(home: impl AsRef<Path>, run: &Run) -> Result<(), Error> {
    let dir = runs_dir(&home);
    std::fs::create_dir_all(&dir).map_err(|e| Error::Io { path: dir.display().to_string(), source: e })?;
    let json = serde_json::to_string(run).map_err(|e| Error::Parse(e.to_string()))?;
    store::write_atomic(run_path(&home, &run.id), &json)
}

/// Remove a run's file. An absent run is a no-op, not an error — the
/// `remove_plan` stance.
pub fn remove_run(home: impl AsRef<Path>, id: &str) -> Result<bool, Error> {
    let path = run_path(&home, id);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Error::Io { path: path.display().to_string(), source: e }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: &str = r#"{"format":"devotional-v1","devotionals":[{
        "id":"sample","days":3,
        "sections":[{"from":1,"to":2,"title":"First"},{"from":3,"to":3,"title":"Second"}],
        "entries":[
          {"day":1,"scripture":[{"book":"John","chapter":3,"verse":16,"end":21}],
           "texts":{"en":{"title":"One","reflection":["a","b"],"activity":"do"}}},
          {"day":2,"scripture":[{"book":"John","chapter":14,"verse":15,"end":18},
                                {"book":"John","chapter":14,"verse":25,"end":27}],
           "texts":{"en":{"title":"Two","reflection":["c"],"activity":"do"},
                    "de":{"title":"Zwei","reflection":["c"],"activity":"tu"}}},
          {"day":3,"scripture":[{"book":"Luke","chapter":5,"verse":16}],
           "texts":{"en":{"title":"Three","reflection":["d"],"activity":"do"}}}],
        "texts":{"en":{"name":"Sample","closing":["bye"]}}}]}"#;

    fn sample() -> Devotional {
        from_str(ONE).unwrap().remove(0)
    }

    #[test]
    fn parses_a_catalogue() {
        let d = sample();
        assert_eq!(d.id, "sample");
        assert_eq!(d.entries.len(), 3);
        // A single verse keeps `end` absent; a multi-range day keeps both.
        assert_eq!(d.entry(3).unwrap().scripture[0].end, None);
        assert_eq!(d.entry(2).unwrap().scripture.len(), 2);
    }

    #[test]
    fn refuses_a_foreign_format_tag() {
        let raw = ONE.replace("devotional-v1", "devotional-v9");
        assert!(from_str(&raw).is_err());
    }

    #[test]
    fn a_missing_catalogue_is_empty_not_an_error() {
        assert_eq!(load("/nonexistent/devotional.json").unwrap(), Vec::new());
    }

    #[test]
    fn sections_and_entries_are_found_by_day() {
        let d = sample();
        assert_eq!(d.section(1).unwrap().title, "First");
        assert_eq!(d.section(2).unwrap().title, "First");
        assert_eq!(d.section(3).unwrap().title, "Second");
        assert!(d.section(4).is_none());
        assert!(d.entry(4).is_none());
    }

    #[test]
    fn an_untranslated_entry_falls_back_to_english() {
        let d = sample();
        // Day 2 has German; day 1 does not, and reads English rather than blank.
        assert_eq!(d.entry(2).unwrap().text("de").unwrap().title, "Zwei");
        assert_eq!(d.entry(1).unwrap().text("de").unwrap().title, "One");
        // …and the booklet is NOT advertised as translated on that basis.
        assert!(d.has_lang("en"));
        assert!(!d.has_lang("de"));
    }

    #[test]
    fn a_fresh_run_opens_on_day_one() {
        let run = Run::new("sample", "2026-08-26T10:00:00Z", Some("en"));
        let t = next_day(&run, 3, "2026-08-26").unwrap();
        assert_eq!((t.day, t.days_done, t.days_total), (1, 0, 3));
        assert!(t.available, "nothing has been banked, so today's entry is on offer");
    }

    /// The pacing rule in one pass: Done banks the day, the next entry is held back
    /// for the rest of the calendar day, and offered again at the next local midnight.
    #[test]
    fn the_next_entry_waits_for_the_next_local_day() {
        let mut run = Run::new("sample", "2026-08-26T10:00:00Z", Some("en"));
        assert!(mark_done(&mut run, 1, "2026-08-26"));

        let same_day = next_day(&run, 3, "2026-08-26").unwrap();
        assert_eq!(same_day.day, 2, "the entry advances immediately — it is only the INVITATION that waits");
        assert_eq!(same_day.days_done, 1);
        assert!(!same_day.available, "a day was already banked today");

        let tomorrow = next_day(&run, 3, "2026-08-27").unwrap();
        assert_eq!(tomorrow.day, 2);
        assert!(tomorrow.available);
    }

    #[test]
    fn banking_a_day_twice_cannot_push_tomorrow_further_away() {
        let mut run = Run::new("sample", "2026-08-26T10:00:00Z", Some("en"));
        assert!(mark_done(&mut run, 1, "2026-08-26"));
        // The same Done arriving the NEXT day (a stale tab, a tap across midnight)
        // must not re-stamp the date and hold day 2 back again.
        assert!(!mark_done(&mut run, 1, "2026-08-27"));
        assert_eq!(run.last_done.as_deref(), Some("2026-08-26"));
        assert!(next_day(&run, 3, "2026-08-27").unwrap().available);
        assert_eq!(run.done, vec![1]);
    }

    #[test]
    fn days_banked_out_of_order_still_advance_to_the_lowest_gap() {
        let mut run = Run::new("sample", "2026-08-26T10:00:00Z", Some("en"));
        mark_done(&mut run, 2, "2026-08-26");
        let t = next_day(&run, 3, "2026-08-27").unwrap();
        assert_eq!(t.day, 1, "the lowest unbanked day is what is open");
        assert_eq!(t.days_done, 1);
    }

    #[test]
    fn a_finished_booklet_asks_nothing() {
        let mut run = Run::new("sample", "2026-08-26T10:00:00Z", Some("en"));
        for day in 1..=3 {
            mark_done(&mut run, day, "2026-08-26");
        }
        assert!(next_day(&run, 3, "2026-08-30").is_none());
    }

    /// A clock that moved backwards must not lock a reader out of their booklet.
    #[test]
    fn a_backwards_clock_unlocks_rather_than_locking_out() {
        let mut run = Run::new("sample", "2026-08-26T10:00:00Z", Some("en"));
        mark_done(&mut run, 1, "2026-08-26");
        assert!(next_day(&run, 3, "2026-08-25").unwrap().available);
    }

    #[test]
    fn a_run_round_trips_through_the_store() {
        let home = std::env::temp_dir().join(format!("plumbline-dev-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let mut run = Run::new("sample", "2026-08-26T10:00:00Z", Some("en"));
        mark_done(&mut run, 1, "2026-08-26");
        write_run(&home, &run).unwrap();

        let (loaded, errs) = load_runs(&home);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(loaded, vec![run]);

        assert!(remove_run(&home, "sample").unwrap());
        assert!(!remove_run(&home, "sample").unwrap(), "removing an absent run is a no-op");
        assert!(load_runs(&home).0.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_run_file_of_a_foreign_format_is_reported_not_loaded() {
        let home = std::env::temp_dir().join(format!("plumbline-dev-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("devotionals")).unwrap();
        std::fs::write(
            home.join("devotionals/sample.json"),
            r#"{"format":"plumbline-devotional-run-v9","id":"sample","started":"s"}"#,
        )
        .unwrap();
        let (runs, errs) = load_runs(&home);
        assert!(runs.is_empty());
        assert_eq!(errs.len(), 1, "the damaged file is reported, and left where it lies");
        assert!(home.join("devotionals/sample.json").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The SHIPPED catalogue, not a fixture — the only test that would notice
    /// `scripts/build-devotional.mjs` and this loader disagreeing.
    #[test]
    fn the_shipped_catalogue_loads() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = repo.join("data/devotional.json");
        if !path.exists() {
            println!("no data pack; skipping");
            return;
        }
        let all = load(&path).unwrap();
        assert!(!all.is_empty());
        // The new-believer flag is `#[serde(default)]`, so a rename mismatch does
        // not error — it silently reads false and the welcome starts nothing.
        assert_eq!(
            all.iter().filter(|d| d.new_believer).count(),
            1,
            "exactly one shipped booklet must be the new-believer one"
        );
        for d in &all {
            assert_eq!(d.entries.len() as u32, d.days);
            assert!(d.text(BASE_LANG).is_some(), "{}: no base-language name", d.id);
            for day in 1..=d.days {
                let e = d.entry(day).unwrap_or_else(|| panic!("{}: no day {day}", d.id));
                assert!(!e.scripture.is_empty());
                assert!(e.text(BASE_LANG).is_some());
                assert!(d.section(day).is_some(), "{}: day {day} is in no section", d.id);
            }
        }
    }
}
