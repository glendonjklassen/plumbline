//! Reading plans + the concept study. docs/READING-PLANS.md is the contract.
//!
//! A **schedule plan** is a word-weighted walk of a scope (whole canon, NT, a book
//! list, or a curated chapter table) cut into days. Pacing is
//! **sequence-anchored**: "today" is the lowest unfinished day, and absence accrues
//! no backlog. A day is done when every chapter it names stands `Read` in
//! `core::reading` — derived, then cached into the plan file's `done` list so a day
//! honoured once survives the reading record under it being cleared.
//!
//! A **concept study** is not a schedule: a non-linear sweep with a preset tag,
//! recording swept chapters (no dwell gate — the reading tracker is off in that
//! mode, shell-side), progress being swept-over-scope. Ending one never touches the
//! tag or its members.
//!
//! Schedules are **generated, not stored**: the plan file keeps the generator's
//! parameters (or a curated table's id) and the walk is deterministic given the
//! corpus. One JSON file per running plan under `home/plans/` — personal study
//! data, in the backup zip beside `reading/`.
//!
//! **Class exclusivity** is a query, not a gate: [`class_conflict`] names the
//! running plan occupying a class and the shell confirms the replacement. The core
//! never guesses at intent.

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

/// The exclusivity classes of the built-in schedules — at most one running plan
/// per class, so NT-in-90 can sit beside Psalms+Proverbs but not beside a second
/// whole-Bible plan.
pub const CLASS_WHOLE_BIBLE: &str = "wholeBible";
pub const CLASS_NEW_TESTAMENT: &str = "newTestament";
pub const CLASS_DEVOTIONAL: &str = "devotional";

/// The icons a plan may wear, as stable tokens (v0.84.0, additive). A shell
/// owns the glyphs; the core owns the vocabulary, so a token in a plan file
/// always means the same picture on every shell. Every built-in carries its own
/// default — unique among the built-ins, so two running plans are told apart at
/// a glance — and a reader may pick any of these for any plan.
pub const ICONS: &[&str] = &[
    "flag",
    "calendar",
    "hourglass",
    "bolt",
    "cross",
    "timeline",
    "music",
    "star",
    "heart",
    "book",
    "leaf",
    "mountain",
];

/// A plan with no icon of its own and no built-in default: the custom plan.
pub const DEFAULT_ICON: &str = "flag";

/// The colours a plan may wear, as hue tokens (v0.84.0, additive). Each theme
/// tunes its own six (`theme::Palette::plan_*`) to clear WCAG AA on its paper,
/// so the plan file stores the hue, never a hex, and a theme change recolours
/// every plan to match. `None` is the theme's gold, the plan surfaces' default.
pub const COLORS: &[&str] = &["rose", "amber", "moss", "teal", "sky", "violet"];

/// What kind of plan a file describes. The serialized names are frozen with
/// the rest of the format: `"schedule"` and `"conceptStudy"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Schedule,
    ConceptStudy,
}

/// One run of chapters: `(book, first, last)`, inclusive. Serialized as a
/// three-element array, `["Isa", 1, 39]`.
pub type Span = (String, u16, u16);

/// What a generated schedule walks. Serialized flat into the generator object:
/// `{"scope":"canon"}` / `{"scope":"nt"}` / `{"scope":"books","books":["Ps","Prov"]}` /
/// `{"scope":"spans","spans":[["Isa",1,39],["Ps",1,150]]}`. `Spans` is the
/// reader's own plan (v0.83.0, additive): a build before it reports the file
/// unreadable and leaves it where it lies, like any plan it cannot parse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "camelCase")]
pub enum Scope {
    Canon,
    Nt,
    Books { books: Vec<String> },
    Spans { spans: Vec<Span> },
}

/// The parameters a schedule is regenerated from on every load.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Generator {
    #[serde(flatten)]
    pub scope: Scope,
    pub days: u32,
}

/// One running plan — the whole on-disk file. Additive evolution only: unknown
/// fields are read past and dropped on the next write (the `overlay-tag-v1`
/// stance). `done` holds 1-based day numbers, sorted, deduped; days may complete
/// out of order. `swept` is the concept study's coverage, chapters sorted per book.
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
    /// The local date (`YYYY-MM-DD`) each day in `done` was finished on — stamped
    /// by the reading write that closed the day ([`note_first_read`]), and what
    /// [`done_today`] reads. Additive: a plan file from before the stamp carries
    /// `done` days with no date, and those simply never read as finished today.
    /// Days complete only by reading logged before the plan started are never
    /// stamped either — nobody read them today.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub done_on: BTreeMap<u32, String>,
    /// Concept study only: the preset tag a tapped verse is filed under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Concept study only: swept chapters, `book id → sorted chapter numbers`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub swept: BTreeMap<String, Vec<u16>>,
    /// Set aside, kept whole: a paused schedule holds its `done` days and its class
    /// but asks nothing — no chip, no today card. Additive, and absent again the
    /// moment a plan resumes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub paused: bool,
    /// The reader's icon for this plan, one of [`ICONS`]; absent means the
    /// built-in's default, or [`DEFAULT_ICON`] (v0.84.0, additive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// The reader's colour for this plan, one of [`COLORS`]; absent means the
    /// theme's gold (v0.84.0, additive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// The icon a plan shows: its own, else its built-in's default, else
/// [`DEFAULT_ICON`]. A token this build does not know (a file from a later
/// build) falls through to the default rather than naming a glyph no shell has.
pub fn effective_icon(plan: &Plan) -> &'static str {
    plan.icon
        .as_deref()
        .and_then(|i| ICONS.iter().copied().find(|k| *k == i))
        .or_else(|| builtins().into_iter().find(|b| b.id == plan.id).map(|b| b.icon))
        .unwrap_or(DEFAULT_ICON)
}

/// Set a plan's icon and colour in one stroke. `None` (or an empty string, which
/// is how the ABI spells it) clears to the default; any other value must be in
/// the vocabulary, so a shell cannot store a token no shell can draw.
pub fn set_appearance(plan: &mut Plan, icon: Option<&str>, color: Option<&str>) -> Result<(), Error> {
    let pick = |v: Option<&str>, vocab: &[&str], what: &str| -> Result<Option<String>, Error> {
        match v.map(str::trim).filter(|s| !s.is_empty()) {
            None => Ok(None),
            Some(t) if vocab.contains(&t) => Ok(Some(t.to_string())),
            Some(t) => Err(Error::Parse(format!("unknown plan {what}: {t}"))),
        }
    };
    plan.icon = pick(icon, ICONS, "icon")?;
    plan.color = pick(color, COLORS, "color")?;
    Ok(())
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
    /// Its default icon, one of [`ICONS`] and unique among the built-ins.
    pub icon: &'static str,
}

/// The lineup the picker offers. Every entry must be START-ABLE into a non-empty
/// schedule today, or it renders as instantly "finished". A generator plan always
/// is; a TABLE plan only where its table file is present, so the FFI offers a table
/// row only after [`load_table`] answers.
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
            icon: "calendar",
        },
        Builtin {
            id: "bible-180",
            name_key: "plans.bible180",
            class: CLASS_WHOLE_BIBLE,
            kind: Kind::Schedule,
            generator: gen(Scope::Canon, 180),
            table: None,
            icon: "hourglass",
        },
        Builtin {
            id: "bible-90",
            name_key: "plans.bible90",
            class: CLASS_WHOLE_BIBLE,
            kind: Kind::Schedule,
            generator: gen(Scope::Canon, 90),
            table: None,
            icon: "bolt",
        },
        Builtin {
            id: "nt-90",
            name_key: "plans.nt90",
            class: CLASS_NEW_TESTAMENT,
            kind: Kind::Schedule,
            generator: gen(Scope::Nt, 90),
            table: None,
            icon: "cross",
        },
        Builtin {
            id: "chronological",
            name_key: "plans.chronological",
            class: CLASS_WHOLE_BIBLE,
            kind: Kind::Schedule,
            generator: None,
            table: Some("chronological"),
            icon: "timeline",
        },
        Builtin {
            id: "psalms-proverbs-30",
            name_key: "plans.psalmsProverbs30",
            class: CLASS_DEVOTIONAL,
            kind: Kind::Schedule,
            generator: gen(Scope::Books { books: vec!["Ps".into(), "Prov".into()] }, 30),
            table: None,
            icon: "music",
        },
    ]
}

// ── curated tables ────────────────────────────────────────────────────────────

/// Serialized into every table file (`data/<id>.json`). Frozen like the rest.
pub const TABLE_FORMAT: &str = "plumbline-plan-table-v1";

/// A curated plan table: the ordered chapter walk and the day count to cut it
/// into. Chronological is the one shipped (`scripts/build-chronological.mjs`
/// compiles it, verifying exactly-once canon coverage).
pub struct PlanTable {
    pub days: u32,
    pub order: Vec<(String, u16)>,
}

/// Read a curated table from the pack (`data/<id>.json`). `None` for anything short
/// of well-formed — absent file, wrong format tag, unknown book, inverted span —
/// so a damaged table hides its plan rather than offering one with a hole in it.
/// Chapter numbers are checked against the CORPUS by the caller, since chapter
/// counts live there and not in the canon table.
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

/// The chapters a scope names, in canon order — a `Books` list is re-ordered
/// whatever order it was written in, and so are `Spans` (see
/// [`normalize_spans`]). Books the corpus does not know are skipped rather than
/// erred.
pub fn scope_chapters(scope: &Scope, words: &ChapterWords) -> Vec<(String, u16)> {
    if let Scope::Spans { spans } = scope {
        return normalize_spans(spans, words)
            .into_iter()
            .flat_map(|(b, first, last)| (first..=last).map(move |c| (b.clone(), c)))
            .collect();
    }
    let books: Vec<&str> = match scope {
        Scope::Canon => canon::BOOKS.iter().map(|b| b.id).collect(),
        Scope::Nt => canon::BOOKS[OT_NT_DIVIDE..].iter().map(|b| b.id).collect(),
        Scope::Books { books } => {
            canon::BOOKS.iter().map(|b| b.id).filter(|id| books.iter().any(|w| w == id)).collect()
        }
        Scope::Spans { .. } => unreachable!("handled above"),
    };
    let mut out = Vec::new();
    for b in books {
        for c in 1..=words.chapters(b) {
            out.push((b.to_string(), c));
        }
    }
    out
}

/// A reader's spans made into a walk: canon order, each clamped to the chapters
/// the corpus has, unknown books and empty runs dropped, and runs in one book
/// that overlap or touch joined — so "Psalms 1–75" and "Psalms 50–150" added
/// separately read every psalm once. Deterministic given the corpus, which is
/// what lets the plan file keep the spans as written.
pub fn normalize_spans(spans: &[Span], words: &ChapterWords) -> Vec<Span> {
    let index = |b: &str| canon::BOOKS.iter().position(|x| x.id == b);
    let mut clean: Vec<(usize, u16, u16)> = spans
        .iter()
        .filter_map(|(b, first, last)| {
            let i = index(b)?;
            let n = words.chapters(b);
            let first = (*first).max(1);
            let last = (*last).min(n);
            (n > 0 && first <= last).then_some((i, first, last))
        })
        .collect();
    clean.sort_unstable();
    let mut out: Vec<(usize, u16, u16)> = Vec::new();
    for (i, first, last) in clean {
        match out.last_mut() {
            Some((pi, _, plast)) if *pi == i && first <= plast.saturating_add(1) => *plast = (*plast).max(last),
            _ => out.push((i, first, last)),
        }
    }
    out.into_iter().map(|(i, first, last)| (canon::BOOKS[i].id.to_string(), first, last)).collect()
}

/// What a spans scope covers, in the reader's language: whole books collapse
/// to "Isaiah–Malachi", a part of a book reads "Psalms 1–75", and the groups
/// join with " · ". Empty for a scope that names no chapter the corpus has.
pub fn describe_spans(spans: &[Span], words: &ChapterWords, lang: crate::i18n::Lang) -> String {
    let index = |b: &str| canon::BOOKS.iter().position(|x| x.id == b).unwrap_or(usize::MAX);
    let whole = |(b, first, last): &Span| *first == 1 && *last == words.chapters(b);
    let mut groups: Vec<String> = Vec::new();
    // Runs of consecutive whole books collapse into one "first–last" group.
    let mut run: Option<(usize, usize)> = None; // canon indices of the run's ends
    let flush = |run: &mut Option<(usize, usize)>, groups: &mut Vec<String>| {
        if let Some((a, z)) = run.take() {
            let name = |i: usize| crate::i18n::book_name(lang, canon::BOOKS[i].id);
            groups.push(if a == z { name(a) } else { format!("{}–{}", name(a), name(z)) });
        }
    };
    for span in normalize_spans(spans, words) {
        let i = index(&span.0);
        if whole(&span) {
            match run {
                Some((a, z)) if z + 1 == i => run = Some((a, i)),
                _ => {
                    flush(&mut run, &mut groups);
                    run = Some((i, i));
                }
            }
        } else {
            flush(&mut run, &mut groups);
            let (b, first, last) = &span;
            let book = crate::i18n::book_name(lang, b);
            groups.push(if first == last {
                crate::i18n::t(lang, "ref.chapter", &[("book", &book), ("chapter", &first.to_string())])
            } else {
                format!("{book} {first}–{last}")
            });
        }
    }
    flush(&mut run, &mut groups);
    groups.join(" · ")
}

/// A custom plan's display name — "Isaiah–Malachi · 60 days" — in the reader's
/// language. Not stored: the file keeps the spans and the day count, and the
/// name follows the interface language.
pub fn custom_name(g: &Generator, words: &ChapterWords, lang: crate::i18n::Lang) -> Option<String> {
    let Scope::Spans { spans } = &g.scope else { return None };
    let scope = describe_spans(spans, words, lang);
    Some(crate::i18n::plural(
        lang,
        "plans.customName.one",
        "plans.customName.other",
        u64::from(g.days),
        &[("scope", &scope)],
    ))
}

/// A reader's own schedule over `spans`, cut into `days`. No class: custom plans
/// do not compete with the built-ins or each other. Refuses a scope with no
/// chapter the corpus has, or no days — a plan that would start finished.
pub fn custom(
    spans: Vec<Span>,
    days: u32,
    words: &ChapterWords,
    now: &str,
    lang: Option<String>,
) -> Result<Plan, Error> {
    if days == 0 {
        return Err(Error::Parse("a plan needs at least one day".into()));
    }
    if scope_chapters(&Scope::Spans { spans: spans.clone() }, words).is_empty() {
        return Err(Error::Parse("a plan needs at least one chapter".into()));
    }
    Ok(Plan {
        format: FORMAT.to_string(),
        id: format!("custom-{}", store::new_id()),
        kind: Kind::Schedule,
        class: None,
        generator: Some(Generator { scope: Scope::Spans { spans }, days }),
        table: None,
        started: now.to_string(),
        lang,
        done: Vec::new(),
        done_on: Default::default(),
        tag: None,
        swept: Default::default(),
        paused: false,
        icon: None,
        color: None,
    })
}

/// Cut an ordered chapter walk into `days` word-balanced days.
///
/// Greedy against cumulative word boundaries, with two guarantees that outrank
/// perfect balance: every chapter appears exactly once (never split), and **no day
/// is empty** — `days` clamps to the chapter count, and once chapters left equal
/// days left each remaining day takes one. Deterministic given the corpus, which is
/// what lets the plan file store parameters instead of the schedule.
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

/// Sequence-anchored "today": the lowest unfinished day, or `None` when the plan is
/// complete. `is_read` is the reading store's answer for one chapter — a closure,
/// so no reading logic is duplicated here.
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

/// Whether a plan-day was finished ON `today` (`YYYY-MM-DD`, the reading store's
/// date grain) — the signal that retires the nav-strip chip for the rest of the
/// calendar day. Pacing stays sequence-anchored; this is only about not asking
/// for more the day a day's worth was given.
///
/// Reads the plan's own `done_on` stamps, written by [`note_first_read`] at the
/// moment a day's last unread chapter got its first full pass — NOT the reading
/// store's dates. Dating a day by its chapters' last reads was wrong twice over
/// (maintainer, 2026-09-23, two plans running: finishing one plan's day made the
/// other plan's chip go too): a chapter re-read today — for the other plan, or
/// on a Sunday morning — re-dated a day this plan had finished last week, and a
/// day read AHEAD for the other plan counted as this plan's reading for today.
///
/// So a stamp counts only BEHIND the frontier — `frontier` is [`next_day`]'s
/// day, `None` once the plan is complete. A day stamped today that the reader
/// has not reached in sequence is working ahead, and the chip keeps asking for
/// today's own. Days honoured only by the `done` cache have no stamp and never
/// count.
pub fn done_today(plan: &Plan, frontier: Option<u32>, today: &str) -> bool {
    plan.done_on.iter().any(|(day, date)| frontier.is_none_or(|f| *day < f) && date == today)
}

/// A chapter's FIRST full pass just landed on `date` (`YYYY-MM-DD`): stamp every
/// day of `sched` that names it and now reads back complete. Returns whether the
/// plan changed, so the caller knows to persist it.
///
/// First pass only, and the CALLER gates it (`reading::Recorded::first_pass`,
/// `reading::mark_read`'s answer). A chapter that was already read cannot close
/// a day — every day naming it was either complete already or still waits on
/// another chapter — so a re-read never stamps, whatever plan it was read for.
/// A day already in `done` is left alone on the same reasoning.
pub fn note_first_read(
    plan: &mut Plan,
    sched: &[Vec<(String, u16)>],
    book: &str,
    chapter: u16,
    is_read: impl Fn(&str, u16) -> bool,
    date: &str,
) -> bool {
    let mut changed = false;
    for (i, chs) in sched.iter().enumerate() {
        let day = i as u32 + 1;
        if plan.done.contains(&day) || !chs.iter().any(|(b, c)| b == book && *c == chapter) {
            continue;
        }
        if chs.iter().all(|(b, c)| is_read(b, *c)) {
            changed |= mark_done(plan, day, date);
        }
    }
    changed
}

/// Record a day as done on `date` (1-based; `done` sorted and deduped). Returns
/// whether it was new. The caller persists — `done` is the cache [`next_day`]
/// consults so a day never un-completes when the reading record under it is
/// later cleared, and `done_on` is what [`done_today`] reads.
pub fn mark_done(plan: &mut Plan, day: u32, date: &str) -> bool {
    match plan.done.binary_search(&day) {
        Ok(_) => false,
        Err(at) => {
            plan.done.insert(at, day);
            plan.done_on.insert(day, date.to_string());
            true
        }
    }
}

// ── the concept study's coverage ──────────────────────────────────────────────

/// Mark a chapter swept. Returns whether it was new. No dwell, no order (see
/// docs/READING-PLANS.md §Concept Study).
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

/// The running plan that already occupies `class`, if any — a query, not a gate:
/// the shell confirms the replacement before stopping it.
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
            done_on: BTreeMap::new(),
            tag: None,
            swept: BTreeMap::new(),
            paused: false,
            icon: None,
            color: None,
        }
    }

    #[test]
    fn every_builtin_wears_its_own_icon_from_the_vocabulary() {
        let all = builtins();
        for b in &all {
            assert!(ICONS.contains(&b.icon), "{}: icon {:?} is not in ICONS", b.id, b.icon);
            assert_eq!(all.iter().filter(|x| x.icon == b.icon).count(), 1, "{}: icon {:?} is shared", b.id, b.icon);
        }
        assert!(ICONS.contains(&DEFAULT_ICON));
    }

    #[test]
    fn appearance_is_validated_stored_and_read_back_with_defaults() {
        // A file from before v0.84.0 carries neither field: the built-in's own icon.
        let mut p = plan("nt-90", Some(CLASS_NEW_TESTAMENT));
        assert_eq!(effective_icon(&p), "cross");
        assert_eq!(p.color, None);
        assert!(!serde_json::to_string(&p).unwrap().contains("icon"), "absent fields stay off the disk");
        // A chosen pair round-trips; an empty string clears; junk is refused whole.
        set_appearance(&mut p, Some("star"), Some("rose")).unwrap();
        let back: Plan = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!((effective_icon(&back), back.color.as_deref()), ("star", Some("rose")));
        set_appearance(&mut p, Some(""), None).unwrap();
        assert_eq!((effective_icon(&p), p.color.as_deref()), ("cross", None));
        assert!(set_appearance(&mut p, Some("dragon"), None).is_err());
        assert!(set_appearance(&mut p, None, Some("#ff0000")).is_err());
        // A token from a later build falls through to the default, not to a hole.
        p.icon = Some("telescope".into());
        assert_eq!(effective_icon(&p), "cross");
        // A custom plan with no icon of its own is the flag.
        let c = plan("custom-abc", None);
        assert_eq!(effective_icon(&c), DEFAULT_ICON);
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
        // 35 words over 2 days → the boundary is 17.5. Gen 1 (10) is short of it,
        // Gen 1+2 (30) crosses it, so day 1 is two chapters and day 2 is one —
        // where a chapter-count split would have put the heavier Gen 2 alone.
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
    fn spans_normalize_to_a_canon_ordered_walk_with_no_chapter_twice() {
        let c = toy();
        let w = ChapterWords::build(&c);
        // Only Gen 1–3 exists here. Written out of order, overlapping, past the
        // end, with an unknown book and an inverted run: one clean walk.
        let spans: Vec<Span> =
            vec![("Gen".into(), 2, 9), ("Rev".into(), 1, 22), ("Gen".into(), 1, 1), ("Gen".into(), 3, 2)];
        assert_eq!(normalize_spans(&spans, &w), vec![("Gen".to_string(), 1, 3)]);
        let order = scope_chapters(&Scope::Spans { spans: spans.clone() }, &w);
        assert_eq!(order, vec![("Gen".to_string(), 1), ("Gen".to_string(), 2), ("Gen".to_string(), 3)]);
        // Disjoint runs in one book stay two runs.
        let two: Vec<Span> = vec![("Gen".into(), 1, 1), ("Gen".into(), 3, 3)];
        assert_eq!(normalize_spans(&two, &w), vec![("Gen".to_string(), 1, 1), ("Gen".to_string(), 3, 3)]);
        // The serialized shape is the frozen flat form.
        let g = Generator { scope: Scope::Spans { spans: two }, days: 2 };
        assert_eq!(
            serde_json::to_string(&g).unwrap(),
            r#"{"scope":"spans","spans":[["Gen",1,1],["Gen",3,3]],"days":2}"#
        );
    }

    #[test]
    fn a_custom_plan_is_named_from_its_spans_and_refuses_an_empty_one() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let en = crate::i18n::Lang::En;
        // A whole book is its name; a part names its chapters; one chapter is a chapter.
        assert_eq!(describe_spans(&[("Gen".into(), 1, 3)], &w, en), "Genesis");
        assert_eq!(describe_spans(&[("Gen".into(), 1, 2)], &w, en), "Genesis 1–2");
        assert_eq!(describe_spans(&[("Gen".into(), 3, 3)], &w, en), "Genesis 3");
        let p = custom(vec![("Gen".into(), 1, 2)], 2, &w, "2026-11-01T12:00:00Z", None).unwrap();
        assert!(p.id.starts_with("custom-"));
        assert_eq!(p.class, None, "no class: custom plans do not compete");
        assert_eq!(custom_name(p.generator.as_ref().unwrap(), &w, en).unwrap(), "Genesis 1–2 · 2 days");
        let one = custom(vec![("Gen".into(), 1, 2)], 1, &w, "2026-11-01T12:00:00Z", None).unwrap();
        assert_eq!(custom_name(one.generator.as_ref().unwrap(), &w, en).unwrap(), "Genesis 1–2 · 1 day");
        // The built-ins have no custom name.
        assert_eq!(custom_name(&Generator { scope: Scope::Nt, days: 90 }, &w, en), None);
        // Nothing to read, or no day to read it on, is refused — not started finished.
        assert!(custom(vec![("Rev".into(), 1, 22)], 10, &w, "2026-11-01T12:00:00Z", None).is_err());
        assert!(custom(vec![("Gen".into(), 1, 2)], 0, &w, "2026-11-01T12:00:00Z", None).is_err());
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

        // Out of order: day 3 read while day 2 is not — today is still day 2
        // (sequence-anchored), and both ends count as done.
        let t = next_day(&p, &sched, |b, ch| (b, ch) == ("Gen", 1) || (b, ch) == ("Gen", 3)).unwrap();
        assert_eq!((t.day, t.days_done), (2, 2));

        // Everything read → the plan is finished.
        assert!(next_day(&p, &sched, |_, _| true).is_none());
    }

    #[test]
    fn done_today_retires_the_day_a_days_worth_was_read() {
        let c = toy();
        let w = ChapterWords::build(&c);
        // Only the shape matters: day 1 must hold two chapters for the leftover case.
        let sched = schedule(&scope_chapters(&Scope::Canon, &w), &w, 2);
        assert_eq!(sched[0].len(), 2, "day 1 must span two chapters for the leftover case");
        let mut p = plan("bible-2", Some(CLASS_WHOLE_BIBLE));
        let read =
            |gen1: bool, gen2: bool| move |b: &str, ch: u16| b == "Gen" && ((ch == 1 && gen1) || (ch == 2 && gen2));

        // Nothing finished: the chip stays.
        assert!(!done_today(&p, Some(1), "2026-08-12"));
        // Half of day 1 read yesterday: the day is still open, nothing is stamped.
        assert!(!note_first_read(&mut p, &sched, "Gen", 1, read(true, false), "2026-08-11"));
        assert!(p.done_on.is_empty());
        assert!(!done_today(&p, Some(1), "2026-08-11"));
        // Yesterday's leftovers finished today: a day's worth, dated today — the
        // chip retires…
        assert!(note_first_read(&mut p, &sched, "Gen", 2, read(true, true), "2026-08-12"));
        assert_eq!(p.done_on[&1], "2026-08-12");
        assert_eq!(p.done, vec![1], "the cache is kept in step");
        assert!(done_today(&p, Some(2), "2026-08-12"));
        // …but only for the rest of that calendar day.
        assert!(!done_today(&p, Some(2), "2026-08-13"));
    }

    /// The two-plan report (maintainer, 2026-09-23): finishing one plan's day for
    /// the day made the OTHER plan's chip go. Under the old rule a day was "done
    /// today" when its chapters' last-read dates said so — and a chapter read for
    /// plan A re-dated whatever day of plan B named it, ahead of B's frontier or
    /// long behind it. This can fail against that bug: the old rule answered true
    /// at both of the `!done_today` lines below.
    #[test]
    fn a_chapter_read_for_another_plan_does_not_retire_this_ones_chip() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let sched = schedule(&scope_chapters(&Scope::Canon, &w), &w, 3);
        assert_eq!(sched.iter().map(Vec::len).collect::<Vec<_>>(), vec![1, 1, 1], "one chapter a day");
        let mut p = plan("bible-3", Some(CLASS_WHOLE_BIBLE));
        let today = "2026-09-23";

        // Day 3's chapter, first read today for a plan that happens to share it.
        // The day is stamped — it IS finished — but this plan is still asking for
        // day 1, and working ahead is not today's reading.
        let gen3 = |b: &str, ch: u16| b == "Gen" && ch == 3;
        assert!(note_first_read(&mut p, &sched, "Gen", 3, gen3, today));
        assert_eq!(p.done_on[&3], today);
        assert!(!done_today(&p, Some(1), today), "a day ahead of the frontier is not today's");

        // Day 1 read today: behind the frontier now — the chip retires.
        let gen1_3 = |b: &str, ch: u16| b == "Gen" && (ch == 1 || ch == 3);
        assert!(note_first_read(&mut p, &sched, "Gen", 1, gen1_3, today));
        assert!(done_today(&p, Some(2), today));

        // Tomorrow, day 1's chapter is read again — a Sunday morning, or the other
        // plan reaching it. The caller never calls this for a re-read (first pass
        // only), and even called, a done day is not re-stamped: the chip asks.
        assert!(!note_first_read(&mut p, &sched, "Gen", 1, gen1_3, "2026-09-24"));
        assert_eq!(p.done_on[&1], today);
        assert!(!done_today(&p, Some(2), "2026-09-24"), "yesterday's day, re-read today, is not today's");

        // The last day closes the plan: with no frontier, every stamp is behind it.
        let all = |b: &str, _: u16| b == "Gen";
        assert!(note_first_read(&mut p, &sched, "Gen", 2, all, "2026-09-25"));
        assert!(done_today(&p, None, "2026-09-25"));
    }

    #[test]
    fn a_cached_done_day_survives_its_reading_record_being_cleared() {
        let c = toy();
        let w = ChapterWords::build(&c);
        let sched = schedule(&scope_chapters(&Scope::Canon, &w), &w, 3);
        let mut p = plan("bible-3", Some(CLASS_WHOLE_BIBLE));
        assert!(mark_done(&mut p, 1, "2026-08-12"));
        assert!(!mark_done(&mut p, 1, "2026-08-13"), "recording twice is a no-op");
        assert_eq!(p.done_on[&1], "2026-08-12", "and keeps the first date");
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
        mark_done(&mut p, 4, "2026-08-12");
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
        for b in &all {
            assert!(b.generator.is_some() != b.table.is_some(), "{}: generator XOR table", b.id);
        }
        let whole: Vec<_> = all.iter().filter(|b| b.class == CLASS_WHOLE_BIBLE).map(|b| b.id).collect();
        // Chronological is in the lineup; the FFI only OFFERS it where its table loads.
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
        write(
            r#"{"format":"plumbline-plan-table-v1","id":"chronological","days":3,"segments":[["Gen",1,2],["Job",1,1],["Gen",3,3]]}"#,
        );
        let t = load_table(&home, "chronological").unwrap();
        assert_eq!(t.days, 3);
        assert_eq!(t.order, vec![("Gen".into(), 1), ("Gen".into(), 2), ("Job".into(), 1), ("Gen".into(), 3)]);

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
