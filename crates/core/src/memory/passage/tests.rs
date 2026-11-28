//! A passage is memorized verse by verse, in order, in one place in the queue.
//!
//! Days are written out (`day(1)` is 2026-03-01) so a schedule can be read off
//! the test: which verses come back on which morning is the feature.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

const TOK: &str = "kjv1769-tok2";

/// Midday on the `n`th of March 2026.
fn day(n: u32) -> String {
    format!("2026-03-{n:02}T12:00:00Z")
}

fn ps(v: u16) -> VRef {
    VRef::new("Ps", 119, v)
}

fn keys(refs: &[VRef]) -> Vec<u16> {
    refs.iter().map(|r| r.verse).collect()
}

/// A home that is this test's alone: tests run in parallel in one process.
struct Home(PathBuf);
impl Home {
    fn new() -> Home {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("plumbline-passage-run-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Home(dir)
    }
    fn cards(&self) -> HashMap<VRef, Card> {
        let (cards, errors) = load_cards(&self.0);
        assert!(errors.is_empty(), "{errors:?}");
        cards
    }
    fn deck(&self) -> Deck {
        Deck::new(&self.cards())
    }
    /// Psalm 119:`from`–`to`, added on `at`.
    fn passage(&self, from: u16, to: u16, at: &str) {
        add(&self.0, &self.cards(), &ps(from), &ps(to), TOK, at).unwrap();
    }
    fn grade(&self, verse: u16, g: Grade, at: &str) -> Card {
        grade(&self.0, &self.cards(), &ps(verse), TOK, g, at).unwrap()
    }
    fn run(&self, head: u16, at: &str) -> Run {
        self.deck().run(&ps(head), at).expect("a row")
    }
    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.0.join("memory"))
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        names.sort();
        names
    }
    /// A passage card as a build before the split wrote it: one schedule for
    /// the chunk, no `perVerse`.
    fn undivided(&self, from: u16, to: u16, reviewed: &[(Grade, &str)]) -> Card {
        let mut card = Card::new_passage(ps(from), &ps(to), TOK, &day(1));
        card.per_verse = false;
        for (g, at) in reviewed {
            review(&mut card, *g, at);
        }
        write_card(&self.0, &card).unwrap();
        card
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ── one row, one place in the queue ──────────────────────────────────────────

#[test]
fn a_long_passage_is_one_row_and_one_place_in_the_queue() {
    let home = Home::new();
    home.passage(1, 38, &day(1));
    let deck = home.deck();
    assert_eq!(deck.slots().len(), 1);
    assert_eq!(deck.queue(&day(1)), [ps(1)], "38 verses, one entry");
    let list = deck.list(&day(1));
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].label.as_str(), list[0].verses, list[0].learned), ("Ps 119:1\u{2013}38", 38, 0));
    assert!(list[0].due);
    // Nothing is written ahead of the reader: one file until a verse is graded.
    assert_eq!(home.files(), ["ps-119-1.json"]);
}

#[test]
fn it_stays_one_row_however_many_of_its_verses_have_cards() {
    let home = Home::new();
    home.passage(1, 38, &day(1));
    for v in 1..=12 {
        home.grade(v, Grade::Hard, &day(1));
    }
    assert_eq!(home.files().len(), 12, "a card per verse reached");
    let deck = home.deck();
    assert_eq!(deck.slots().len(), 1);
    assert_eq!(deck.list(&day(2)).len(), 1);
    assert_eq!(deck.list(&day(2))[0].learned, 12);
    assert_eq!(deck.queue(&day(2)), [ps(1)]);
    assert_eq!(deck.today(&day(2), 0).refs, [ps(1)]);
}

// ── the run ──────────────────────────────────────────────────────────────────

#[test]
fn a_new_passage_offers_its_first_verse_and_nothing_to_review() {
    let home = Home::new();
    home.passage(1, 8, &day(1));
    let run = home.run(1, &day(1));
    assert!(run.due.is_empty());
    assert_eq!(keys(&run.fresh), [1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(run.learned, 0);
    assert!(run.actionable());
}

#[test]
fn passing_a_verse_opens_the_next_one_in_the_same_sitting() {
    let home = Home::new();
    home.passage(1, 8, &day(1));
    for (i, g) in [Grade::Hard, Grade::Good, Grade::Easy].into_iter().enumerate() {
        let next = i as u16 + 1;
        assert_eq!(home.run(1, &day(1)).fresh[0], ps(next), "before grading verse {next}");
        home.grade(next, g, &day(1));
    }
    let run = home.run(1, &day(1));
    assert_eq!(keys(&run.fresh), [4, 5, 6, 7, 8]);
    assert_eq!(run.learned, 3);
    assert!(run.due.is_empty(), "nothing graded today is due today");
}

#[test]
fn again_does_not_open_the_next_verse() {
    let home = Home::new();
    home.passage(1, 8, &day(1));
    home.grade(1, Grade::Good, &day(1));
    home.grade(2, Grade::Again, &day(1));
    // Verse 2 is still the one offered — now, and when the passage is opened
    // again later, which is what a rule kept only in the shell would lose.
    let run = home.run(1, &day(1));
    assert_eq!(run.fresh[0], ps(2));
    assert_eq!(run.learned, 1);
    home.grade(2, Grade::Again, &day(1));
    assert_eq!(home.run(1, &day(2)).fresh[0], ps(2), "and the next morning");
    home.grade(2, Grade::Hard, &day(2));
    assert_eq!(home.run(1, &day(2)).fresh[0], ps(3));
}

#[test]
fn the_maintainers_week() {
    // "Maybe I get through 1–4 today, and 2 I really kill, so I mark it good.
    //  Tomorrow it's gonna have me memorize 1, 3, 4, 5, and then I leave the app
    //  for a month, well it just offers me 1–4 again."
    let home = Home::new();
    home.passage(1, 38, &day(1));
    home.grade(1, Grade::Hard, &day(1));
    home.grade(2, Grade::Good, &day(1));
    home.grade(3, Grade::Hard, &day(1));
    home.grade(4, Grade::Hard, &day(1));

    let tomorrow = home.run(1, &day(2));
    assert_eq!(keys(&tomorrow.due), [1, 3, 4], "2 was known; it sits tomorrow out");
    assert_eq!(tomorrow.fresh[0], ps(5), "then the next new verse");
    assert!(tomorrow.actionable());

    let a_month_on = home.run(1, "2026-04-02T12:00:00Z");
    assert_eq!(keys(&a_month_on.due), [1, 2, 3, 4], "from the beginning again");
    assert_eq!(a_month_on.fresh[0], ps(5));
}

#[test]
fn verses_known_well_drop_out_of_the_run_and_the_rest_do_not() {
    let home = Home::new();
    home.passage(1, 6, &day(1));
    for v in 1..=3 {
        home.grade(v, Grade::Hard, &day(1));
    }
    // Day 2: all three back. 1 is passed again (→ 6 days), 2 lapses, 3 is passed.
    assert_eq!(keys(&home.run(1, &day(2)).due), [1, 2, 3]);
    home.grade(1, Grade::Good, &day(2));
    home.grade(2, Grade::Again, &day(2));
    home.grade(3, Grade::Good, &day(2));
    // Day 3: only the one that lapsed, then the new verse.
    let run = home.run(1, &day(3));
    assert_eq!(keys(&run.due), [2]);
    assert_eq!(run.fresh[0], ps(4));
    // A lapsed verse was learned once: it is a review, not the frontier.
    assert_eq!(run.learned, 3);
    // Day 8: 1 and 3 are due again (the 2nd + 6 days).
    home.grade(2, Grade::Good, &day(3));
    assert_eq!(keys(&home.run(1, &day(7)).due), [2]);
    assert_eq!(keys(&home.run(1, &day(8)).due), [1, 2, 3]);
}

#[test]
fn due_verses_come_in_verse_order_whatever_order_they_fell_due() {
    let home = Home::new();
    home.passage(1, 5, &day(1));
    home.grade(1, Grade::Easy, &day(1)); // due the 5th
    home.grade(2, Grade::Hard, &day(1)); // due the 2nd
    home.grade(3, Grade::Good, &day(1)); // due the 3rd
    assert_eq!(keys(&home.run(1, &day(2)).due), [2]);
    assert_eq!(keys(&home.run(1, &day(3)).due), [2, 3]);
    assert_eq!(keys(&home.run(1, &day(5)).due), [1, 2, 3]);
}

#[test]
fn a_finished_passage_has_no_new_verse_and_leaves_the_queue_until_one_is_due() {
    let home = Home::new();
    home.passage(1, 3, &day(1));
    for v in 1..=3 {
        home.grade(v, Grade::Easy, &day(1));
    }
    let run = home.run(1, &day(2));
    assert!(run.fresh.is_empty() && run.due.is_empty());
    assert!(!run.actionable());
    assert!(home.deck().queue(&day(2)).is_empty());
    assert!(!home.deck().list(&day(2))[0].due);
    assert_eq!(home.deck().queue(&day(5)), [ps(1)], "back when its verses fall due");
    assert_eq!(keys(&home.run(1, &day(5)).due), [1, 2, 3]);
}

// ── the queue and the day's cap ──────────────────────────────────────────────

#[test]
fn a_passage_worked_today_leaves_the_queue_though_it_has_verses_left() {
    let home = Home::new();
    home.passage(1, 38, &day(1));
    assert_eq!(home.deck().queue(&day(1)), [ps(1)]);
    home.grade(1, Grade::Good, &day(1));
    // 37 verses are unlearned, and the day's run is done all the same:
    // otherwise the tile would say "1 to review" every hour until verse 38.
    assert!(home.deck().queue(&day(1)).is_empty());
    assert!(!home.run(1, &day(1)).actionable());
    assert_eq!(home.deck().today(&day(1), 0).refs, Vec::<VRef>::new());
    // The run itself still offers the next verse, for the reader who asks.
    assert_eq!(home.run(1, &day(1)).fresh[0], ps(2));
    assert_eq!(home.deck().queue(&day(2)), [ps(1)], "and it is back in the morning");
}

#[test]
fn a_failed_first_verse_is_the_days_work_too() {
    let home = Home::new();
    home.passage(1, 4, &day(1));
    home.grade(1, Grade::Again, &day(1));
    let run = home.run(1, &day(1));
    assert!(run.worked_today && !run.actionable());
    assert_eq!(home.deck().today(&day(1), 5).reviewed_today, 1);
    assert_eq!(home.deck().queue(&day(2)), [ps(1)]);
}

#[test]
fn a_whole_passage_spends_one_of_the_days_cap() {
    let home = Home::new();
    home.passage(1, 38, &day(1));
    for v in [VRef::new("Gen", 1, 1), VRef::new("John", 3, 16)] {
        write_card(&home.0, &Card::new(v, TOK, &day(1))).unwrap();
    }
    // Cap 2 of three rows: Genesis and the psalm, in reading order.
    let q = home.deck().today(&day(1), 2);
    assert_eq!((q.due_total, q.reviewed_today), (3, 0));
    assert_eq!(q.refs, [VRef::new("Gen", 1, 1), ps(1)]);

    // Ten verses of the psalm in one sitting is one row worked, not ten.
    for v in 1..=10 {
        home.grade(v, Grade::Good, &day(1));
    }
    let q = home.deck().today(&day(1), 2);
    assert_eq!(q.reviewed_today, 1);
    assert_eq!(q.refs, [VRef::new("Gen", 1, 1)], "one of the cap's two is left");
    assert_eq!(q.due_total, 2);
}

#[test]
fn a_run_left_part_way_is_offered_again_and_not_charged_twice() {
    let home = Home::new();
    home.passage(1, 6, &day(1));
    for v in 1..=3 {
        home.grade(v, Grade::Hard, &day(1));
    }
    write_card(&home.0, &Card::new(VRef::new("John", 3, 16), TOK, &day(1))).unwrap();
    write_card(&home.0, &Card::new(VRef::new("Rom", 8, 28), TOK, &day(1))).unwrap();
    // Day 2: verse 1 reviewed, then the reader puts the phone down.
    home.grade(1, Grade::Good, &day(2));
    let run = home.run(1, &day(2));
    assert!(run.worked_today);
    assert_eq!(keys(&run.due), [2, 3]);

    // Cap 1 and one row worked: the cap is spent, and the unfinished run is
    // still there. John and Romans wait.
    let q = home.deck().today(&day(2), 1);
    assert_eq!(q.reviewed_today, 1);
    assert_eq!(q.refs, [ps(1)]);
    // Cap 2: the unfinished run, and ONE more — it took a place already. John
    // leads: it has waited since the 1st, the psalm's verses since the 2nd.
    let q = home.deck().today(&day(2), 2);
    assert_eq!(q.refs, [VRef::new("John", 3, 16), ps(1)]);
    assert_eq!(q.due_total, 3);
}

#[test]
fn the_most_overdue_row_leads_the_day() {
    let home = Home::new();
    home.passage(1, 4, &day(1));
    home.grade(1, Grade::Hard, &day(1)); // due the 2nd
    let mut gen = Card::new(VRef::new("Gen", 1, 1), TOK, &day(1));
    review(&mut gen, Grade::Good, &day(8)); // due the 10th
    write_card(&home.0, &gen).unwrap();
    write_card(&home.0, &Card::new(VRef::new("Exod", 3, 14), TOK, &day(12))).unwrap(); // new today
    let q = home.deck().today(&day(12), 0);
    assert_eq!(q.refs, [ps(1), VRef::new("Gen", 1, 1), VRef::new("Exod", 3, 14)]);
    // Reading order is the hub's, and the plain queue's.
    assert_eq!(home.deck().queue(&day(12)), [VRef::new("Gen", 1, 1), VRef::new("Exod", 3, 14), ps(1)]);
}

// ── one verse is the passage of one ──────────────────────────────────────────

#[test]
fn a_single_verse_runs_as_a_passage_of_one() {
    let home = Home::new();
    let v = VRef::new("John", 3, 16);
    write_card(&home.0, &Card::new(v.clone(), TOK, &day(1))).unwrap();
    let run = home.deck().run(&v, &day(1)).unwrap();
    assert_eq!((run.label.as_str(), run.verses.len()), ("John 3:16", 1));
    assert_eq!(run.fresh, std::slice::from_ref(&v));
    assert!(run.due.is_empty() && run.actionable());

    grade(&home.0, &home.cards(), &v, TOK, Grade::Hard, &day(1)).unwrap();
    let run = home.deck().run(&v, &day(1)).unwrap();
    assert!(run.fresh.is_empty() && run.due.is_empty() && !run.actionable());
    let run = home.deck().run(&v, &day(2)).unwrap();
    assert_eq!(run.due, std::slice::from_ref(&v));
    assert_eq!(run.learned, 1);
}

#[test]
fn grading_a_verse_nothing_is_memorizing_starts_a_single_card() {
    let home = Home::new();
    let v = VRef::new("John", 11, 35);
    assert!(home.deck().run(&v, &day(1)).is_none());
    let card = grade(&home.0, &home.cards(), &v, TOK, Grade::Good, &day(1)).unwrap();
    assert_eq!((card.through, card.per_verse, card.reps), (None, false, 1));
    assert_eq!(home.deck().slots().len(), 1);
}

#[test]
fn the_first_pass_spaces_by_how_well_it_went() {
    for (g, days) in [(Grade::Hard, 1), (Grade::Good, 2), (Grade::Easy, 4)] {
        let mut c = Card::new(ps(1), TOK, &day(1));
        review(&mut c, g, &day(1));
        assert_eq!(c.interval_days, days, "{g:?}");
        // A second pass graded Good is six days whatever the first was.
        review(&mut c, Grade::Good, &day(10));
        assert_eq!(c.interval_days, 6, "{g:?}");
    }
    for (g, days) in [(Grade::Hard, 3), (Grade::Good, 6), (Grade::Easy, 10)] {
        let mut c = Card::new(ps(1), TOK, &day(1));
        review(&mut c, Grade::Good, &day(1));
        review(&mut c, g, &day(3));
        assert_eq!(c.interval_days, days, "second pass, {g:?}");
    }
    let mut c = Card::new(ps(1), TOK, &day(1));
    review(&mut c, Grade::Again, &day(1));
    assert_eq!((c.interval_days, c.reps), (1, 0));
}

#[test]
fn the_buttons_can_say_when_each_grade_brings_the_verse_back() {
    let home = Home::new();
    home.passage(1, 4, &day(1));
    // New: tomorrow, tomorrow, 2 days, 4 days.
    let new = Card::new(ps(1), TOK, &day(1));
    assert_eq!(next_intervals(&new, &day(1)), [1, 1, 2, 4]);
    // Asking changed nothing.
    assert_eq!(new, Card::new(ps(1), TOK, &day(1)));
    // The second pass: three days, six, ten — and a lapse is tomorrow.
    let once = home.grade(1, Grade::Hard, &day(1));
    assert_eq!(next_intervals(&once, &day(2)), [1, 3, 6, 10]);
    let twice = home.grade(1, Grade::Good, &day(2));
    let [again, hard, good, easy] = next_intervals(&twice, &day(8));
    assert_eq!(again, 1);
    assert!(6 < hard && hard < good && good < easy, "{hard} {good} {easy}");
    // What it says is what the grade then does.
    assert_eq!(home.grade(1, Grade::Good, &day(8)).interval_days, good);
}

#[test]
fn no_two_passing_grades_ever_schedule_the_same_day() {
    // Three buttons with one caption is the defect: SM-2 gave "6 days" to all
    // three on the second pass, and from the third separated them only by the
    // ease, which rounding swallows on a short interval. Walked down every
    // path of grades four deep, at the ease floor too, where they sit closest.
    fn walk(card: &Card, depth: u32, path: &mut Vec<Grade>) {
        let [again, hard, good, easy] = next_intervals(card, &day(1));
        assert_eq!(again, 1, "{path:?}");
        assert!(hard < good && good < easy, "{path:?}: {hard} {good} {easy}");
        // And a pass never brings a verse back sooner than it last did.
        if card.reps > 0 {
            assert!(hard > card.interval_days, "{path:?}: {hard} after {}", card.interval_days);
        }
        if depth == 0 {
            return;
        }
        for g in [Grade::Again, Grade::Hard, Grade::Good, Grade::Easy] {
            let mut next = card.clone();
            review(&mut next, g, &day(1));
            path.push(g);
            walk(&next, depth - 1, path);
            path.pop();
        }
    }
    walk(&Card::new(ps(1), TOK, &day(1)), 4, &mut Vec::new());
    let floored = Card { ease: 1.3, reps: 2, interval_days: 3, ..Card::new(ps(1), TOK, &day(1)) };
    walk(&floored, 4, &mut Vec::new());
}

// ── what is on disk ──────────────────────────────────────────────────────────

#[test]
fn a_verse_of_a_passage_is_filed_as_any_single_verse_is() {
    let home = Home::new();
    home.passage(1, 4, &day(1));
    home.grade(1, Grade::Good, &day(1));
    home.grade(2, Grade::Good, &day(1));
    assert_eq!(home.files(), ["ps-119-1.json", "ps-119-2.json"]);
    let head = std::fs::read_to_string(home.0.join("memory/ps-119-1.json")).unwrap();
    assert!(head.contains("\"through\": \"Ps 119:4\"") && head.contains("\"perVerse\": true"), "{head}");
    // What a build before the split reads: a plain card on verse 2.
    let second = std::fs::read_to_string(home.0.join("memory/ps-119-2.json")).unwrap();
    assert!(!second.contains("through") && !second.contains("perVerse"), "{second}");
    assert!(second.contains("\"format\": \"overlay-memory-v1\""));
}

#[test]
fn grading_the_first_verse_keeps_the_passage_a_passage() {
    let home = Home::new();
    home.passage(1, 4, &day(1));
    for at in [day(1), day(3), day(9)] {
        home.grade(1, Grade::Good, &at);
    }
    let head = &home.cards()[&ps(1)];
    assert_eq!((head.through.clone(), head.per_verse, head.reviews.len()), (Some(ps(4)), true, 3));
}

#[test]
fn a_single_verse_card_never_gains_the_passage_key() {
    let card = Card::new(ps(1), TOK, &day(1));
    assert!(!to_json(&card).unwrap().contains("perVerse"));
    // Nor does a card that claims it without an end to its passage.
    let stray = Card { per_verse: true, ..card };
    assert!(!to_json(&stray).unwrap().contains("perVerse"));
}

// ── passages from before the split ───────────────────────────────────────────

#[test]
fn an_undivided_passage_reads_as_every_verse_on_the_chunks_schedule() {
    let home = Home::new();
    let chunk = home.undivided(1, 5, &[(Grade::Good, &day(1)), (Grade::Good, &day(3))]);
    assert_eq!(chunk.due, "2026-03-09");
    let run = home.run(1, &day(9));
    assert_eq!(keys(&run.due), [1, 2, 3, 4, 5], "not taught again from verse 1");
    assert!(run.fresh.is_empty());
    assert_eq!(run.learned, 5);
    assert!(home.run(1, &day(8)).due.is_empty(), "and not before the chunk was due");
    // Reading it wrote nothing.
    assert_eq!(home.files(), ["ps-119-1.json"]);
    assert!(!home.cards()[&ps(1)].per_verse);
}

#[test]
fn the_first_grade_splits_it_and_each_verse_then_goes_its_own_way() {
    let home = Home::new();
    home.undivided(1, 4, &[(Grade::Good, &day(1)), (Grade::Good, &day(3))]);
    home.grade(3, Grade::Again, &day(9));
    assert_eq!(home.files().len(), 4, "a card for every verse");
    let cards = home.cards();
    assert!(cards[&ps(1)].per_verse);
    // The graded verse lapsed; the others still hold the chunk's schedule.
    assert_eq!((cards[&ps(3)].reps, cards[&ps(3)].due.as_str()), (0, "2026-03-10"));
    for v in [1, 2, 4] {
        assert_eq!((cards[&ps(v)].reps, cards[&ps(v)].due.as_str()), (2, "2026-03-09"), "verse {v}");
    }
    // The chunk's two reviews stay on the first verse and nowhere else.
    assert_eq!(cards[&ps(1)].reviews.len(), 2);
    assert_eq!(cards[&ps(2)].reviews.len(), 0);
    assert_eq!(cards[&ps(3)].reviews.len(), 1);
    // Verse 3 lapsed with no pass in its own log: it is a review tomorrow, not
    // a verse the passage has yet to teach.
    assert!(learned(&cards[&ps(3)]));
    assert_eq!(cards[&ps(3)].extra.get(SPLIT_FROM), Some(&Value::String("Ps 119:1".into())));
    assert_eq!(activity_by_day(&cards).iter().map(|d| d.reviews).sum::<u32>(), 3);

    // Grading the first verse now moves the first verse only.
    home.grade(1, Grade::Good, &day(9));
    let cards = home.cards();
    assert_eq!(cards[&ps(1)].reps, 3);
    assert_eq!((cards[&ps(2)].reps, cards[&ps(2)].due.as_str()), (2, "2026-03-09"));
    assert_eq!(keys(&home.run(1, &day(10)).due), [2, 3, 4]);
}

#[test]
fn splitting_twice_changes_nothing() {
    let home = Home::new();
    home.undivided(1, 3, &[(Grade::Good, &day(1))]);
    home.grade(2, Grade::Good, &day(3));
    let after_one: Vec<String> =
        home.files().iter().map(|f| std::fs::read_to_string(home.0.join("memory").join(f)).unwrap()).collect();
    // A second device restores the undivided card over the split one; the next
    // grade there must arrive at the same verse cards, not reset them.
    let mut head = home.cards()[&ps(1)].clone();
    head.per_verse = false;
    write_card(&home.0, &head).unwrap();
    assert_eq!(home.cards()[&ps(2)].reps, 2, "the verse's own card is kept over the chunk's");
    home.grade(3, Grade::Good, &day(3));
    let cards = home.cards();
    assert!(cards[&ps(1)].per_verse);
    assert_eq!(cards[&ps(2)].reviews.len(), 1);
    assert_eq!(after_one.len(), 3);
    assert_eq!(std::fs::read_to_string(home.0.join("memory/ps-119-2.json")).unwrap(), after_one[1]);
}

#[test]
fn an_undivided_passage_never_reviewed_starts_at_its_first_verse() {
    let home = Home::new();
    home.undivided(1, 5, &[]);
    let run = home.run(1, &day(2));
    assert_eq!(keys(&run.fresh), [1, 2, 3, 4, 5]);
    assert!(run.due.is_empty());
    home.grade(1, Grade::Good, &day(2));
    assert_eq!(home.files(), ["ps-119-1.json"], "nothing to split: no verse had a schedule");
    assert!(home.cards()[&ps(1)].per_verse);
    assert_eq!(home.run(1, &day(2)).fresh[0], ps(2));
}

#[test]
fn an_undivided_passage_that_lapsed_is_still_learned() {
    let home = Home::new();
    // Passed, then failed: `reps` is 0 and it was known once.
    home.undivided(1, 3, &[(Grade::Good, &day(1)), (Grade::Again, &day(3))]);
    let run = home.run(1, &day(4));
    assert_eq!(keys(&run.due), [1, 2, 3]);
    assert!(run.fresh.is_empty(), "three reviews, not three new verses");
}

#[test]
fn a_verse_with_its_own_card_keeps_it_inside_an_undivided_passage() {
    let home = Home::new();
    let mut own = Card::new(ps(2), TOK, &day(1));
    for at in [day(1), day(3), day(9)] {
        review(&mut own, Grade::Easy, &at);
    }
    write_card(&home.0, &own).unwrap();
    home.undivided(1, 3, &[(Grade::Good, &day(1))]);
    let deck = spread(&home.cards());
    assert_eq!(deck[&ps(2)], own);
    assert_eq!(deck[&ps(3)].reps, 1);
    home.grade(3, Grade::Good, &day(3));
    assert_eq!(home.cards()[&ps(2)], own, "splitting does not write over it");
}

#[test]
fn a_file_without_the_key_loads_as_undivided_and_one_with_it_as_split() {
    let home = Home::new();
    std::fs::create_dir_all(home.0.join("memory")).unwrap();
    let file = |per_verse: &str| {
        format!(
            r#"{{"format":"overlay-memory-v1","ref":"Ps 119:1","through":"Ps 119:4"{per_verse},
                "tokenization":"kjv1769-tok2","created":"2026-03-01T12:00:00Z",
                "ease":2.5,"intervalDays":1,"reps":1,"lapses":0,"due":"2026-03-02",
                "reviews":[{{"at":"2026-03-01T12:00:00Z","grade":"good","intervalDays":1}}]}}"#
        )
    };
    std::fs::write(home.0.join("memory/ps-119-1.json"), file("")).unwrap();
    assert!(!home.cards()[&ps(1)].per_verse);
    assert_eq!(home.run(1, &day(2)).learned, 4, "the chunk's review covers all four");

    std::fs::write(home.0.join("memory/ps-119-1.json"), file(r#","perVerse":true"#)).unwrap();
    assert!(home.cards()[&ps(1)].per_verse);
    let run = home.run(1, &day(2));
    assert_eq!((run.learned, keys(&run.fresh)), (1, vec![2, 3, 4]), "that review was verse 1's alone");
}

// ── adding ───────────────────────────────────────────────────────────────────

#[test]
fn a_verse_already_memorized_becomes_the_passages_first_verse_and_keeps_its_work() {
    let home = Home::new();
    write_card(&home.0, &Card::new(ps(1), TOK, &day(1))).unwrap();
    home.grade(1, Grade::Good, &day(1));
    home.grade(1, Grade::Good, &day(3));
    home.passage(1, 6, &day(4));
    let head = &home.cards()[&ps(1)];
    assert_eq!((head.through.clone(), head.per_verse), (Some(ps(6)), true));
    assert_eq!((head.reps, head.reviews.len(), head.created.as_str()), (2, 2, day(1).as_str()));
    let run = home.run(1, &day(4));
    assert_eq!((run.learned, run.fresh[0].clone()), (1, ps(2)));
}

#[test]
fn adding_over_an_existing_passage_changes_nothing() {
    let home = Home::new();
    home.passage(1, 6, &day(1));
    home.grade(1, Grade::Good, &day(1));
    let before = std::fs::read_to_string(home.0.join("memory/ps-119-1.json")).unwrap();
    home.passage(1, 20, &day(2));
    assert_eq!(std::fs::read_to_string(home.0.join("memory/ps-119-1.json")).unwrap(), before);
}

#[test]
fn an_end_that_is_not_later_in_the_chapter_adds_the_one_verse() {
    for bad in [ps(3), ps(2), VRef::new("Ps", 120, 4), VRef::new("Prov", 119, 9)] {
        let home = Home::new();
        add(&home.0, &home.cards(), &ps(3), &bad, TOK, &day(1)).unwrap();
        let card = &home.cards()[&ps(3)];
        assert_eq!((card.through.clone(), card.per_verse), (None, false), "{}", bad.ref_key());
    }
    // And never turns a verse already there into anything else.
    let home = Home::new();
    write_card(&home.0, &Card::new(ps(3), TOK, &day(1))).unwrap();
    let before = home.cards();
    add(&home.0, &before, &ps(3), &ps(2), TOK, &day(2)).unwrap();
    assert_eq!(home.cards(), before);
}

#[test]
fn verses_memorized_singly_inside_a_new_passage_are_its_verses_not_rows_of_their_own() {
    let home = Home::new();
    write_card(&home.0, &Card::new(ps(3), TOK, &day(1))).unwrap();
    home.grade(3, Grade::Easy, &day(1));
    assert_eq!(home.deck().slots().len(), 1);
    home.passage(1, 5, &day(2));
    let deck = home.deck();
    assert_eq!(deck.slots().len(), 1, "verse 3 is inside the passage now");
    assert_eq!(deck.slots()[0].head, ps(1));
    let run = home.run(1, &day(2));
    assert_eq!(run.learned, 1);
    assert_eq!(keys(&run.fresh), [1, 2, 4, 5], "its work counts; the passage still starts at verse 1");
    assert_eq!(keys(&home.run(1, &day(5)).due), [3]);
    // Its own ref still answers, as a run of one, so anything holding it works.
    let own = deck.run(&ps(3), &day(5)).unwrap();
    assert_eq!((own.label.as_str(), own.verses.len(), keys(&own.due)), ("Ps 119:3", 1, vec![3]));
}

#[test]
fn two_passages_over_the_same_verses_are_two_rows_sharing_the_cards() {
    let home = Home::new();
    home.passage(1, 4, &day(1));
    home.passage(3, 6, &day(1));
    assert_eq!(
        home.deck().slots().iter().map(|s| s.label.clone()).collect::<Vec<_>>(),
        ["Ps 119:1\u{2013}4", "Ps 119:3\u{2013}6"]
    );
    home.grade(3, Grade::Hard, &day(1));
    assert_eq!(home.run(1, &day(2)).learned, 1);
    assert_eq!(home.run(3, &day(2)).learned, 1);
    assert_eq!(keys(&home.run(3, &day(2)).due), [3]);
    // One verse, one card, however many passages hold it.
    assert_eq!(home.deck().verses().len(), 6);
    assert_eq!(home.deck().coverage(&day(2)).len(), 6);
    let head = &home.cards()[&ps(3)];
    assert_eq!((head.through.clone(), head.reviews.len()), (Some(ps(6)), 1), "still a passage after its grade");
}

// ── removing ─────────────────────────────────────────────────────────────────

#[test]
fn removing_a_passage_removes_every_verses_card() {
    let home = Home::new();
    home.passage(1, 6, &day(1));
    for v in 1..=3 {
        home.grade(v, Grade::Good, &day(1));
    }
    write_card(&home.0, &Card::new(ps(40), TOK, &day(1))).unwrap();
    let plan = removal(&home.cards(), &ps(1));
    // The three with files; verses 4–6 never had one, and need no record.
    assert_eq!(keys(&plan.delete), [1, 2, 3]);
    assert_eq!(plan.rewrite, None);
    for v in &plan.delete {
        remove_card(&home.0, v).unwrap();
    }
    assert_eq!(home.files(), ["ps-119-40.json"], "and nothing outside it");
}

#[test]
fn removing_a_single_verse_removes_that_card() {
    let home = Home::new();
    write_card(&home.0, &Card::new(ps(40), TOK, &day(1))).unwrap();
    assert_eq!(removal(&home.cards(), &ps(40)), Removal { delete: vec![ps(40)], rewrite: None });
    // Nothing there: the same plan, and deleting it is a no-op.
    assert_eq!(removal(&home.cards(), &ps(41)).delete, [ps(41)]);
}

#[test]
fn removing_a_verse_inside_a_passage_makes_it_new_again_and_leaves_the_passage() {
    let home = Home::new();
    home.passage(1, 4, &day(1));
    home.grade(1, Grade::Good, &day(1));
    home.grade(2, Grade::Good, &day(1));
    let plan = removal(&home.cards(), &ps(2));
    assert_eq!(plan, Removal { delete: vec![ps(2)], rewrite: None });
    remove_card(&home.0, &ps(2)).unwrap();
    assert_eq!(home.deck().slots().len(), 1);
    assert_eq!(home.run(1, &day(1)).fresh[0], ps(2));
}

#[test]
fn removing_one_of_two_overlapping_passages_keeps_what_the_other_covers() {
    let home = Home::new();
    home.passage(1, 4, &day(1));
    home.passage(3, 6, &day(1));
    for v in 1..=6 {
        home.grade(v, Grade::Good, &day(1));
    }
    // 1–4 goes: verses 1 and 2 are its alone; 3 and 4 are the other's too.
    let plan = removal(&home.cards(), &ps(1));
    assert_eq!((keys(&plan.delete), plan.rewrite.clone()), (vec![1, 2], None));

    // 3–6 goes instead: 5 and 6 are its alone, 4 is shared — and 3, its own
    // first verse, is inside 1–4, so its card stays and stops being a passage.
    let plan = removal(&home.cards(), &ps(3));
    assert_eq!(keys(&plan.delete), [5, 6]);
    let kept = plan.rewrite.expect("verse 3's card is rewritten");
    assert_eq!((kept.verse.clone(), kept.through.clone(), kept.per_verse, kept.reviews.len()), (ps(3), None, false, 1));
    for v in &plan.delete {
        remove_card(&home.0, v).unwrap();
    }
    write_card(&home.0, &kept).unwrap();
    let deck = home.deck();
    assert_eq!(deck.slots().len(), 1);
    assert_eq!(home.run(1, &day(1)).learned, 4);
}

// ── the map, the sections, the pace ──────────────────────────────────────────

#[test]
fn the_map_holds_every_verse_and_shades_each_by_its_own_card() {
    let home = Home::new();
    home.passage(1, 5, &day(1));
    home.grade(1, Grade::Good, &day(1));
    home.grade(2, Grade::Again, &day(1));
    let cov = home.deck().coverage(&day(2));
    assert_eq!(
        cov.iter().map(|v| v.ref_key.as_str()).collect::<Vec<_>>(),
        ["Ps 119:1", "Ps 119:2", "Ps 119:3", "Ps 119:4", "Ps 119:5"]
    );
    assert_eq!(
        cov.iter().map(|v| v.mastery).collect::<Vec<_>>(),
        [Mastery::Young, Mastery::Learning, Mastery::New, Mastery::New, Mastery::New]
    );
    assert_eq!(cov[0].last_at.as_deref(), Some(day(1).as_str()));
    assert_eq!(cov[2].last_at, None);
    assert!(!cov[0].due && cov[1].due && !cov[2].due, "Good waits two days; Again is back tomorrow");
}

#[test]
fn the_row_shows_the_weakest_verse_learned_so_far() {
    let home = Home::new();
    home.passage(1, 5, &day(1));
    assert_eq!(home.deck().list(&day(1))[0].mastery, Mastery::New);
    for at in [day(1), day(3), day(9), day(25)] {
        home.grade(1, Grade::Easy, &at);
    }
    assert_eq!(mastery(&home.cards()[&ps(1)]), Mastery::Mature);
    assert_eq!(home.deck().list(&day(25))[0].mastery, Mastery::Mature);
    home.grade(2, Grade::Hard, &day(25));
    let row = &home.deck().list(&day(25))[0];
    assert_eq!((row.mastery, row.learned, row.verses), (Mastery::Young, 2, 5));
    assert_eq!(row.reps, 5, "four passes on verse 1, one on verse 2");
}

#[test]
fn a_section_counts_the_passages_verses_once_and_its_reviews_where_they_were_made() {
    let home = Home::new();
    home.passage(1, 10, &day(1));
    home.passage(8, 12, &day(1));
    home.grade(1, Grade::Good, &day(1));
    home.grade(8, Grade::Good, &day(1));
    home.grade(8, Grade::Good, &day(3));
    let sections = coverage_by_section(&home.cards());
    let with: Vec<_> = sections.iter().filter(|s| s.cards > 0).collect();
    assert_eq!(with.len(), 1);
    assert_eq!((with[0].cards, with[0].mature, with[0].reviews), (12, 0, 3), "12 verses, not 15");
}

#[test]
fn the_pace_counts_a_passages_verses_the_day_it_was_added() {
    let home = Home::new();
    home.passage(1, 10, &day(1));
    write_card(&home.0, &Card::new(VRef::new("John", 3, 16), TOK, &day(1))).unwrap();
    // A week on, three verses are reached for the first time: their cards are
    // created then, and they were added on the 1st.
    for v in 1..=3 {
        home.grade(v, Grade::Good, &day(8));
    }
    let windows = pace(&home.cards(), &day(8)).unwrap();
    let get = |id: &str| windows.iter().find(|w| w.id == id).unwrap();
    assert_eq!((get("day").added, get("day").reviews), (0, 3));
    assert_eq!(get("all").added, 11, "ten in the passage and the one verse");
}

// ── rows and labels ──────────────────────────────────────────────────────────

#[test]
fn rows_are_in_reading_order_passages_and_verses_together() {
    let home = Home::new();
    home.passage(9, 16, &day(1));
    for v in [VRef::new("Rev", 22, 21), VRef::new("Gen", 1, 1), ps(1), ps(40)] {
        write_card(&home.0, &Card::new(v, TOK, &day(1))).unwrap();
    }
    let labels: Vec<String> = home.deck().list(&day(1)).into_iter().map(|c| c.label).collect();
    assert_eq!(labels, ["Gen 1:1", "Ps 119:1", "Ps 119:9\u{2013}16", "Ps 119:40", "Rev 22:21"]);
}

#[test]
fn the_same_cards_always_read_as_the_same_rows() {
    // Rows come out of a hash map; the order must not.
    let home = Home::new();
    home.undivided(1, 6, &[(Grade::Good, &day(1))]);
    home.undivided(4, 9, &[(Grade::Good, &day(1)), (Grade::Good, &day(3))]);
    let first = Deck::new(&home.cards());
    for _ in 0..20 {
        let again = Deck::new(&home.cards());
        assert_eq!(again.slots(), first.slots());
        assert_eq!(again.list(&day(9)), first.list(&day(9)));
        assert_eq!(again.coverage(&day(9)), first.coverage(&day(9)));
    }
    // Where the two chunks overlap, the earlier one's schedule is the verse's.
    assert_eq!(spread(&home.cards())[&ps(5)].reps, 1);
    assert_eq!(spread(&home.cards())[&ps(7)].reps, 2);
}
