//! Reading plans + the concept study. [docs/READING-PLANS.md] is the contract; the
//! decisions live there, this header only orients.
//!
//! A **schedule plan** is a word-weighted walk of a scope (whole canon, NT, a
//! book list, or a curated chapter table) cut into days. Pacing is
//! **sequence-anchored**: "today" is the lowest unfinished day, and absence
//! accrues no backlog. Whether a day is done is **derived from the reading
//! records** (`core::reading`) — a day completes when every chapter it names
//! stands `Read` — and cached into the plan file's `done` list so a day
//! honoured once stays honoured even if the reading record underneath it is
//! later cleared.
//!
//! A **concept study** is not a schedule at all: a non-linear concept sweep
//! with a preset tag. It records which chapters have been swept (no dwell gate
//! — the reading tracker is OFF in concept-study mode, shell-side) and its
//! progress is swept-over-scope. Ending a concept study never touches the tag
//! or its members — what was gathered is the point.
//!
//! Schedules are **generated, not stored**: the plan file keeps the generator's
//! parameters (or a curated table's id) and the walk is deterministic given the
//! corpus, so re-running it is cheaper and safer than trusting a materialized
//! copy to stay in step with anything. One JSON file per running plan under
//! `home/plans/` — personal study data, in the backup zip beside `reading/`.
//!
//! **Class exclusivity** is a query, not a gate: [`class_conflict`] names the
//! running plan that occupies a class, the shell confirms the replacement, and
//! only then stops one and starts the other. The core never guesses at intent.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::canon;
use crate::reading::ChapterWords;
use crate::reference::OT_NT_DIVIDE;
use crate::store;
use crate::Error;

/// Serialized into every plan file. Frozen, like every on-disk format tag.
pub const FORMAT: &str = "plumbline-plan-v1";

/// The exclusivity classes of the built-in schedules. A reader runs at most one
/// plan per class — two "get through the whole thing" plans at once is a
/// contradiction, while NT-in-90 beside Psalms+Proverbs is the point.
pub const CLASS_WHOLE_BIBLE: &str = "wholeBible";
pub const CLASS_NEW_TESTAMENT: &str = "newTestament";
pub const CLASS_DEVOTIONAL: &str = "devotional";

/// What kind of plan a file describes. The serialized names are frozen with
/// the rest of the format: `"schedule"` and `"conceptStudy"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Schedule,
    ConceptStudy,
}

/// What a generated schedule walks. Serialized flat into the generator object:
/// `{"scope":"canon"}` / `{"scope":"nt"}` / `{"scope":"books","books":["Ps","Prov"]}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "camelCase")]
pub enum Scope {
    Canon,
    Nt,
    Books { books: Vec<String> },
}

/// The parameters a schedule is regenerated from on every load.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Generator {
    #[serde(flatten)]
    pub scope: Scope,
    pub days: u32,
}

/// One running plan — the whole on-disk file.
///
/// Additive evolution only: unknown fields are read past (serde default) and
/// dropped on the next write, the `overlay-tag-v1` stance. `done` holds
/// **1-based day numbers**, sorted, deduped; days complete out of order and
/// stay recorded. `swept` is the concept study's coverage, chapters sorted per book.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub format: String,
    pub id: String,
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<Generator>,
    /// A curated table's id (e.g. `"chronological"`) instead of a generator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    pub started: String,
    /// Provenance (I18N.md): stamped at CREATE, never on re-save; absent means
    /// unknown, not English.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub done: Vec<u32>,
    /// Concept study only: the preset tag a tapped verse is filed under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Concept study only: swept chapters, `book id → sorted chapter numbers`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub swept: BTreeMap<String, Vec<u16>>,
}

/// A built-in plan the picker offers. `name_key` is an i18n catalogue id —
/// every word the reader sees is core data.
pub struct Builtin {
    pub id: &'static str,
    pub name_key: &'static str,
    pub class: &'static str,
    pub kind: Kind,
    pub generator: Option<Generator>,
    pub table: Option<&'static str>,
}

/// The lineup the picker offers. Every entry here must be START-ABLE into a
/// non-empty schedule TODAY: a builtin that produces an empty schedule renders
/// as instantly "finished". A generator plan always can; a TABLE plan can only
/// where its table file is present, so the FFI offers a table row after
/// [`load_table`] answers — a home missing `data/chronological.json` hides the
/// row rather than offering a plan that starts finished.
pub fn builtins() -> Vec<Builtin> {
    let gen = |scope: Scope, days: u32| Some(Generator { scope, days });
    vec![
        Builtin {
            id: "bible-365",
            name_key: "plans.bible365",
            class: CLASS_WHOLE_BIBLE,
            kind: Kind::Schedule,
            generator: gen(Scope::Canon, 365),
            table: None,
        },
        Builtin {
            id: "bible-180",
            name_key: "plans.bible180",
            class: CLASS_WHOLE_BIBLE,
            kind: Kind::Schedule,
            generator: gen(Scope::Canon, 180),
            table: None,
        },
        Builtin {
            id: "bible-90",
            name_key: "plans.bible90",
            class: CLASS_WHOLE_BIBLE,
            kind: Kind::Schedule,
            generator: gen(Scope::Canon, 90),
            table: None,
        },
        Builtin {
            id: "nt-90",
            name_key: "plans.nt90",
            class: CLASS_NEW_TESTAMENT,
            kind: Kind::Schedule,
            generator: gen(Scope::Nt, 90),
            table: None,
        },
        Builtin {
            id: "chronological",
            name_key: "plans.chronological",
            class: CLASS_WHOLE_BIBLE,
            kind: Kind::Schedule,
            generator: None,
            table: Some("chronological"),
        },
        Builtin {
            id: "psalms-proverbs-30",
            name_key: "plans.psalmsProverbs30",
            class: CLASS_DEVOTIONAL,
            kind: Kind::Schedule,
            generator: gen(Scope::Books { books: vec!["Ps".into(), "Prov".into()] }, 30),
            table: None,
        },
    ]
}

// ── curated tables ────────────────────────────────────────────────────────────

/// Serialized into every table file (`data/<id>.json`). Frozen like the rest.
pub const TABLE_FORMAT: &str = "plumbline-plan-table-v1";

/// A curated plan table, loaded: the ordered chapter walk and the day count the
/// schedule cuts it into. Chronological is the one shipped
/// (`scripts/build-chronological.mjs` compiles it from
/// `data-prep/chronological/order.json`, verifying exactly-once canon coverage).
pub struct PlanTable {
    pub days: u32,
    pub order: Vec<(String, u16)>,
}

/// Read a curated table from the pack (`data/<id>.json`). `None` for anything
/// short of a well-formed table — absent file, wrong format tag, an unknown
/// book, an inverted span — because a damaged table must hide its plan rather
/// than offer one with a hole in it. Chapter numbers are checked against the
/// CORPUS by the caller (chapter counts live there, not in the canon table);
/// exactly-once canon coverage is the build script's guarantee.
pub fn load_table(home: impl AsRef<Path>, id: &str) -> Option<PlanTable> {
    #[derive(Deserialize)]
    struct WireTable {
        format: String,
        days: u32,
        segments: Vec<(String, u16, u16)>,
    }
    let path = home.as_ref().join("data").join(format!("{}.json", store::slug(id, "table")));
    let s = std::fs::read_to_string(path).ok()?;
    let t: WireTable = serde_json::from_str(&s).ok()?;
    if t.format != TABLE_FORMAT || t.days == 0 || t.segments.is_empty() {
        return None;
    }
    let mut order = Vec::new();
    for (book, first, last) in t.segments {
        if canon::book_by_id(&book).is_none() || first < 1 || last < first {
            return None;
        }
        for c in first..=last {
            order.push((book.clone(), c));
        }
    }
    Some(PlanTable { days: t.days, order })
}

// ── the schedule walk ─────────────────────────────────────────────────────────

/// The chapters a scope names, in canon order (a `Books` list is re-ordered to
/// canon order whatever order it was written in — a plan is read in reading
/// order). Books the corpus does not know are skipped rather than erred: the
/// German corpus shares the KJV's addresses, so in practice nothing skips.
pub fn scope_chapters(scope: &Scope, words: &ChapterWords) -> Vec<(String, u16)> {
    let books: Vec<&str> = match scope {
        Scope::Canon => canon::BOOKS.iter().map(|b| b.id).collect(),
        Scope::Nt => canon::BOOKS[OT_NT_DIVIDE..].iter().map(|b| b.id).collect(),
        Scope::Books { books } => {
            canon::BOOKS.iter().map(|b| b.id).filter(|id| books.iter().any(|w| w == id)).collect()
        }
    };
    let mut out = Vec::new();
    for b in books {
        for c in 1..=words.chapters(b) {
            out.push((b.to_string(), c));
        }
    }
    out
}

/// Cut an ordered chapter walk into `days` word-balanced days.
///
/// Greedy against cumulative word boundaries, with two guarantees that matter
/// more than perfect balance: every chapter appears exactly once (chapters are
/// never split), and **no day is empty** — `days` clamps to the chapter count,
/// and when the chapters left equal the days left, each remaining day takes
/// exactly one. Deterministic given the corpus, which is what lets the plan
/// file store parameters instead of the schedule.
pub fn schedule(order: &[(String, u16)], words: &ChapterWords, days: u32) -> Vec<Vec<(String, u16)>> {
    let n = order.len();
    if n == 0 {
        return Vec::new();
    }
    let days = (days.max(1) as usize).min(n);
    // `.max(1)` per chapter: a wordless chapter (impossible in shipped data,
    // cheap to guard) must still advance the walk.
    let total: u64 = order.iter().map(|(b, c)| u64::from(words.words(b, *c).max(1))).sum();

    let mut out: Vec<Vec<(String, u16)>> = Vec::with_capacity(days);
    let mut cur: Vec<(String, u16)> = Vec::new();
    let mut cum: u64 = 0;
    for (i, (b, c)) in order.iter().enumerate() {
        cur.push((b.clone(), *c));
        cum += u64::from(words.words(b, *c).max(1));
        let day = out.len();
        if day + 1 < days {
            let chapters_left = n - i - 1;
            let days_left = days - day - 1;
            let boundary = total * (day as u64 + 1) / days as u64;
            if cum >= boundary || chapters_left == days_left {
                out.push(std::mem::take(&mut cur));
            }
        }
    }
    out.push(cur);
    out
}

// ── where the plan stands ─────────────────────────────────────────────────────

/// The day card: 1-based day number, its chapters, and the plan's shape.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Today {
    pub day: u32,
    pub chapters: Vec<(String, u16)>,
    pub days_done: u32,
    pub days_total: u32,
}

/// Whether day `day` (1-based) of `sched` is done: cached in the plan, or
/// derived — every chapter of the day reads back `Read` from the tracker.
fn day_done(plan: &Plan, sched: &[Vec<(String, u16)>], day: u32, is_read: &impl Fn(&str, u16) -> bool) -> bool {
    if plan.done.contains(&day) {
        return true;
    }
    sched.get(day as usize - 1).is_some_and(|chs| !chs.is_empty() && chs.iter().all(|(b, c)| is_read(b, *c)))
}

/// Sequence-anchored "today": the lowest unfinished day, or `None` when the
/// plan is complete. `is_read` is the reading store's answer for one chapter —
/// the derivation IS the progress model (decision #2), so this takes the
/// closure rather than duplicating any reading logic here.
pub fn next_day(plan: &Plan, sched: &[Vec<(String, u16)>], is_read: impl Fn(&str, u16) -> bool) -> Option<Today> {
    let total = sched.len() as u32;
    let mut days_done = 0;
    let mut next: Option<u32> = None;
    for d in 1..=total {
        if day_done(plan, sched, d, &is_read) {
            days_done += 1;
        } else if next.is_none() {
            next = Some(d);
        }
    }
    next.map(|day| Today { day, chapters: sched[day as usize - 1].clone(), days_done, days_total: total })
}

/// Record a day as done (1-based; sorted, deduped). Returns whether it was new.
/// The caller persists — this is the cache [`next_day`] consults so a day never
/// un-completes when the reading record under it is later cleared.
pub fn mark_done(plan: &mut Plan, day: u32) -> bool {
    match plan.done.binary_search(&day) {
        Ok(_) => false,
        Err(at) => {
            plan.done.insert(at, day);
            true
        }
    }
}

// ── the concept study's coverage ──────────────────────────────────────────────

/// Mark a chapter swept. Returns whether it was new. No dwell, no order — the
/// generosity is the design (docs/READING-PLANS.md §Concept Study).
pub fn sweep(plan: &mut Plan, book: &str, chapter: u16) -> bool {
    let chs = plan.swept.entry(book.to_string()).or_default();
    match chs.binary_search(&chapter) {
        Ok(_) => false,
        Err(at) => {
            chs.insert(at, chapter);
            true
        }
    }
}

pub fn is_swept(plan: &Plan, book: &str, chapter: u16) -> bool {
    plan.swept.get(book).is_some_and(|chs| chs.binary_search(&chapter).is_ok())
}

/// Swept chapters over the scope's total — the concept study's progress pair.
pub fn sweep_progress(plan: &Plan, scope_total: usize) -> (usize, usize) {
    (plan.swept.values().map(Vec::len).sum(), scope_total)
}

// ── the store ─────────────────────────────────────────────────────────────────

fn plans_dir(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref().join("plans")
}

fn plan_path(home: impl AsRef<Path>, id: &str) -> PathBuf {
    plans_dir(home).join(format!("{}.json", store::slug(id, "plan")))
}

/// Every running plan, plus one message per file that would not load. A damaged
/// file is reported and left where it lies — never overwritten (the `reading`
/// stance), because the reader's plan history is theirs, not ours.
pub fn load_plans(home: impl AsRef<Path>) -> (Vec<Plan>, Vec<String>) {
    let mut plans = Vec::new();
    let mut errs = Vec::new();
    let dir = plans_dir(&home);
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return (plans, errs), // no dir yet: no plans, not an error
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
            .and_then(|s| serde_json::from_str::<Plan>(&s).map_err(|e| e.to_string()))
        {
            Ok(p) if p.format == FORMAT => plans.push(p),
            Ok(p) => errs.push(format!("{}: unknown format {:?}", path.display(), p.format)),
            Err(e) => errs.push(format!("{}: {e}", path.display())),
        }
    }
    (plans, errs)
}

/// The running plan that already occupies `class`, if any — the shell confirms
/// the replacement before stopping it. A query, not a gate: the core never
/// guesses at intent.
pub fn class_conflict<'a>(loaded: &'a [Plan], class: &str) -> Option<&'a Plan> {
    loaded.iter().find(|p| p.class.as_deref() == Some(class))
}

/// Write (or rewrite) a plan file atomically.
pub fn write_plan(home: impl AsRef<Path>, plan: &Plan) -> Result<(), Error> {
    let dir = plans_dir(&home);
    std::fs::create_dir_all(&dir).map_err(|e| Error::Io { path: dir.display().to_string(), source: e })?;
    let json = serde_json::to_string(plan).map_err(|e| Error::Parse(e.to_string()))?;
    store::write_atomic(plan_path(&home, &plan.id), &json)
}

/// Remove a plan's file. An absent plan is a no-op, not an error — the
/// `remove_thread` stance.
pub fn remove_plan(home: impl AsRef<Path>, id: &str) -> Result<bool, Error> {
    let path = plan_path(&home, id);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Error::Io { path: path.display().to_string(), source: e }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;
    use std::path::PathBuf;

    /// Gen 1: 5×2 words. Gen 2: 2×10. Gen 3: 1×5. (reading.rs's toy shape.)
    fn toy() -> corpus::Corpus {
        fn verse(b: &str, c: u16, v: u16, words: usize) -> String {
            let toks: Vec<String> = (0..words).map(|i| format!(r#"["","w{i}","",[],0]"#)).collect();
            format!(r#"{{"b":"{b}","c":{c},"v":{v},"t":[{}]}}"#, toks.join(","))
        }
        let mut lines = vec![serde_json::to_string(&corpus::corpus_header(canon::TOKENIZATION_VERSION, 8)).unwrap()];
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
        let d = std::env::temp_dir().join(format!("plumbline-plan-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn plan(id: &str, class: Option<&str>) -> Plan {
        Plan {
            format: FORMAT.into(),
            id: id.into(),
            kind: Kind::Schedule,
            class: class.map(Into::into),
            generator: Some(Generator { scope: Scope::Canon, days: 3 }),
            table: None,
            started: "2026-08-08T12:00:00Z".into(),
            lang: Some("en".into()),
            done: Vec::new(),
            tag: None,
            swept: BTreeMap::new(),
        }
    }

    #[test]
    fn a_schedule_covers_every_chapter_once_with_no_empty_day() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let order = scope_chapters(&Scope::Canon, &w);
        assert_eq!(order.len(), 3, "the toy corpus has three chapters");
        for days in 1..=5u32 {
            let s = schedule(&order, &w, days);
            assert_eq!(s.len(), days.min(3) as usize, "days clamp to the chapter count");
            assert!(s.iter().all(|d| !d.is_empty()), "no day may be empty at days={days}");
            let flat: Vec<_> = s.iter().flatten().cloned().collect();
            assert_eq!(flat, order, "every chapter exactly once, in order (days={days})");
        }
    }

    #[test]
    fn the_walk_balances_by_words_not_chapters() {
        let c = toy();
        let w = ChapterWords::build(&c);
        // 35 words over 2 days → the boundary sits at 17.5. Gen 1 (10) alone is
        // short of it, Gen 1+2 (30) crosses it — so day 1 is two chapters and
        // day 2 is one, where a chapter-count split would have put the heavier
        // Gen 2 alone on either side.
        let s = schedule(&scope_chapters(&Scope::Canon, &w), &w, 2);
        assert_eq!(s[0].len(), 2, "day 1 runs to the word boundary");
        assert_eq!(s[1], vec![("Gen".to_string(), 3)]);
    }

    #[test]
    fn a_books_scope_reads_in_canon_order_whatever_order_it_was_written() {
        let c = toy();
        let w = ChapterWords::build(&c);
        // Only Gen exists in the toy corpus; an unknown book contributes nothing.
        let order = scope_chapters(&Scope::Books { books: vec!["Rev".into(), "Gen".into()] }, &w);
        assert_eq!(order.len(), 3);
        assert!(order.iter().all(|(b, _)| b == "Gen"));
    }

    #[test]
    fn next_day_derives_from_the_reader_not_the_calendar() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let sched = schedule(&scope_chapters(&Scope::Canon, &w), &w, 3);
        let p = plan("bible-3", Some(CLASS_WHOLE_BIBLE));

        // Nothing read: today is day 1.
        let t = next_day(&p, &sched, |_, _| false).unwrap();
        assert_eq!((t.day, t.days_done, t.days_total), (1, 0, 3));

        // Day 1's chapter read → today is day 2, and no calendar was consulted.
        let t = next_day(&p, &sched, |b, ch| (b, ch) == ("Gen", 1)).unwrap();
        assert_eq!((t.day, t.days_done), (2, 1));

        // Days complete OUT OF ORDER: day 3 read while day 2 is not — today is
        // still day 2 (sequence-anchored), and both ends count as done.
        let t = next_day(&p, &sched, |b, ch| (b, ch) == ("Gen", 1) || (b, ch) == ("Gen", 3)).unwrap();
        assert_eq!((t.day, t.days_done), (2, 2));

        // Everything read → the plan is finished.
        assert!(next_day(&p, &sched, |_, _| true).is_none());
    }

    #[test]
    fn a_cached_done_day_survives_its_reading_record_being_cleared() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let sched = schedule(&scope_chapters(&Scope::Canon, &w), &w, 3);
        let mut p = plan("bible-3", Some(CLASS_WHOLE_BIBLE));
        assert!(mark_done(&mut p, 1));
        assert!(!mark_done(&mut p, 1), "recording twice is a no-op");
        // The tracker now denies everything (record cleared) — day 1 stays done.
        let t = next_day(&p, &sched, |_, _| false).unwrap();
        assert_eq!((t.day, t.days_done), (2, 1));
    }

    #[test]
    fn sweep_records_once_and_progress_counts_the_scope() {
        let mut p = plan("run-grace", None);
        p.kind = Kind::ConceptStudy;
        p.tag = Some("grace".into());
        assert!(sweep(&mut p, "Gen", 2));
        assert!(!sweep(&mut p, "Gen", 2), "sweeping twice is a no-op");
        assert!(sweep(&mut p, "Gen", 1));
        assert!(is_swept(&p, "Gen", 1) && !is_swept(&p, "Gen", 3));
        assert_eq!(sweep_progress(&p, 3), (2, 3));
        assert_eq!(p.swept["Gen"], vec![1, 2], "chapters stay sorted");
    }

    #[test]
    fn the_store_round_trips_and_reports_damage_without_destroying_it() {
        let home = scratch("store");
        let mut p = plan("bible-365", Some(CLASS_WHOLE_BIBLE));
        mark_done(&mut p, 4);
        write_plan(&home, &p).unwrap();

        // A second, classless concept study beside it.
        let mut run = plan("run-grace", None);
        run.kind = Kind::ConceptStudy;
        run.tag = Some("grace".into());
        write_plan(&home, &run).unwrap();

        // And one damaged file, which must be reported, kept, and skipped.
        std::fs::write(plans_dir(&home).join("broken.json"), "{not json").unwrap();

        let (plans, errs) = load_plans(&home);
        assert_eq!(plans.len(), 2);
        assert_eq!(errs.len(), 1, "the damaged file is one report: {errs:?}");
        assert!(std::fs::read_to_string(plans_dir(&home).join("broken.json")).is_ok(), "never overwritten");
        assert_eq!(plans.iter().find(|q| q.id == "bible-365").unwrap(), &p, "round-trips whole");

        // Class conflict is a query the shell asks before replacing.
        assert_eq!(class_conflict(&plans, CLASS_WHOLE_BIBLE).unwrap().id, "bible-365");
        assert!(class_conflict(&plans, CLASS_DEVOTIONAL).is_none());

        // Stop: removes the file; absent is a no-op, not an error.
        assert!(remove_plan(&home, "bible-365").unwrap());
        assert!(!remove_plan(&home, "bible-365").unwrap());
        let (plans, _) = load_plans(&home);
        assert_eq!(plans.len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_file_from_a_future_version_is_read_past_not_lost() {
        let home = scratch("future");
        std::fs::create_dir_all(plans_dir(&home)).unwrap();
        // Unknown fields (additive evolution) survive a load; an unknown FORMAT
        // is refused with a report, because its semantics are unknowable.
        std::fs::write(
            plan_path(&home, "bible-365"),
            r#"{"format":"plumbline-plan-v1","id":"bible-365","kind":"schedule","class":"wholeBible",
                "generator":{"scope":"canon","days":365},"started":"2026-08-08T12:00:00Z",
                "done":[1],"futureField":{"x":1}}"#,
        )
        .unwrap();
        std::fs::write(
            plan_path(&home, "vNext"),
            r#"{"format":"plumbline-plan-v9","id":"vNext","kind":"schedule","started":"s"}"#,
        )
        .unwrap();
        let (plans, errs) = load_plans(&home);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].done, vec![1]);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("plumbline-plan-v9"), "{errs:?}");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn builtins_are_wired_for_what_the_picker_needs() {
        let all = builtins();
        // Every builtin has exactly one of generator/table, and the classes
        // divide the lineup the way decision #3 says.
        for b in &all {
            assert!(b.generator.is_some() != b.table.is_some(), "{}: generator XOR table", b.id);
        }
        let whole: Vec<_> = all.iter().filter(|b| b.class == CLASS_WHOLE_BIBLE).map(|b| b.id).collect();
        // Chronological joined the lineup with its shipped table (decision #4);
        // the FFI still only OFFERS it where the table actually loads.
        assert_eq!(whole, vec!["bible-365", "bible-180", "bible-90", "chronological"]);
        let chrono = all.iter().find(|b| b.id == "chronological").unwrap();
        assert_eq!(chrono.table, Some("chronological"));
        assert!(all.iter().any(|b| b.class == CLASS_NEW_TESTAMENT));
        assert!(all.iter().any(|b| b.class == CLASS_DEVOTIONAL));
    }

    #[test]
    fn a_curated_table_loads_expands_and_refuses_damage() {
        let home = std::env::temp_dir().join(format!("plumbline-plan-table-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        let write = |json: &str| std::fs::write(home.join("data").join("chronological.json"), json).unwrap();

        // Absent: hidden, not an error.
        assert!(load_table(&home, "chronological").is_none());

        // Well-formed: segments expand in order, days carried through.
        write(r#"{"format":"plumbline-plan-table-v1","id":"chronological","days":3,"segments":[["Gen",1,2],["Job",1,1],["Gen",3,3]]}"#);
        let t = load_table(&home, "chronological").unwrap();
        assert_eq!(t.days, 3);
        assert_eq!(
            t.order,
            vec![("Gen".into(), 1), ("Gen".into(), 2), ("Job".into(), 1), ("Gen".into(), 3)]
        );

        // Damage hides the plan rather than shipping a hole: wrong format tag,
        // an unknown book, an inverted span, an empty table.
        write(r#"{"format":"plumbline-plan-table-v9","days":3,"segments":[["Gen",1,2]]}"#);
        assert!(load_table(&home, "chronological").is_none());
        write(r#"{"format":"plumbline-plan-table-v1","days":3,"segments":[["Genesis",1,2]]}"#);
        assert!(load_table(&home, "chronological").is_none());
        write(r#"{"format":"plumbline-plan-table-v1","days":3,"segments":[["Gen",5,2]]}"#);
        assert!(load_table(&home, "chronological").is_none());
        write(r#"{"format":"plumbline-plan-table-v1","days":3,"segments":[]}"#);
        assert!(load_table(&home, "chronological").is_none());

        let _ = std::fs::remove_dir_all(&home);
    }
}
