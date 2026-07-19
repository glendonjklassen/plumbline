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
    near: HashMap<String, (Vec<String>, Vec<String>)>,
    concept: HashMap<String, ConceptView>,
    xrefs: HashMap<String, Vec<XrefView>>,
    study_xrefs: HashMap<String, Vec<StudyXrefView>>,
    similar: HashMap<String, (Vec<SimilarView>, Vec<SimilarView>)>,
    verse_tags: HashMap<String, Vec<(usize, String)>>,
    notes: HashMap<String, Vec<String>>,
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
    fn concept_near(&self, code: &str, _k: usize) -> (Vec<String>, Vec<String>) {
        self.near.get(code).cloned().unwrap_or_default()
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
    fn similar_verses(&self, verse: &str, _k: usize) -> (Vec<SimilarView>, Vec<SimilarView>) {
        self.similar.get(verse).cloned().unwrap_or_default()
    }
    fn verse_tags(&self, verse: &str) -> Vec<(usize, String)> {
        self.verse_tags.get(verse).cloned().unwrap_or_default()
    }
    fn verse_notes(&self, verse: &str) -> Vec<String> {
        self.notes.get(verse).cloned().unwrap_or_default()
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
        self.search.clone().unwrap_or(SearchView::Hits { how: String::new(), total: 0, capped: false, hits: Vec::new() })
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
fn simple_word_study_is_just_display_word_and_dictionary() {
    let mut f = Fake::default();
    f.displays.insert("John 3:16".into(), "John 3:16".into());
    f.words.insert(("John 3:16".into(), 1), "God".into());
    f.occ_count.insert("G2316".into(), 1317);
    f.entries.insert(
        "G2316".into(),
        StrongsView { lemma: Some("θεός".into()), def: Some("a deity".into()), kjv: Some("God".into()), ..Default::default() },
    );

    let blocks = word_study(&f, f.full, "John 3:16", 1, &["G2316".to_string()]);
    // Header, the big word, a rule, the code header, lemma, def, KJV.
    assert_eq!(text_of(&blocks[0]), "John 3:16");
    assert_eq!(text_of(&blocks[1]), "God");
    assert!(matches!(blocks[2], Block::Rule));
    assert_eq!(text_of(&blocks[3]), "G2316   1317 occurrences ▸");
    // The occurrence link is pre-baked.
    assert!(uris(&blocks).contains(&"occ:G2316".to_string()));
    // No R&D sections in simple mode.
    assert!(section_titles(&blocks).is_empty());
    // No legend in simple mode.
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
    let mut f = Fake::default();
    f.full = true;
    f.displays.insert("John 3:16".into(), "John 3:16".into());
    f.words.insert(("John 3:16".into(), 3), "loved".into());
    f.morph.insert(("John 3:16".into(), 3), "verb, aorist active".into());
    f.occ_count.insert("G25".into(), 43);
    f.entries.insert("G25".into(), StrongsView { lemma: Some("ἀγαπάω".into()), def: Some("to love".into()), ..Default::default() });
    f.renderings.insert("G25".into(), vec![RenderingView { rendering: "loved".into(), total: 30 }, RenderingView { rendering: "beloved".into(), total: 13 }]);
    f.word_codes.insert("loved".into(), vec!["G25".into(), "G5368".into()]);
    f.glosses.insert("G5368".into(), "to be a friend".into());
    f.bridge.insert("G25".into(), vec![BridgePartnerView { code: "H157".into(), sources: vec!["Septuagint".into(), "1769 renderings".into()], tiers: vec!["human".into(), "machine".into()], research_grade: true }]);
    f.glosses.insert("H157".into(), "to love".into());
    f.near.insert("G25".into(), (vec!["G5368".into()], vec!["H157".into()]));
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
        vec![
            "RENDERINGS",
            "SAME ROOT ACROSS TESTAMENTS",
            "SIMILAR CONCEPTS",
            "APPEARS ALONGSIDE",
            "WHERE IT CONCENTRATES",
            "LEITWORT",
        ]
    );
    // The morph gloss appears (Full only).
    assert!(blocks.iter().any(|b| text_of(b) == "verb, aorist active"));
    // The tapped rendering "loved" is bold; "beloved" is not. (Match the
    // renderings list by its unique "beloved" run, not the big word display.)
    let rend_para = blocks.iter().find_map(|b| match b {
        Block::Para { runs, .. } if runs.iter().any(|r| r.text == "beloved") => Some(runs),
        _ => None,
    }).unwrap();
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
    assert!(uris(&blocks).contains(&"conceptmap:G25".to_string()));
    assert!(blocks.iter().any(|b| text_of(b).contains("where this comes from")));
}

#[test]
fn verse_extras_gate_on_full_and_prebake_author_uris() {
    let mut f = Fake::default();
    f.full = true;
    f.displays.insert("John 3:16".into(), "John 3:16".into());
    f.words.insert(("John 3:16".into(), 1), "God".into());
    f.occ_count.insert("G2316".into(), 1);
    f.entries.insert("G2316".into(), StrongsView { lemma: Some("θεός".into()), ..Default::default() });
    f.xrefs.insert("John 3:16".into(), vec![XrefView { verse: "John 3:18".into(), display: "John 3:18".into(), weave: "Belief".into(), weave_index: Some(2) }]);
    f.study_xrefs.insert("John 3:16".into(), vec![StudyXrefView { to: "Rom 5:8".into(), to_display: "Rom 5:8".into(), end: None, end_display: None }]);
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
    // Simple mode drops the author actions + TSK + tags but keeps margin notes.
    let mut simple = f;
    simple.full = false;
    let sb = word_study(&simple, simple.full, "John 3:16", 1, &["G2316".to_string()]);
    let su = uris(&sb);
    assert!(!su.iter().any(|x| x.starts_with("addtag")));
    assert!(!su.iter().any(|x| x.starts_with("tag:")));
    assert!(sb.iter().any(|b| text_of(b) == "Or, begotten")); // margin notes survive
    // Weave xrefs are not full-gated, so they still show in simple mode.
    assert!(su.contains(&"go:John:3:18".to_string()));
}

#[test]
fn code_study_card_is_standalone_with_legend_in_full() {
    let mut f = Fake::default();
    f.full = true;
    f.occ_count.insert("G5368".into(), 25);
    f.entries.insert("G5368".into(), StrongsView { lemma: Some("φιλέω".into()), def: Some("to be a friend".into()), ..Default::default() });
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
        OccurrencesView { total: 5, verses: vec![("John 3:16".into(), "John 3:16".into()), ("Rom 1:1".into(), "Rom 1:1".into())] },
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
    let mut f = Fake::default();
    f.threads = vec![ThreadView {
        name: "Grace".into(),
        notes: "on unmerited favour".into(),
        entries: vec![ThreadEntryView {
            verse: "John 1:14".into(),
            display: "John 1:14".into(),
            text: vec!["full".into(), "of".into(), "grace".into()],
            note: Some("the Word".into()),
        }],
    }];
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
    let mut f = Fake::default();
    f.tags = vec![TagView {
        name: "kingdom".into(),
        members: vec![
            TagMemberView { kind: "verse".into(), verse: Some("Matt 6:33".into()), display: Some("Matt 6:33".into()), strongs: None, note: Some("seek first".into()) },
            TagMemberView { kind: "strongs".into(), verse: None, display: None, strongs: Some("G932".into()), note: None },
        ],
    }];
    assert!(uris(&tags_list(&f)).contains(&"tag:0".to_string()));
    let d = tag_detail(&f, 0);
    let u = uris(&d);
    assert!(u.contains(&"go:Matt:6:33".to_string()));
    assert!(u.contains(&"occ:G932".to_string()));
    assert!(d.iter().any(|b| text_of(b).contains("seek first")));
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
            .map(|_| WeaveLinkView { a: "Gen 1:1".into(), a_display: "Gen 1:1".into(), b: "John 1:1".into(), b_display: "John 1:1".into(), label: String::new(), span_a: None, span_b: None })
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
    let mut f = Fake::default();
    f.full = true;
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
        VerseTokensView { tokens: vec![
            TokenView { render: "a".into(), added: false },
            TokenView { render: "living".into(), added: false },
            TokenView { render: "soul".into(), added: true },
        ] },
    );
    let blocks = compare_card(&f, f.full, 0);
    assert_eq!(text_of(&blocks[0]), "Adam   type");
    assert!(uris(&blocks).contains(&"editweavenotes:0".to_string()));
    assert!(blocks.iter().any(|b| text_of(b) == "“living soul”"));
    // The Gen 2:7 token para: token 1 ("living") in span → bold; token 2
    // ("soul") is translator-added → italic + faded.
    let side = blocks.iter().find_map(|b| match b {
        Block::Para { runs, indent: true, .. } if runs.iter().any(|r| r.text.trim() == "living") => Some(runs),
        _ => None,
    }).unwrap();
    assert!(side.iter().find(|r| r.text.trim() == "living").unwrap().bold);
    let soul = side.iter().find(|r| r.text.trim() == "soul").unwrap();
    assert!(soul.italic && soul.color == Color::Faded && !soul.bold);
}

#[test]
fn suggested_queue_actions() {
    let mut f = Fake::default();
    f.suggested = vec![SuggestedView {
        index: 0,
        name: "Ransom".into(),
        kind: "prophecy".into(),
        notes: String::new(),
        lib_index: Some(4),
        links: vec![SuggestedLinkView { a: "Isa 53:5".into(), a_display: "Isa 53:5".into(), b: "1Pet 2:24".into(), b_display: "1Pet 2:24".into(), label: "stripes".into() }],
    }];
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
    let mut g = Fake::default();
    g.search = Some(SearchView::Goto { book: "John".into(), chapter: 3, verse: Some(16), display: "John 3:16".into() });
    let gb = search(&g, "john 3:16");
    assert!(uris(&gb).contains(&"go:John:3:16".to_string()));

    // Hits: header + per-hit link + a snippet windowed around the match.
    let mut f = Fake::default();
    f.search = Some(SearchView::Hits {
        how: "phrase".into(),
        total: 2,
        capped: true,
        hits: vec![SearchHitView { verse: "John 3:16".into(), display: "John 3:16".into(), note: true, why: "3× love".into() }],
    });
    f.bodies.insert("John 3:16".into(), "For God so loved the world that he gave his only begotten Son".into());
    let blocks = search(&f, "loved");
    assert_eq!(text_of(&blocks[0]), "2 results");
    assert!(uris(&blocks).contains(&"go:John:3:16".to_string()));
    // The snippet bolds the matched word.
    let snip = blocks.iter().find_map(|b| match b {
        Block::Para { runs, indent: true, .. } => Some(runs),
        _ => None,
    }).unwrap();
    assert!(snip.iter().any(|r| r.text == "loved" && r.bold));
    // capped → an "N more" tail (2 total − 1 shown).
    assert!(blocks.iter().any(|b| text_of(b) == "… 1 more"));
}

