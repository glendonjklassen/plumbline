//! Unit tests for the panel producer, driven by a hand-built fake
//! [`PanelSource`] — so the block output is checked with no shell, no engine,
//! and no data files. Each view asserts the block shape + the pre-baked URIs a
//! shell routes back through the dispatcher.

use super::*;
use std::collections::HashMap;

/// A minimal in-memory source: enough tagged data to exercise every branch.
#[derive(Default)]
struct Fake {
    full: bool,
    words: HashMap<(String, u32), String>,
    displays: HashMap<String, String>,
    morph: HashMap<(String, u32), String>,
    occ_count: HashMap<String, usize>,
    entries: HashMap<String, StrongsView>,
    glosses: HashMap<String, String>,
    lemmas: HashMap<String, String>,
    renderings: HashMap<String, Vec<RenderingView>>,
    rendering_refs: HashMap<(String, String), RenderingRefsView>,
    word_codes: HashMap<String, Vec<String>>,
    occurrences: HashMap<String, OccurrencesView>,
    bridge: HashMap<String, Vec<BridgePartnerView>>,
    concept: HashMap<String, ConceptView>,
    xrefs: HashMap<String, Vec<XrefView>>,
    study_xrefs: HashMap<String, Vec<StudyXrefView>>,
    verse_tags: HashMap<String, Vec<(usize, String)>>,
    notes: HashMap<String, Vec<String>>,
    user_notes: HashMap<String, String>,
    threads: Vec<ThreadView>,
    tags: Vec<TagView>,
    weaves: Vec<WeaveView>,
    suggested: Vec<SuggestedView>,
    tokens: HashMap<String, VerseTokensView>,
    bodies: HashMap<String, String>,
    search: Option<SearchView>,
    word_usage: Option<WordUsageView>,
    code_usage: Option<WordUsageView>,
}

impl PanelSource for Fake {
    fn token_word(&self, verse: &str, token: u32) -> Option<String> {
        self.words.get(&(verse.to_string(), token)).cloned()
    }
    fn verse_display(&self, refkey: &str) -> Option<String> {
        self.displays.get(refkey).cloned()
    }
    fn morph_gloss(&self, verse: &str, token: u32) -> Option<String> {
        self.morph.get(&(verse.to_string(), token)).cloned()
    }
    fn occurrence_count(&self, code: &str) -> usize {
        self.occ_count.get(code).copied().unwrap_or(0)
    }
    fn word_usage(&self, _word: &str, _scope: &str, _page: u32) -> Option<WordUsageView> {
        self.word_usage.clone()
    }
    fn code_usage(&self, _code: &str, _scope: &str, _page: u32) -> Option<WordUsageView> {
        self.code_usage.clone()
    }
    fn strongs(&self, code: &str) -> Option<StrongsView> {
        self.entries.get(code).cloned()
    }
    fn gloss(&self, code: &str) -> Option<String> {
        self.glosses.get(code).cloned()
    }
    fn chip(&self, code: &str) -> ChipView {
        ChipView {
            code: code.to_string(),
            gloss: self.glosses.get(code).cloned(),
            lemma: self.lemmas.get(code).cloned(),
        }
    }
    fn renderings(&self, code: &str) -> Vec<RenderingView> {
        self.renderings.get(code).cloned().unwrap_or_default()
    }
    fn rendering_refs(&self, code: &str, rendering: &str) -> Option<RenderingRefsView> {
        self.rendering_refs.get(&(code.to_string(), render_key(rendering))).cloned()
    }
    fn word_codes(&self, word: &str) -> Vec<String> {
        self.word_codes.get(word).cloned().unwrap_or_default()
    }
    fn occurrences(&self, code: &str) -> OccurrencesView {
        self.occurrences.get(code).cloned().unwrap_or_default()
    }
    fn bridge_partners(&self, code: &str) -> Vec<BridgePartnerView> {
        self.bridge.get(code).cloned().unwrap_or_default()
    }
    fn concept(&self, code: &str) -> Option<ConceptView> {
        self.concept.get(code).cloned()
    }
    fn verse_xrefs(&self, verse: &str) -> Vec<XrefView> {
        self.xrefs.get(verse).cloned().unwrap_or_default()
    }
    fn study_xrefs(&self, verse: &str) -> Vec<StudyXrefView> {
        self.study_xrefs.get(verse).cloned().unwrap_or_default()
    }
    fn verse_tags(&self, verse: &str) -> Vec<(usize, String)> {
        self.verse_tags.get(verse).cloned().unwrap_or_default()
    }
    fn verse_notes(&self, verse: &str) -> Vec<String> {
        self.notes.get(verse).cloned().unwrap_or_default()
    }
    fn user_note(&self, verse: &str) -> Option<String> {
        self.user_notes.get(verse).cloned()
    }
    fn threads(&self) -> Vec<ThreadView> {
        self.threads.clone()
    }
    fn tags(&self) -> Vec<TagView> {
        self.tags.clone()
    }
    fn weaves(&self) -> Vec<WeaveView> {
        self.weaves.clone()
    }
    fn suggested(&self) -> Vec<SuggestedView> {
        self.suggested.clone()
    }
    fn verse_tokens(&self, refkey: &str) -> Option<VerseTokensView> {
        self.tokens.get(refkey).cloned()
    }
    fn verse_body(&self, refkey: &str) -> Option<String> {
        self.bodies.get(refkey).cloned()
    }
    fn search(&self, _query: &str) -> SearchView {
        self.search.clone().unwrap_or(SearchView::Hits {
            how: String::new(),
            total: 0,
            capped: false,
            hits: Vec::new(),
        })
    }
}

/// Every link `uri` present across the blocks (order-preserving), for asserting
/// the routes a shell would fire.
fn uris(blocks: &[Block]) -> Vec<String> {
    let mut out = Vec::new();
    for b in blocks {
        if let Block::Para { runs, .. } = b {
            for r in runs {
                if let Some(u) = &r.uri {
                    out.push(u.clone());
                }
            }
        }
    }
    out
}

/// The concatenated visible text of a block (runs joined), for content asserts.
fn text_of(b: &Block) -> String {
    match b {
        Block::Para { runs, .. } => runs.iter().map(|r| r.text.as_str()).collect(),
        Block::Section { title, mark } => {
            let mut s = title.clone();
            if let Some((g, _)) = mark {
                s.push_str("  ");
                s.push_str(g);
            }
            s
        }
        Block::Rule => String::new(),
    }
}

fn section_titles(blocks: &[Block]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|b| match b {
            Block::Section { title, .. } => Some(title.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn go_uri_splits_on_the_last_space() {
    assert_eq!(go_uri("Gen 1:7"), "go:Gen:1:7");
    assert_eq!(go_uri("1 John 3:16"), "go:1 John:3:16");
    assert_eq!(go_uri("weird"), "go:weird");
}

#[test]
fn parse_link_round_trips_the_producer_uris() {
    // The go: verb a producer bakes parses back to the same reference — even
    // for a multi-word book.
    assert_eq!(
        parse_link(&go_uri("1 John 3:16")),
        Some(PanelLink::Go { book: "1 John".into(), chapter: 3, verse: Some(16) })
    );
    assert_eq!(parse_link("go:John:3"), Some(PanelLink::Go { book: "John".into(), chapter: 3, verse: None }));

    // Read verbs.
    assert_eq!(parse_link("occ:G25"), Some(PanelLink::Occurrences { code: "G25".into() }));
    assert_eq!(
        parse_link("rend:G25:loved"),
        Some(PanelLink::Rendering { code: "G25".into(), rendering: "loved".into() })
    );
    assert_eq!(
        parse_link("code:G5368:loved"),
        Some(PanelLink::CodeStudy { code: "G5368".into(), word: "loved".into() })
    );
    // code: with no word is allowed.
    assert_eq!(parse_link("code:G5368"), Some(PanelLink::CodeStudy { code: "G5368".into(), word: "".into() }));
    assert_eq!(parse_link("thread:2"), Some(PanelLink::Thread { index: 2 }));
    assert_eq!(parse_link("tag:0"), Some(PanelLink::Tag { index: 0 }));
    assert_eq!(parse_link("weave:4"), Some(PanelLink::Weave { index: 4 }));

    // Write verbs (refkeys may contain spaces + a colon; only the verb splits).
    assert_eq!(parse_link("addtag:John 3:16"), Some(PanelLink::AddTag { refkey: "John 3:16".into() }));
    assert_eq!(parse_link("addthread:John 3:16"), Some(PanelLink::AddThread { refkey: "John 3:16".into() }));
    assert_eq!(parse_link("untag:1:John 3:16"), Some(PanelLink::Untag { tag: 1, refkey: "John 3:16".into() }));
    assert_eq!(parse_link("makeweave:2"), Some(PanelLink::MakeWeave { tag: 2 }));
    assert_eq!(parse_link("makeweave:x"), None);
    assert_eq!(parse_link("approve:3"), Some(PanelLink::Approve { index: 3 }));
    assert_eq!(parse_link("reject:3"), Some(PanelLink::Reject { index: 3 }));
    assert_eq!(parse_link("deletethread:2"), Some(PanelLink::DeleteThread { index: 2 }));
    assert_eq!(parse_link("deletetag:0"), Some(PanelLink::DeleteTag { index: 0 }));
    assert_eq!(parse_link("deleteweave:4"), Some(PanelLink::DeleteWeave { index: 4 }));
    assert_eq!(parse_link("deleteweave:x"), None);
    assert_eq!(parse_link("editthreadnotes:2"), Some(PanelLink::EditThreadNotes { index: 2 }));
    assert_eq!(parse_link("editweavenotes:5"), Some(PanelLink::EditWeaveNotes { index: 5 }));
    assert_eq!(parse_link("editentrynote:2:4"), Some(PanelLink::EditEntryNote { thread: 2, entry: 4 }));

    // Personal note + help verbs.
    assert_eq!(parse_link("editnote:John 3:16"), Some(PanelLink::EditNote { refkey: "John 3:16".into() }));
    assert_eq!(parse_link("guide"), Some(PanelLink::Guide));
    assert_eq!(parse_link("about"), Some(PanelLink::About));

    // Unknown verb / malformed payload → None (the shell ignores the click).
    assert_eq!(parse_link("bogus:x"), None);
    assert_eq!(parse_link("thread:notanumber"), None);
    assert_eq!(parse_link("go:John"), None); // chapter missing
    assert_eq!(parse_link("rend:G25"), None); // rendering missing
}

#[test]
fn simple_word_study_is_just_display_word_and_dictionary() {
    let mut f = Fake::default();
    f.displays.insert("John 3:16".into(), "John 3:16".into());
    f.words.insert(("John 3:16".into(), 1), "God".into());
    f.occ_count.insert("G2316".into(), 1317);
    f.entries.insert(
        "G2316".into(),
        StrongsView {
            lemma: Some("θεός".into()),
            def: Some("a deity".into()),
            kjv: Some("God".into()),
            ..Default::default()
        },
    );

    let blocks = word_study(&f, f.full, "John 3:16", 1, &["G2316".to_string()]);
    // Header, the big word, the reader's own note slot (near the top — their
    // words before the evidence), a rule, the code header, lemma, def, KJV.
    assert_eq!(text_of(&blocks[0]), "John 3:16");
    assert_eq!(text_of(&blocks[1]), "God");
    assert!(text_of(&blocks[2]).starts_with("✎"), "the note slot leads with the pencil");
    assert!(matches!(blocks[3], Block::Rule));
    assert_eq!(text_of(&blocks[4]), "G2316   1317 occurrences ▸");
    // The occurrence + note links are pre-baked; author actions are ungated.
    let u = uris(&blocks);
    assert!(u.contains(&"occ:G2316".to_string()));
    assert!(u.contains(&"editnote:John 3:16".to_string()));
    assert!(u.contains(&"addtag:John 3:16".to_string()));
    // No analysis sections with every gate off.
    assert!(section_titles(&blocks).is_empty());
    // No legend with every gate off.
    assert!(!blocks.iter().any(|b| text_of(b).contains("where this comes from")));
}

#[test]
fn untagged_word_says_so() {
    let mut f = Fake::default();
    f.displays.insert("John 3:16".into(), "John 3:16".into());
    f.words.insert(("John 3:16".into(), 0), "the".into());
    let blocks = word_study(&f, f.full, "John 3:16", 0, &[]);
    assert!(blocks.iter().any(|b| text_of(b) == "no Strong's tag on this word"));
}

#[test]
fn full_word_study_orders_the_tiers_and_marks_them() {
    let mut f = Fake { full: true, ..Default::default() };
    f.displays.insert("John 3:16".into(), "John 3:16".into());
    f.words.insert(("John 3:16".into(), 3), "loved".into());
    f.morph.insert(("John 3:16".into(), 3), "verb, aorist active".into());
    f.occ_count.insert("G25".into(), 43);
    f.entries.insert(
        "G25".into(),
        StrongsView { lemma: Some("ἀγαπάω".into()), def: Some("to love".into()), ..Default::default() },
    );
    f.renderings.insert(
        "G25".into(),
        vec![
            RenderingView { rendering: "loved".into(), total: 30 },
            RenderingView { rendering: "beloved".into(), total: 13 },
        ],
    );
    f.word_codes.insert("loved".into(), vec!["G25".into(), "G5368".into()]);
    f.glosses.insert("G5368".into(), "to be a friend".into());
    f.bridge.insert(
        "G25".into(),
        vec![BridgePartnerView {
            code: "H157".into(),
            sources: vec!["Septuagint".into(), "1769 renderings".into()],
            tiers: vec!["human".into(), "machine".into()],
            research_grade: true,
        }],
    );
    f.glosses.insert("H157".into(), "to love".into());
    f.concept.insert(
        "G25".into(),
        ConceptView {
            community: vec!["G26".into()],
            top_books: vec![("John".into(), 30)],
            ot: 0,
            nt: 43,
            leitwort: Some(LeitwortView { n: 43, win_count: 20, score: 4.2, label: "John 13–17".into() }),
        },
    );

    let blocks = word_study(&f, f.full, "John 3:16", 3, &["G25".to_string()]);
    let titles = section_titles(&blocks);
    // Tier order is fixed and complete.
    assert_eq!(
        titles,
        vec!["RENDERINGS", "SAME ROOT ACROSS TESTAMENTS", "APPEARS ALONGSIDE", "MOST USED IN", "LEITWORT",]
    );
    // The morph gloss appears (Full only).
    assert!(blocks.iter().any(|b| text_of(b) == "verb, aorist active"));
    // The tapped rendering "loved" is bold; "beloved" is not. (Match the
    // renderings list by its unique "beloved" run, not the big word display.)
    let rend_para = blocks
        .iter()
        .find_map(|b| match b {
            Block::Para { runs, .. } if runs.iter().any(|r| r.text == "beloved") => Some(runs),
            _ => None,
        })
        .unwrap();
    assert!(rend_para.iter().find(|r| r.text == "loved").unwrap().bold);
    assert!(!rend_para.iter().find(|r| r.text == "beloved").unwrap().bold);
    // The rendering chip routes rend:CODE:rendering.
    assert!(uris(&blocks).contains(&"rend:G25:loved".to_string()));
    // The reverse lens points at the other code's own card, carrying the word.
    assert!(uris(&blocks).contains(&"code:G5368:loved".to_string()));
    // Bridge partner sources are humanized + joined, and its tier marks show.
    let bridge_para = blocks.iter().find(|b| text_of(b).contains("Septuagint + 1769 renderings")).unwrap();
    if let Block::Para { runs, .. } = bridge_para {
        // human, machine, and the research flask (no god).
        assert!(runs.iter().any(|r| r.text == " †" && r.color == Color::TierHuman));
        assert!(runs.iter().any(|r| r.text == " ≈" && r.color == Color::TierMachine));
        assert!(runs.iter().any(|r| r.text == " ⚗" && r.color == Color::TierResearch));
        assert!(!runs.iter().any(|r| r.text == " ✝"));
    }
    // The concept-map link + legend close the card.
    assert!(blocks.iter().any(|b| text_of(b).contains("where this comes from")));
}

#[test]
fn verse_extras_gate_on_full_and_prebake_author_uris() {
    let mut f = Fake { full: true, ..Default::default() };
    f.displays.insert("John 3:16".into(), "John 3:16".into());
    f.words.insert(("John 3:16".into(), 1), "God".into());
    f.occ_count.insert("G2316".into(), 1);
    f.entries.insert("G2316".into(), StrongsView { lemma: Some("θεός".into()), ..Default::default() });
    f.xrefs.insert(
        "John 3:16".into(),
        vec![XrefView {
            verse: "John 3:18".into(),
            display: "John 3:18".into(),
            weave: "Belief".into(),
            weave_index: Some(2),
        }],
    );
    f.study_xrefs.insert(
        "John 3:16".into(),
        vec![StudyXrefView { to: "Rom 5:8".into(), to_display: "Rom 5:8".into(), end: None, end_display: None }],
    );
    f.verse_tags.insert("John 3:16".into(), vec![(0, "grace".into())]);
    f.notes.insert("John 3:16".into(), vec!["Or, begotten".into()]);

    let blocks = word_study(&f, f.full, "John 3:16", 1, &["G2316".to_string()]);
    let u = uris(&blocks);
    assert!(u.contains(&"addtag:John 3:16".to_string()));
    assert!(u.contains(&"addthread:John 3:16".to_string()));
    assert!(u.contains(&"weave:2".to_string())); // xref weave → compare card
    assert!(u.contains(&"go:John:3:18".to_string())); // xref verse → navigate
    assert!(u.contains(&"go:Rom:5:8".to_string())); // TSK
    assert!(u.contains(&"tag:0".to_string()));
    assert!(u.contains(&"untag:0:John 3:16".to_string()));
    assert!(blocks.iter().any(|b| text_of(b) == "Or, begotten"));
    // Text-only mode drops the TSK tier but KEEPS the reader's own data:
    // author actions, tags, margin notes, weave xrefs — tags accumulate in any
    // mode.
    let mut simple = f;
    simple.full = false;
    let sb = word_study(&simple, simple.full, "John 3:16", 1, &["G2316".to_string()]);
    let su = uris(&sb);
    assert!(su.iter().any(|x| x.starts_with("addtag")));
    assert!(su.iter().any(|x| x.starts_with("tag:")));
    assert!(!su.contains(&"go:Rom:5:8".to_string())); // TSK is human-gated
    assert!(sb.iter().any(|b| text_of(b) == "Or, begotten")); // margin notes survive
                                                              // Weave xrefs are the reader's own — still shown.
    assert!(su.contains(&"go:John:3:18".to_string()));
}

/// Tapping a verse says WHICH WEAVES IT IS IN, not just what it links to. The
/// two are different questions and the partner list answers only the second: a
/// verse with six links into one weave is one membership, and a verse in three
/// weaves must name all three even when one of them contributes a single link.
#[test]
fn a_verse_names_the_weaves_it_belongs_to_once_each() {
    let mut f = Fake { full: true, ..Default::default() };
    f.displays.insert("John 3:16".into(), "John 3:16".into());
    f.words.insert(("John 3:16".into(), 1), "God".into());
    f.occ_count.insert("G2316".into(), 1);
    f.entries.insert("G2316".into(), StrongsView { lemma: Some("θεός".into()), ..Default::default() });
    let x = |verse: &str, weave: &str, i: Option<usize>| XrefView {
        verse: verse.into(),
        display: verse.into(),
        weave: weave.into(),
        weave_index: i,
    };
    f.xrefs.insert(
        "John 3:16".into(),
        vec![
            // Three links into ONE weave, then a second weave, then back to the
            // first — the interleaving is what a canon-ordered partner list
            // actually looks like, and what a naive "dedupe neighbours" breaks on.
            x("John 3:18", "Belief", Some(2)),
            x("Rom 5:8", "Love of God", Some(5)),
            x("John 6:47", "Belief", Some(2)),
            x("1 John 4:9", "Love of God", Some(5)),
            // A weave the library cannot resolve: named, but not a link.
            x("Isa 53:5", "Unfiled", None),
        ],
    );

    let blocks = word_study(&f, f.full, "John 3:16", 1, &["G2316".to_string()]);
    let texts: Vec<String> = blocks.iter().map(text_of).collect();

    // The heading counts DISTINCT weaves (three), not the five links.
    assert!(texts.iter().any(|t| t == "in 3 weaves"), "membership heading missing from {texts:?}");
    // Each weave named exactly once, in the order its first link appeared.
    let names: Vec<&String> =
        texts.iter().filter(|t| ["Belief", "Love of God", "Unfiled"].contains(&t.as_str())).collect();
    assert_eq!(names, vec!["Belief", "Love of God", "Unfiled"], "each weave once, in first-seen order");

    // A resolvable weave is a link to its compare card; an unresolvable one is
    // still named, just not clickable.
    let u = uris(&blocks);
    assert!(u.contains(&"weave:2".to_string()));
    assert!(u.contains(&"weave:5".to_string()));
    // The partner list is still there and still separate — five links, all of them.
    // `go:` keeps the book's display form, space and all (go_uri splits on the
    // LAST space) — see go_uri_splits_on_the_last_space above.
    assert!(u.contains(&"go:1 John:4:9".to_string()));
    assert!(texts.iter().any(|t| t == "cross-references (5)"), "the partner list must stay");
}

/// One weave, one link: the heading has to be singular, and the section must
/// not appear at all for a verse in no weave.
#[test]
fn weave_membership_is_singular_at_one_and_absent_at_none() {
    let mut f = Fake { full: true, ..Default::default() };
    f.displays.insert("John 3:16".into(), "John 3:16".into());
    f.words.insert(("John 3:16".into(), 1), "God".into());
    f.occ_count.insert("G2316".into(), 1);
    f.entries.insert("G2316".into(), StrongsView { lemma: Some("θεός".into()), ..Default::default() });

    let bare = word_study(&f, f.full, "John 3:16", 1, &["G2316".to_string()]);
    assert!(
        !bare.iter().any(|b| text_of(b).starts_with("in ") && text_of(b).contains("weave")),
        "a verse in no weave gets no membership section"
    );

    f.xrefs.insert(
        "John 3:16".into(),
        vec![XrefView {
            verse: "John 3:18".into(),
            display: "John 3:18".into(),
            weave: "Belief".into(),
            weave_index: Some(2),
        }],
    );
    let one = word_study(&f, f.full, "John 3:16", 1, &["G2316".to_string()]);
    assert!(one.iter().any(|b| text_of(b) == "in 1 weave"), "singular at one");
}

#[test]
fn gates_split_human_and_machine_tiers() {
    let mut f = Fake::default();
    f.displays.insert("John 3:16".into(), "John 3:16".into());
    f.words.insert(("John 3:16".into(), 3), "loved".into());
    f.occ_count.insert("G25".into(), 43);
    f.entries.insert("G25".into(), StrongsView { lemma: Some("ἀγαπάω".into()), ..Default::default() });
    f.renderings.insert("G25".into(), vec![RenderingView { rendering: "loved".into(), total: 30 }]);
    f.concept.insert("G25".into(), ConceptView { community: vec!["G5368".into()], ..Default::default() });
    f.study_xrefs.insert(
        "John 3:16".into(),
        vec![StudyXrefView { to: "Rom 5:8".into(), to_display: "Rom 5:8".into(), end: None, end_display: None }],
    );

    // Human only: renderings + TSK, no machine analytics.
    let hb = word_study_gated(&f, Gates { human: true, machine: false }, "John 3:16", 3, &["G25".to_string()]);
    let ht = section_titles(&hb);
    assert!(ht.contains(&"RENDERINGS".to_string()));
    assert!(!ht.contains(&"APPEARS ALONGSIDE".to_string()));
    assert!(uris(&hb).contains(&"go:Rom:5:8".to_string()));

    // Machine only: analytics + concept map, no renderings/TSK.
    let mb = word_study_gated(&f, Gates { human: false, machine: true }, "John 3:16", 3, &["G25".to_string()]);
    let mt = section_titles(&mb);
    assert!(!mt.contains(&"RENDERINGS".to_string()));
    assert!(mt.contains(&"APPEARS ALONGSIDE".to_string()));
    assert!(!uris(&mb).contains(&"go:Rom:5:8".to_string()));
}

#[test]
fn code_study_card_is_standalone_with_legend_in_full() {
    let mut f = Fake { full: true, ..Default::default() };
    f.occ_count.insert("G5368".into(), 25);
    f.entries.insert(
        "G5368".into(),
        StrongsView { lemma: Some("φιλέω".into()), def: Some("to be a friend".into()), ..Default::default() },
    );
    let blocks = code_study_card(&f, f.full, "G5368", "loved");
    assert_eq!(text_of(&blocks[0]), ""); // opens with a rule
    assert!(matches!(blocks[0], Block::Rule));
    assert!(blocks.iter().any(|b| text_of(b).contains("where this comes from")));
}

#[test]
fn concordance_caps_and_counts() {
    let mut f = Fake::default();
    f.entries.insert("G2316".into(), StrongsView { lemma: Some("θεός".into()), ..Default::default() });
    f.occurrences.insert(
        "G2316".into(),
        OccurrencesView {
            total: 5,
            verses: vec![("John 3:16".into(), "John 3:16".into()), ("Rom 1:1".into(), "Rom 1:1".into())],
        },
    );
    let blocks = concordance(&f, "G2316");
    assert_eq!(text_of(&blocks[0]), "G2316  θεός");
    assert_eq!(text_of(&blocks[1]), "5 occurrences");
    assert!(uris(&blocks).contains(&"go:John:3:16".to_string()));
    // total (5) > shown (2) → an "N more" tail.
    assert!(blocks.iter().any(|b| text_of(b) == "… 3 more"));

    // An absent code says so.
    let empty = concordance(&f, "G9999");
    assert!(empty.iter().any(|b| text_of(b) == "no occurrences of G9999"));
}

#[test]
fn rendering_concordance_matches_by_normalized_key() {
    let mut f = Fake::default();
    f.rendering_refs.insert(
        ("G25".into(), render_key("Loved")),
        RenderingRefsView { rendering: "loved".into(), total: 1, refs: vec![("John 3:16".into(), "John 3:16".into())] },
    );
    // A differently-cased query still matches the normalized key.
    let blocks = rendering_concordance(&f, "G25", "LOVED");
    assert!(text_of(&blocks[0]).contains("“loved”"));
    assert!(uris(&blocks).contains(&"go:John:3:16".to_string()));
    // A missing rendering says so.
    let miss = rendering_concordance(&f, "G25", "hated");
    assert!(miss.iter().any(|b| text_of(b).contains("no “hated” rendering")));
}

#[test]
fn threads_list_and_detail() {
    let f = Fake {
        threads: vec![ThreadView {
            name: "Grace".into(),
            notes: "on unmerited favour".into(),
            entries: vec![
                ThreadEntryView {
                    verse: "John 1:14".into(),
                    display: "John 1:14".into(),
                    text: vec!["full".into(), "of".into(), "grace".into()],
                    note: Some("the Word".into()),
                },
                ThreadEntryView {
                    verse: "Eph 2:8".into(),
                    display: "Ephesians 2:8".into(),
                    text: vec!["by".into(), "grace".into()],
                    note: None,
                },
            ],
        }],
        ..Default::default()
    };
    let list = threads_list(&f);
    assert_eq!(text_of(&list[0]), "Threads (1)");
    assert!(uris(&list).contains(&"thread:0".to_string()));

    // The read view: clean rows — content, navigation, and ONE action: the
    // edit pencil, an icon pinned to the end of the name row. No drag (dropped
    // as the reorder gesture, 2026-08-30), no management links, no word
    // buttons.
    let detail = thread_detail(&f, 0, false);
    assert!(text_of(&detail[0]).starts_with("Grace"));
    let u = uris(&detail);
    assert!(u.contains(&"go:John:1:14".to_string()));
    assert!(u.contains(&"threadedit:0:1".to_string()), "{u:?}");
    assert!(!u.iter().any(|x| {
        x.starts_with("moveentry:")
            || x.starts_with("removeentry:")
            || x.starts_with("editentrynote:")
            || x.starts_with("editthreadnotes:")
            || x.starts_with("deletethread:")
    }));
    assert!(detail.iter().all(|b| !matches!(b, Block::Para { drag: Some(_), .. })));
    assert!(detail.iter().any(|b| text_of(b) == "“full of grace”"));
    assert!(detail.iter().any(|b| text_of(b) == "— the Word"));
    let pencil = detail
        .iter()
        .find_map(|b| match b {
            Block::Para { runs, .. } => runs.iter().find(|r| r.text == "✎"),
            _ => None,
        })
        .expect("the edit pencil");
    assert!(pencil.end);
    assert_eq!(pencil.uri.as_deref(), Some("threadedit:0:1"));

    // Edit mode: management moves here — delete across from the stat, the
    // Notes ＋ header, and per-entry ↑ ↓ ＋ ✕ pinned to the end of the
    // reference row (arrows omitted at the ends). The pencil, lit, leads out.
    let editing = thread_detail(&f, 0, true);
    let u = uris(&editing);
    assert!(u.contains(&"editthreadnotes:0".to_string()));
    assert!(u.contains(&"deletethread:0".to_string()));
    assert!(u.contains(&"editentrynote:0:0".to_string()));
    assert!(u.contains(&"removeentry:0:1".to_string()));
    assert!(u.contains(&"moveentry:0:0:1".to_string()));
    assert!(u.contains(&"moveentry:0:1:-1".to_string()));
    assert!(!u.contains(&"moveentry:0:0:-1".to_string()));
    assert!(!u.contains(&"moveentry:0:1:1".to_string()));
    assert!(u.contains(&"threadedit:0:0".to_string()));
    assert!(editing.iter().all(|b| !matches!(b, Block::Para { drag: Some(_), .. })));
    // The first entry's reference row carries its controls as end-pinned
    // glyphs, in order: ↓ (no ↑ at the top), note ＋, then ✕.
    let entry_row = editing
        .iter()
        .find_map(|b| match b {
            Block::Para { runs, .. } if runs.first().is_some_and(|r| r.text == "John 1:14") => Some(runs.clone()),
            _ => None,
        })
        .expect("the first entry's reference row");
    let trail: Vec<&str> = entry_row.iter().filter(|r| r.end).map(|r| r.text.as_str()).collect();
    assert_eq!(trail, ["↓", "＋", "✕"]);

    // An out-of-range index falls back to the list.
    assert_eq!(text_of(&thread_detail(&f, 9, false)[0]), "Threads (1)");
}

#[test]
fn tags_list_and_detail_verse_and_code_members() {
    let f = Fake {
        tags: vec![TagView {
            name: "kingdom".into(),
            category: None,
            members: vec![
                TagMemberView {
                    kind: "verse".into(),
                    verse: Some("Matt 6:33".into()),
                    display: Some("Matt 6:33".into()),
                    strongs: None,
                    note: Some("seek first".into()),
                },
                TagMemberView {
                    kind: "strongs".into(),
                    verse: None,
                    display: None,
                    strongs: Some("G932".into()),
                    note: None,
                },
            ],
        }],
        ..Default::default()
    };
    assert!(uris(&tags_list(&f)).contains(&"tag:0".to_string()));
    let d = tag_detail(&f, 0);
    let u = uris(&d);
    assert!(u.contains(&"go:Matt:6:33".to_string()));
    assert!(u.contains(&"occ:G932".to_string()));
    assert!(d.iter().any(|b| text_of(b).contains("seek first")));
}

/// `kind` decides the row, not whether `verse` happens to be filled. A member
/// tagged from a word study can carry both — a Strong's code and the verse it
/// was tagged from — and it is still a code row. Reading the presence of
/// `verse` alone turns every such member into a navigation link and loses the
/// occurrence lookup that is the whole point of tagging a code.
#[test]
fn a_code_member_stays_a_code_row_even_when_it_carries_a_verse() {
    let f = Fake {
        tags: vec![TagView {
            name: "kingdom".into(),
            category: None,
            members: vec![TagMemberView {
                kind: "strongs".into(),
                verse: Some("Matt 6:33".into()),
                display: Some("Matt 6:33".into()),
                strongs: Some("G932".into()),
                note: None,
            }],
        }],
        ..Default::default()
    };
    let u = uris(&tag_detail(&f, 0));
    assert!(u.contains(&"occ:G932".to_string()), "the code row is gone: {u:?}");
    assert!(!u.contains(&"go:Matt:6:33".to_string()), "a code member was rendered as a verse link: {u:?}");
}

#[test]
fn weaves_list_sorts_by_link_count_desc() {
    let mut f = Fake::default();
    let mk = |index, name: &str, n: usize| WeaveView {
        index,
        name: name.into(),
        kind_label: "type".into(),
        notes: String::new(),
        suggested: false,
        links: (0..n)
            .map(|_| WeaveLinkView {
                a: "Gen 1:1".into(),
                a_display: "Gen 1:1".into(),
                b: "John 1:1".into(),
                b_display: "John 1:1".into(),
                label: String::new(),
                span_a: None,
                span_b: None,
            })
            .collect(),
    };
    f.weaves = vec![mk(0, "small", 1), mk(1, "big", 3)];
    let list = weaves_list(&f);
    // "big" (3 links) sorts before "small" (1); links carry weave:INDEX.
    let u = uris(&list);
    assert_eq!(u, vec!["weave:1".to_string(), "weave:0".to_string()]);
}

#[test]
fn compare_card_spans_bold_and_added_italic() {
    let mut f = Fake { full: true, ..Default::default() };
    f.weaves = vec![WeaveView {
        index: 0,
        name: "Adam".into(),
        kind_label: "type".into(),
        notes: "first and last".into(),
        suggested: false,
        links: vec![WeaveLinkView {
            a: "Gen 2:7".into(),
            a_display: "Gen 2:7".into(),
            b: "1Cor 15:45".into(),
            b_display: "1Cor 15:45".into(),
            label: "living soul".into(),
            span_a: Some([1, 1]),
            span_b: None,
        }],
    }];
    f.tokens.insert(
        "Gen 2:7".into(),
        VerseTokensView {
            tokens: vec![
                TokenView { render: "a".into(), added: false },
                TokenView { render: "living".into(), added: false },
                TokenView { render: "soul".into(), added: true },
            ],
        },
    );
    let blocks = compare_card(&f, f.full, 0);
    assert_eq!(text_of(&blocks[0]), "Adam   type");
    assert!(uris(&blocks).contains(&"editweavenotes:0".to_string()));
    assert!(blocks.iter().any(|b| text_of(b) == "“living soul”"));
    // The Gen 2:7 token para: token 1 ("living") in span → bold; token 2
    // ("soul") is translator-added → italic + faded.
    let side = blocks
        .iter()
        .find_map(|b| match b {
            Block::Para { runs, indent: true, .. } if runs.iter().any(|r| r.text.trim() == "living") => Some(runs),
            _ => None,
        })
        .unwrap();
    assert!(side.iter().find(|r| r.text.trim() == "living").unwrap().bold);
    let soul = side.iter().find(|r| r.text.trim() == "soul").unwrap();
    assert!(soul.italic && soul.color == Color::Faded && !soul.bold);
}

#[test]
fn suggested_queue_actions() {
    let f = Fake {
        suggested: vec![SuggestedView {
            index: 0,
            name: "Ransom".into(),
            kind: "prophecy".into(),
            notes: String::new(),
            lib_index: Some(4),
            links: vec![SuggestedLinkView {
                a: "Isa 53:5".into(),
                a_display: "Isa 53:5".into(),
                b: "1Pet 2:24".into(),
                b_display: "1Pet 2:24".into(),
                label: "stripes".into(),
            }],
        }],
        ..Default::default()
    };
    let blocks = suggested(&f);
    let u = uris(&blocks);
    assert!(u.contains(&"approve:0".to_string()));
    assert!(u.contains(&"reject:0".to_string()));
    assert!(u.contains(&"weave:4".to_string())); // compare (lib_index)
    assert!(u.contains(&"editweavenotes:4".to_string()));
    assert!(u.contains(&"go:Isa:53:5".to_string()));
}

#[test]
fn search_goto_vs_hits_with_snippet() {
    // Goto: a direct navigation link.
    let g = Fake {
        search: Some(SearchView::Goto {
            book: "John".into(),
            chapter: 3,
            verse: Some(16),
            display: "John 3:16".into(),
        }),
        ..Default::default()
    };
    let gb = search(&g, "john 3:16");
    assert!(uris(&gb).contains(&"go:John:3:16".to_string()));

    // Hits: header + per-hit link + a snippet windowed around the match.
    let mut f = Fake {
        search: Some(SearchView::Hits {
            how: "phrase".into(),
            total: 2,
            capped: true,
            hits: vec![SearchHitView {
                verse: "John 3:16".into(),
                display: "John 3:16".into(),
                note: true,
                why: "3× love".into(),
            }],
        }),
        ..Default::default()
    };
    f.bodies.insert("John 3:16".into(), "For God so loved the world that he gave his only begotten Son".into());
    let blocks = search(&f, "loved");
    assert_eq!(text_of(&blocks[0]), "2 results");
    assert!(uris(&blocks).contains(&"go:John:3:16".to_string()));
    // The snippet bolds the matched word.
    let snip = blocks
        .iter()
        .find_map(|b| match b {
            Block::Para { runs, indent: true, .. } => Some(runs),
            _ => None,
        })
        .unwrap();
    assert!(snip.iter().any(|r| r.text == "loved" && r.bold));
    // capped → an "N more" tail (2 total − 1 shown).
    assert!(blocks.iter().any(|b| text_of(b) == "… 1 more"));
}

#[test]
fn word_study_shows_personal_note_and_edit_link_in_both_modes() {
    let mut f = Fake::default();
    f.displays.insert("John 3:16".into(), "John 3:16".into());
    f.words.insert(("John 3:16".into(), 1), "God".into());
    f.entries.insert("G2316".into(), StrongsView { lemma: Some("θεός".into()), ..Default::default() });
    f.user_notes.insert("John 3:16".into(), "the whole gospel in one verse".into());

    // Simple mode still surfaces the personal note + an edit link.
    let sb = word_study(&f, false, "John 3:16", 1, &["G2316".to_string()]);
    assert!(uris(&sb).contains(&"editnote:John 3:16".to_string()));
    assert!(sb.iter().any(|b| text_of(b) == "the whole gospel in one verse"));
    let row = sb.iter().map(text_of).find(|t| t.contains("edit")).expect("the note row");
    // ONE pencil on the row, not two. The label IS the glyph and the verb used to
    // carry a second — "✎   ✎ edit" (maintainer, 2026-08-26). Counting rather
    // than matching a literal, because the gap between them is three spaces and a
    // `contains("✎ edit")` would pass again the moment either side changes width.
    assert_eq!(row.matches('✎').count(), 1, "two pencils on the note row: {row:?}");
    assert!(row.contains("edit"));

    // With no note, the link says "add" — and still just the one pencil.
    let mut g = Fake::default();
    g.displays.insert("John 3:16".into(), "John 3:16".into());
    g.words.insert(("John 3:16".into(), 1), "God".into());
    g.entries.insert("G2316".into(), StrongsView::default());
    let gb = word_study(&g, false, "John 3:16", 1, &["G2316".to_string()]);
    let add = gb.iter().map(text_of).find(|t| t.contains("add")).expect("the note row");
    assert_eq!(add.matches('✎').count(), 1, "two pencils on the note row: {add:?}");
}

#[test]
fn guide_and_about_render_combined() {
    // Guide & About are now one combined card: the guide opens with its tour and
    // inlines the About content (edition + covenant) at the end.
    let guide = guide_blocks();
    assert!(guide.iter().any(|b| text_of(b).contains("Using Plumbline")));
    assert!(guide.iter().any(|b| text_of(b).contains("COVENANT")));
    // The standalone About card (the `about` link verb) still renders on its own.
    let about = about_blocks();
    assert!(about.iter().any(|b| text_of(b).contains("covenant") || text_of(b).contains("COVENANT")));
}

/// THE GUIDE IS THE LAST PLACE ENGLISH HID.
///
/// Roughly forty paragraphs of it lived as literals in this file — every other
/// user-visible string had been moved into the catalogue, so a German reader met a
/// fully German app right up until they opened Guide & About, and then met a wall
/// of English.
///
/// The completeness test in `i18n.rs` proves every id HAS German. It cannot prove
/// the guide asks for those ids: literals left behind would sail past it, since a
/// literal is not a missing key. So this reads the rendered card in German and
/// looks for the English it used to be built from.
///
/// The phrases are the section headings and a few distinctive sentence openings —
/// they must not appear in the German card at all. `PROVENANCE`/`BIBLIOGRAPHY.md`
/// are deliberately not among them: the credits name real projects and a filename,
/// which stay as they are in every language.
///
/// MUTATION: put any one paragraph back as a literal — e.g. `Block::para(vec![
/// Run::new("MAKE IT YOURS", …)])` in `guide_blocks` instead of its id. Red here,
/// and green in `every_shipped_string_is_translated`, which is why both exist.
#[test]
fn the_guide_is_readable_by_a_german_reader() {
    const ENGLISH_ONLY_IN_THE_OLD_GUIDE: [&str; 10] = [
        "Using Plumbline",
        "READ THE BIBLE",
        "SHARE THE GOSPEL",
        "PREPARE TO TEACH",
        "STUDY A PASSAGE",
        "HIDE IT IN YOUR HEART",
        "MAKE IT YOURS",
        "THE COVENANT",
        "Whatever you came here to do",
        "A weave is a connection you FIND",
    ];
    // `guide_blocks_in`, NOT `set_active`: the active language is a process global,
    // so the first draft of this test set it to German and broke eight unrelated
    // tests that were reading English in parallel at the time. That is what the
    // explicit-language entry point is for.
    let german = guide_blocks_in(crate::i18n::Lang::De).iter().map(text_of).collect::<Vec<_>>().join("\n");

    for phrase in ENGLISH_ONLY_IN_THE_OLD_GUIDE {
        assert!(
            !german.contains(phrase),
            "the German guide still says {phrase:?} — that paragraph is a literal, not a catalogue id"
        );
    }
    // And it is not empty or half-built: the German headings are really there.
    for id in ["guide.title", "guide.read.title", "guide.memorize.title", "about.covenant.title"] {
        let heading = crate::i18n::t(crate::i18n::Lang::De, id, &[]);
        assert!(german.contains(&heading), "the German guide is missing {id} ({heading:?})");
    }
    // Roughly forty paragraphs of prose, so a card that lost its body is caught
    // rather than passing for want of English.
    assert!(german.len() > 4_000, "the German guide is only {} characters — it lost its body", german.len());
    // A MISTYPED ID RENDERS AS ITSELF (`i18n::t` returns the id, on purpose), and
    // that is invisible to every assertion above: `guide.yours.p9` is not English
    // and not a missing key. So no id may survive into the rendered card.
    for leak in ["guide.", "about.", "panel."] {
        assert!(!german.contains(leak), "the German guide printed a raw id containing {leak:?}:\n{german}");
    }
}

/// A heading over nothing is a panel that looks broken. Threads, tags and the
/// review queue always said what to do when empty; the weave library and a
/// fruitless search did not — fixing them in the core fixes both shells, which is
/// the whole reason the copy lives here.
#[test]
fn an_empty_weave_library_says_what_to_do() {
    let f = Fake::default();
    let blocks = weaves_list(&f);
    assert_eq!(text_of(&blocks[0]), "Weaves (0)");
    assert!(blocks.len() > 1, "the empty weave library is a bare heading and nothing else");
    let body = blocks[1..].iter().map(text_of).collect::<Vec<_>>().join(" ");
    assert!(body.contains("make weave"), "the empty state should name the way in: {body}");
}

/// "0 results" alone leaves the reader unable to tell a typo from a broken
/// search. The guidance names the three shapes a query can take.
#[test]
fn a_search_with_no_hits_says_what_a_query_can_be() {
    let f = Fake {
        search: Some(SearchView::Hits { how: String::new(), total: 0, capped: false, hits: Vec::new() }),
        ..Default::default()
    };
    let blocks = search(&f, "quinquagesima");
    assert_eq!(text_of(&blocks[0]), "0 results");
    assert!(blocks.len() > 1, "0 results with no guidance under it");
    let body = blocks[1..].iter().map(text_of).collect::<Vec<_>>().join(" ");
    for want in ["John 3:16", "H430"] {
        assert!(body.contains(want), "the guidance should show a {want}-shaped example: {body}");
    }
}

/// And the guidance is for an empty result, not a decoration on every search: a
/// search WITH hits must not carry it.
#[test]
fn the_no_hits_guidance_stays_out_of_a_search_that_found_something() {
    let f = Fake {
        search: Some(SearchView::Hits {
            how: String::new(),
            total: 1,
            capped: false,
            hits: vec![SearchHitView {
                verse: "John 3:16".into(),
                display: "John 3:16".into(),
                note: false,
                why: String::new(),
            }],
        }),
        ..Default::default()
    };
    let body = search(&f, "loved").iter().map(text_of).collect::<Vec<_>>().join(" ");
    assert!(!body.contains("Nothing matched"), "the empty-state guidance leaked into a search with hits: {body}");
}

/// Every list producer answers with SOMETHING, whatever the reader has (or has
/// not) made. The web shell treats an empty block list as "not loaded yet", so a
/// producer that returned nothing would show "— loading —" for ever.
#[test]
fn no_list_producer_answers_with_nothing() {
    let f = Fake::default();
    for (what, blocks) in [
        ("threads", threads_list(&f)),
        ("tags", tags_list(&f)),
        ("weaves", weaves_list(&f)),
        ("suggested", suggested(&f)),
        ("search", search(&f, "nothing at all")),
    ] {
        assert!(!blocks.is_empty(), "the {what} panel is empty with an empty source");
    }
}

// ── the word-usage card (word-first study candidate) ──────────────────────────

fn usage_view() -> WordUsageView {
    WordUsageView {
        word: "mercy".into(),
        total: 3,
        in_scope: 3,
        ot: 2,
        nt: 1,
        books: vec![("Gen".into(), "Genesis".into(), 2), ("Matt".into(), "Matthew".into(), 1)],
        lines: vec![WordLineView {
            refkey: "Gen 1:1".into(),
            display: "Genesis 1:1".into(),
            segs: vec![("In the beginning ".into(), false), ("mercy".into(), true), (" endured.".into(), false)],
        }],
        page: 1,
        pages: 3,
    }
}

/// Every `(text, uri, bold)` in the card's paragraphs, flattened.
fn flat_runs(blocks: &[Block]) -> Vec<(String, Option<String>, bool)> {
    let mut out = Vec::new();
    for b in blocks {
        if let Block::Para { runs, .. } = b {
            for r in runs {
                out.push((r.text.clone(), r.uri.clone(), r.bold));
            }
        }
    }
    out
}

fn usage_query<'a>(word: &'a str, scope: &'a str, page: u32, codes: &'a [String]) -> UsageQuery<'a> {
    UsageQuery { word, lens: None, scope, page, origin: None, codes }
}

#[test]
fn wusage_link_round_trips() {
    assert_eq!(
        parse_link("wusage:2:mercy:book:Gen"),
        Some(PanelLink::WordUsage { word: "mercy".into(), scope: "book:Gen".into(), page: 2 })
    );
    // Scope omitted → "all".
    assert_eq!(
        parse_link("wusage:0:mercy"),
        Some(PanelLink::WordUsage { word: "mercy".into(), scope: "all".into(), page: 0 })
    );
    assert_eq!(parse_link("wusage:0::ot"), None); // word missing
    assert_eq!(parse_link("wusage:x:mercy:all"), None); // page not a number

    // The lens verb: page, code, word, then the scope remainder.
    assert_eq!(
        parse_link("lusage:1:H2617:mercy:book:Gen"),
        Some(PanelLink::CodeUsage { code: "H2617".into(), word: "mercy".into(), scope: "book:Gen".into(), page: 1 })
    );
    assert_eq!(parse_link("lusage:0::mercy:all"), None); // code missing

    // The thread edit-mode toggle.
    assert_eq!(parse_link("threadedit:3:1"), Some(PanelLink::ThreadEditMode { index: 3, edit: true }));
    assert_eq!(parse_link("threadedit:3:0"), Some(PanelLink::ThreadEditMode { index: 3, edit: false }));
    assert_eq!(parse_link("threadedit:3:2"), None);

    // What the producer bakes parses back unchanged.
    assert_eq!(
        parse_link(&wusage_uri("mercy", "book:Gen", 1)),
        Some(PanelLink::WordUsage { word: "mercy".into(), scope: "book:Gen".into(), page: 1 })
    );
    assert_eq!(
        parse_link(&lusage_uri("H2617", "mercy", "ot", 2)),
        Some(PanelLink::CodeUsage { code: "H2617".into(), word: "mercy".into(), scope: "ot".into(), page: 2 })
    );
}

#[test]
fn word_usage_card_waits_politely() {
    // A source with no word index (or one still warming) answers None; the
    // card carries the loading line rather than a false "0 occurrences".
    let f = Fake::default();
    let blocks = word_usage_card(&f, Gates::from_bits(0), &usage_query("mercy", "all", 0, &[]));
    let texts: Vec<String> = flat_runs(&blocks).into_iter().map(|(t, _, _)| t).collect();
    assert!(texts.iter().any(|t| t.contains("loading")), "{texts:?}");
}

#[test]
fn word_usage_card_lays_out_the_evidence() {
    let codes = vec!["H2617".to_string()];
    let f = Fake { word_usage: Some(usage_view()), ..Default::default() };
    let blocks = word_usage_card(&f, Gates::from_bits(0), &usage_query("mercy", "all", 1, &codes));
    let runs = flat_runs(&blocks);
    let uri_of = |text: &str| runs.iter().find(|(t, _, _)| t == text).and_then(|(_, u, _)| u.clone());

    // Scope chips, spelled like the search screen's: the active one is inert;
    // the others re-open at page 0.
    assert_eq!(uri_of("Everywhere"), None);
    assert_eq!(uri_of("Old Testament"), Some("wusage:0:mercy:ot".into()));
    assert_eq!(uri_of("New Testament"), Some("wusage:0:mercy:nt".into()));

    // Distribution: book chips in the order the view gave them — canon order.
    // The producer must never reorder these into a by-count ranking (the
    // card's no-ranking rule): sorting by count here would swap Genesis after
    // a higher-count book and this positional check would see it.
    let gen = runs.iter().position(|(t, _, _)| t == "Genesis").expect("Genesis chip");
    let matt = runs.iter().position(|(t, _, _)| t == "Matthew").expect("Matthew chip");
    assert!(gen < matt);
    assert_eq!(uri_of("Genesis"), Some("wusage:0:mercy:book:Gen".into()));

    // Paging from page 1 of 3: both arrows, scope carried.
    assert_eq!(uri_of("‹"), Some("wusage:0:mercy:all".into()));
    assert_eq!(uri_of("›"), Some("wusage:2:mercy:all".into()));

    // The code shows twice, correctly: as the lens chip (labelled by the code
    // here — this Fake has no dictionary entry to supply a lemma) and as the
    // dictionary footer, through the existing code: verb.
    let h_uris: Vec<String> = runs.iter().filter(|(t, _, _)| t == "H2617").filter_map(|(_, u, _)| u.clone()).collect();
    assert!(h_uris.contains(&"lusage:0:H2617:mercy:all".to_string()), "{h_uris:?}");
    assert!(h_uris.contains(&"code:H2617:mercy".to_string()), "{h_uris:?}");
}

#[test]
fn word_usage_card_lens_switches_to_the_original_word() {
    // A tagged word offers the original word as a chip, LABELLED BY LEMMA;
    // switching the lens re-links every scope/page verb through lusage:, so
    // the reader stays in the original word wherever they scope to.
    let codes = vec!["H2617".to_string()];
    let mut f = Fake { code_usage: Some(usage_view()), ..Default::default() };
    f.entries.insert(
        "H2617".into(),
        StrongsView { lemma: Some("חֶסֶד".into()), xlit: Some("chesed".into()), ..Default::default() },
    );

    let q = UsageQuery { word: "mercy", lens: Some("H2617"), scope: "all", page: 1, origin: None, codes: &codes };
    let blocks = word_usage_card(&f, Gates::from_bits(0), &q);
    let runs = flat_runs(&blocks);
    // First LINKED run with this text — the headline repeats the word inertly.
    let uri_of = |text: &str| runs.iter().filter(|(t, _, _)| t == text).find_map(|(_, u, _)| u.clone());

    // The surface chip leads back; the lemma chip is the active (inert) one,
    // and the original word is named in full for a reader without the script.
    assert_eq!(uri_of("mercy"), Some("wusage:0:mercy:all".into()));
    assert!(runs.iter().any(|(t, u, bold)| t == "חֶסֶד" && u.is_none() && *bold));
    assert!(runs.iter().any(|(t, _, _)| t == "chesed"));

    // Scope chips and paging carry the lens.
    assert_eq!(uri_of("Old Testament"), Some("lusage:0:H2617:mercy:ot".into()));
    assert_eq!(uri_of("›"), Some("lusage:2:H2617:mercy:all".into()));
}

#[test]
fn word_usage_card_notes_are_a_section_with_a_plus() {
    // The reader's note slot: a "Notes" header with a ＋, then what they wrote
    // (the old row read "✎  add" — maintainer, 2026-08-30).
    let f = Fake {
        word_usage: Some(usage_view()),
        displays: HashMap::from([("Gen 1:1".to_string(), "Genesis 1:1".to_string())]),
        user_notes: HashMap::from([("Gen 1:1".to_string(), "steadfast love".to_string())]),
        ..Default::default()
    };
    let q = UsageQuery { word: "mercy", lens: None, scope: "all", page: 0, origin: Some(("Gen 1:1", 2)), codes: &[] };
    let blocks = word_usage_card(&f, Gates::from_bits(0), &q);
    let runs = flat_runs(&blocks);
    assert!(runs.iter().any(|(t, u, bold)| t == "Notes" && u.is_none() && *bold));
    assert!(runs.iter().any(|(t, u, _)| t == "＋" && u.as_deref() == Some("editnote:Gen 1:1")));
    assert!(runs.iter().any(|(t, _, _)| t == "steadfast love"));
    // And no stray pencil: the glyph row this replaced is gone.
    assert!(!runs.iter().any(|(t, _, _)| t.contains('✎')));
    // The ＋ is pinned to the row's end, across from the header.
    let plus = blocks
        .iter()
        .find_map(|b| match b {
            Block::Para { runs, .. } => runs.iter().find(|r| r.text == "＋"),
            _ => None,
        })
        .expect("the notes ＋");
    assert!(plus.end);
}

#[test]
fn occurrence_lines_are_scripture_verbatim() {
    // No app words among the verses (the card's rule): the verse row must be
    // the segs' text EXACTLY — concatenated, it IS the verse — with the
    // studied word's tokens as the only emphasized runs.
    //
    // This fails against the bug it describes by construction rather than by
    // mutation (CLAUDE.md, UI testing): a producer that decorates the verse
    // row — a label, an ellipsis, a separator run — changes the concatenation
    // asserted below, and one that emphasizes anything else flips a `bold`.
    let f = Fake { word_usage: Some(usage_view()), ..Default::default() };
    let blocks = word_usage_card(&f, Gates::from_bits(0), &usage_query("mercy", "all", 1, &[]));
    let line = blocks
        .iter()
        .find_map(|b| match b {
            Block::Para { runs, indent: true, .. } => Some(runs.clone()),
            _ => None,
        })
        .expect("an indented verse row");
    let joined: String = line.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(joined, "In the beginning mercy endured.");
    let bolded: Vec<&str> = line.iter().filter(|r| r.bold).map(|r| r.text.as_str()).collect();
    assert_eq!(bolded, vec!["mercy"]);
}
