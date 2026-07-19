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
        verse_break: 0,
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
fn renderings_and_word_codes() {
    unsafe {
        let e = open();

        // Forward lens: G25 (agapao) is rendered "loved" in John 3:16, token 3.
        let r: Value =
            serde_json::from_str(&take(pure_engine_renderings_json(e, c"G25".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(r["code"], "G25");
        let rs = r["renderings"].as_array().unwrap();
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0]["rendering"], "loved");
        assert_eq!(rs[0]["total"], 1);
        assert_eq!(rs[0]["capped"], false);
        assert_eq!(rs[0]["refs"][0]["verse"], "John 3:16");
        assert_eq!(rs[0]["refs"][0]["span"][0], 3);
        assert_eq!(rs[0]["refs"][0]["span"][1], 3);

        // Unknown/untagged code → empty renderings, not null.
        let empty: Value = serde_json::from_str(
            &take(pure_engine_renderings_json(e, c"H9999".as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(empty["renderings"].as_array().unwrap().len(), 0);

        // Reverse lens: the surface word "God" (normalized) maps to G2316.
        let w: Value =
            serde_json::from_str(&take(pure_engine_word_codes_json(e, c"God".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(w["word"], "God");
        let codes = w["codes"].as_array().unwrap();
        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0]["code"], "G2316");
        assert_eq!(codes[0]["count"], 1);

        // A translator-supplied (added) word carries no codes.
        let the: Value =
            serde_json::from_str(&take(pure_engine_word_codes_json(e, c"the".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(the["codes"].as_array().unwrap().len(), 0);

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

#[test]
fn authoring_round_trip_via_abi() {
    use std::ffi::CString;
    unsafe {
        // A temp home with the two data files pure_engine_open expects.
        let home = std::env::temp_dir().join(format!("pure-ffi-author-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = pure_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null());
        assert!(!e.is_null());

        let c = |s: &str| CString::new(s).unwrap();
        let stamp = c("2026-01-01T00:00:00Z");

        // Author: tag a verse, add a verse to a thread, weave two verses.
        // A null return means success.
        assert!(pure_engine_tag_add(e, c("Messianic").as_ptr(), c("verse").as_ptr(), c("John 3:16").as_ptr(), ptr::null(), stamp.as_ptr()).is_null());
        assert!(pure_engine_tag_add(e, c("Messianic").as_ptr(), c("concept").as_ptr(), c("G2316").as_ptr(), ptr::null(), stamp.as_ptr()).is_null());
        assert!(pure_engine_thread_add(e, c("Road").as_ptr(), c("John 3:16").as_ptr(), ptr::null(), stamp.as_ptr()).is_null());
        assert!(pure_engine_weave_add_link(e, c("Links").as_ptr(), c("John 3:16").as_ptr(), c("John 3:18").as_ptr(), stamp.as_ptr()).is_null());

        // Read back through the ABI (the engine reloaded after each write).
        let tags: Value = serde_json::from_str(&take(pure_engine_tags_json(e)).unwrap()).unwrap();
        assert_eq!(tags["tags"][0]["name"], "Messianic");
        assert_eq!(tags["tags"][0]["members"].as_array().unwrap().len(), 2);
        assert_eq!(tags["tags"][0]["members"][0]["verse"], "John 3:16");
        assert_eq!(tags["tags"][0]["members"][1]["strongs"], "G2316");

        let threads: Value = serde_json::from_str(&take(pure_engine_threads_json(e)).unwrap()).unwrap();
        assert_eq!(threads["threads"][0]["name"], "Road");
        assert_eq!(threads["threads"][0]["entries"][0]["verse"], "John 3:16");

        let xrefs: Value =
            serde_json::from_str(&take(pure_engine_verse_xrefs_json(e, c("John 3:16").as_ptr())).unwrap()).unwrap();
        assert_eq!(xrefs["partners"][0]["verse"], "John 3:18");
        assert_eq!(xrefs["partners"][0]["weave"], "Links");

        // Edit notes: thread doc, an entry note, and the weave doc.
        assert!(pure_engine_thread_set_notes(e, c("Road").as_ptr(), c("the gospel road").as_ptr()).is_null());
        assert!(pure_engine_thread_entry_set_note(e, c("Road").as_ptr(), 0, c("start here").as_ptr()).is_null());
        assert!(pure_engine_weave_set_notes(e, c("Links").as_ptr(), c("belief and judgment").as_ptr()).is_null());
        let threads: Value = serde_json::from_str(&take(pure_engine_threads_json(e)).unwrap()).unwrap();
        assert_eq!(threads["threads"][0]["notes"], "the gospel road");
        assert_eq!(threads["threads"][0]["entries"][0]["note"], "start here");
        // Clearing an entry note (null) and error paths.
        assert!(pure_engine_thread_entry_set_note(e, c("Road").as_ptr(), 0, ptr::null()).is_null());
        assert!(take(pure_engine_weave_set_notes(e, c("Nope").as_ptr(), c("x").as_ptr())).unwrap().contains("weave"));
        assert!(take(pure_engine_thread_entry_set_note(e, c("Road").as_ptr(), 9, ptr::null())).unwrap().contains("entry"));

        // Error paths: a bad target kind, and a bytes-opened engine has no home.
        assert!(take(pure_engine_tag_add(e, c("X").as_ptr(), c("bogus").as_ptr(), c("v").as_ptr(), ptr::null(), stamp.as_ptr()))
            .unwrap()
            .contains("kind"));
        let bytes_engine = open();
        assert!(take(pure_engine_tag_add(bytes_engine, c("X").as_ptr(), c("verse").as_ptr(), c("John 3:16").as_ptr(), ptr::null(), stamp.as_ptr()))
            .unwrap()
            .contains("home"));
        pure_engine_free(bytes_engine);

        pure_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn suggested_weave_review_via_abi() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("pure-ffi-review-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        // Seed two suggested weaves the engine will discover on open.
        let sug = home.join("weaves").join("suggested");
        std::fs::create_dir_all(&sug).unwrap();
        let one = r#"{"format":"overlay-weave-v2","name":"Born again","kind":"prophecy","tokenization":"kjv1769-tok2","notes":"","created":"c","approved":false,"links":[{"a":"John 3:16","b":"John 3:18"}]}"#;
        let two = r#"{"format":"overlay-weave-v2","name":"Only Son","kind":"quotation","tokenization":"kjv1769-tok2","notes":"","created":"c","approved":false,"links":[{"a":"John 3:16","b":"John 3:18"}]}"#;
        std::fs::write(sug.join("born-again.json"), one).unwrap();
        std::fs::write(sug.join("only-son.json"), two).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = pure_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let c = |s: &str| CString::new(s).unwrap();

        // List: both show up, ordered, each with its ordinal index.
        let listed: Value =
            serde_json::from_str(&take(pure_engine_suggested_weaves_json(e)).unwrap()).unwrap();
        let items = listed["suggested"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["index"], 0);
        assert_eq!(items[0]["links"][0]["aDisplay"], "John 3:16");

        // Approve index 0 → it leaves the suggested queue (one left).
        assert!(pure_engine_weave_approve(e, 0).is_null());
        let after: Value =
            serde_json::from_str(&take(pure_engine_suggested_weaves_json(e)).unwrap()).unwrap();
        assert_eq!(after["suggested"].as_array().unwrap().len(), 1);
        // The approved weave now asserts its cross-reference from weaves/.
        let xrefs: Value =
            serde_json::from_str(&take(pure_engine_verse_xrefs_json(e, c("John 3:16").as_ptr())).unwrap()).unwrap();
        assert!(xrefs["partners"].as_array().unwrap().iter().any(|p| p["verse"] == "John 3:18"));

        // Reject the remaining one (now index 0) → queue empties.
        assert!(pure_engine_weave_reject(e, 0).is_null());
        let empty: Value =
            serde_json::from_str(&take(pure_engine_suggested_weaves_json(e)).unwrap()).unwrap();
        assert!(empty["suggested"].as_array().unwrap().is_empty());

        // Error paths: out-of-range index, and a bytes-opened engine has no home.
        assert!(take(pure_engine_weave_approve(e, 9)).unwrap().contains("index"));
        let bytes_engine = open();
        assert!(take(pure_engine_weave_reject(bytes_engine, 0)).unwrap().contains("home"));
        pure_engine_free(bytes_engine);

        pure_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn rnd_tier_via_abi() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("pure-ffi-rnd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::create_dir_all(home.join("bridge")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        // Tiny aligned embedding over the fixture's codes + one Hebrew row.
        std::fs::write(
            home.join("data").join("concept-vectors.vec"),
            "4 2\nG2316 1 0\nG25 0.9 0.1\nG4100 0.2 1\nH7225 0.95 0.05\n",
        )
        .unwrap();
        std::fs::write(
            home.join("data").join("concept-vectors.vec.meta"),
            r#"{"tokenization":"kjv1769-tok2","aligned":"procrustes"}"#,
        )
        .unwrap();
        // Morphology annotating "loved" (token 3, G25) in John 3:16.
        std::fs::write(
            home.join("data").join("morphology.jsonl"),
            "{\"format\":\"overlay-morphology-v1\",\"tokenization\":\"kjv1769-tok2\",\"source\":\"test\"}\n\
             {\"b\":\"John\",\"c\":3,\"v\":16,\"e\":[[3,\"G25\",null,\"V-AAI-3S\"]]}\n",
        )
        .unwrap();
        // External bridge witnesses + trust priors. lxx alone is machine-tier;
        // the quotation pair adds a God-tier, research-grade partner so the
        // authority-tier wire fields are exercised end to end.
        std::fs::write(
            home.join("bridge").join("lxx.json"),
            r#"{"format":"overlay-bridge-sources-v1","links":[{"h":"H7225","g":"G25","source":"lxx"},{"h":"H430","g":"G25","source":"quotation"}]}"#,
        )
        .unwrap();
        std::fs::write(home.join("data").join("source-priors.json"), r#"{"priors":{"lxx":0.85,"_default":0.5}}"#).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = pure_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let c = |s: &str| CString::new(s).unwrap();

        // Concept neighbours: same-testament near, Hebrew cross (aligned).
        let n: Value = serde_json::from_str(&take(pure_engine_concept_neighbours_json(e, c("G25").as_ptr(), 5)).unwrap()).unwrap();
        assert_eq!(n["code"], "G25");
        assert!(n["near"].as_array().unwrap().iter().all(|x| x["code"].as_str().unwrap().starts_with('G')));
        assert!(n["cross"].as_array().unwrap().iter().any(|x| x["code"] == "H7225"));

        // Fused bridge partner from the external witness.
        let b: Value = serde_json::from_str(&take(pure_engine_bridge_partners_json(e, c("G25").as_ptr())).unwrap()).unwrap();
        let p = b["partners"].as_array().unwrap().iter().find(|x| x["code"] == "H7225").unwrap();
        assert_eq!(p["sources"][0], "lxx");
        assert!((p["prior"].as_f64().unwrap() - 0.85).abs() < 1e-6);
        // Authority tiers (additive): lxx is machine-only, not research-grade.
        assert_eq!(p["tiers"], serde_json::json!(["machine"]));
        assert_eq!(p["researchGrade"], serde_json::json!(false));
        // The quotation partner is God-tier content by a machine method, and
        // research-grade until the harvest is audited.
        let q = b["partners"].as_array().unwrap().iter().find(|x| x["code"] == "H430").unwrap();
        assert_eq!(q["sources"][0], "quotation");
        assert_eq!(q["tiers"], serde_json::json!(["god", "machine"]));
        assert_eq!(q["researchGrade"], serde_json::json!(true));

        // Morphology of "loved".
        let m: Value = serde_json::from_str(&take(pure_engine_morph_json(e, c("John 3:16").as_ptr(), 3)).unwrap()).unwrap();
        assert_eq!(m["code"], "V-AAI-3S");
        assert_eq!(m["gloss"], "aorist active indicative, 3rd singular");
        // A token with no annotation → null.
        assert!(pure_engine_morph_json(e, c("John 3:16").as_ptr(), 1).is_null());

        // "Verses like this" (lazy SIF build) → the other Greek verse.
        let s: Value = serde_json::from_str(&take(pure_engine_similar_verses_json(e, c("John 3:16").as_ptr(), 5)).unwrap()).unwrap();
        assert_eq!(s["verse"], "John 3:16");
        assert!(s["in"].as_array().unwrap().iter().any(|x| x["verse"] == "John 3:18"));

        // A bytes-opened engine has no embedding/morph → those return null.
        let bytes_engine = open();
        assert!(pure_engine_concept_neighbours_json(bytes_engine, c("G25").as_ptr(), 5).is_null());
        assert!(pure_engine_morph_json(bytes_engine, c("John 3:16").as_ptr(), 3).is_null());
        pure_engine_free(bytes_engine);

        pure_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn parity_endpoints_via_abi() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("pure-ffi-parity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();
        // Margin notes + TSK cross-references + a spanned weave.
        std::fs::write(
            home.join("data").join("kjv-notes.jsonl"),
            r#"{"b":"John","c":3,"v":16,"note":"Or, begotten"}"#,
        )
        .unwrap();
        std::fs::write(
            home.join("data").join("cross-references.tsv"),
            "John 3:16\tJohn 3:18\t\t5\nJohn 3:16\tJohn 3:18\tJohn 3:18\t2\n",
        )
        .unwrap();
        std::fs::create_dir_all(home.join("weaves")).unwrap();
        std::fs::write(
            home.join("weaves").join("spanned.json"),
            r#"{"format":"overlay-weave-v2","name":"Spanned","kind":"type","tokenization":"kjv1769-tok2","notes":"n","created":"c","approved":true,"links":[{"a":"John 3:16","b":"John 3:18","label":"faith","approved":true,"spanA":[1,3],"spanB":[2,2]},{"a":"Gen 1:1","b":"John 3:16"}]}"#,
        )
        .unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = pure_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let c = |s: &str| CString::new(s).unwrap();

        // Margin notes: present verse → notes; absent verse → null.
        let notes: Value = serde_json::from_str(
            &take(pure_engine_verse_notes_json(e, c("John 3:16").as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(notes["notes"][0], "Or, begotten");
        assert!(pure_engine_verse_notes_json(e, c("John 3:18").as_ptr()).is_null());

        // TSK: best-voted first, range end carried.
        let xr: Value = serde_json::from_str(
            &take(pure_engine_study_xrefs_json(e, c("John 3:16").as_ptr())).unwrap(),
        )
        .unwrap();
        let refs = xr["refs"].as_array().unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0]["votes"], 5);
        assert!(refs[0]["end"].is_null());
        assert_eq!(refs[1]["end"], "John 3:18");

        // Weave library: spans, approval, kind label, resolvability.
        let ws: Value =
            serde_json::from_str(&take(pure_engine_weaves_json(e)).unwrap()).unwrap();
        let w = &ws["weaves"][0];
        assert_eq!(w["name"], "Spanned");
        assert_eq!(w["kind"], "type");
        assert_eq!(w["suggested"], false);
        let links = w["links"].as_array().unwrap();
        // Links are canon-ordered inside the weave; find by span presence.
        let spanned = links.iter().find(|l| !l["spanA"].is_null()).unwrap();
        assert_eq!(spanned["spanA"][0], 1);
        assert_eq!(spanned["spanA"][1], 3);
        assert_eq!(spanned["spanB"][0], 2);
        assert_eq!(spanned["label"], "faith");
        assert_eq!(spanned["resolved"], true);
        // The Gen 1:1 endpoint is not in this two-verse corpus.
        let dangling = links.iter().find(|l| l["spanA"].is_null()).unwrap();
        assert_eq!(dangling["resolved"], false);

        // Author a spanned link through the ABI and read it back.
        assert!(pure_engine_weave_add_link_spans(
            e,
            c("Pinned").as_ptr(),
            c("John 3:16").as_ptr(),
            c("John 3:18").as_ptr(),
            3,
            1,
            -1,
            -1,
            c("2026-01-01T00:00:00Z").as_ptr(),
        )
        .is_null());
        let ws: Value =
            serde_json::from_str(&take(pure_engine_weaves_json(e)).unwrap()).unwrap();
        let pinned = ws["weaves"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["name"] == "Pinned")
            .unwrap();
        // Reversed bounds normalise; the span-less side stays null.
        assert_eq!(pinned["links"][0]["spanA"][0], 1);
        assert_eq!(pinned["links"][0]["spanA"][1], 3);
        assert!(pinned["links"][0]["spanB"].is_null());

        // Link pairs: deduped canonical pairs, each endpoint located, with the
        // resolvability flag (the Gen 1:1 endpoint is outside this corpus).
        let lp: Value =
            serde_json::from_str(&take(pure_engine_link_pairs_json(e)).unwrap()).unwrap();
        let pairs = lp["pairs"].as_array().unwrap();
        let resolved = pairs.iter().find(|p| p["resolved"] == true).unwrap();
        assert_eq!(resolved["a"], "John 3:16");
        assert_eq!(resolved["aBook"], "John");
        assert_eq!(resolved["aChapter"], 3);
        assert_eq!(resolved["aVerse"], 16);
        assert_eq!(resolved["b"], "John 3:18");
        let dangling = pairs.iter().find(|p| p["resolved"] == false).unwrap();
        assert_eq!(dangling["a"], "Gen 1:1");
        assert_eq!(dangling["aBook"], "Gen");
        assert_eq!(dangling["b"], "John 3:16");

        // Canon segments: the 8 frozen bands + the OT/NT divide, straight from
        // core::reference (no shell hardcode).
        let cs: Value =
            serde_json::from_str(&take(pure_engine_canon_segments_json(e)).unwrap()).unwrap();
        assert_eq!(cs["otNtDivide"], 39);
        let segs = cs["segments"].as_array().unwrap();
        assert_eq!(segs.len(), 8);
        assert_eq!(segs[0]["label"], "Law");
        assert_eq!(segs[0]["first"], 0);
        assert_eq!(segs[0]["last"], 4);
        assert_eq!(segs[4]["label"], "Gospels");
        assert_eq!(segs[4]["first"], 39);

        // Chord map: book-pair density folded over the deduped pairs. This
        // corpus's two pairs are (Gen 1:1↔John 3:16) and (John 3:16↔John 3:18)
        // — one cross-testament Gen↔John and one John↔John self-pair, each once.
        // Unresolved endpoints (Gen 1:1 is outside the corpus) still count, so
        // the map reflects authored density, not drawability.
        let cm: Value =
            serde_json::from_str(&take(pure_engine_chord_map_json(e)).unwrap()).unwrap();
        assert_eq!(cm["otNtDivide"], 39);
        assert_eq!(cm["bookCount"], 66);
        assert_eq!(cm["max"], 1);
        let cpairs = cm["pairs"].as_array().unwrap();
        assert_eq!(cpairs.len(), 2);
        // Gen↔John: Gen is book 0, John is in the NT (index ≥ 39); a <= b holds.
        let cross = cpairs
            .iter()
            .find(|p| p["a"].as_u64() != p["b"].as_u64())
            .expect("a cross-book pair");
        assert_eq!(cross["a"], 0);
        assert!(cross["b"].as_u64().unwrap() >= 39);
        assert_eq!(cross["count"], 1);
        // John↔John self-pair.
        let selfp = cpairs
            .iter()
            .find(|p| p["a"].as_u64() == p["b"].as_u64())
            .expect("a self-pair");
        assert!(selfp["a"].as_u64().unwrap() >= 39);
        assert_eq!(selfp["count"], 1);

        // Concept map: centre label (gloss over lemma), spokes, and the
        // canon-ordered dispersion. G2316 (θεός / "God") occurs once, in John.
        let cmap: Value = serde_json::from_str(
            &take(pure_engine_concept_map_json(e, c("G2316").as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(cmap["code"], "G2316");
        assert!(cmap["centerLabel"].as_str().unwrap().contains("θεός"));
        assert_eq!(cmap["otNtDivide"], 39);
        assert_eq!(cmap["bookCount"], 66);
        let bb = cmap["byBook"].as_array().unwrap();
        assert_eq!(bb.len(), 66);
        // Exactly one occurrence, and it lands in the NT (John, index ≥ 39).
        let total: u64 = bb.iter().map(|x| x.as_u64().unwrap()).sum();
        assert_eq!(total, 1);
        assert_eq!(bb[..39].iter().map(|x| x.as_u64().unwrap()).sum::<u64>(), 0);
        // No embedding artifact here → no semantic (gold) spokes; community
        // spokes (if any) are all green.
        assert!(cmap["spokes"].as_array().unwrap().iter().all(|s| s["semantic"] == false));

        // Constellation: the "Spanned" weave has one resolvable link (John
        // 3:16↔John 3:18); its Gen 1:1↔John 3:16 link is unresolved, so it never
        // becomes a lane. The "Pinned" weave (authored above) also resolves.
        let con: Value = serde_json::from_str(
            &take(pure_engine_constellation_json(e, 0, ptr::null())).unwrap(),
        )
        .unwrap();
        assert_eq!(con["nPins"], 0);
        assert_eq!(con["laneCapacity"], 18);
        let lanes = con["lanes"].as_array().unwrap();
        assert!(!lanes.is_empty());
        let spanned = lanes.iter().find(|l| l["name"] == "Spanned").expect("Spanned lane");
        // One resolvable link → one edge, two nodes (John 3:16 and John 3:18).
        assert_eq!(spanned["edges"].as_array().unwrap().len(), 1);
        let nodes = spanned["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        for n in nodes {
            let x = n["x"].as_f64().unwrap();
            let lf = n["laneFrac"].as_f64().unwrap();
            assert!((0.0..=1.0).contains(&x));
            assert!(lf > 0.13 && lf < 0.87);
            assert_eq!(n["book"], "John");
        }
        // Pin the "Spanned" lane by its weave index → it becomes lane 0, pinned.
        let sidx = spanned["weaveIndex"].as_u64().unwrap();
        let pins = CString::new(format!("[{sidx}]")).unwrap();
        let con2: Value = serde_json::from_str(
            &take(pure_engine_constellation_json(e, 0, pins.as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(con2["nPins"], 1);
        assert_eq!(con2["lanes"][0]["weaveIndex"], sidx);
        assert_eq!(con2["lanes"][0]["pinned"], true);
        assert!(con2["caption"].as_str().unwrap().starts_with("1 pinned · "));

        pure_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn concept_and_gloss_via_abi() {
    use std::ffi::CString;
    unsafe {
        let e = open();
        let c = |s: &str| CString::new(s).unwrap();

        // Concept stats for a code that occurs once, in the NT.
        let cj: Value = serde_json::from_str(
            &take(pure_engine_concept_json(e, c("G2316").as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(cj["total"], 1);
        assert_eq!(cj["ot"], 0);
        assert_eq!(cj["nt"], 1);
        assert_eq!(cj["topBooks"][0]["book"], "John");
        assert_eq!(cj["byBook"]["John"], 1);
        // Too few occurrences for a leitwort (min 8).
        assert!(cj["leitwort"].is_null());
        // Unknown code → null.
        assert!(pure_engine_concept_json(e, c("H9999").as_ptr()).is_null());

        // English gloss: the modal KJV rendering carrying the code.
        assert_eq!(take(pure_engine_gloss(e, c("G2316").as_ptr())).unwrap(), "God");
        assert_eq!(take(pure_engine_gloss(e, c("G25").as_ptr())).unwrap(), "loved");
        // Untagged code distils the dictionary; unknown → null.
        assert!(pure_engine_gloss(e, c("H9999").as_ptr()).is_null());

        pure_engine_free(e);
    }
}

#[test]
fn config_round_trip_via_abi() {
    use std::ffi::CString;
    unsafe {
        // Redirect the per-user config dir into a temp sandbox.
        let dir = std::env::temp_dir().join(format!("pure-ffi-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if cfg!(target_os = "windows") {
            std::env::set_var("APPDATA", &dir);
        } else {
            std::env::set_var("XDG_CONFIG_HOME", &dir);
        }

        // No file yet → defaults + firstRun.
        let loaded: Value =
            serde_json::from_str(&take(pure_config_load_json()).unwrap()).unwrap();
        assert_eq!(loaded["firstRun"], true);
        assert_eq!(loaded["studyMode"], "simple");

        // Save a full-study, two-pane session and read it back.
        let saved = r#"{"studyMode":"full","bodySize":21.0,"openPanes":[{"book":"Gen","chapter":15},{"book":"Rom","chapter":4}],"activePane":1}"#;
        let sc = CString::new(saved).unwrap();
        assert!(pure_config_save_json(sc.as_ptr()).is_null());
        let loaded: Value =
            serde_json::from_str(&take(pure_config_load_json()).unwrap()).unwrap();
        assert_eq!(loaded["firstRun"], false);
        assert_eq!(loaded["studyMode"], "full");
        assert_eq!(loaded["bodySize"], 21.0);
        assert_eq!(loaded["openPanes"][1]["book"], "Rom");
        assert_eq!(loaded["activePane"], 1);

        // Garbage json is an error, not a panic.
        let bad = CString::new("{nope").unwrap();
        assert!(take(pure_config_save_json(bad.as_ptr())).unwrap().contains("bad config json"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The study-panel block endpoints (the P0.1 content model) over the ABI: the
/// producer's blocks reach a shell as camelCase JSON with the pre-baked link
/// URIs a shell routes back through the dispatcher.
#[test]
fn panel_blocks_via_abi() {
    unsafe {
        let e = open(); // bytes-opened KJV/STRONGS (John 3:16 tags G2316, G25)
        let c = |s: &str| CString::new(s).unwrap();

        // Every run `uri` across a blocks payload.
        fn uris(v: &Value) -> Vec<String> {
            v["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|b| b["runs"].as_array())
                .flatten()
                .filter_map(|r| r["uri"].as_str().map(str::to_string))
                .collect()
        }

        // Word study on "God" (John 3:16 token 1 → G2316), Full study.
        let ws: Value = serde_json::from_str(
            &take(pure_engine_word_study_blocks_json(e, c("John 3:16").as_ptr(), 1, true)).unwrap(),
        )
        .unwrap();
        // Opens with the verse display, then the big word.
        assert_eq!(ws["blocks"][0]["kind"], "para");
        assert_eq!(ws["blocks"][0]["runs"][0]["text"], "John 3:16");
        assert_eq!(ws["blocks"][1]["runs"][0]["text"], "God");
        // The code's occurrence link is baked in.
        assert!(uris(&ws).contains(&"occ:G2316".to_string()));
        // A tier section header carries its provenance mark.
        assert!(ws["blocks"].as_array().unwrap().iter().any(|b| b["kind"] == "section"));

        // Simple mode drops the R&D sections.
        let simple: Value = serde_json::from_str(
            &take(pure_engine_word_study_blocks_json(e, c("John 3:16").as_ptr(), 1, false)).unwrap(),
        )
        .unwrap();
        assert!(!simple["blocks"].as_array().unwrap().iter().any(|b| b["kind"] == "section"));

        // Concordance: the go: link for the one occurrence verse.
        let cc: Value =
            serde_json::from_str(&take(pure_engine_concordance_blocks_json(e, c("G2316").as_ptr())).unwrap()).unwrap();
        assert!(uris(&cc).contains(&"go:John:3:16".to_string()));

        // A word search → ranked hits, each a go: link (John 3:16 has "God").
        let sr: Value =
            serde_json::from_str(&take(pure_engine_search_blocks_json(e, c("God").as_ptr())).unwrap()).unwrap();
        assert!(uris(&sr).contains(&"go:John:3:16".to_string()));

        // A blank query is null (not an empty payload).
        assert!(pure_engine_search_blocks_json(e, c("   ").as_ptr()).is_null());

        pure_engine_free(e);
    }
}

/// The panel link parser over the ABI: a URI the panel bakes routes back to a
/// typed verb a shell dispatches on, and an unknown verb is null.
#[test]
fn route_link_via_abi() {
    use std::ffi::CString;
    unsafe {
        let c = |s: &str| CString::new(s).unwrap();
        let route = |s: &str| -> Option<Value> {
            take(pure_route_link_json(c(s).as_ptr())).map(|j| serde_json::from_str(&j).unwrap())
        };

        let go = route("go:1 John:3:16").unwrap();
        assert_eq!(go["verb"], "go");
        assert_eq!(go["book"], "1 John");
        assert_eq!(go["chapter"], 3);
        assert_eq!(go["verse"], 16);

        let occ = route("occ:G25").unwrap();
        assert_eq!(occ["verb"], "occurrences");
        assert_eq!(occ["code"], "G25");

        // A refkey with a colon survives (only the verb splits).
        let untag = route("untag:2:John 3:16").unwrap();
        assert_eq!(untag["verb"], "untag");
        assert_eq!(untag["tag"], 2);
        assert_eq!(untag["refKey"], "John 3:16");

        let edit = route("editentrynote:1:4").unwrap();
        assert_eq!(edit["verb"], "editEntryNote");
        assert_eq!(edit["thread"], 1);
        assert_eq!(edit["entry"], 4);

        // Unknown verb / malformed → null.
        assert!(pure_route_link_json(c("bogus:x").as_ptr()).is_null());
        assert!(pure_route_link_json(c("thread:nan").as_ptr()).is_null());
    }
}
