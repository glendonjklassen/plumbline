//! End-to-end exercise of the C ABI, driven exactly as a foreign caller would:
//! open from bytes, walk the TOC, lay out a chapter through a C measurement
//! callback, hit-test a word, look up Strong's + occurrences, search, and free
//! every handle/string. No GUI, fully deterministic (monospace measurement).

use super::*;
use serde_json::Value;
use std::ffi::CStr;

const KJV: &str = concat!(
    r#"{"format":"x","tokenization":"kjv1769-tok2","verses":2}"#,
    "\n",
    r#"{"b":"John","c":3,"t":[["","For","",[],0],["","God","",["G2316"],0],["","so","",[],0],["","loved","",["G25"],0],["","the","",[],1],["","world",".",[],0]],"v":16}"#,
    "\n",
    r#"{"b":"John","c":3,"t":[["","He","",[],8],["","that","",[],0],["","believeth","",["G4100"],0]],"v":18}"#,
);

const STRONGS: &str = r#"{
  "G2316":{"lemma":"θεός","xlit":"theos","pron":"theh'-os","strongs_def":"a deity","kjv_def":"God"},
  "G25":{"lemma":"ἀγαπάω","strongs_def":"to love"},
  "G4100":{"lemma":"πιστεύω","kjv_def":"believe"}
}"#;

/// Monospace measurement over the C ABI: every char is 10px. Deterministic, so
/// layout coordinates are exactly predictable.
extern "C" fn mono_measure(_ctx: *mut c_void, text: *const c_char) -> f32 {
    if text.is_null() {
        return 0.0;
    }
    let s = unsafe { CStr::from_ptr(text) }.to_str().unwrap_or("");
    s.chars().count() as f32 * 10.0
}

/// Take ownership of a returned C string, freeing it through the real ABI.
unsafe fn take(p: *mut c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = CStr::from_ptr(p).to_str().unwrap().to_owned();
    pure_study_string_free(p);
    Some(s)
}

unsafe fn open() -> *mut PureEngine {
    let mut err: *mut c_char = ptr::null_mut();
    let e = pure_engine_open_from_bytes(
        KJV.as_ptr(),
        KJV.len(),
        STRONGS.as_ptr(),
        STRONGS.len(),
        &mut err,
    );
    assert!(err.is_null(), "unexpected open error: {:?}", take(err));
    assert!(!e.is_null(), "engine should open");
    e
}

fn cfg() -> PureLayoutConfig {
    PureLayoutConfig {
        width: 10_000.0, // wide: everything on one line
        line_height: 20.0,
        space_width: 5.0,
        verse_num_gap: 4.0,
        para_indent: 16.0,
        para_spacing: 8.0,
    }
}

#[test]
fn version_roundtrips_through_c_abi() {
    let s = unsafe { take(pure_study_version()) }.unwrap();
    assert_eq!(s, env!("CARGO_PKG_VERSION"));
}

#[test]
fn open_from_bytes_reports_errors() {
    unsafe {
        // Bad UTF-8 in the corpus bytes.
        let mut err: *mut c_char = ptr::null_mut();
        let e = pure_engine_open_from_bytes(
            [0xff, 0xfe].as_ptr(),
            2,
            STRONGS.as_ptr(),
            STRONGS.len(),
            &mut err,
        );
        assert!(e.is_null());
        assert!(take(err).unwrap().contains("UTF-8"));

        // Malformed strongs JSON.
        let mut err2: *mut c_char = ptr::null_mut();
        let e2 = pure_engine_open_from_bytes(
            KJV.as_ptr(),
            KJV.len(),
            b"{not json".as_ptr(),
            9,
            &mut err2,
        );
        assert!(e2.is_null());
        assert!(take(err2).unwrap().contains("strongs.json"));
    }
}

#[test]
fn toc_and_chapter_count() {
    unsafe {
        let e = open();
        let toc: Value = serde_json::from_str(&take(pure_engine_toc_json(e)).unwrap()).unwrap();
        let books = toc["books"].as_array().unwrap();
        assert_eq!(books.len(), 66, "canon has 66 books");
        let john = books.iter().find(|b| b["id"] == "John").unwrap();
        assert_eq!(john["name"], "John");
        assert_eq!(john["chapters"], 3, "our corpus has John up to chapter 3");

        let c = pure_engine_chapter_count(e, c"John".as_ptr());
        assert_eq!(c, 3);
        // Unknown book on a valid engine floors at 1 (a safe UI range floor);
        // only a null engine yields 0 (see `null_and_freed_handles_are_safe`).
        assert_eq!(pure_engine_chapter_count(e, c"Nope".as_ptr()), 1);
        pure_engine_free(e);
    }
}

#[test]
fn layout_then_hit_test_a_word() {
    unsafe {
        let e = open();
        let dl = pure_engine_layout_chapter(
            e,
            c"John".as_ptr(),
            3,
            cfg(),
            Some(mono_measure),
            ptr::null_mut(),
        );
        assert!(!dl.is_null());
        assert!(pure_layout_item_count(dl) > 0);
        assert!(pure_layout_height(dl) >= 20.0);

        // Parse the JSON to locate the word "God" (John 3:16, token index 1).
        let list: Value = serde_json::from_str(&take(pure_layout_to_json(dl)).unwrap()).unwrap();
        let items = list["items"].as_array().unwrap();
        let god = items
            .iter()
            .find(|it| it["kind"] == "word" && it["text"] == "God")
            .expect("word 'God' should be laid out");
        assert_eq!(god["verse"], "John 3:16");
        assert_eq!(god["tokenIndex"], 1);
        assert_eq!(god["strongs"][0], "G2316");
        // Every item carries all keys as explicit null (for strict decoders):
        // a word still has `verseNumber: null`.
        assert!(god.as_object().unwrap().contains_key("verseNumber"));
        assert!(god["verseNumber"].is_null());

        // Hit-test the centre of that box on the live handle.
        let cx = god["x"].as_f64().unwrap() as f32 + god["w"].as_f64().unwrap() as f32 / 2.0;
        let cy = god["y"].as_f64().unwrap() as f32 + god["h"].as_f64().unwrap() as f32 / 2.0;
        let hit: Value =
            serde_json::from_str(&take(pure_layout_hit_test_json(dl, cx, cy)).unwrap()).unwrap();
        assert_eq!(hit["verse"], "John 3:16");
        assert_eq!(hit["tokenIndex"], 1);
        assert_eq!(hit["strongs"][0], "G2316");

        // A verse number resolves to no word.
        let num = items.iter().find(|it| it["kind"] == "verseNumber").unwrap();
        // ...and a verse-number item still has `verse: null` present.
        assert!(num.as_object().unwrap().contains_key("verse"));
        assert!(num["verse"].is_null());
        let nx = num["x"].as_f64().unwrap() as f32 + 1.0;
        let ny = num["y"].as_f64().unwrap() as f32 + 1.0;
        assert!(pure_layout_hit_test_json(dl, nx, ny).is_null());

        // The paragraph flag on John 3:18's first word puts it on a new line.
        let ys: std::collections::BTreeSet<i64> =
            items.iter().map(|it| it["y"].as_f64().unwrap() as i64).collect();
        assert!(ys.len() > 1, "paragraph break should add a line");

        pure_layout_free(dl);
        pure_engine_free(e);
    }
}

#[test]
fn layout_absent_or_out_of_range_chapter_is_null() {
    unsafe {
        let e = open();
        let m: PureMeasureFn = Some(mono_measure);
        // A chapter not present in this corpus (only John 3 exists here).
        assert!(pure_engine_layout_chapter(e, c"John".as_ptr(), 1, cfg(), m, ptr::null_mut())
            .is_null());
        // An unknown book.
        assert!(pure_engine_layout_chapter(e, c"Nope".as_ptr(), 3, cfg(), m, ptr::null_mut())
            .is_null());
        // A chapter outside the u16 domain must NOT wrap into a real chapter
        // (65539 as u16 == 3, which does exist — regression guard).
        assert!(pure_engine_layout_chapter(e, c"John".as_ptr(), 65539, cfg(), m, ptr::null_mut())
            .is_null());
        pure_engine_free(e);
    }
}

#[test]
fn null_measure_yields_null_layout() {
    unsafe {
        let e = open();
        let dl =
            pure_engine_layout_chapter(e, c"John".as_ptr(), 3, cfg(), None, ptr::null_mut());
        assert!(dl.is_null(), "a null measure callback must fail cleanly");
        pure_engine_free(e);
    }
}

#[test]
fn strongs_entry_and_occurrences() {
    unsafe {
        let e = open();

        let entry: Value =
            serde_json::from_str(&take(pure_engine_strongs_json(e, c"G2316".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(entry["code"], "G2316");
        assert_eq!(entry["lemma"], "θεός");
        assert_eq!(entry["kjv"], "God");
        // Derived / pron absent-vs-present handled as null.
        assert_eq!(entry["deriv"], Value::Null);

        // Unknown code → null.
        assert!(pure_engine_strongs_json(e, c"H9999".as_ptr()).is_null());

        let occ: Value = serde_json::from_str(
            &take(pure_engine_strongs_occurrences_json(e, c"G2316".as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(occ["code"], "G2316");
        assert_eq!(occ["total"], 1);
        assert_eq!(occ["capped"], false);
        assert_eq!(occ["verses"][0], "John 3:16");

        pure_engine_free(e);
    }
}

#[test]
fn verse_and_token_lookup() {
    unsafe {
        let e = open();
        let verse: Value =
            serde_json::from_str(&take(pure_engine_verse_json(e, c"John 3:16".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(verse["reference"], "John 3:16");
        assert!(verse["body"].as_str().unwrap().contains("God"));
        assert_eq!(verse["tokens"].as_array().unwrap().len(), 6);

        // Token index 4 ("the") carries the KJV-added flag in our sample.
        let tok: Value = serde_json::from_str(
            &take(pure_engine_token_json(e, c"John 3:16".as_ptr(), 4)).unwrap(),
        )
        .unwrap();
        assert_eq!(tok["word"], "the");
        assert_eq!(tok["flags"], PURE_FLAG_ADDED);

        // Out-of-range token / bad ref → null.
        assert!(pure_engine_token_json(e, c"John 3:16".as_ptr(), 99).is_null());
        assert!(pure_engine_verse_json(e, c"garbage".as_ptr()).is_null());
        assert!(pure_engine_verse_json(e, c"John 9:9".as_ptr()).is_null());

        pure_engine_free(e);
    }
}

#[test]
fn search_word_reference_and_bare_strongs() {
    unsafe {
        let e = open();

        // Word search.
        let hits: Value =
            serde_json::from_str(&take(pure_engine_search_json(e, c"loved".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(hits["kind"], "hits");
        assert!(hits["total"].as_u64().unwrap() >= 1);
        assert_eq!(hits["hits"][0]["verse"], "John 3:16");

        // Reference query → goto.
        let goto: Value =
            serde_json::from_str(&take(pure_engine_search_json(e, c"John 3".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(goto["kind"], "goto");
        assert_eq!(goto["book"], "John");
        assert_eq!(goto["chapter"], 3);
        assert_eq!(goto["verse"], Value::Null);

        // Bare Strong's code → verses tagged with it.
        let tagged: Value =
            serde_json::from_str(&take(pure_engine_search_json(e, c"G2316".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(tagged["kind"], "hits");
        assert_eq!(tagged["hits"][0]["verse"], "John 3:16");

        // Blank query → null.
        assert!(pure_engine_search_json(e, c"   ".as_ptr()).is_null());

        pure_engine_free(e);
    }
}

#[test]
fn null_and_freed_handles_are_safe() {
    unsafe {
        // Null handles never crash.
        assert!(pure_engine_toc_json(ptr::null()).is_null());
        assert_eq!(pure_engine_chapter_count(ptr::null(), c"John".as_ptr()), 0);
        assert!(pure_layout_to_json(ptr::null()).is_null());
        assert_eq!(pure_layout_height(ptr::null()), 0.0);
        assert!(pure_layout_hit_test_json(ptr::null(), 0.0, 0.0).is_null());
        // Freeing null is a no-op.
        pure_engine_free(ptr::null_mut());
        pure_layout_free(ptr::null_mut());
        pure_study_string_free(ptr::null_mut());
    }
}
