//! End-to-end exercise of the C ABI, driven exactly as a foreign caller would:
//! open from bytes, walk the TOC, lay out a chapter through a C measurement
//! callback, hit-test a word, look up Strong's + occurrences, search, and free
//! every handle/string. No GUI, fully deterministic (monospace measurement).

use super::*;
// The reading-map endpoints live in their own module (see reading_map.rs — lib.rs
// was already past the no-3k-line rule); `use super::*` does not reach into it.
use crate::reading_map::*;
use serde_json::Value;
use std::ffi::{CStr, CString};

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
    plumbline_string_free(p);
    Some(s)
}

unsafe fn open() -> *mut PlumblineEngine {
    let mut err: *mut c_char = ptr::null_mut();
    let e = plumbline_engine_open_from_bytes(KJV.as_ptr(), KJV.len(), STRONGS.as_ptr(), STRONGS.len(), &mut err);
    assert!(err.is_null(), "unexpected open error: {:?}", take(err));
    assert!(!e.is_null(), "engine should open");
    e
}

fn cfg() -> PlumblineLayoutConfig {
    PlumblineLayoutConfig {
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
    let s = unsafe { take(plumbline_version()) }.unwrap();
    assert_eq!(s, env!("CARGO_PKG_VERSION"));
}

#[test]
fn open_from_bytes_reports_errors() {
    unsafe {
        // Bad UTF-8 in the corpus bytes.
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open_from_bytes([0xff, 0xfe].as_ptr(), 2, STRONGS.as_ptr(), STRONGS.len(), &mut err);
        assert!(e.is_null());
        assert!(take(err).unwrap().contains("UTF-8"));

        // Malformed strongs JSON.
        let mut err2: *mut c_char = ptr::null_mut();
        let e2 = plumbline_engine_open_from_bytes(KJV.as_ptr(), KJV.len(), b"{not json".as_ptr(), 9, &mut err2);
        assert!(e2.is_null());
        assert!(take(err2).unwrap().contains("strongs.json"));
    }
}

#[test]
fn toc_and_chapter_count() {
    unsafe {
        let e = open();
        let toc: Value = serde_json::from_str(&take(plumbline_engine_toc_json(e)).unwrap()).unwrap();
        let books = toc["books"].as_array().unwrap();
        assert_eq!(books.len(), 66, "canon has 66 books");
        let john = books.iter().find(|b| b["id"] == "John").unwrap();
        assert_eq!(john["name"], "John");
        assert_eq!(john["chapters"], 3, "our corpus has John up to chapter 3");

        let c = plumbline_engine_chapter_count(e, c"John".as_ptr());
        assert_eq!(c, 3);
        // Unknown book on a valid engine floors at 1 (a safe UI range floor);
        // only a null engine yields 0 (see `null_and_freed_handles_are_safe`).
        assert_eq!(plumbline_engine_chapter_count(e, c"Nope".as_ptr()), 1);
        plumbline_engine_free(e);
    }
}

#[test]
fn layout_then_hit_test_a_word() {
    unsafe {
        let e = open();
        let dl = plumbline_engine_layout_chapter(e, c"John".as_ptr(), 3, cfg(), Some(mono_measure), ptr::null_mut());
        assert!(!dl.is_null());
        assert!(plumbline_layout_item_count(dl) > 0);
        assert!(plumbline_layout_height(dl) >= 20.0);

        // Parse the JSON to locate the word "God" (John 3:16, token index 1).
        let list: Value = serde_json::from_str(&take(plumbline_layout_to_json(dl)).unwrap()).unwrap();
        let items = list["items"].as_array().unwrap();
        let god =
            items.iter().find(|it| it["kind"] == "word" && it["text"] == "God").expect("word 'God' should be laid out");
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
        let hit: Value = serde_json::from_str(&take(plumbline_layout_hit_test_json(dl, cx, cy)).unwrap()).unwrap();
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
        assert!(plumbline_layout_hit_test_json(dl, nx, ny).is_null());

        // The paragraph flag on John 3:18's first word puts it on a new line.
        let ys: std::collections::BTreeSet<i64> = items.iter().map(|it| it["y"].as_f64().unwrap() as i64).collect();
        assert!(ys.len() > 1, "paragraph break should add a line");

        plumbline_layout_free(dl);
        plumbline_engine_free(e);
    }
}

#[test]
fn layout_absent_or_out_of_range_chapter_is_null() {
    unsafe {
        let e = open();
        let m: PlumblineMeasureFn = Some(mono_measure);
        // A chapter not present in this corpus (only John 3 exists here).
        assert!(plumbline_engine_layout_chapter(e, c"John".as_ptr(), 1, cfg(), m, ptr::null_mut()).is_null());
        // An unknown book.
        assert!(plumbline_engine_layout_chapter(e, c"Nope".as_ptr(), 3, cfg(), m, ptr::null_mut()).is_null());
        // A chapter outside the u16 domain must NOT wrap into a real chapter
        // (65539 as u16 == 3, which does exist — regression guard).
        assert!(plumbline_engine_layout_chapter(e, c"John".as_ptr(), 65539, cfg(), m, ptr::null_mut()).is_null());
        plumbline_engine_free(e);
    }
}

#[test]
fn null_measure_yields_null_layout() {
    unsafe {
        let e = open();
        let dl = plumbline_engine_layout_chapter(e, c"John".as_ptr(), 3, cfg(), None, ptr::null_mut());
        assert!(dl.is_null(), "a null measure callback must fail cleanly");
        plumbline_engine_free(e);
    }
}

#[test]
fn strongs_entry_and_occurrences() {
    unsafe {
        let e = open();

        let entry: Value =
            serde_json::from_str(&take(plumbline_engine_strongs_json(e, c"G2316".as_ptr())).unwrap()).unwrap();
        assert_eq!(entry["code"], "G2316");
        assert_eq!(entry["lemma"], "θεός");
        assert_eq!(entry["kjv"], "God");
        // Derived / pron absent-vs-present handled as null.
        assert_eq!(entry["deriv"], Value::Null);

        // Unknown code → null.
        assert!(plumbline_engine_strongs_json(e, c"H9999".as_ptr()).is_null());

        let occ: Value =
            serde_json::from_str(&take(plumbline_engine_strongs_occurrences_json(e, c"G2316".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(occ["code"], "G2316");
        assert_eq!(occ["total"], 1);
        assert_eq!(occ["capped"], false);
        assert_eq!(occ["verses"][0], "John 3:16");

        plumbline_engine_free(e);
    }
}

#[test]
fn renderings_and_word_codes() {
    unsafe {
        let e = open();

        // Forward lens: G25 (agapao) is rendered "loved" in John 3:16, token 3.
        let r: Value =
            serde_json::from_str(&take(plumbline_engine_renderings_json(e, c"G25".as_ptr())).unwrap()).unwrap();
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
        let empty: Value =
            serde_json::from_str(&take(plumbline_engine_renderings_json(e, c"H9999".as_ptr())).unwrap()).unwrap();
        assert_eq!(empty["renderings"].as_array().unwrap().len(), 0);

        // Reverse lens: the surface word "God" (normalized) maps to G2316.
        let w: Value =
            serde_json::from_str(&take(plumbline_engine_word_codes_json(e, c"God".as_ptr())).unwrap()).unwrap();
        assert_eq!(w["word"], "God");
        let codes = w["codes"].as_array().unwrap();
        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0]["code"], "G2316");
        assert_eq!(codes[0]["count"], 1);

        // A translator-supplied (added) word carries no codes.
        let the: Value =
            serde_json::from_str(&take(plumbline_engine_word_codes_json(e, c"the".as_ptr())).unwrap()).unwrap();
        assert_eq!(the["codes"].as_array().unwrap().len(), 0);

        plumbline_engine_free(e);
    }
}

#[test]
fn verse_and_token_lookup() {
    unsafe {
        let e = open();
        let verse: Value =
            serde_json::from_str(&take(plumbline_engine_verse_json(e, c"John 3:16".as_ptr())).unwrap()).unwrap();
        assert_eq!(verse["reference"], "John 3:16");
        assert!(verse["body"].as_str().unwrap().contains("God"));
        assert_eq!(verse["tokens"].as_array().unwrap().len(), 6);

        // Token index 4 ("the") carries the KJV-added flag in our sample.
        let tok: Value =
            serde_json::from_str(&take(plumbline_engine_token_json(e, c"John 3:16".as_ptr(), 4)).unwrap()).unwrap();
        assert_eq!(tok["word"], "the");
        assert_eq!(tok["flags"], PLUMBLINE_FLAG_ADDED);

        // Out-of-range token / bad ref → null.
        assert!(plumbline_engine_token_json(e, c"John 3:16".as_ptr(), 99).is_null());
        assert!(plumbline_engine_verse_json(e, c"garbage".as_ptr()).is_null());
        assert!(plumbline_engine_verse_json(e, c"John 9:9".as_ptr()).is_null());

        plumbline_engine_free(e);
    }
}

#[test]
fn search_word_reference_and_bare_strongs() {
    unsafe {
        let e = open();

        // Word search.
        let hits: Value =
            serde_json::from_str(&take(plumbline_engine_search_json(e, c"loved".as_ptr())).unwrap()).unwrap();
        assert_eq!(hits["kind"], "hits");
        assert!(hits["total"].as_u64().unwrap() >= 1);
        assert_eq!(hits["hits"][0]["verse"], "John 3:16");

        // Reference query → goto.
        let goto: Value =
            serde_json::from_str(&take(plumbline_engine_search_json(e, c"John 3".as_ptr())).unwrap()).unwrap();
        assert_eq!(goto["kind"], "goto");
        assert_eq!(goto["book"], "John");
        assert_eq!(goto["chapter"], 3);
        assert_eq!(goto["verse"], Value::Null);

        // Bare Strong's code → verses tagged with it.
        let tagged: Value =
            serde_json::from_str(&take(plumbline_engine_search_json(e, c"G2316".as_ptr())).unwrap()).unwrap();
        assert_eq!(tagged["kind"], "hits");
        assert_eq!(tagged["hits"][0]["verse"], "John 3:16");

        // Blank query → null.
        assert!(plumbline_engine_search_json(e, c"   ".as_ptr()).is_null());

        plumbline_engine_free(e);
    }
}

#[test]
fn null_and_freed_handles_are_safe() {
    unsafe {
        // Null handles never crash.
        assert!(plumbline_engine_toc_json(ptr::null()).is_null());
        assert_eq!(plumbline_engine_chapter_count(ptr::null(), c"John".as_ptr()), 0);
        assert!(plumbline_layout_to_json(ptr::null()).is_null());
        assert_eq!(plumbline_layout_height(ptr::null()), 0.0);
        assert!(plumbline_layout_hit_test_json(ptr::null(), 0.0, 0.0).is_null());
        // Freeing null is a no-op.
        plumbline_engine_free(ptr::null_mut());
        plumbline_layout_free(ptr::null_mut());
        plumbline_string_free(ptr::null_mut());
    }
}

#[test]
fn authoring_round_trip_via_abi() {
    use std::ffi::CString;
    unsafe {
        // A temp home with the two data files plumbline_engine_open expects.
        let home = std::env::temp_dir().join(format!("plumbline-ffi-author-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null());
        assert!(!e.is_null());

        let c = |s: &str| CString::new(s).unwrap();
        let stamp = c("2026-01-01T00:00:00Z");

        // Author: tag a verse, add a verse to a thread, weave two verses.
        // A null return means success.
        assert!(plumbline_engine_tag_add(
            e,
            c("Messianic").as_ptr(),
            c("verse").as_ptr(),
            c("John 3:16").as_ptr(),
            ptr::null(),
            stamp.as_ptr()
        )
        .is_null());
        assert!(plumbline_engine_tag_add(
            e,
            c("Messianic").as_ptr(),
            c("concept").as_ptr(),
            c("G2316").as_ptr(),
            ptr::null(),
            stamp.as_ptr()
        )
        .is_null());
        assert!(plumbline_engine_thread_add(
            e,
            c("Road").as_ptr(),
            c("John 3:16").as_ptr(),
            ptr::null(),
            stamp.as_ptr()
        )
        .is_null());
        assert!(plumbline_engine_weave_add_link(
            e,
            c("Links").as_ptr(),
            c("John 3:16").as_ptr(),
            c("John 3:18").as_ptr(),
            stamp.as_ptr()
        )
        .is_null());

        // Read back through the ABI (the engine reloaded after each write).
        let tags: Value = serde_json::from_str(&take(plumbline_engine_tags_json(e)).unwrap()).unwrap();
        assert_eq!(tags["tags"][0]["name"], "Messianic");
        assert_eq!(tags["tags"][0]["members"].as_array().unwrap().len(), 2);
        assert_eq!(tags["tags"][0]["members"][0]["verse"], "John 3:16");
        assert_eq!(tags["tags"][0]["members"][1]["strongs"], "G2316");

        let threads: Value = serde_json::from_str(&take(plumbline_engine_threads_json(e)).unwrap()).unwrap();
        assert_eq!(threads["threads"][0]["name"], "Road");
        assert_eq!(threads["threads"][0]["entries"][0]["verse"], "John 3:16");

        let xrefs: Value =
            serde_json::from_str(&take(plumbline_engine_verse_xrefs_json(e, c("John 3:16").as_ptr())).unwrap())
                .unwrap();
        assert_eq!(xrefs["partners"][0]["verse"], "John 3:18");
        assert_eq!(xrefs["partners"][0]["weave"], "Links");

        // Edit notes: thread doc, an entry note, and the weave doc.
        assert!(plumbline_engine_thread_set_notes(e, c("Road").as_ptr(), c("the gospel road").as_ptr()).is_null());
        assert!(plumbline_engine_thread_entry_set_note(e, c("Road").as_ptr(), 0, c("start here").as_ptr()).is_null());
        assert!(plumbline_engine_weave_set_notes(e, c("Links").as_ptr(), c("belief and judgment").as_ptr()).is_null());
        let threads: Value = serde_json::from_str(&take(plumbline_engine_threads_json(e)).unwrap()).unwrap();
        assert_eq!(threads["threads"][0]["notes"], "the gospel road");
        assert_eq!(threads["threads"][0]["entries"][0]["note"], "start here");
        // Clearing an entry note (null) and error paths.
        assert!(plumbline_engine_thread_entry_set_note(e, c("Road").as_ptr(), 0, ptr::null()).is_null());
        assert!(take(plumbline_engine_weave_set_notes(e, c("Nope").as_ptr(), c("x").as_ptr()))
            .unwrap()
            .contains("weave"));
        assert!(take(plumbline_engine_thread_entry_set_note(e, c("Road").as_ptr(), 9, ptr::null()))
            .unwrap()
            .contains("entry"));

        // Error paths: a bad target kind, and a bytes-opened engine has no home.
        assert!(take(plumbline_engine_tag_add(
            e,
            c("X").as_ptr(),
            c("bogus").as_ptr(),
            c("v").as_ptr(),
            ptr::null(),
            stamp.as_ptr()
        ))
        .unwrap()
        .contains("kind"));
        let bytes_engine = open();
        assert!(take(plumbline_engine_tag_add(
            bytes_engine,
            c("X").as_ptr(),
            c("verse").as_ptr(),
            c("John 3:16").as_ptr(),
            ptr::null(),
            stamp.as_ptr()
        ))
        .unwrap()
        .contains("home"));
        plumbline_engine_free(bytes_engine);

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn weave_from_tag_via_abi() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-tagweave-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let e = plumbline_engine_open(home_c.as_ptr(), ptr::null_mut());
        assert!(!e.is_null());
        let c = |s: &str| CString::new(s).unwrap();
        let stamp = c("2026-07-25T00:00:00Z");

        // Accumulate a topic tag over time (a concept member rides along and
        // must be ignored by the conversion), then weave it.
        for (kind, value) in [("verse", "John 3:18"), ("verse", "John 3:16"), ("concept", "G4100")] {
            assert!(plumbline_engine_tag_add(
                e,
                c("Belief").as_ptr(),
                c(kind).as_ptr(),
                c(value).as_ptr(),
                ptr::null(),
                stamp.as_ptr()
            )
            .is_null());
        }
        assert!(plumbline_engine_weave_from_tag(e, c("belief").as_ptr(), ptr::null(), ptr::null(), stamp.as_ptr())
            .is_null());

        let weaves: Value = serde_json::from_str(&take(plumbline_engine_weaves_json(e)).unwrap()).unwrap();
        assert_eq!(weaves["weaves"][0]["name"], "Belief"); // null name → the tag's
        let links = weaves["weaves"][0]["links"].as_array().unwrap();
        assert_eq!(links.len(), 1); // canon-ordered chain of the two verses
        assert_eq!(links[0]["a"], "John 3:16");
        assert_eq!(links[0]["b"], "John 3:18");

        // A named subset with one ref is not a weave; an unknown tag errors.
        assert!(take(plumbline_engine_weave_from_tag(
            e,
            c("Belief").as_ptr(),
            c(r#"["John 3:16"]"#).as_ptr(),
            c("Solo").as_ptr(),
            stamp.as_ptr()
        ))
        .unwrap()
        .contains("two distinct"));
        assert!(take(plumbline_engine_weave_from_tag(e, c("Nope").as_ptr(), ptr::null(), ptr::null(), stamp.as_ptr()))
            .unwrap()
            .contains("no tag"));

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn suggested_weave_review_via_abi() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-review-{}", std::process::id()));
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
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let c = |s: &str| CString::new(s).unwrap();

        // List: both show up, ordered, each with its ordinal index.
        let listed: Value = serde_json::from_str(&take(plumbline_engine_suggested_weaves_json(e)).unwrap()).unwrap();
        let items = listed["suggested"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["index"], 0);
        assert_eq!(items[0]["links"][0]["aDisplay"], "John 3:16");

        // Approve index 0 → it leaves the suggested queue (one left).
        assert!(plumbline_engine_weave_approve(e, 0).is_null());
        let after: Value = serde_json::from_str(&take(plumbline_engine_suggested_weaves_json(e)).unwrap()).unwrap();
        assert_eq!(after["suggested"].as_array().unwrap().len(), 1);
        // The approved weave now asserts its cross-reference from weaves/.
        let xrefs: Value =
            serde_json::from_str(&take(plumbline_engine_verse_xrefs_json(e, c("John 3:16").as_ptr())).unwrap())
                .unwrap();
        assert!(xrefs["partners"].as_array().unwrap().iter().any(|p| p["verse"] == "John 3:18"));

        // Reject the remaining one (now index 0) → queue empties.
        assert!(plumbline_engine_weave_reject(e, 0).is_null());
        let empty: Value = serde_json::from_str(&take(plumbline_engine_suggested_weaves_json(e)).unwrap()).unwrap();
        assert!(empty["suggested"].as_array().unwrap().is_empty());

        // Error paths: out-of-range index, and a bytes-opened engine has no home.
        assert!(take(plumbline_engine_weave_approve(e, 9)).unwrap().contains("index"));
        let bytes_engine = open();
        assert!(take(plumbline_engine_weave_reject(bytes_engine, 0)).unwrap().contains("home"));
        plumbline_engine_free(bytes_engine);

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn rnd_tier_via_abi() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-rnd-{}", std::process::id()));
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
        std::fs::write(home.join("data").join("source-priors.json"), r#"{"priors":{"lxx":0.85,"_default":0.5}}"#)
            .unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let c = |s: &str| CString::new(s).unwrap();

        // Fused bridge partner from the external witness.
        let b: Value =
            serde_json::from_str(&take(plumbline_engine_bridge_partners_json(e, c("G25").as_ptr())).unwrap()).unwrap();
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
        let m: Value =
            serde_json::from_str(&take(plumbline_engine_morph_json(e, c("John 3:16").as_ptr(), 3)).unwrap()).unwrap();
        assert_eq!(m["code"], "V-AAI-3S");
        assert_eq!(m["gloss"], "aorist active indicative, 3rd singular");
        // A token with no annotation → null.
        assert!(plumbline_engine_morph_json(e, c("John 3:16").as_ptr(), 1).is_null());

        // A bytes-opened engine has no morphology → it returns null.
        let bytes_engine = open();
        assert!(plumbline_engine_morph_json(bytes_engine, c("John 3:16").as_ptr(), 3).is_null());
        plumbline_engine_free(bytes_engine);

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn parity_endpoints_via_abi() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-parity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();
        // Margin notes + TSK cross-references + a spanned weave.
        std::fs::write(home.join("data").join("kjv-notes.jsonl"), r#"{"b":"John","c":3,"v":16,"note":"Or, begotten"}"#)
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
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let c = |s: &str| CString::new(s).unwrap();

        // Margin notes: present verse → notes; absent verse → null.
        let notes: Value =
            serde_json::from_str(&take(plumbline_engine_verse_notes_json(e, c("John 3:16").as_ptr())).unwrap())
                .unwrap();
        assert_eq!(notes["notes"][0], "Or, begotten");
        assert!(plumbline_engine_verse_notes_json(e, c("John 3:18").as_ptr()).is_null());

        // TSK: best-voted first, range end carried.
        let xr: Value =
            serde_json::from_str(&take(plumbline_engine_study_xrefs_json(e, c("John 3:16").as_ptr())).unwrap())
                .unwrap();
        let refs = xr["refs"].as_array().unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0]["votes"], 5);
        assert!(refs[0]["end"].is_null());
        assert_eq!(refs[1]["end"], "John 3:18");

        // Weave library: spans, approval, kind label, resolvability.
        let ws: Value = serde_json::from_str(&take(plumbline_engine_weaves_json(e)).unwrap()).unwrap();
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
        assert!(plumbline_engine_weave_add_link_spans(
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
        let ws: Value = serde_json::from_str(&take(plumbline_engine_weaves_json(e)).unwrap()).unwrap();
        let pinned = ws["weaves"].as_array().unwrap().iter().find(|w| w["name"] == "Pinned").unwrap();
        // Reversed bounds normalise; the span-less side stays null.
        assert_eq!(pinned["links"][0]["spanA"][0], 1);
        assert_eq!(pinned["links"][0]["spanA"][1], 3);
        assert!(pinned["links"][0]["spanB"].is_null());

        // Link pairs: deduped canonical pairs, each endpoint located, with the
        // resolvability flag (the Gen 1:1 endpoint is outside this corpus).
        let lp: Value = serde_json::from_str(&take(plumbline_engine_link_pairs_json(e)).unwrap()).unwrap();
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
        let cs: Value = serde_json::from_str(&take(plumbline_engine_canon_segments_json(e)).unwrap()).unwrap();
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
        let cm: Value = serde_json::from_str(&take(plumbline_engine_chord_map_json(e)).unwrap()).unwrap();
        assert_eq!(cm["otNtDivide"], 39);
        assert_eq!(cm["bookCount"], 66);
        assert_eq!(cm["max"], 1);
        let cpairs = cm["pairs"].as_array().unwrap();
        assert_eq!(cpairs.len(), 2);
        // Gen↔John: Gen is book 0, John is in the NT (index ≥ 39); a <= b holds.
        let cross = cpairs.iter().find(|p| p["a"].as_u64() != p["b"].as_u64()).expect("a cross-book pair");
        assert_eq!(cross["a"], 0);
        assert!(cross["b"].as_u64().unwrap() >= 39);
        assert_eq!(cross["count"], 1);
        // John↔John self-pair.
        let selfp = cpairs.iter().find(|p| p["a"].as_u64() == p["b"].as_u64()).expect("a self-pair");
        assert!(selfp["a"].as_u64().unwrap() >= 39);
        assert_eq!(selfp["count"], 1);

        // Constellation: the "Spanned" weave has one resolvable link (John
        // 3:16↔John 3:18); its Gen 1:1↔John 3:16 link is unresolved, so it never
        // becomes a lane. The "Pinned" weave (authored above) also resolves.
        let con: Value =
            serde_json::from_str(&take(plumbline_engine_constellation_json(e, 0, ptr::null())).unwrap()).unwrap();
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
        let con2: Value =
            serde_json::from_str(&take(plumbline_engine_constellation_json(e, 0, pins.as_ptr())).unwrap()).unwrap();
        assert_eq!(con2["nPins"], 1);
        assert_eq!(con2["lanes"][0]["weaveIndex"], sidx);
        assert_eq!(con2["lanes"][0]["pinned"], true);
        assert!(con2["caption"].as_str().unwrap().starts_with("1 pinned · "));

        plumbline_engine_free(e);
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
        let cj: Value =
            serde_json::from_str(&take(plumbline_engine_concept_json(e, c("G2316").as_ptr())).unwrap()).unwrap();
        assert_eq!(cj["total"], 1);
        assert_eq!(cj["ot"], 0);
        assert_eq!(cj["nt"], 1);
        assert_eq!(cj["topBooks"][0]["book"], "John");
        assert_eq!(cj["byBook"]["John"], 1);
        // Too few occurrences for a leitwort (min 8).
        assert!(cj["leitwort"].is_null());
        // Unknown code → null.
        assert!(plumbline_engine_concept_json(e, c("H9999").as_ptr()).is_null());

        // English gloss: the modal KJV rendering carrying the code.
        assert_eq!(take(plumbline_engine_gloss(e, c("G2316").as_ptr())).unwrap(), "God");
        assert_eq!(take(plumbline_engine_gloss(e, c("G25").as_ptr())).unwrap(), "loved");
        // Untagged code distils the dictionary; unknown → null.
        assert!(plumbline_engine_gloss(e, c("H9999").as_ptr()).is_null());

        plumbline_engine_free(e);
    }
}

#[test]
fn config_round_trip_via_abi() {
    use std::ffi::CString;
    unsafe {
        // Redirect the per-user config dir into a temp sandbox.
        let dir = std::env::temp_dir().join(format!("plumbline-ffi-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if cfg!(target_os = "windows") {
            std::env::set_var("APPDATA", &dir);
        } else {
            std::env::set_var("XDG_CONFIG_HOME", &dir);
        }

        // No file yet → defaults + firstRun.
        let loaded: Value = serde_json::from_str(&take(plumbline_config_load_json()).unwrap()).unwrap();
        assert_eq!(loaded["firstRun"], true);
        assert_eq!(loaded["studyMode"], "simple");

        // Save a full-study, two-pane session and read it back.
        let saved = r#"{"studyMode":"full","bodySize":21.0,"openPanes":[{"book":"Gen","chapter":15},{"book":"Rom","chapter":4}],"activePane":1}"#;
        let sc = CString::new(saved).unwrap();
        assert!(plumbline_config_save_json(sc.as_ptr()).is_null());
        let loaded: Value = serde_json::from_str(&take(plumbline_config_load_json()).unwrap()).unwrap();
        assert_eq!(loaded["firstRun"], false);
        assert_eq!(loaded["studyMode"], "full");
        assert_eq!(loaded["bodySize"], 21.0);
        assert_eq!(loaded["openPanes"][1]["book"], "Rom");
        assert_eq!(loaded["activePane"], 1);
        // Not asked for → off (AUDIT 2026-07-29: the field used to be missing
        // from the wire state entirely, so a shell's save dropped it).
        assert_eq!(loaded["akjvOverlay"], false);

        // The plain-English overlay is a reader preference like any other: what
        // a shell saves is what the next load hands back.
        let saved = r#"{"studyMode":"full","bodySize":21.0,"akjvOverlay":true}"#;
        let sc = CString::new(saved).unwrap();
        assert!(plumbline_config_save_json(sc.as_ptr()).is_null());
        let loaded: Value = serde_json::from_str(&take(plumbline_config_load_json()).unwrap()).unwrap();
        assert_eq!(loaded["akjvOverlay"], true, "the shells' overlay switch did not survive a save");

        // Garbage json is an error, not a panic.
        let bad = CString::new("{nope").unwrap();
        assert!(take(plumbline_config_save_json(bad.as_ptr())).unwrap().contains("bad config json"));

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
            &take(plumbline_engine_word_study_blocks_json(e, c("John 3:16").as_ptr(), 1, true)).unwrap(),
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
            &take(plumbline_engine_word_study_blocks_json(e, c("John 3:16").as_ptr(), 1, false)).unwrap(),
        )
        .unwrap();
        assert!(!simple["blocks"].as_array().unwrap().iter().any(|b| b["kind"] == "section"));

        // Concordance: the go: link for the one occurrence verse.
        let cc: Value =
            serde_json::from_str(&take(plumbline_engine_concordance_blocks_json(e, c("G2316").as_ptr())).unwrap())
                .unwrap();
        assert!(uris(&cc).contains(&"go:John:3:16".to_string()));

        // A word search → ranked hits, each a go: link (John 3:16 has "God").
        let sr: Value =
            serde_json::from_str(&take(plumbline_engine_search_blocks_json(e, c("God").as_ptr())).unwrap()).unwrap();
        assert!(uris(&sr).contains(&"go:John:3:16".to_string()));

        // A blank query is null (not an empty payload).
        assert!(plumbline_engine_search_blocks_json(e, c("   ").as_ptr()).is_null());

        plumbline_engine_free(e);
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
            take(plumbline_route_link_json(c(s).as_ptr())).map(|j| serde_json::from_str(&j).unwrap())
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

        // The new Tier-0 verbs route too.
        let note = route("editnote:John 3:16").unwrap();
        assert_eq!(note["verb"], "editNote");
        assert_eq!(note["refKey"], "John 3:16");
        assert_eq!(route("guide").unwrap()["verb"], "guide");
        assert_eq!(route("about").unwrap()["verb"], "about");

        // Unknown verb / malformed → null.
        assert!(plumbline_route_link_json(c("bogus:x").as_ptr()).is_null());
        assert!(plumbline_route_link_json(c("thread:nan").as_ptr()).is_null());
    }
}

/// The Tier-0 endpoints over the ABI: copy text, personal notes (write → read),
/// the theme palette, guide/about blocks, and index warming. Exercised through a
/// temp home exactly as a shell would.
#[test]
fn tier0_endpoints_via_abi() {
    use std::ffi::CString;
    unsafe {
        // Engine-independent endpoints first (no home needed).
        let c = |s: &str| CString::new(s).unwrap();
        // A null / unknown theme falls back to light; "night" is true-black.
        let light: Value = serde_json::from_str(&take(plumbline_theme_palette_json(ptr::null())).unwrap()).unwrap();
        assert_eq!(light["paper"], "#fcf9f4");
        assert_eq!(light["dark"], false);
        let palette: Value =
            serde_json::from_str(&take(plumbline_theme_palette_json(c("night").as_ptr())).unwrap()).unwrap();
        assert_eq!(palette["paper"], "#000000");
        assert_eq!(palette["dark"], true);
        let guide: Value = serde_json::from_str(&take(plumbline_panel_guide_blocks_json()).unwrap()).unwrap();
        assert!(!guide["blocks"].as_array().unwrap().is_empty());
        assert!(!take(plumbline_panel_about_blocks_json()).unwrap().is_empty());

        // A temp home for the authoring endpoints.
        let home = std::env::temp_dir().join(format!("plumbline-ffi-tier0-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();
        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null());
        assert!(!e.is_null());
        let stamp = c("2026-01-01T00:00:00Z");

        // Copy: plain, ref-suffixed, markdown.
        let plain = take(plumbline_engine_copy_text(e, c("John 3:16").as_ptr(), c("verse").as_ptr())).unwrap();
        assert!(plain.starts_with("For God so loved"));
        let refd = take(plumbline_engine_copy_text(e, c("John 3:16").as_ptr(), c("verseRef").as_ptr())).unwrap();
        assert!(refd.ends_with("— John 3:16 (KJV)"));
        assert!(plumbline_engine_copy_text(e, c("John 3:16").as_ptr(), c("bogus").as_ptr()).is_null());

        // Personal note: absent → null, set → readable, cleared → null again.
        assert!(plumbline_engine_user_note_json(e, c("John 3:16").as_ptr()).is_null());
        assert!(plumbline_engine_user_note_set(e, c("John 3:16").as_ptr(), c("golden text").as_ptr(), stamp.as_ptr())
            .is_null());
        let note: Value =
            serde_json::from_str(&take(plumbline_engine_user_note_json(e, c("John 3:16").as_ptr())).unwrap()).unwrap();
        assert_eq!(note["text"], "golden text");
        let all: Value = serde_json::from_str(&take(plumbline_engine_user_notes_json(e)).unwrap()).unwrap();
        assert_eq!(all["notes"].as_array().unwrap().len(), 1);
        assert!(plumbline_engine_user_note_set(e, c("John 3:16").as_ptr(), c("").as_ptr(), stamp.as_ptr()).is_null());
        assert!(plumbline_engine_user_note_json(e, c("John 3:16").as_ptr()).is_null());

        // Memorization (Tier 2 #15): grade → card, drill, recall, coverage, activity.
        assert!(plumbline_engine_memory_grade(e, c("John 3:16").as_ptr(), c("good").as_ptr(), stamp.as_ptr()).is_null());
        let card: Value =
            serde_json::from_str(&take(plumbline_engine_memory_card_json(e, c("John 3:16").as_ptr())).unwrap())
                .unwrap();
        assert_eq!(card["ref"], "John 3:16");
        assert_eq!(card["reps"], 1);
        assert_eq!(card["mastery"], "young"); // 1-day interval after one Good
        assert_eq!(card["reviews"].as_array().unwrap().len(), 1);
        // An unknown grade is rejected (non-null error).
        assert!(
            !plumbline_engine_memory_grade(e, c("John 3:16").as_ptr(), c("bogus").as_ptr(), stamp.as_ptr()).is_null()
        );

        // Drill: first-letter skeleton + (level-0) unblanked form of the verse.
        let drill: Value =
            serde_json::from_str(&take(plumbline_engine_memory_drill_json(e, c("John 3:16").as_ptr(), 0)).unwrap())
                .unwrap();
        assert!(drill["text"].as_str().unwrap().starts_with("For God so loved"));
        assert_eq!(drill["firstLetters"], "F G s l t w.");
        assert!(!drill["blanked"].as_str().unwrap().contains('_')); // nothing hidden at level 0

        // Recall scoring: a perfect (case/punctuation-tolerant) recall is 1.0.
        let sc: Value = serde_json::from_str(
            &take(plumbline_engine_memory_score_json(
                e,
                c("John 3:16").as_ptr(),
                c("for god so loved the world").as_ptr(),
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(sc["accuracy"], 1.0);

        // Coverage + activity, from the review log.
        let cov: Value =
            serde_json::from_str(&take(plumbline_engine_memory_coverage_json(e, stamp.as_ptr())).unwrap()).unwrap();
        assert_eq!(cov["verses"][0]["ref"], "John 3:16");
        let gospels = cov["sections"].as_array().unwrap().iter().find(|s| s["label"] == "Gospels").unwrap().clone();
        assert_eq!(gospels["cards"], 1);
        let act: Value = serde_json::from_str(&take(plumbline_engine_memory_activity_json(e)).unwrap()).unwrap();
        assert_eq!(act["days"].as_array().unwrap().len(), 1);

        // Seed a card without reviewing ("Memorize this verse") → new, reps 0.
        assert!(plumbline_engine_memory_add(e, c("John 3:18").as_ptr(), stamp.as_ptr()).is_null());
        let seeded: Value =
            serde_json::from_str(&take(plumbline_engine_memory_card_json(e, c("John 3:18").as_ptr())).unwrap())
                .unwrap();
        assert_eq!(seeded["reps"], 0);
        assert_eq!(seeded["mastery"], "new");
        assert!(plumbline_engine_memory_remove(e, c("John 3:18").as_ptr()).is_null());

        // Remove → the card is gone.
        assert!(plumbline_engine_memory_remove(e, c("John 3:16").as_ptr()).is_null());
        assert!(plumbline_engine_memory_card_json(e, c("John 3:16").as_ptr()).is_null());

        // A passage memorized as ONE chunk (2026-07-27): the card is keyed by
        // its first verse, labelled as a range, and drilled/scored over the
        // whole span — not one card per verse. (This fixture holds John 3:16 and
        // 3:18 only, which also exercises a span whose middle verse is absent.)
        assert!(plumbline_engine_memory_add_passage(
            e,
            c("John 3:16").as_ptr(),
            c("John 3:18").as_ptr(),
            stamp.as_ptr()
        )
        .is_null());
        let pc: Value =
            serde_json::from_str(&take(plumbline_engine_memory_card_json(e, c("John 3:16").as_ptr())).unwrap())
                .unwrap();
        assert_eq!(pc["ref"], "John 3:16");
        assert_eq!(pc["label"], "John 3:16\u{2013}18");
        assert_eq!(pc["through"], "John 3:18");
        // Only the first verse addresses the card — inner verses are not cards.
        assert!(plumbline_engine_memory_card_json(e, c("John 3:18").as_ptr()).is_null());

        let pd: Value =
            serde_json::from_str(&take(plumbline_engine_memory_drill_json(e, c("John 3:16").as_ptr(), 0)).unwrap())
                .unwrap();
        assert_eq!(pd["label"], "John 3:16\u{2013}18");
        assert_eq!(pd["verses"], 2, "two of the three verses exist in this fixture");
        let ptext = pd["text"].as_str().unwrap().to_string();
        assert_eq!(ptext, "For God so loved the world. He that believeth");
        assert_eq!(pd["firstLetters"], "F G s l t w. H t b");
        // Typing only the opening verse of a passage cannot score full marks.
        let half: Value = serde_json::from_str(
            &take(plumbline_engine_memory_score_json(
                e,
                c("John 3:16").as_ptr(),
                c("For God so loved the world.").as_ptr(),
            ))
            .unwrap(),
        )
        .unwrap();
        let acc = half["accuracy"].as_f64().unwrap();
        assert!(acc > 0.5 && acc < 1.0, "half a passage scores partial, got {acc}");
        // Typing the whole passage back scores it in full.
        let whole: Value = serde_json::from_str(
            &take(plumbline_engine_memory_score_json(e, c("John 3:16").as_ptr(), c(&ptext).as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(whole["accuracy"], 1.0);

        // The hub lists ONE row for the passage; the map shades every verse of it.
        let pcov: Value =
            serde_json::from_str(&take(plumbline_engine_memory_coverage_json(e, stamp.as_ptr())).unwrap()).unwrap();
        assert_eq!(pcov["cards"].as_array().unwrap().len(), 1);
        assert_eq!(pcov["cards"][0]["ref"], "John 3:16");
        assert_eq!(pcov["cards"][0]["label"], "John 3:16\u{2013}18");
        assert_eq!(pcov["cards"][0]["verses"], 3);
        assert_eq!(
            pcov["verses"].as_array().unwrap().iter().map(|v| v["ref"].as_str().unwrap()).collect::<Vec<_>>(),
            ["John 3:16", "John 3:17", "John 3:18"]
        );
        // Grading the passage keeps it one card, still spanning.
        assert!(plumbline_engine_memory_grade(e, c("John 3:16").as_ptr(), c("good").as_ptr(), stamp.as_ptr()).is_null());
        let graded: Value =
            serde_json::from_str(&take(plumbline_engine_memory_card_json(e, c("John 3:16").as_ptr())).unwrap())
                .unwrap();
        assert_eq!((graded["reps"].as_u64(), graded["through"].as_str()), (Some(1), Some("John 3:18")));
        assert!(plumbline_engine_memory_remove(e, c("John 3:16").as_ptr()).is_null());

        // A backwards end is not a passage — it seeds a plain single-verse card.
        assert!(plumbline_engine_memory_add_passage(
            e,
            c("John 3:18").as_ptr(),
            c("John 3:16").as_ptr(),
            stamp.as_ptr()
        )
        .is_null());
        let flat: Value =
            serde_json::from_str(&take(plumbline_engine_memory_card_json(e, c("John 3:18").as_ptr())).unwrap())
                .unwrap();
        assert_eq!(flat["label"], "John 3:18");
        assert!(flat["through"].is_null());
        assert!(plumbline_engine_memory_remove(e, c("John 3:18").as_ptr()).is_null());
        // An end verse that does not exist is refused, not silently flattened.
        assert!(!plumbline_engine_memory_add_passage(
            e,
            c("John 3:16").as_ptr(),
            c("John 3:999").as_ptr(),
            stamp.as_ptr()
        )
        .is_null());
        assert!(plumbline_engine_memory_card_json(e, c("John 3:16").as_ptr()).is_null());

        // Warming is a null-on-success no-op that stays callable.
        assert!(plumbline_engine_warm_indexes(e).is_null());

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// Boot-phase timing harness for TODO #28 (PWA mobile performance): times
/// engine open + each lazy analytics build over the repo's own data home.
/// Ignored by default — run manually, release mode, when tuning boot:
///   cargo test --release -p plumbline-ffi timing_harness -- --ignored --nocapture
#[test]
#[ignore]
fn timing_harness() {
    use std::time::Instant;
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let home = std::ffi::CString::new(repo.to_str().unwrap()).unwrap();
    let mut err: *mut c_char = ptr::null_mut();

    let t = Instant::now();
    let e = unsafe { plumbline_engine_open(home.as_ptr(), &mut err) };
    assert!(!e.is_null(), "open failed: {:?}", unsafe { opt_str(err) });
    println!("open (corpus+strongs+xref+bridge+morph+embed): {:?}", t.elapsed());
    let eng = unsafe { &*e };

    let t = Instant::now();
    let _ = eng.concept();
    println!("concept build:  {:?}", t.elapsed());

    let t = Instant::now();
    let _ = eng.leitwort();
    println!("leitwort scan:  {:?}", t.elapsed());

    unsafe { plumbline_engine_free(e) };
}

/// Companion to [`timing_harness`]: the open-time loads, individually.
#[test]
#[ignore]
fn timing_harness_open_parts() {
    use std::time::Instant;
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data = repo.join("data");

    let t = Instant::now();
    let m = morph::load_morph(canon::TOKENIZATION_VERSION, data.join("morphology.jsonl"));
    println!("morphology parse: {:?} (loaded: {})", t.elapsed(), m.is_some());

    let t = Instant::now();
    let x = crossref::load_cross_refs(crossref::cross_refs_path(&repo));
    println!("crossref parse:   {:?} (entries: {})", t.elapsed(), x.len());
}

/// Companion to [`timing_harness`]: concept-build internals + core index builds.
#[test]
#[ignore]
fn timing_harness_concept_parts() {
    use std::time::Instant;
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    let t = Instant::now();
    let corpus = corpus::load_corpus(repo.join("data/kjv.jsonl")).unwrap();
    println!("corpus load:      {:?}", t.elapsed());

    let t = Instant::now();
    let strongs = strongs::load_strongs(repo.join("data/strongs.json")).unwrap();
    println!("strongs parse:    {:?} ({} entries)", t.elapsed(), strongs.len());

    let t = Instant::now();
    let _ = SearchIx::build(&corpus);
    println!("search ix build:  {:?}", t.elapsed());

    let t = Instant::now();
    let _ = OccurrenceIx::build(&corpus);
    println!("occ ix build:     {:?}", t.elapsed());

    let t = Instant::now();
    let _ = Renderings::build(&corpus);
    println!("renderings build: {:?}", t.elapsed());

    let t = Instant::now();
    let ix = concept::build_concept_ix(&corpus);
    println!("concept ix:       {:?} ({} codes)", t.elapsed(), ix.len());

    let t = Instant::now();
    let df = concept::verse_frequency(&corpus);
    let co = concept::co_occurrence(&corpus);
    println!("co-occurrence:    {:?} ({} pairs)", t.elapsed(), co.len());

    let t = Instant::now();
    let edges = concept::ppmi(corpus.len(), &df, &co);
    println!("ppmi:             {:?} ({} edges)", t.elapsed(), edges.len());

    let t = Instant::now();
    let knn = concept::mutual_knn(10, &edges);
    println!("mutual knn:       {:?} ({} kept)", t.elapsed(), knn.len());

    let t = Instant::now();
    let comms = concept::communities(30, &knn);
    println!("communities:      {:?} ({} groups)", t.elapsed(), comms.len());

    // The warm phases that are still ONE call each (2026-07-27): whichever is
    // worst is the next slice to cut.
    let t = Instant::now();
    let lw = burst::discover_leitworter(&burst::BurstParams::default(), &corpus);
    println!("leitwort:         {:?} ({} found)", t.elapsed(), lw.len());
}

/// The web boot order (TODO #28): open on the core pack only, warm, then the
/// R&D artifacts arrive late — `load_rnd_data` + a re-warm must light up the
/// embedding/morphology tiers, and the early warm must NOT have pinned the
/// SIF model empty.
/// The overlay rides in with stage 2 (beside Strong's), and is refused when it
/// was aligned to a different tokenization — an overlay over the wrong text
/// points every span at the wrong word, quietly, in scripture.
#[test]
fn akjv_overlay_loads_with_stage_two() {
    use std::ffi::CString;
    const OVERLAY: &str = concat!(
        r#"{"format":"overlay-akjv-v1","tokenization":"kjv1769-tok2","source":"AKJV"}"#,
        "\n",
        r#"{"b":"John","c":3,"v":16,"d":[[4,4,"this"]]}"#,
    );
    for (stamp, expect) in [("kjv1769-tok2", true), ("kjv1611-tok1", false)] {
        unsafe {
            let home = std::env::temp_dir().join(format!("plumbline-ffi-akjv-{}-{stamp}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(home.join("data")).unwrap();
            std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
            std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();
            std::fs::write(home.join("data").join("akjv.jsonl"), OVERLAY.replace("kjv1769-tok2", stamp)).unwrap();

            let home_c = CString::new(home.to_str().unwrap()).unwrap();
            let mut err: *mut c_char = ptr::null_mut();
            let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
            assert!(!e.is_null(), "engine opened");
            let eng = &*e;
            // Not there until stage 2 runs — the boot path is the text alone.
            assert!(eng.akjv().is_none(), "overlay is not on the boot path");
            eng.load_core_data();
            let got = eng.akjv();
            assert_eq!(got.is_some(), expect, "stamp {stamp}");
            if let Some(a) = got {
                let v = VRef::new("John", 3, 16);
                assert_eq!(a.span_at(&v, 4).map(|s| s.text.as_str()), Some("this"));
                assert_eq!(a.span_at(&v, 3), None);
            }
            plumbline_engine_free(e);
            let _ = std::fs::remove_dir_all(&home);
        }
    }
}

/// The overlay end to end: off by default, on when asked, and the layout it
/// produces still hit-tests back to the right corpus token.
#[test]
fn akjv_overlay_relays_the_chapter_and_keeps_hit_testing() {
    use std::ffi::CString;
    const OVERLAY: &str = concat!(
        r#"{"format":"overlay-akjv-v1","tokenization":"kjv1769-tok2","source":"AKJV"}"#,
        "\n",
        // "the world." -> "the earth." : token 5 of John 3:16 in the fixture.
        r#"{"b":"John","c":3,"v":16,"d":[[5,5,"earth"]]}"#,
    );
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-akjvlay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();
        std::fs::write(home.join("data").join("akjv.jsonl"), OVERLAY).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(!e.is_null());
        (*e).load_core_data();
        assert!(plumbline_engine_akjv_available(e), "the home carries one");

        let words = |e: *const PlumblineEngine| -> Vec<String> {
            let dl =
                plumbline_engine_layout_chapter(e, c"John".as_ptr(), 3, cfg(), Some(mono_measure), ptr::null_mut());
            assert!(!dl.is_null());
            let json = take(plumbline_layout_to_json(dl)).unwrap();
            plumbline_layout_free(dl);
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            v["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["kind"] == "word")
                .map(|i| i["text"].as_str().unwrap().trim().to_string())
                .collect()
        };

        // Off by default: the text is the KJV.
        let plain = words(e);
        assert!(plain.contains(&"world.".to_string()), "KJV by default: {plain:?}");

        plumbline_engine_set_akjv_overlay(e, true);
        let over = words(e);
        assert!(over.contains(&"earth.".to_string()), "overlaid: {over:?}");
        assert!(!over.contains(&"world.".to_string()));
        // Same number of painted words — a swap, not an insertion.
        assert_eq!(plain.len(), over.len());

        // The tap answer: what it says now, and what it replaced.
        let j = take(plumbline_engine_akjv_token_json(e, c"John 3:16".as_ptr(), 5)).unwrap();
        let t: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(t["akjv"], "earth");
        assert_eq!(t["kjv"], "world.");
        // A word the AKJV left alone has no answer at all.
        assert!(plumbline_engine_akjv_token_json(e, c"John 3:16".as_ptr(), 1).is_null());

        plumbline_engine_set_akjv_overlay(e, false);
        assert!(words(e).contains(&"world.".to_string()), "toggles back off");

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn rnd_data_loads_after_open() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-laternd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let c = |s: &str| CString::new(s).unwrap();

        // Core-pack boot: warm builds concept + leitwort; the R&D tiers are dark.
        assert!(plumbline_engine_warm_indexes(e).is_null());
        assert!(plumbline_engine_morph_json(e, c("John 3:16").as_ptr(), 3).is_null());

        // A no-op load while the files are still missing is harmless.
        assert!(plumbline_engine_load_rnd_data(e).is_null());

        // The R&D pack arrives (same artifacts as rnd_tier_via_abi).
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
        std::fs::write(
            home.join("data").join("morphology.jsonl"),
            "{\"format\":\"overlay-morphology-v1\",\"tokenization\":\"kjv1769-tok2\",\"source\":\"test\"}\n\
             {\"b\":\"John\",\"c\":3,\"v\":16,\"e\":[[3,\"G25\",null,\"V-AAI-3S\"]]}\n",
        )
        .unwrap();

        assert!(plumbline_engine_load_rnd_data(e).is_null());
        assert!(plumbline_engine_warm_indexes(e).is_null());

        let m: Value =
            serde_json::from_str(&take(plumbline_engine_morph_json(e, c("John 3:16").as_ptr(), 3)).unwrap()).unwrap();
        assert_eq!(m["code"], "V-AAI-3S");
        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// A corpus of `chapters * per` verses over Psalms, every verse carrying codes
/// the test embedding covers. Deliberately bigger than one warm slice — see
/// `sif_model_is_built_in_slices` for why that is the whole point.
fn generated_kjv(chapters: u16, per: u16) -> String {
    const CODES: [&str; 4] = ["G2316", "G25", "G4100", "H7225"];
    let mut out =
        format!(r#"{{"format":"x","tokenization":"kjv1769-tok2","verses":{}}}"#, chapters as usize * per as usize);
    for c in 1..=chapters {
        for v in 1..=per {
            let code = CODES[(c as usize + v as usize) % CODES.len()];
            out.push('\n');
            out.push_str(&format!(
                r#"{{"b":"Ps","c":{c},"v":{v},"t":[["","the","",[],0],["","word","",["{code}"],0]]}}"#
            ));
        }
    }
    out
}

/// A reader's tap must never BUILD an index while a sliced warm is running.
///
/// This is the bug the whole 2026-07-28 investigation was chasing, and it hid for
/// three days because `call` — the op every engine request arrives on in the web
/// shell — was not timed. Once it was, the phone named it immediately:
///
///     SLOWEST ENGINE CALLS
///        21966 ms  wordStudyBlocks
///        11352 ms  wordStudyBlocks
///     worst single stall   21984 ms
///
/// Tap a word before the chunked warm reaches them and `wordStudyBlocks` built
/// the occurrence index, the rendering lens, the cross-references, the concept
/// model and the bridge in ONE synchronous lump — 22 seconds during which the
/// only thread that can answer a tap answered nothing, including its own
/// downloads (a 2.6 MB file the network delivered in 1,673 ms was collected
/// 23,825 ms later). Slicing the warm is pointless if a tap can undo it.
///
/// Both halves matter, so both are asserted: the tap builds NOTHING, and the tap
/// still answers. An engine that returned an error, or empty blocks, would
/// satisfy the first half and be useless.
#[test]
fn a_tap_never_builds_indexes_under_a_sliced_warm() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-tapbuild-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        // Bigger than one warm slice, so the warm is genuinely mid-flight when
        // the tap lands — which is the situation being tested.
        std::fs::write(home.join("data").join("kjv.jsonl"), generated_kjv(150, 20)).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let eng = &*e;
        eng.load_core_data();

        // ONE slice: the shell declaring that it drives a chunked warm.
        assert_eq!(eng.warm_next(crate::WARM_SLICE), 1, "the warm is mid-flight");
        assert!(eng.occ_ix.get().is_none(), "precondition: nothing built yet");

        // The reader taps a word.
        let blocks = take(plumbline_engine_word_study_blocks2_json(
            e,
            c"Ps 1:2".as_ptr(),
            1,
            3, // human + machine: every tier on, the most a tap can ask for
        ))
        .expect("the tap answered");

        for (built, what) in [
            (eng.occ_ix.get().is_some(), "the occurrence index"),
            (eng.renderings.get().is_some(), "the rendering lens"),
            (eng.xref_ix.get().is_some(), "the cross-reference index"),
            (eng.concept.get().is_some(), "the concept model"),
            (eng.leitwort.get().is_some(), "the leitwort scan"),
            (eng.bridge.get().is_some(), "the bridge"),
        ] {
            assert!(
                !built,
                "a tap BUILT {what} while a sliced warm was running — that is the 22-second \
                 freeze, and it strands every download in flight with it"
            );
        }

        // ...and it still said something. A tap that answers nothing is not a fix.
        let v: Value = serde_json::from_str(&blocks).unwrap();
        assert!(!v["blocks"].as_array().unwrap().is_empty(), "the tap built nothing AND answered nothing: {blocks}");

        // Once the warm finishes, the same tap is fully furnished — the sections
        // that were skipped are not skipped forever.
        let mut n = 0;
        while eng.warm_next(crate::WARM_SLICE) == 1 {
            n += 1;
            assert!(n < 10_000, "warm never terminated");
        }
        assert!(eng.occ_ix.get().is_some(), "the warm built the occurrence index");
        assert!(eng.renderings.get().is_some(), "the warm built the rendering lens");
        let after: Value =
            serde_json::from_str(&take(plumbline_engine_word_study_blocks2_json(e, c"Ps 1:2".as_ptr(), 1, 3)).unwrap())
                .unwrap();
        assert!(
            after["blocks"].as_array().unwrap().len() > v["blocks"].as_array().unwrap().len(),
            "the warm added nothing to the study — the deferred sections never filled in"
        );

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// The window the first attempt at this fix missed entirely.
///
/// v0.29.0 armed the no-building rule on the first `warm_next` call, which reads
/// as equivalent and is not. The web's warm starts only after stage 2 has been
/// fetched AND parsed — ~550 ms after text appears on a phone — and a reader taps
/// a word inside that gap. So the flag was still off, the tap built all five
/// indexes, and the phone froze for 26,042 ms on the very build that shipped the
/// fix. A desktop could not reproduce it: stage 2 there takes 40 ms and the warm
/// happens to win the race.
///
/// The rule is therefore declared AT OPEN, and this pins the case the previous
/// version passed while failing: no warm has run at all, not one slice.
#[test]
fn a_tap_before_the_warm_has_even_started_builds_nothing() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-tapearly-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), generated_kjv(150, 20)).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let eng = &*e;
        // What the web shell does the instant the engine opens — and NOTHING
        // else. No stage 2, no warm slice, no layout.
        eng.set_defer_builds(true);
        eng.load_core_data();

        let blocks =
            take(plumbline_engine_word_study_blocks2_json(e, c"Ps 1:2".as_ptr(), 1, 3)).expect("the tap answered");

        for (built, what) in [
            (eng.occ_ix.get().is_some(), "the occurrence index"),
            (eng.renderings.get().is_some(), "the rendering lens"),
            (eng.xref_ix.get().is_some(), "the cross-reference index"),
            (eng.concept.get().is_some(), "the concept model"),
            (eng.leitwort.get().is_some(), "the leitwort scan"),
            (eng.bridge.get().is_some(), "the bridge"),
        ] {
            assert!(
                !built,
                "a tap in the gap BEFORE the warm starts built {what} — this is the exact window \
                 that froze a phone for 26 seconds on the release that claimed to fix it"
            );
        }
        let v: Value = serde_json::from_str(&blocks).unwrap();
        assert!(!v["blocks"].as_array().unwrap().is_empty(), "and it still answered");

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// NO reader-facing export may build a lazy index. Not one.
///
/// Gating only the word-study path was whack-a-mole and it lost the very next
/// round: with `wordStudyBlocks` fixed, the phone's slowest call became
///
///     10205 ms  conceptMap
///
/// — a different door into the same room, because `plumbline_engine_concept_map_json`
/// reached straight past the panel's "ready" accessors and called `e.concept()`.
/// Every export below can be triggered by an ordinary reader before the warm has
/// finished, so every one of them is walked here and the whole engine is checked
/// afterwards. A new export that forgets the rule fails this test rather than the
/// maintainer's phone.
///
/// `search` is deliberately NOT in this list: it is an explicit query where an
/// empty answer would be a wrong answer rather than a partial one, and the warm
/// builds its index first for exactly that reason.
#[test]
fn no_reader_facing_export_builds_an_index_under_a_sliced_warm() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-noexport-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), generated_kjv(150, 20)).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let eng = &*e;
        eng.set_defer_builds(true);
        eng.load_core_data();

        let vref = c"Ps 1:2";
        let code = c"G2316";
        // Every one of these is reachable from a tap, a panel, or a map the reader
        // can open in the first seconds of a launch.
        let _ = take(plumbline_engine_word_study_blocks2_json(e, vref.as_ptr(), 1, 3));
        let _ = take(plumbline_engine_concept_json(e, code.as_ptr()));
        let _ = take(plumbline_engine_strongs_occurrences_json(e, code.as_ptr()));
        let _ = take(plumbline_engine_study_xrefs_json(e, vref.as_ptr()));

        for (built, what) in [
            (eng.occ_ix.get().is_some(), "the occurrence index"),
            (eng.renderings.get().is_some(), "the rendering lens"),
            (eng.xref_ix.get().is_some(), "the cross-reference index"),
            (eng.concept.get().is_some(), "the concept model"),
            (eng.leitwort.get().is_some(), "the leitwort scan"),
            (eng.bridge.get().is_some(), "the bridge"),
        ] {
            assert!(
                !built,
                "a reader-facing export built {what} while a sliced warm was running — that is a \
                 multi-second freeze of the only thread that answers taps, and it strands every \
                 download in flight behind it"
            );
        }

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// The control for the test above: WITHOUT a sliced warm — the Android path,
/// which calls `plumbline_engine_warm_indexes` and builds everything up front —
/// a tap still builds on demand exactly as it always has. The deferral is scoped
/// to shells that promised to slice, and this pins that scope.
#[test]
fn a_tap_still_builds_on_demand_when_no_sliced_warm_is_running() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-tapeager-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), generated_kjv(150, 20)).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let eng = &*e;
        eng.load_core_data();
        assert!(eng.occ_ix.get().is_none());

        // No warm_next call: nothing has promised to slice anything.
        let _ = take(plumbline_engine_word_study_blocks2_json(e, c"Ps 1:2".as_ptr(), 1, 3));
        assert!(eng.occ_ix.get().is_some(), "a tap on a shell that does NOT slice must still build what it needs");

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// The web's stage-1 boot (TODO #28): open on the corpus ALONE — text first —
/// then strongs.json arrives and `load_core_data` lights the dictionary up.
#[test]
fn core_data_loads_after_open() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-stage1-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null(), "corpus-only open must succeed");
        let c = |s: &str| CString::new(s).unwrap();

        // The text reads; the dictionary is dark.
        assert!(!plumbline_engine_verse_json(e, c("John 3:16").as_ptr()).is_null());
        assert!(plumbline_engine_strongs_json(e, c("G2316").as_ptr()).is_null());

        // Stage 2 lands.
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();
        assert!(plumbline_engine_load_core_data(e).is_null());
        let st: Value =
            serde_json::from_str(&take(plumbline_engine_strongs_json(e, c("G2316").as_ptr())).unwrap()).unwrap();
        assert_eq!(st["code"], "G2316");

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// The reading map across the ABI: the anchor is stamped by the first call that
/// needs it, dwell accrues, a full pass lands, a by-hand date lands, and the
/// book roll-up follows its chapters.
#[test]
fn reading_map_round_trip_via_abi() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-reading-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let c = |s: &str| CString::new(s).unwrap();
        let now = c("2026-07-28T12:00:00Z");

        // Nothing read yet: every book unread, and a fresh anchor keeps it calm.
        let books: Value =
            serde_json::from_str(&take(plumbline_engine_reading_books_json(e, now.as_ptr())).unwrap()).unwrap();
        assert_eq!(books["books"].as_array().unwrap().len(), 66);
        assert_eq!(books["since"], "2026-07-28");
        assert_eq!(books["spec"]["staleDays"], 365);
        assert_eq!(books["spec"]["completeAt"], 0.9);
        let john = books["books"].as_array().unwrap().iter().find(|b| b["book"] == "John").unwrap();
        assert_eq!(john["standing"], "unread");
        // Unread glows from the FIRST launch: the map's job on day one is to show
        // a reader where to go, and one that starts dark shows nothing.
        assert_eq!(john["glow"], 1.0, "unread must invite immediately");

        // The anchor is written once and must not move on a later call.
        let later: Value = serde_json::from_str(
            &take(plumbline_engine_reading_books_json(e, c("2027-01-01T00:00:00Z").as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(later["since"], "2026-07-28", "the start date is stamped once");
        // And it does not fade or build with time — unread is unread, whenever
        // you look. (Only John is in this toy corpus, so it is the only book with
        // any words to weight.)
        let john = later["books"].as_array().unwrap().iter().find(|b| b["book"] == "John").unwrap();
        assert_eq!(john["glow"], 1.0, "unread does not ramp; it is lit throughout");

        // A dwell report that is all scroll and no time credits nothing.
        let rec: Value = serde_json::from_str(
            &take(plumbline_engine_reading_record_json(e, c("John").as_ptr(), 3, 18, 0.0, now.as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(rec["pct"], 0.0);
        assert_eq!(rec["completed"], false);

        // Time enough for the whole chapter, having scrolled it: a full pass.
        let rec: Value = serde_json::from_str(
            &take(plumbline_engine_reading_record_json(e, c("John").as_ptr(), 3, 18, 60.0, now.as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(rec["completed"], true);
        assert_eq!(rec["pct"], 1.0);
        assert_eq!(rec["lastRead"], "2026-07-28");

        let chs: Value = serde_json::from_str(
            &take(plumbline_engine_reading_chapters_json(e, c("John").as_ptr(), now.as_ptr())).unwrap(),
        )
        .unwrap();
        let ch3 = chs["chapters"].as_array().unwrap().iter().find(|ch| ch["chapter"] == 3).unwrap();
        assert_eq!(ch3["standing"], "read");
        assert_eq!(ch3["days"], 0);
        assert_eq!(ch3["glow"], 0.0, "just read, so quiet");

        // By hand, for a paper Bible — full credit on the date given.
        assert!(plumbline_engine_reading_forget(e, c("John").as_ptr(), 3).is_null());
        assert!(plumbline_engine_reading_mark_read(e, c("John").as_ptr(), 3, c("2025-01-01").as_ptr()).is_null());
        let chs: Value = serde_json::from_str(
            &take(plumbline_engine_reading_chapters_json(e, c("John").as_ptr(), now.as_ptr())).unwrap(),
        )
        .unwrap();
        let ch3 = chs["chapters"].as_array().unwrap().iter().find(|ch| ch["chapter"] == 3).unwrap();
        assert_eq!(ch3["standing"], "read");
        assert_eq!(ch3["lastRead"], "2025-01-01");
        assert_eq!(ch3["glow"], 1.0, "over a year ago is a full glow");

        // The book follows its chapters: this toy John has only chapter 3.
        let books: Value =
            serde_json::from_str(&take(plumbline_engine_reading_books_json(e, now.as_ptr())).unwrap()).unwrap();
        let john = books["books"].as_array().unwrap().iter().find(|b| b["book"] == "John").unwrap();
        assert_eq!(john["standing"], "read");
        assert_eq!(john["read"], 1);
        assert_eq!(john["days"], 573);

        // Unknown books are refused rather than invented.
        assert!(plumbline_engine_reading_chapters_json(e, c("Nope").as_ptr(), now.as_ptr()).is_null());
        assert!(!plumbline_engine_reading_mark_read(e, c("Nope").as_ptr(), 1, c("2026-01-01").as_ptr()).is_null());

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

// ── the wire's key sets (golden) ──────────────────────────────────────────────
//
// `#[serde(rename_all)]` on an **enum** renames its VARIANTS, not the fields
// inside them. `WireBlock` shipped `mark_glyph` / `mark_color` / `top_gap` while
// both shells asked for `markGlyph` / `markColor` / `topGap`; Android's decoder
// ignores unknown keys (as it must, for additive evolution), so tier marks and
// paragraph gaps simply never rendered there, and the web had matched the wrong
// names so nothing looked broken. A test naming one field would not have caught
// that — no key was missing, every key was misspelled.
//
// So the tests below pin the COMPLETE key set of every variant of every tagged
// union on the wire, and check each key against the contract itself (camelCase).
// Changing a payload on purpose is one edited list; a rename or a wrong-cased
// new field fails here with the whole set in the diff.

/// An object's JSON keys, sorted — what the golden lists compare against.
fn json_keys(v: &Value) -> Vec<&str> {
    let mut ks: Vec<&str> =
        v.as_object().unwrap_or_else(|| panic!("expected a JSON object, got {v}")).keys().map(String::as_str).collect();
    ks.sort_unstable();
    ks
}

/// The frozen contract in one assertion: every wire key is camelCase. This holds
/// even when the golden list beside it agrees with the bug, because a golden is
/// only as good as the person who typed it.
fn assert_camel_keys(keys: &[&str], what: &str) {
    for k in keys {
        assert!(
            !k.contains('_') && !k.starts_with(|c: char| c.is_uppercase()),
            "{what}: key `{k}` is not camelCase. The wire is camelCase; on an enum, \
             `rename_all` renames the variants — struct-variant fields need \
             `rename_all_fields = \"camelCase\"`."
        );
    }
}

/// A block's `kind` token, as a match that must stay exhaustive: a new
/// [`crate::wire::WireBlock`] variant will not compile until it is added here —
/// and the golden test below wants a sample of it too.
fn block_kind(b: &crate::wire::WireBlock) -> &'static str {
    use crate::wire::WireBlock as B;
    match b {
        B::Section { .. } => "section",
        B::Para { .. } => "para",
        B::Rule => "rule",
    }
}

/// The study panel's whole content model, key by key: every block variant and
/// both shapes of run.
#[test]
fn wire_block_keys_are_golden() {
    use plumbline_core::panel::{Block, Color, Run};

    let blocks = vec![
        Block::Section { title: "Scholarship".into(), mark: Some(("◆".into(), Color::TierHuman)) },
        Block::Section { title: "Plain".into(), mark: None },
        Block::Para {
            runs: vec![Run::new("plain", 16.0, Color::Ink), Run::new("God", 16.0, Color::Gold).link("occ:G2316")],
            indent: true,
            top_gap: true,
        },
        Block::Rule,
    ];
    let panel = crate::wire::blocks_to_wire(blocks);
    // One sample per variant (see `block_kind`).
    let kinds: Vec<&str> = panel.blocks.iter().map(block_kind).collect();
    assert_eq!(kinds, ["section", "section", "para", "rule"]);

    let v = serde_json::to_value(&panel).unwrap();
    assert_eq!(json_keys(&v), ["blocks"]);
    let b = v["blocks"].as_array().unwrap();

    // A marked and an unmarked section emit the SAME keys — `markGlyph` /
    // `markColor` are explicit nulls, which is what lets a strict decoder bind
    // them as always-present optionals.
    let golden: &[(&str, &[&str])] = &[
        ("section (marked)", &["kind", "markColor", "markGlyph", "title"]),
        ("section (plain)", &["kind", "markColor", "markGlyph", "title"]),
        ("para", &["indent", "kind", "runs", "topGap"]),
        ("rule", &["kind"]),
    ];
    assert_eq!(b.len(), golden.len());
    for (block, (what, keys)) in b.iter().zip(golden) {
        assert_eq!(json_keys(block), *keys, "{what}");
        assert_camel_keys(&json_keys(block), what);
    }

    // A run carries `uri` only when it is a link.
    let runs: &[(&str, &[&str])] = &[
        ("run (plain)", &["bold", "color", "italic", "size", "text"]),
        ("run (link)", &["bold", "color", "italic", "size", "text", "uri"]),
    ];
    for (run, (what, keys)) in b[2]["runs"].as_array().unwrap().iter().zip(runs) {
        assert_eq!(json_keys(run), *keys, "{what}");
        assert_camel_keys(&json_keys(run), what);
    }

    // And the three keys the bug hid actually carry their values.
    assert_eq!(b[0]["markGlyph"], "◆");
    assert_eq!(b[0]["markColor"], "tierHuman");
    assert!(b[1]["markGlyph"].is_null());
    assert_eq!(b[2]["topGap"], true);
}

/// A link's `verb` token — exhaustive on purpose, like [`block_kind`].
fn link_verb(l: &crate::wire::WirePanelLink) -> &'static str {
    use crate::wire::WirePanelLink as L;
    match l {
        L::Go { .. } => "go",
        L::Occurrences { .. } => "occurrences",
        L::Rendering { .. } => "rendering",
        L::CodeStudy { .. } => "codeStudy",
        L::Thread { .. } => "thread",
        L::Tag { .. } => "tag",
        L::Weave { .. } => "weave",
        L::AddTag { .. } => "addTag",
        L::AddThread { .. } => "addThread",
        L::Untag { .. } => "untag",
        L::MakeWeave { .. } => "makeWeave",
        L::Approve { .. } => "approve",
        L::Reject { .. } => "reject",
        L::EditThreadNotes { .. } => "editThreadNotes",
        L::EditWeaveNotes { .. } => "editWeaveNotes",
        L::EditEntryNote { .. } => "editEntryNote",
        L::EditNote { .. } => "editNote",
        L::Guide => "guide",
        L::About => "about",
    }
}

/// Every panel-link verb, keys and all. `refKey` is the field most exposed to
/// this class of bug — three verbs carry it, and serde spells it `ref_key` unless
/// told otherwise.
#[test]
fn wire_panel_link_keys_are_golden() {
    // (the URI the panel bakes, the verb on the wire, the complete key set)
    let golden: &[(&str, &str, &[&str])] = &[
        ("go:1 John:3:16", "go", &["book", "chapter", "verb", "verse"]),
        ("occ:G25", "occurrences", &["code", "verb"]),
        ("rend:G25:loved", "rendering", &["code", "rendering", "verb"]),
        ("code:G25:loved", "codeStudy", &["code", "verb", "word"]),
        ("thread:0", "thread", &["index", "verb"]),
        ("tag:0", "tag", &["index", "verb"]),
        ("weave:0", "weave", &["index", "verb"]),
        ("addtag:John 3:16", "addTag", &["refKey", "verb"]),
        ("addthread:John 3:16", "addThread", &["refKey", "verb"]),
        ("untag:2:John 3:16", "untag", &["refKey", "tag", "verb"]),
        ("makeweave:1", "makeWeave", &["tag", "verb"]),
        ("approve:0", "approve", &["index", "verb"]),
        ("reject:0", "reject", &["index", "verb"]),
        ("editthreadnotes:0", "editThreadNotes", &["index", "verb"]),
        ("editweavenotes:0", "editWeaveNotes", &["index", "verb"]),
        ("editentrynote:1:4", "editEntryNote", &["entry", "thread", "verb"]),
        ("editnote:John 3:16", "editNote", &["refKey", "verb"]),
        ("guide", "guide", &["verb"]),
        ("about", "about", &["verb"]),
    ];
    let mut verbs = std::collections::BTreeSet::new();
    for (uri, verb, keys) in golden {
        let parsed = plumbline_core::panel::parse_link(uri).unwrap_or_else(|| panic!("{uri} should route"));
        let wire = crate::wire::link_to_wire(parsed);
        assert_eq!(link_verb(&wire), *verb, "{uri}");
        let v = serde_json::to_value(&wire).unwrap();
        assert_eq!(v["verb"], *verb, "{uri}");
        assert_eq!(json_keys(&v), *keys, "{uri}");
        assert_camel_keys(&json_keys(&v), uri);
        verbs.insert(*verb);
    }
    // One row per variant, so `link_verb`'s exhaustive match is a real tripwire.
    assert_eq!(verbs.len(), golden.len(), "each verb wants exactly one golden row");
}

/// A search answer's `kind` token — exhaustive on purpose, like [`block_kind`].
fn search_kind(a: &crate::wire::WireSearch) -> &'static str {
    use crate::wire::WireSearch as S;
    match a {
        S::Goto { .. } => "goto",
        S::Hits { .. } => "hits",
    }
}

/// The search answer's keys: the third tagged union a shell decodes strictly.
#[test]
fn wire_search_keys_are_golden() {
    use plumbline_core::search::{SearchAnswer, SearchHit};

    let goto =
        crate::wire::search_to_wire(&SearchAnswer::GoTo { book: "John".to_string(), chapter: 3, verse: Some(16) });
    assert_eq!(search_kind(&goto), "goto");
    let v = serde_json::to_value(&goto).unwrap();
    assert_eq!(json_keys(&v), ["book", "chapter", "display", "kind", "verse"]);
    assert_camel_keys(&json_keys(&v), "goto");

    let hits = crate::wire::search_to_wire(&SearchAnswer::Hits {
        how: "the word “God”".to_string(),
        total: 2,
        hits: vec![SearchHit {
            vref: plumbline_core::VRef::new("John".to_string(), 3, 16),
            note: false,
            why: String::new(),
        }],
    });
    assert_eq!(search_kind(&hits), "hits");
    let v = serde_json::to_value(&hits).unwrap();
    assert_eq!(json_keys(&v), ["capped", "hits", "how", "kind", "total"]);
    assert_camel_keys(&json_keys(&v), "hits");
    assert_eq!(json_keys(&v["hits"][0]), ["display", "note", "verse", "why"]);
    assert_camel_keys(&json_keys(&v["hits"][0]), "hit");
    // `total` above the returned hits is the honest count, and says so.
    assert_eq!(v["capped"], true);
}

// ── the token-flag mirror: core → this crate → the header → both shells ──────
//
// A shell paints an italic, a divine-name ink or the AKJV's dotted underline by
// bit-testing `flags` off a display-list item, so each of those bits is one
// contract with four copies: the core's `FLAG_*`, this crate's exported
// `PLUMBLINE_FLAG_*`, the `#define` cbindgen folds into include/plumbline.h, and
// the shell's own named constant. lib.rs carries the mechanism that keeps copies
// 1 and 2 honest (`const _: () = assert!(…)`) and cbindgen keeps 3 in step.
//
// `FLAG_RERENDERED` went around all of it. It arrived with the AKJV overlay as
// `core::akjv::FLAG_RERENDERED = 16` and was then written straight into both
// shells as a bare `16` — never exported here, never asserted, never in the
// header. Three unrelated copies of one number: the single place the value was
// written down was not a place either shell read.
//
// These two are SOURCE assertions and the choice is forced (the precedent is
// `f55a668`, which took the same route for the same reason). The copies agree
// today, so every behavioural test — paint an AKJV word, read the bit back —
// passes while the mechanism is entirely absent. That is the failure the working
// rules record twice. What was broken is not the number but that nothing checked
// the number, so the check is on the wiring.

/// Read a repo file for the guards below (this crate sits two levels down).
fn repo_file(rel: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

/// Every source file under `rel` with one of `exts`, as `(repo-relative path,
/// contents)`, sorted so a failure names files in a stable order.
///
/// The bare-literal guard walks a whole shell TREE rather than the one file that
/// bit-tests flags today: the recurrence it exists to stop is a *new* paint site
/// written with a `16` in it, and naming files would leave every new file
/// unguarded by default.
fn repo_tree(rel: &str, exts: &[&str]) -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel);
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot read dir {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.contains(&e)) {
                let text =
                    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                let shown = path
                    .strip_prefix(&root)
                    .map(|p| format!("{rel}/{}", p.display()))
                    .unwrap_or_else(|_| path.display().to_string());
                out.push((shown, text));
            }
        }
    }
    out.sort();
    out
}

/// `NAME → value` for every line beginning with `prefix` and assigning a decimal
/// with `=`: Rust `pub const PLUMBLINE_FLAG_ADDED: u32 = 1;`, Kotlin `const val
/// ADDED = 1`, TS `export const FLAG_ADDED = 1;`. The value is read after the
/// LAST `=` so Rust's `: u32` cannot be mistaken for it.
fn assigned_flags(src: &str, prefix: &str) -> std::collections::BTreeMap<String, u32> {
    let mut out = std::collections::BTreeMap::new();
    for line in src.lines() {
        let Some(rest) = line.trim_start().strip_prefix(prefix) else { continue };
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        let value = line.rsplit_once('=').and_then(|(_, rhs)| {
            rhs.trim_start().chars().take_while(char::is_ascii_digit).collect::<String>().parse::<u32>().ok()
        });
        if let (false, Some(v)) = (name.is_empty(), value) {
            out.insert(name, v);
        }
    }
    out
}

/// `NAME → value` for the generated header's `#define PLUMBLINE_FLAG_<NAME> <n>`.
fn header_flags(src: &str) -> std::collections::BTreeMap<String, u32> {
    let mut out = std::collections::BTreeMap::new();
    for line in src.lines() {
        let Some(rest) = line.strip_prefix("#define PLUMBLINE_FLAG_") else { continue };
        if let Some((name, value)) = rest.split_once(' ') {
            if let Ok(v) = value.trim().parse::<u32>() {
                out.insert(name.to_string(), v);
            }
        }
    }
    out
}

/// The body of Kotlin's `object PlumblineFlags { … }` — the Android shell's
/// mirror is read from that block alone, not from every `const val` in the file.
fn kotlin_flags_object(src: &str) -> &str {
    let at = src.find("object PlumblineFlags {").expect(
        "StudyEngine.kt no longer declares `object PlumblineFlags` — that object is the \
         Android shell's mirror of the header's flag bits; if it moved, move this guard",
    );
    let body = &src[at..];
    let end = body.find("\n}").expect("unterminated `object PlumblineFlags`");
    &body[..end]
}

/// Every place a shell bit-tests `flags` against something that is NOT one of the
/// named mirrors `mirrored` accepts, as `line: operand`.
///
/// `flags & FLAG_RERENDERED` is the contract; `flags & 16` is the bug this file
/// exists to stop coming back — and so is `flags & MY_OWN_BIT`, because a
/// privately named 16 answers to nothing either. `op` is the shell language's
/// bitwise-and (`&` in TS, the `and` infix in Kotlin).
///
/// Deliberately narrow in SHAPE: it recognises `flags <op> [(] <operand>`, which
/// is the shape both shells write and the shape a re-hardcode takes — the
/// optional paren so `flags & (16 | 4)` is judged on its first term rather than
/// skipped. A reversed `16 & flags` would slip past. The looser rule — any number
/// beside a bitwise-and on a line mentioning flags — false-positives on ordinary
/// masking (`(n >> 16) & 255`), and a guard that cries wolf gets deleted.
fn unchecked_flag_tests(src: &str, op: &str, mirrored: &dyn Fn(&str) -> bool) -> Vec<String> {
    // A word operator (`and`) must be followed by space, or `flags android` and
    // friends would parse as a bit test.
    let word_op = op.ends_with(|c: char| c.is_ascii_alphanumeric());
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let code = line.trim();
        if code.starts_with("//") || code.starts_with('*') || code.starts_with("/*") {
            continue;
        }
        let mut rest = code;
        while let Some(at) = rest.find("flags") {
            rest = &rest[at + "flags".len()..];
            let Some(after) = rest.trim_start().strip_prefix(op) else { continue };
            if word_op && !after.starts_with(char::is_whitespace) {
                continue;
            }
            let operand: String = after
                .trim_start()
                .trim_start_matches('(')
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
                .collect();
            // Empty operand: `it.flags && x`, `flags and -x` — not a bit test
            // against a name this guard can judge.
            if operand.is_empty() || mirrored(&operand) {
                continue;
            }
            out.push(format!("{}: `flags {op} {operand}`", i + 1));
        }
    }
    out
}

/// Half one: every flag bit is exported to the C header, and every export is
/// pinned to the core by the compile-time assert. Adding a `PLUMBLINE_FLAG_*`
/// without the assert, or without regenerating the header, fails here.
#[test]
fn flag_bits_are_exported_with_their_assertion() {
    let lib = repo_file("crates/ffi/src/lib.rs");
    let exported = assigned_flags(&lib, "pub const PLUMBLINE_FLAG_");

    // The parser is checked against the compiled constants, so a silent parse
    // miss cannot make the rest of this test vacuous. A new bit lands here first.
    let live = [
        ("ADDED", PLUMBLINE_FLAG_ADDED, corpus::FLAG_ADDED),
        ("DIVINE", PLUMBLINE_FLAG_DIVINE, corpus::FLAG_DIVINE),
        ("TITLE", PLUMBLINE_FLAG_TITLE, corpus::FLAG_TITLE),
        ("PARA", PLUMBLINE_FLAG_PARA, corpus::FLAG_PARA),
        ("RERENDERED", PLUMBLINE_FLAG_RERENDERED, akjv::FLAG_RERENDERED),
    ];
    assert_eq!(
        exported.len(),
        live.len(),
        "lib.rs exports {} PLUMBLINE_FLAG_* constants but this guard knows {} — extend the \
         list (and the header, and both shells' mirrors): {exported:?}",
        exported.len(),
        live.len()
    );
    for (name, abi, core) in live {
        assert_eq!(
            abi, core,
            "PLUMBLINE_FLAG_{name} and the core's own constant disagree — the exported bit is \
             what both shells paint by"
        );
        assert_eq!(
            exported.get(name),
            Some(&abi),
            "lib.rs no longer exports PLUMBLINE_FLAG_{name} = {abi} as a plain literal, so \
             cbindgen cannot fold it into a #define and the shells have nothing to mirror"
        );
        assert!(
            lib.contains(&format!("assert!(PLUMBLINE_FLAG_{name} ==")),
            "PLUMBLINE_FLAG_{name} is exported with no `const _: () = assert!(PLUMBLINE_FLAG_\
             {name} == …)` beside it: it can drift from the core silently, which is exactly how \
             FLAG_RERENDERED came to be a bare 16 in two shells"
        );
    }

    let header = header_flags(&repo_file("crates/ffi/include/plumbline.h"));
    assert_eq!(
        header, exported,
        "include/plumbline.h's PLUMBLINE_FLAG_* #defines are not lib.rs's exports — regenerate: \
         cargo run -p plumbline-ffi --features bindgen --bin plumbline-bindgen"
    );
}

/// Half two: each shell's flag constants mirror the header exactly, and no paint
/// site tests a bare number. A shell that re-hardcodes 16 fails here.
#[test]
fn flag_bits_are_mirrored_by_both_shells() {
    let header = header_flags(&repo_file("crates/ffi/include/plumbline.h"));
    assert!(!header.is_empty(), "the generated header exports no flag bits at all");

    let paint = repo_file("apps/web/src/reader/paint.ts");
    let study_engine = repo_file("apps/android/app/src/main/java/dev/plumbline/StudyEngine.kt");

    let web = assigned_flags(&paint, "export const FLAG_");
    let android = assigned_flags(kotlin_flags_object(&study_engine), "const val ");

    for (shell, path, mirror) in [
        ("web", "apps/web/src/reader/paint.ts", &web),
        ("Android", "apps/android/app/src/main/java/dev/plumbline/StudyEngine.kt", &android),
    ] {
        assert!(
            !mirror.is_empty(),
            "no flag-bit constants found in {path} — the {shell} shell's mirror moved, so this \
             guard is checking nothing; point it at the new home"
        );
        for (name, value) in mirror {
            assert_eq!(
                header.get(name),
                Some(value),
                "the {shell} shell declares flag bit {name} = {value}, and the C header exports \
                 {:?} under that name. A shell's number must mirror an exported #define: add \
                 `pub const PLUMBLINE_FLAG_{name}` to crates/ffi/src/lib.rs with its \
                 `const _: () = assert!(… == core)` and regenerate the header, or the value in \
                 {path} answers to nothing",
                header.get(name)
            );
        }
    }

    // The other half, over the whole of each shell's SOURCE TREE: every place a
    // shell bit-tests `flags`, the operand is one of the mirror constants just
    // checked against the header. Two things follow, and together they are the
    // mechanism this item was about. A bare `flags & 16` fails — that is the
    // literal coming back. So does `flags & SOME_LOCAL_BIT`, because a privately
    // named 16 answers to nothing either; the only operands that pass are names
    // the loop above pinned to an exported #define.
    //
    // Tree-wide rather than file-named: the recurrence is a NEW paint site, and
    // it will not be in the one file that tests flags today.
    for (shell, root, exts, op, prefix, mirror, anchor) in [
        ("web", "apps/web/src", &["ts", "svelte"][..], "&", "FLAG_", &web, "reader/paint.ts"),
        (
            "Android",
            "apps/android/app/src/main/java",
            &["kt"][..],
            "and",
            "PlumblineFlags.",
            &android,
            "ui/ReaderPane.kt",
        ),
    ] {
        let files = repo_tree(root, exts);
        assert!(
            files.len() > 5,
            "only {} source files under {root} — the {shell} shell's tree moved, so this guard is \
             walking almost nothing",
            files.len()
        );
        // The site that paints the AKJV mark must be inside the walk, or the
        // guard could pass by looking in the wrong place.
        assert!(
            files.iter().any(|(p, _)| p.ends_with(anchor)),
            "{anchor} is not under {root} any more — the {shell} shell's flag-testing paint site \
             moved out of the walked tree; point this guard at its new root"
        );
        let mirrored = |operand: &str| operand.strip_prefix(prefix).is_some_and(|n| mirror.contains_key(n));
        let unchecked: Vec<String> = files
            .iter()
            .flat_map(|(path, src)| {
                unchecked_flag_tests(src, op, &mirrored).into_iter().map(move |hit| format!("{path}:{hit}"))
            })
            .collect();
        assert!(
            unchecked.is_empty(),
            "the {shell} shell bit-tests `flags` against something that is not a mirror of an \
             exported #define — write `{prefix}<NAME>` for a bit that crates/ffi/src/lib.rs \
             exports and the header defines, or the value is hardcoded where nothing can check \
             it: {unchecked:?}"
        );
    }
}

#[test]
fn hymnal_round_trip_via_abi() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-hymnal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();
        std::fs::write(
            home.join("data").join("hymnal.json"),
            r##"{"format":"hymnal-v1","hymns":[
              {"id":"amazing-grace","number":14,"tune":"NEW BRITAIN","meter":"8.6.8.6","key":"G",
               "texts":{"en":{"title":"Amazing Grace","author":"John Newton","year":1779,
                 "stanzas":["A[G]mazing grace! how [C]sweet the [G]sound,\nThat [G]saved a [Em]wretch like [D]me!"],
                 "chorus":null}}},
              {"id":"ein-feste-burg","number":3,"tune":"EIN FESTE BURG","meter":"8.7.8.7.6.6.6.6.7","key":"C",
               "texts":{"de":{"title":"Ein feste Burg ist unser Gott","author":"Martin Luther",
                 "stanzas":["[C]Ein feste [G]Burg ist [C]unser Gott"],"chorus":"[F]Refrain [C]here"},
                "en":{"title":"A Mighty Fortress Is Our God","author":"Martin Luther",
                 "translator":"Frederick H. Hedge",
                 "stanzas":["[C]A mighty [G]fortress [C]is our God"],"chorus":null}}}
            ]}"##,
        )
        .unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null());
        assert!(!e.is_null());

        // The index: number order (3 before 14), per-language titles and
        // chord-stripped first lines. The key set is the golden wire shape —
        // a decoder that ignores unknown keys reads nothing when one renames.
        let ix: Value = serde_json::from_str(&take(plumbline_engine_hymnal_json(e)).unwrap()).unwrap();
        let hymns = ix["hymns"].as_array().unwrap();
        assert_eq!(hymns.len(), 2);
        assert_eq!(hymns[0]["id"], "ein-feste-burg");
        assert_eq!(hymns[1]["number"], 14);
        assert_eq!(
            hymns[0].as_object().unwrap().keys().collect::<Vec<_>>(),
            ["firstLines", "id", "meter", "number", "titles", "tune"]
        );
        assert_eq!(hymns[0]["titles"]["de"], "Ein feste Burg ist unser Gott");
        assert_eq!(hymns[0]["titles"]["en"], "A Mighty Fortress Is Our God");
        assert_eq!(hymns[1]["firstLines"]["en"], "Amazing grace! how sweet the sound,");

        // One hymn, untransposed: chords split into parts as authored.
        let g: Value =
            serde_json::from_str(&take(plumbline_engine_hymn_json(e, c"amazing-grace".as_ptr(), 0)).unwrap()).unwrap();
        assert_eq!(
            g.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["id", "key", "meter", "number", "texts", "transpose", "transposedKey", "tune"]
        );
        assert_eq!((g["key"].as_str(), g["transposedKey"].as_str()), (Some("G"), Some("G")));
        let line0 = &g["texts"]["en"]["stanzas"][0]["lines"][0]["parts"];
        assert_eq!(line0[0]["chord"], Value::Null);
        assert_eq!(line0[0]["text"], "A");
        assert_eq!(line0[1]["chord"], "G");
        assert_eq!(line0[1]["text"], "mazing grace! how ");

        // Transposed +3 from G: the target key is Bb, so chords spell FLAT.
        let up: Value =
            serde_json::from_str(&take(plumbline_engine_hymn_json(e, c"amazing-grace".as_ptr(), 3)).unwrap()).unwrap();
        assert_eq!((up["transpose"].as_i64(), up["transposedKey"].as_str()), (Some(3), Some("Bb")));
        let uline = &up["texts"]["en"]["stanzas"][0]["lines"][0]["parts"];
        assert_eq!(uline[1]["chord"], "Bb");
        assert_eq!(uline[2]["chord"], "Eb");
        let uline2 = &up["texts"]["en"]["stanzas"][0]["lines"][1]["parts"];
        assert_eq!(uline2[2]["chord"], "Gm");

        // Both languages ship on one hymn; the chorus carries its own chart.
        let burg: Value =
            serde_json::from_str(&take(plumbline_engine_hymn_json(e, c"ein-feste-burg".as_ptr(), 0)).unwrap()).unwrap();
        assert_eq!(burg["texts"]["en"]["translator"], "Frederick H. Hedge");
        assert_eq!(burg["texts"]["de"]["translator"], Value::Null);
        assert_eq!(burg["texts"]["de"]["chorus"]["lines"][0]["parts"][0]["chord"], "F");

        // Unknown id is null; a wild transpose folds into one octave.
        assert!(plumbline_engine_hymn_json(e, c"no-such-hymn".as_ptr(), 0).is_null());
        let far: Value =
            serde_json::from_str(&take(plumbline_engine_hymn_json(e, c"amazing-grace".as_ptr(), 15)).unwrap()).unwrap();
        assert_eq!(far["transposedKey"], "Bb", "15 semitones is 3");

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn hymnal_absent_is_empty_book() {
    unsafe {
        // Opened from bytes: no home, so no hymnal — the tab is just empty.
        let e = open();
        let ix: Value = serde_json::from_str(&take(plumbline_engine_hymnal_json(e)).unwrap()).unwrap();
        assert_eq!(ix["hymns"].as_array().unwrap().len(), 0);
        plumbline_engine_free(e);
    }
}

#[test]
fn hymnal_arriving_after_open_is_not_cached_empty() {
    use std::ffi::CString;
    unsafe {
        // The web's real sequence: the engine opens on stage 1, and
        // data/hymnal.json lands with the study stage moments later. A hymn
        // tab opened in the gap probes an empty book — and that probe must
        // not become the session's answer.
        let home = std::env::temp_dir().join(format!("plumbline-ffi-hymnal-late-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null());
        assert!(!e.is_null());

        // Probed before the file exists: empty, not an error.
        let before: Value = serde_json::from_str(&take(plumbline_engine_hymnal_json(e)).unwrap()).unwrap();
        assert_eq!(before["hymns"].as_array().unwrap().len(), 0);

        // The study stage arrives.
        std::fs::write(
            home.join("data").join("hymnal.json"),
            r#"{"format":"hymnal-v1","hymns":[{"id":"amazing-grace","number":14,"tune":"NEW BRITAIN","meter":"8.6.8.6","key":"G","texts":{"en":{"title":"Amazing Grace","author":"John Newton","stanzas":["A[G]mazing grace"],"chorus":null}}}]}"#,
        )
        .unwrap();

        // The shell's re-fetch (invalidate() on coreReady) asks again — and
        // now the book is here. This is the line the OnceLock used to break.
        let after: Value = serde_json::from_str(&take(plumbline_engine_hymnal_json(e)).unwrap()).unwrap();
        assert_eq!(after["hymns"].as_array().unwrap().len(), 1, "a late hymnal must fill on re-fetch");
        assert!(!plumbline_engine_hymn_json(e, c"amazing-grace".as_ptr(), 0).is_null());

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn the_catalogue_crosses_the_abi_whole_and_falls_back_to_english() {
    unsafe {
        // Engine-independent: the shells need their chrome before an engine
        // exists, and this is the call they make at startup.
        let en: Value =
            serde_json::from_str(&take(plumbline_i18n_catalog_json(c"en".as_ptr(), ptr::null())).unwrap()).unwrap();
        assert_eq!(en["lang"], "en");
        let strings = en["strings"].as_object().unwrap();
        assert!(strings.len() > 20, "the English catalogue looks empty: {} keys", strings.len());
        assert_eq!(strings["nav.read"], "Read");

        // A picker needs every language labelled in ITSELF.
        let langs = en["languages"].as_array().unwrap();
        assert!(langs.iter().any(|l| l["code"] == "en" && l["endonym"] == "English"));
        assert!(langs.iter().any(|l| l["code"] == "de" && l["endonym"] == "Deutsch"));

        // German resolves to German and answers EVERY English key, translated
        // or not — a shell must never meet a missing id.
        let de: Value =
            serde_json::from_str(&take(plumbline_i18n_catalog_json(c"de".as_ptr(), ptr::null())).unwrap()).unwrap();
        assert_eq!(de["lang"], "de");
        let de_strings = de["strings"].as_object().unwrap();
        for k in strings.keys() {
            assert!(de_strings.contains_key(k), "the German catalogue is missing {k}");
        }

        // A region tag is still that language; anything unknown is English
        // rather than an error, so an unsupported locale gets a working app.
        for (asked, want) in [("de-CH", "de"), ("de_AT", "de"), ("en-GB", "en"), ("fr", "en"), ("", "en")] {
            let cs = CString::new(asked).unwrap();
            let got: Value =
                serde_json::from_str(&take(plumbline_i18n_catalog_json(cs.as_ptr(), ptr::null())).unwrap()).unwrap();
            assert_eq!(got["lang"], want, "{asked:?} should resolve to {want}");
        }

        // The DEVICE locale is the second argument and only decides when the
        // reader has not: a German phone opens in German with nobody visiting
        // Settings, and a reader who picked English keeps it.
        let device_wins: Value =
            serde_json::from_str(&take(plumbline_i18n_catalog_json(ptr::null(), c"de-DE".as_ptr())).unwrap()).unwrap();
        assert_eq!(device_wins["lang"], "de");
        let choice_wins: Value =
            serde_json::from_str(&take(plumbline_i18n_catalog_json(c"en".as_ptr(), c"de-DE".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(choice_wins["lang"], "en");

        // Both null is not a crash and not null — it is English.
        let none: Value =
            serde_json::from_str(&take(plumbline_i18n_catalog_json(ptr::null(), ptr::null())).unwrap()).unwrap();
        assert_eq!(none["lang"], "en");
    }
}

/// The German corpus, opened for real from the repo's own `data/`.
///
/// Reads the shipped 15 MB file rather than a fixture, deliberately: the claim
/// worth testing is not that the loader works — `corpus.rs` covers that — but
/// that THE FILE WE SHIP sits at the KJV's verse addresses and comes back as
/// German. A fixture would prove neither.
///
/// `#[ignore]`d so the default `cargo test` stays fast and works in a checkout
/// with no data pack hydrated. Run it with:
///
/// ```sh
/// cargo test --locked -p plumbline-ffi -- --ignored german_corpus
/// ```
#[test]
#[ignore]
fn german_corpus_opens_at_the_kjv_addresses_and_reads_german() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !repo.join("data/luther1912.jsonl").exists() {
        eprintln!("no data/luther1912.jsonl in this checkout — skipping");
        return;
    }
    let home = CString::new(repo.to_str().unwrap()).unwrap();

    unsafe {
        // The language is what selects the text, and it is set before open —
        // exactly as both shells do it.
        let _ = take(plumbline_i18n_set_language(c"de".as_ptr(), ptr::null()));
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home.as_ptr(), &mut err);
        assert!(!e.is_null(), "German open failed: {:?}", opt_str(err));

        // The same address, in the other language. John 3:16 is the test the
        // whole design turns on: if the German corpus had its own versification
        // this refKey would land somewhere else.
        let v: Value =
            serde_json::from_str(&take(plumbline_engine_verse_json(e, c"John 3:16".as_ptr())).unwrap()).unwrap();
        let body = v["body"].as_str().unwrap();
        assert!(body.contains("Gott"), "John 3:16 is not German: {body}");
        assert!(!body.contains("God so loved"), "John 3:16 came back in English: {body}");

        // Book names come from the catalogue for the same reader.
        let toc: Value = serde_json::from_str(&take(plumbline_engine_toc_json(e)).unwrap()).unwrap();
        let books = toc["books"].as_array().unwrap();
        assert_eq!(books.len(), 66);
        assert_eq!(books.iter().find(|b| b["id"] == "Gen").unwrap()["name"], "1. Mose");
        // Every book present, with the KJV's chapter counts.
        assert_eq!(books.iter().find(|b| b["id"] == "Ps").unwrap()["chapters"], 150);
        assert_eq!(books.iter().find(|b| b["id"] == "Mal").unwrap()["chapters"], 4, "Malachi keeps the KJV's 4");
        assert_eq!(books.iter().find(|b| b["id"] == "Joel").unwrap()["chapters"], 3, "Joel keeps the KJV's 3");

        // The plain-English overlay is a delta over KJV TOKEN RUNS, so it must
        // not be offered here — it would rewrite whichever German words happened
        // to sit at those indices.
        //
        // AFTER `load_core_data`, which is what loads the overlay: without this
        // call the assertion below is vacuously true, because nothing had tried
        // to load one yet. (Found by mutation-testing this test — removing the
        // gate left it green.)
        let _ = take(plumbline_engine_load_core_data(e));
        assert!(!plumbline_engine_akjv_available(e), "the KJV overlay was offered over German text");
        // And Strong's is withheld for the same reason: its codes are attached to
        // KJV tokens, so a German word study would be looking up whatever code
        // sat at that index in the other text.
        let w: Value =
            serde_json::from_str(&take(plumbline_engine_token_json(e, c"John 3:16".as_ptr(), 0)).unwrap()).unwrap();
        assert!(w["strongs"].as_array().is_none_or(|a| a.is_empty()), "a German token carries Strong's codes: {w}");

        // THE STUDY CARD SAYS WHY IT IS EMPTY, in German, rather than showing
        // English evidence about the KJV's words (UAT, 2026-08-03). Everything in
        // it — Strong's, morphology, renderings, cross-references — is keyed to
        // KJV token indices, so on this text it would describe different words.
        let blocks = take(plumbline_engine_word_study_blocks2_json(e, c"John 3:16".as_ptr(), 1, 3)).unwrap();
        let de_notice = plumbline_core::i18n::t(plumbline_core::i18n::Lang::De, "study.onlyKjv", &[]);
        assert!(
            blocks.contains(de_notice.split(" — ").next().unwrap()),
            "the German study card does not say why it is empty: {blocks}"
        );
        // And none of the token-keyed English evidence leaked in beside it.
        for english in ["no Strong's tag", "Renderings", "same root"] {
            assert!(!blocks.contains(english), "English study prose {english:?} reached a German reader: {blocks}");
        }
        // Nor any of the PANEL'S OWN LABELS, which were English on a German
        // screen until UAT caught it — they are catalogue strings now.
        for label in ["＋ tag verse", "＋ add to thread", "your note", "cross-references ("] {
            assert!(!blocks.contains(label), "the study panel's English label {label:?} reached a German reader");
        }
        assert!(
            blocks.contains(&plumbline_core::i18n::t(plumbline_core::i18n::Lang::De, "panel.tagVerse", &[])),
            "the German study card has no German tag action: {blocks}"
        );

        // THE CROSS-REFERENCES STAY. They key on refKey, not on a token index, so
        // they are as true of this text as of the KJV — and they are a lot of real
        // study value. My first pass returned early and threw them away; only
        // reading this test's own failure output showed it.
        assert!(
            blocks.contains(&plumbline_core::i18n::t(
                plumbline_core::i18n::Lang::De,
                "panel.studyXrefs",
                &[("n", "23")]
            )),
            "the German study card lost its cross-references: {blocks}"
        );
        assert!(blocks.contains("Römer 5,8"), "a German cross-reference is not in German: {blocks}");

        plumbline_engine_free(e);
        // English again, so nothing after this test inherits German.
        let _ = take(plumbline_i18n_set_language(c"en".as_ptr(), ptr::null()));
    }
}
