//! Reading plans + the concept study — the C ABI.
//!
//! Plans load fresh from `home/plans/` on every call (a plan file is tens of
//! bytes and a reader has a handful, so there is no cache to invalidate). All of
//! these tolerate an engine opened from bytes (no home) — the list reads empty,
//! authoring returns the standard "no home" error.
//!
//! Schedule completion is derived: a day is done when every chapter it names
//! reads back as a completed pass from the reading store ([`chapter_read`]). The
//! plan file's `done` cache is consulted first, so a day honoured once stays
//! honoured even after its reading record is cleared.

use std::ffi::c_char;
use std::ptr;

use plumbline_core::plan::{self, Plan};
use plumbline_core::reading::{self, ChapterWords};
use plumbline_core::{canon, i18n, store};
use serde::Serialize;

use crate::{guard, guard_err, opt_str, out_json, out_string, PlumblineEngine};

// ── wire DTOs ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WirePlans {
    /// The plans the reader is running, with derived state.
    running: Vec<WireRunning>,
    /// The catalogue the picker offers, with the class each occupies.
    builtins: Vec<WireBuiltin>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireRunning {
    id: String,
    kind: plan::Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    class: Option<String>,
    /// When the plan was started; shown with the name for a set-aside plan.
    started: String,
    /// Set aside: holds its place, asks nothing (no chip, no today card —
    /// shells filter on this).
    paused: bool,
    /// Concept study only: the preset tag a tap files under.
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    /// Schedule only: a full plan-day was finished today (`plan::done_today`),
    /// which retires the nav-strip chip for the rest of the calendar day.
    #[serde(skip_serializing_if = "Option::is_none")]
    done_today: Option<bool>,
    /// Schedule only: today's card (null once the plan is finished).
    #[serde(skip_serializing_if = "Option::is_none")]
    today: Option<WireToday>,
    /// Schedule only: whole-plan progress in days.
    #[serde(skip_serializing_if = "Option::is_none")]
    schedule_progress: Option<[u32; 2]>,
    /// Concept study only: chapters swept over the scope total.
    #[serde(skip_serializing_if = "Option::is_none")]
    sweep_progress: Option<[u32; 2]>,
    /// Concept study only: the swept chapters, `book id → sorted chapter
    /// numbers`, for painting coverage. Present (even when empty) whenever
    /// `sweep_progress` is, so a shell can tell "nothing swept" from "not a
    /// concept study".
    #[serde(skip_serializing_if = "Option::is_none")]
    swept: Option<std::collections::BTreeMap<String, Vec<u16>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireToday {
    day: u32,
    /// Today's chapters as `{book, chapter, display, read}`.
    chapters: Vec<WireDayChapter>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireDayChapter {
    book: String,
    chapter: u16,
    display: String,
    read: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireBuiltin {
    id: String,
    name_key: String,
    class: String,
    kind: plan::Kind,
}

/// Whether a chapter reads as a completed pass: the reading record carries a
/// `last_read` date. Independent of `now` (a completed read does not un-complete
/// with time), which is why plan completion needs no clock.
fn chapter_read(store: &reading::Store, book: &str, chapter: u16) -> bool {
    last_read_day(store, book, chapter).is_some()
}

/// The `YYYY-MM-DD` a chapter's last full pass landed on, `None` if it has
/// never had one — what [`plan::done_today`] dates a finished plan-day by.
fn last_read_day(store: &reading::Store, book: &str, chapter: u16) -> Option<String> {
    store.get(book)?.iter().find(|r| r.chapter == chapter)?.last_read.clone()
}

/// The schedule a plan describes, or empty when it cannot be built (a
/// curated-table plan whose table file is absent or damaged — the same signal
/// that hides its picker row below). A generator plan is always buildable.
fn schedule_of(plan: &Plan, words: &ChapterWords, home: Option<&std::path::Path>) -> Vec<Vec<(String, u16)>> {
    match (&plan.generator, &plan.table) {
        (Some(g), _) => plan::schedule(&plan::scope_chapters(&g.scope, words), words, g.days),
        (None, Some(id)) => {
            let Some(t) = home.and_then(|h| plan::load_table(h, id)) else { return Vec::new() };
            // Chapters the corpus does not carry are skipped, `scope_chapters`' stance.
            let order: Vec<_> = t.order.into_iter().filter(|(b, c)| *c <= words.chapters(b)).collect();
            plan::schedule(&order, words, t.days)
        }
        (None, None) => Vec::new(),
    }
}

fn running_state(
    plan: &Plan,
    words: &ChapterWords,
    store: &reading::Store,
    home: Option<&std::path::Path>,
    today_day: &str,
) -> WireRunning {
    let mut w = WireRunning {
        id: plan.id.clone(),
        kind: plan.kind,
        class: plan.class.clone(),
        started: plan.started.clone(),
        paused: plan.paused,
        tag: plan.tag.clone(),
        done_today: None,
        today: None,
        schedule_progress: None,
        sweep_progress: None,
        swept: None,
    };
    match plan.kind {
        plan::Kind::Schedule => {
            let sched = schedule_of(plan, words, home);
            let is_read = |b: &str, c: u16| chapter_read(store, b, c);
            let today = plan::next_day(plan, &sched, is_read);
            let days_total = sched.len() as u32;
            let days_done = today.as_ref().map_or(days_total, |t| t.days_done);
            w.schedule_progress = Some([days_done, days_total]);
            w.done_today = Some(plan::done_today(&sched, |b, c| last_read_day(store, b, c), today_day));
            w.today = today.map(|t| WireToday {
                day: t.day,
                chapters: t
                    .chapters
                    .iter()
                    .map(|(b, c)| WireDayChapter {
                        book: b.clone(),
                        chapter: *c,
                        // Never `canon::display_name` here: that is the frozen English
                        // table `refKey` is built from. `ref.chapter` is the catalogue
                        // template the rest of the app names a chapter with — a template,
                        // not a separator, because a language may not put the book first.
                        display: i18n::t(
                            i18n::active(),
                            "ref.chapter",
                            &[("book", &i18n::book_name(i18n::active(), b)), ("chapter", &c.to_string())],
                        ),
                        read: chapter_read(store, b, *c),
                    })
                    .collect(),
            });
        }
        plan::Kind::ConceptStudy => {
            let (swept, total) = plan::sweep_progress(plan, canon_chapter_total(words));
            w.sweep_progress = Some([swept as u32, total as u32]);
            w.swept = Some(plan.swept.clone());
        }
    }
    w
}

/// Total chapters in the corpus — the concept study's scope denominator (whole canon).
fn canon_chapter_total(words: &ChapterWords) -> usize {
    canon::book_ids().map(|b| words.chapters(b) as usize).sum()
}

/// Every running plan with derived state, plus the builtin catalogue for the
/// picker, as `{running:[…], builtins:[…]}`. Never null on a live engine.
/// `now` is what dates each schedule's `doneToday`; null reads as "no day",
/// so the flag is simply false everywhere.
///
/// # Safety
/// `engine` is a live engine; `now` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_plans_json(
    engine: *const PlumblineEngine,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let plans = e.home.as_ref().map(|h| plan::load_plans(h).0).unwrap_or_default();
        let words = e.reading_words();
        let store = e.home.as_ref().map(|h| reading::load(h).0).unwrap_or_default();
        let home = e.home.as_deref();
        let today = opt_str(now).map(reading::day_of).unwrap_or_default();
        let running = plans.iter().map(|p| running_state(p, words, &store, home, &today)).collect();
        let builtins = plan::builtins()
            .into_iter()
            // A table plan is offered only where its table loads: every offered row must
            // start into a non-empty schedule, and a home without the file would start
            // one that reads instantly "finished".
            .filter(|b| b.table.is_none_or(|id| home.is_some_and(|h| plan::load_table(h, id).is_some())))
            .map(|b| WireBuiltin {
                id: b.id.to_string(),
                name_key: b.name_key.to_string(),
                class: b.class.to_string(),
                kind: b.kind,
            })
            .collect();
        out_json(&WirePlans { running, builtins })
    })
}

/// Start a built-in schedule plan by its `id` (see `plumbline_engine_plans_json`
/// `builtins`). Starting a plan whose class is already occupied replaces the
/// running one — the shell confirms first — and passing an already-running id
/// re-seeds it from scratch. Null on success, else an owned error string.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_plan_start(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    now: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else { return out_string("null engine".into()) };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".into());
        };
        let (Some(id), Some(now)) = (opt_str(id), opt_str(now)) else {
            return out_string("null or invalid argument".into());
        };
        let Some(b) = plan::builtins().into_iter().find(|b| b.id == id) else {
            return out_string(format!("unknown plan: {id}"));
        };
        // A table plan without its table must refuse to start, not start "finished".
        // The picker hides it for the same reason, but a stale shell could still ask.
        if let Some(table) = b.table {
            if plan::load_table(&home, table).is_none() {
                return out_string(format!("plan table missing: {table}"));
            }
        }
        // Replace the class occupant, if any and not this same id.
        let existing = plan::load_plans(&home).0;
        if let Some(conflict) = plan::class_conflict(&existing, b.class) {
            if conflict.id != b.id {
                if let Err(e) = plan::remove_plan(&home, &conflict.id) {
                    return out_string(e.to_string());
                }
            }
        }
        let started = plan::Plan {
            format: plan::FORMAT.to_string(),
            id: b.id.to_string(),
            kind: b.kind,
            class: Some(b.class.to_string()),
            generator: b.generator,
            table: b.table.map(str::to_string),
            started: now.to_string(),
            lang: lang_stamp(),
            done: Vec::new(),
            tag: None,
            swept: Default::default(),
            paused: false,
        };
        match plan::write_plan(&home, &started) {
            Ok(()) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Start (or resume) a concept study for `tag`, returning the run's plan id —
/// what the shell writes into `config.conceptStudy` to enter the mode. The id is
/// derived from the tag (`run-<slug>`), so re-starting a concept already being
/// swept resumes it with coverage intact rather than forking a second run. The
/// tag itself need not exist yet; the first tap-to-tag creates it.
///
/// Returns the id on success, or an error string prefixed with `!` (which no
/// plan id can start with) so the one out-parameter carries both.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_concept_study_start(
    engine: *mut PlumblineEngine,
    tag: *const c_char,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(engine) = engine.as_mut() else { return out_string("!null engine".into()) };
        let Some(home) = engine.home.clone() else {
            return out_string("!engine has no home directory (opened from bytes); cannot author".into());
        };
        let (Some(tag), Some(now)) = (opt_str(tag), opt_str(now)) else {
            return out_string("!null or invalid argument".into());
        };
        if tag.trim().is_empty() {
            return out_string("!a concept study needs a tag to file under".into());
        }
        let id = format!("run-{}", store::slug(tag, "run"));
        // Resume an existing run for this tag rather than clobber its coverage.
        if let Some(found) = plan::load_plans(&home).0.into_iter().find(|p| p.id == id) {
            return out_string(found.id);
        }
        let run = plan::Plan {
            format: plan::FORMAT.to_string(),
            id: id.clone(),
            kind: plan::Kind::ConceptStudy,
            class: None,
            generator: None,
            table: None,
            started: now.to_string(),
            lang: lang_stamp(),
            done: Vec::new(),
            tag: Some(tag.trim().to_string()),
            swept: Default::default(),
            paused: false,
        };
        match plan::write_plan(&home, &run) {
            Ok(()) => out_string(id),
            Err(e) => out_string(format!("!{e}")),
        }
    })
}

/// Mark a chapter swept in a concept study (generous: no dwell, any order), and
/// persist. A non-concept-study id, or a chapter already swept, is a harmless no-op.
/// Null on success, else an owned error string.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_concept_study_sweep(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    book: *const c_char,
    chapter: u32,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else { return out_string("null engine".into()) };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".into());
        };
        let (Some(id), Some(book)) = (opt_str(id), opt_str(book)) else {
            return out_string("null or invalid argument".into());
        };
        if canon::book_by_id(book).is_none() {
            return out_string(format!("unknown book: {book}"));
        }
        let Some(mut run) = plan::load_plans(&home).0.into_iter().find(|p| p.id == id) else {
            return ptr::null_mut(); // no such run: nothing to sweep, not an error
        };
        if run.kind != plan::Kind::ConceptStudy {
            return ptr::null_mut();
        }
        if plan::sweep(&mut run, book, chapter as u16) {
            if let Err(e) = plan::write_plan(&home, &run) {
                return out_string(e.to_string());
            }
        }
        ptr::null_mut()
    })
}

/// Stop a plan — remove its file. Ending a concept study leaves its tag and every
/// verse gathered untouched (the point of the sweep). An absent id is a no-op.
/// Null on success, else an owned error string.
///
/// # Safety
/// `engine` is valid; `id` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_plan_stop(engine: *mut PlumblineEngine, id: *const c_char) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else { return out_string("null engine".into()) };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".into());
        };
        let Some(id) = opt_str(id) else { return out_string("null or invalid argument".into()) };
        match plan::remove_plan(&home, id) {
            Ok(_) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Pause or resume a plan: its file, progress and class stay put, and shells
/// stop asking for its today (no chip, no card) while `paused`. A concept study
/// can pause too. An absent id is an error — pausing a plan that is not running
/// means the shell's list is stale, and saying so beats a silent no-op. Null on
/// success, else an owned error string.
///
/// # Safety
/// `engine` is valid; `id` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_plan_set_paused(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    paused: bool,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else { return out_string("null engine".into()) };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".into());
        };
        let Some(id) = opt_str(id) else { return out_string("null or invalid argument".into()) };
        let Some(mut found) = plan::load_plans(&home).0.into_iter().find(|p| p.id == id) else {
            return out_string(format!("no running plan: {id}"));
        };
        if found.paused == paused {
            return ptr::null_mut(); // already there: a double-tap is not an error
        }
        found.paused = paused;
        match plan::write_plan(&home, &found) {
            Ok(()) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// The provenance stamp for a new plan file: the process's active language, the
/// same `i18n::stamp` source every other authored file records at create. `None`
/// on the source language's default — absent means "unknown", the migration
/// signal the field exists for.
fn lang_stamp() -> Option<String> {
    let code = i18n::stamp();
    (!code.is_empty()).then_some(code)
}
