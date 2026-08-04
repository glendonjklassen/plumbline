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
    assert!(text_of(&blocks[2]).starts_with("your note"));
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
    // author actions, tags, margin notes, weave xrefs (2026-07-25 change —
    // tags accumulate in any mode).
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
            entries: vec![ThreadEntryView {
                verse: "John 1:14".into(),
                display: "John 1:14".into(),
                text: vec!["full".into(), "of".into(), "grace".into()],
                note: Some("the Word".into()),
            }],
        }],
        ..Default::default()
    };
    let list = threads_list(&f);
    assert_eq!(text_of(&list[0]), "Threads (1)");
    assert!(uris(&list).contains(&"thread:0".to_string()));

    let detail = thread_detail(&f, 0);
    assert_eq!(text_of(&detail[0]), "Grace");
    let u = uris(&detail);
    assert!(u.contains(&"editthreadnotes:0".to_string()));
    assert!(u.contains(&"editentrynote:0:0".to_string()));
    assert!(u.contains(&"go:John:1:14".to_string()));
    assert!(detail.iter().any(|b| text_of(b) == "“full of grace”"));
    assert!(detail.iter().any(|b| text_of(b) == "— the Word"));

    // An out-of-range index falls back to the list.
    assert_eq!(text_of(&thread_detail(&f, 9)[0]), "Threads (1)");
}

#[test]
fn tags_list_and_detail_verse_and_code_members() {
    let f = Fake {
        tags: vec![TagView {
            name: "kingdom".into(),
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
    assert!(sb.iter().any(|b| text_of(b).contains("✎ edit")));

    // With no note, the link says "add".
    let mut g = Fake::default();
    g.displays.insert("John 3:16".into(), "John 3:16".into());
    g.words.insert(("John 3:16".into(), 1), "God".into());
    g.entries.insert("G2316".into(), StrongsView::default());
    let gb = word_study(&g, false, "John 3:16", 1, &["G2316".to_string()]);
    assert!(gb.iter().any(|b| text_of(b).contains("✎ add")));
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
/// of English (2026-08-04).
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
