//! Passages, verse by verse.
//!
//! All memorization is one thing: a verse with a card. A passage is a run of
//! such verses that the reader works through in order, and a single verse is
//! the passage of one. What a passage adds is only *grouping*:
//!
//! - it is ONE row of the hub and ONE place in the queue, however long it is;
//! - working it is a **run**: its learned verses that are due, in verse order,
//!   then the first verse not learned yet. Passing that one offers the next in
//!   the same sitting; nothing opens a verse ahead of the one before it;
//! - the next day the run starts from the beginning again with whatever is
//!   due, so a verse the reader knows well drops out of it and the rest do not.
//!
//! On disk nothing new exists but one key. The card on the passage's first
//! verse already carried `through`; each other verse gets an ordinary card of
//! its own the first time it is graded, in the file a single-verse card for it
//! would use. A build from before the split reads all of it: the first verse's
//! card as the passage it always was, the rest as single verses.
//!
//! A passage card from such a build has one schedule for the whole chunk and
//! no `perVerse`. [`spread`] reads it as every verse holding that schedule,
//! without writing anything, and [`grade`] makes it real on the first grade.

use std::collections::HashSet;

use super::*;

/// Whether the reader has PASSED this verse at least once — what opens the
/// verse after it. A verse only ever graded Again is not learned: it stays the
/// one the passage offers next, in this sitting and the next.
///
/// A verse split from a chunk has that chunk's schedule and no log, so its
/// passes are not in `reviews`; and once it lapses its `reps` are 0 as well.
/// [`SPLIT_FROM`] is what still says it was learned.
pub fn learned(card: &Card) -> bool {
    card.reps > 0 || card.reviews.iter().any(|r| r.grade.passed()) || card.extra.contains_key(SPLIT_FROM)
}

/// Additive key on a verse's card: the refKey of the first verse of the chunk
/// it was split from, which is where the reviews it was learned by are logged.
pub const SPLIT_FROM: &str = "splitFrom";

/// A passage card from before passages were split into verses.
fn undivided(card: &Card) -> bool {
    card.through.is_some() && !card.per_verse
}

/// The card a verse of an undivided passage is taken to have: the chunk's
/// schedule, and no reviews — the log stays on the first verse, where it was
/// written, so no count of reviews is multiplied by the passage's length.
fn inherited(head: &Card, verse: VRef) -> Card {
    let mut card = Card::new(verse, &head.tok_version, &head.created);
    card.extra.insert(SPLIT_FROM.to_string(), Value::String(head.verse.ref_key()));
    Card { ease: head.ease, interval_days: head.interval_days, reps: head.reps, due: head.due.clone(), ..card }
}

/// Undivided passages in reading order, so that where two overlap a shared
/// verse takes the earlier one's schedule every time, not whichever a hash
/// map yields first.
fn undivided_heads(cards: &HashMap<VRef, Card>) -> Vec<&Card> {
    let mut heads: Vec<&Card> = cards.values().filter(|c| undivided(c)).collect();
    heads.sort_by_key(|c| c.verse.reading_key());
    heads
}

/// Every verse's card as it stands. The cards on disk, plus — for a passage
/// reviewed as one chunk and not yet split — a card for each of its other
/// verses holding the chunk's schedule. A verse with a card of its own keeps
/// it. A chunk never reviewed spreads nothing: its verses are simply not
/// learned yet.
pub fn spread(cards: &HashMap<VRef, Card>) -> HashMap<VRef, Card> {
    let mut out = cards.clone();
    for head in undivided_heads(cards).into_iter().filter(|c| learned(c)) {
        for v in head.verses().into_iter().skip(1) {
            out.entry(v.clone()).or_insert_with(|| inherited(head, v));
        }
    }
    out
}

/// Grade ONE verse at `now`, creating its card on first review.
///
/// If the verse lies in a passage not yet split, that passage is split first:
/// every verse [`spread`] gave a schedule is written as a real card, and the
/// first verse's card is marked `perVerse`. The verses are written before the
/// mark, so a write that fails part-way leaves a passage that still reads as
/// undivided and splits again, to the same cards, on the next grade.
pub fn grade(
    home: impl AsRef<Path>,
    loaded: &HashMap<VRef, Card>,
    vref: &VRef,
    tok_version: &str,
    grade: Grade,
    now: &str,
) -> Result<Card, Error> {
    let deck = spread(loaded);
    for head in undivided_heads(loaded).into_iter().filter(|c| c.verses().contains(vref)) {
        for v in head.verses().into_iter().skip(1) {
            if let (false, Some(card)) = (loaded.contains_key(&v), deck.get(&v)) {
                write_card(&home, card)?;
            }
        }
        write_card(&home, &Card { per_verse: true, ..head.clone() })?;
    }
    let mut card = deck.get(vref).cloned().unwrap_or_else(|| Card::new(vref.clone(), tok_version, now));
    card.per_verse = card.through.is_some();
    review(&mut card, grade, now);
    write_card(&home, &card)?;
    Ok(card)
}

/// Start memorizing `start`…`through`. Nothing is written for the verses
/// after the first: each gets its card when the reader reaches it.
///
/// A verse already being memorized on its own becomes the passage's first
/// verse and keeps its schedule and log. A `start` that already opens a
/// passage is left alone — remove it first to give it a different end. An end
/// that is not a later verse of the same chapter adds the one verse.
pub fn add(
    home: impl AsRef<Path>,
    loaded: &HashMap<VRef, Card>,
    start: &VRef,
    through: &VRef,
    tok_version: &str,
    now: &str,
) -> Result<(), Error> {
    match loaded.get(start) {
        Some(card) if card.through.is_some() || !Card::spans(start, through) => Ok(()),
        Some(card) => write_card(&home, &Card { through: Some(through.clone()), per_verse: true, ..card.clone() }),
        None => write_card(&home, &Card::new_passage(start.clone(), through, tok_version, now)),
    }
}

/// What removing the row at `vref` does to the files.
#[derive(Clone, Debug, PartialEq)]
pub struct Removal {
    /// Cards to delete. Each needs its deletion recorded, or a restore brings
    /// it back (`crate::tombstone`).
    pub delete: Vec<VRef>,
    /// The first verse's card, kept as a plain verse because another passage
    /// still covers it.
    pub rewrite: Option<Card>,
}

/// Plan the removal of the row at `vref`: the one card, or — for a passage —
/// the card of every verse in it. A verse another passage also covers is kept
/// for that passage; if that verse is this passage's first, its card stays and
/// only stops being a passage.
pub fn removal(cards: &HashMap<VRef, Card>, vref: &VRef) -> Removal {
    let Some(card) = cards.get(vref).filter(|c| c.through.is_some()) else {
        return Removal { delete: vec![vref.clone()], rewrite: None };
    };
    let kept: HashSet<VRef> =
        cards.values().filter(|c| c.through.is_some() && c.verse != *vref).flat_map(Card::verses).collect();
    let delete = card.verses().into_iter().filter(|v| cards.contains_key(v) && !kept.contains(v)).collect();
    let rewrite = kept.contains(vref).then(|| Card { through: None, per_verse: false, ..card.clone() });
    Removal { delete, rewrite }
}

/// One row of the hub and one place in the queue: a passage, or a verse that
/// no passage covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    /// The row's key — its first verse.
    pub head: VRef,
    /// `"Ps 23:1–6"`, or `"Gen 1:7"`.
    pub label: String,
    /// Every verse in it, in order.
    pub verses: Vec<VRef>,
}

/// What working a row means at one moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub head: VRef,
    pub label: String,
    /// Every verse of the row, in order.
    pub verses: Vec<VRef>,
    /// Verses the reader has passed at least once ([`learned`]).
    pub learned: u32,
    /// Learned verses due for review, in verse order: the run's first part.
    pub due: Vec<VRef>,
    /// Verses not learned yet, in verse order. The run offers the first, and
    /// each next one only once the one before it is passed. In a passage being
    /// worked front to back these are its tail; a verse that was memorized on
    /// its own before the passage was added can leave a gap behind it.
    pub fresh: Vec<VRef>,
    /// Whether any verse of the row was graded on `now`'s calendar day.
    pub worked_today: bool,
    /// The day the longest-waiting verse of the run fell due.
    earliest_due: Option<i64>,
}

impl Run {
    /// Whether the row belongs in the queue: a verse is due, or there is a new
    /// verse and the row has not been touched today. Without the second half a
    /// passage would stay in the queue all day, every day, until its last
    /// verse — the reader who has done today's run is done; going further is
    /// the hub's drill button.
    pub fn actionable(&self) -> bool {
        !self.due.is_empty() || (!self.fresh.is_empty() && !self.worked_today)
    }
}

/// The cards read as rows.
pub struct Deck {
    cards: HashMap<VRef, Card>,
    slots: Vec<Slot>,
    /// Every verse some passage covers.
    covered: HashSet<VRef>,
}

impl Deck {
    pub fn new(cards: &HashMap<VRef, Card>) -> Deck {
        let cards = spread(cards);
        let covered: HashSet<VRef> = cards.values().filter(|c| c.through.is_some()).flat_map(Card::verses).collect();
        let mut slots: Vec<Slot> = cards
            .values()
            .filter(|c| c.through.is_some() || !covered.contains(&c.verse))
            .map(|c| Slot { head: c.verse.clone(), label: c.label(), verses: c.verses() })
            .collect();
        slots.sort_by_key(|s| s.head.reading_key());
        Deck { cards, slots, covered }
    }

    /// The rows, in reading order.
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// The run for the row at `head`. A verse inside a passage that is not its
    /// first answers as a run of that one verse, so any card can be drilled by
    /// its own ref. `None` when nothing is being memorized there.
    pub fn run(&self, head: &VRef, now: &str) -> Option<Run> {
        if let Some(slot) = self.slots.iter().find(|s| s.head == *head) {
            return Some(self.run_of(slot, now));
        }
        let card = self.cards.get(head)?;
        Some(self.run_of(&Slot { head: head.clone(), label: card.verse.ref_key(), verses: vec![head.clone()] }, now))
    }

    fn run_of(&self, slot: &Slot, now: &str) -> Run {
        let today = now.get(..10).unwrap_or(now);
        let mut run = Run {
            head: slot.head.clone(),
            label: slot.label.clone(),
            verses: slot.verses.clone(),
            learned: 0,
            due: Vec::new(),
            fresh: Vec::new(),
            worked_today: false,
            earliest_due: None,
        };
        for v in &slot.verses {
            let card = self.cards.get(v);
            // A failed first attempt is today's work too, though it taught nothing.
            run.worked_today |= card.is_some_and(|c| c.reviews.iter().any(|r| r.at.starts_with(today)));
            let mut waiting_since = |card: &Card| {
                let day = date_to_days(&card.due).unwrap_or(i64::MIN);
                run.earliest_due = Some(run.earliest_due.map_or(day, |d| d.min(day)));
            };
            let Some(card) = card.filter(|c| learned(c)) else {
                // The verse offered next has waited since it was added, if it
                // has a card: a verse added last week leads one added today.
                if let (true, Some(card)) = (run.fresh.is_empty(), card) {
                    waiting_since(card);
                }
                run.fresh.push(v.clone());
                continue;
            };
            run.learned += 1;
            if is_due(card, now) {
                waiting_since(card);
                run.due.push(v.clone());
            }
        }
        run
    }

    fn runs(&self, now: &str) -> Vec<Run> {
        self.slots.iter().map(|s| self.run_of(s, now)).collect()
    }

    /// The rows with work in them at `now`, in reading order.
    pub fn queue(&self, now: &str) -> Vec<VRef> {
        self.runs(now).into_iter().filter(Run::actionable).map(|r| r.head).collect()
    }

    /// [`Deck::queue`] under a daily `cap` (`0` = no limit), most overdue first.
    ///
    /// The cap counts rows worked today. A row worked today that still has
    /// verses due — a run the reader left part-way — is always offered and is
    /// not charged twice: it is in the count already.
    pub fn today(&self, now: &str, cap: u32) -> TodayQueue {
        let today = date_to_days(now).unwrap_or(i64::MAX);
        let runs = self.runs(now);
        let reviewed_today = runs.iter().filter(|r| r.worked_today).count();
        let mut due: Vec<&Run> = runs.iter().filter(|r| r.actionable()).collect();
        due.sort_by_key(|r| (r.earliest_due.unwrap_or(today).min(today), r.head.reading_key()));
        let mut room = if cap == 0 { usize::MAX } else { (cap as usize).saturating_sub(reviewed_today) };
        let refs = due
            .iter()
            .filter(|r| {
                let take = r.worked_today || room > 0;
                if take && !r.worked_today {
                    room -= 1;
                }
                take
            })
            .map(|r| r.head.clone())
            .collect();
        TodayQueue { refs, due_total: due.len(), reviewed_today, cap }
    }

    /// The hub's rows.
    pub fn list(&self, now: &str) -> Vec<CardSummary> {
        fn rank(m: Mastery) -> u8 {
            match m {
                Mastery::New => 0,
                Mastery::Learning => 1,
                Mastery::Young => 2,
                Mastery::Mature => 3,
            }
        }
        self.runs(now)
            .into_iter()
            .map(|run| {
                let cards: Vec<&Card> = run.verses.iter().filter_map(|v| self.cards.get(v)).collect();
                let lowest = cards
                    .iter()
                    .filter(|c| learned(c))
                    .map(|c| mastery(c))
                    .min_by_key(|m| rank(*m))
                    .unwrap_or(Mastery::New);
                CardSummary {
                    ref_key: run.head.ref_key(),
                    due: run.actionable(),
                    label: run.label,
                    verses: run.verses.len() as u32,
                    learned: run.learned,
                    mastery: lowest,
                    reps: cards.iter().map(|c| c.reps).sum(),
                    lapses: cards.iter().map(|c| c.lapses).sum(),
                }
            })
            .collect()
    }

    /// Every verse being memorized, once, in reading order, with its card if
    /// the reader has reached it.
    pub fn verses(&self) -> Vec<(VRef, Option<&Card>)> {
        let mut all: Vec<&VRef> = self.slots.iter().flat_map(|s| &s.verses).collect();
        all.sort_by_key(|v| v.reading_key());
        all.dedup();
        all.into_iter().map(|v| (v.clone(), self.cards.get(v))).collect()
    }

    /// The coverage map's verses.
    pub fn coverage(&self, now: &str) -> Vec<VerseCoverage> {
        self.verses()
            .into_iter()
            .map(|(verse, card)| VerseCoverage {
                ref_key: verse.ref_key(),
                mastery: card.map_or(Mastery::New, mastery),
                reps: card.map_or(0, |c| c.reps),
                lapses: card.map_or(0, |c| c.lapses),
                last_at: card.and_then(|c| c.reviews.last()).map(|r| r.at.clone()),
                due: card.is_some_and(|c| is_due(c, now)),
            })
            .collect()
    }

    /// How many verses `card` added to what is being memorized, on the day it
    /// was created: a passage adds all of its verses then, so the card a verse
    /// of it gets later adds none.
    pub fn added_by(&self, card: &Card) -> u32 {
        match &card.through {
            Some(_) => card.verses().len() as u32,
            None if self.covered.contains(&card.verse) => 0,
            None => 1,
        }
    }
}

#[cfg(test)]
mod tests;
