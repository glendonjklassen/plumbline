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
    plumbline_string_free(p);
    Some(s)
}

unsafe fn open() -> *mut PlumblineEngine {
    let mut err: *mut c_char = ptr::null_mut();
    let e = plumbline_engine_open_from_bytes(
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
        let e = plumbline_engine_open_from_bytes(
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
        let e2 = plumbline_engine_open_from_bytes(
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
        let dl = plumbline_engine_layout_chapter(
            e,
            c"John".as_ptr(),
            3,
            cfg(),
            Some(mono_measure),
            ptr::null_mut(),
        );
        assert!(!dl.is_null());
        assert!(plumbline_layout_item_count(dl) > 0);
        assert!(plumbline_layout_height(dl) >= 20.0);

        // Parse the JSON to locate the word "God" (John 3:16, token index 1).
        let list: Value = serde_json::from_str(&take(plumbline_layout_to_json(dl)).unwrap()).unwrap();
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
            serde_json::from_str(&take(plumbline_layout_hit_test_json(dl, cx, cy)).unwrap()).unwrap();
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
        let ys: std::collections::BTreeSet<i64> =
            items.iter().map(|it| it["y"].as_f64().unwrap() as i64).collect();
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
        assert!(plumbline_engine_layout_chapter(e, c"John".as_ptr(), 1, cfg(), m, ptr::null_mut())
            .is_null());
        // An unknown book.
        assert!(plumbline_engine_layout_chapter(e, c"Nope".as_ptr(), 3, cfg(), m, ptr::null_mut())
            .is_null());
        // A chapter outside the u16 domain must NOT wrap into a real chapter
        // (65539 as u16 == 3, which does exist — regression guard).
        assert!(plumbline_engine_layout_chapter(e, c"John".as_ptr(), 65539, cfg(), m, ptr::null_mut())
            .is_null());
        plumbline_engine_free(e);
    }
}

#[test]
fn null_measure_yields_null_layout() {
    unsafe {
        let e = open();
        let dl =
            plumbline_engine_layout_chapter(e, c"John".as_ptr(), 3, cfg(), None, ptr::null_mut());
        assert!(dl.is_null(), "a null measure callback must fail cleanly");
        plumbline_engine_free(e);
    }
}

#[test]
fn strongs_entry_and_occurrences() {
    unsafe {
        let e = open();

        let entry: Value =
            serde_json::from_str(&take(plumbline_engine_strongs_json(e, c"G2316".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(entry["code"], "G2316");
        assert_eq!(entry["lemma"], "θεός");
        assert_eq!(entry["kjv"], "God");
        // Derived / pron absent-vs-present handled as null.
        assert_eq!(entry["deriv"], Value::Null);

        // Unknown code → null.
        assert!(plumbline_engine_strongs_json(e, c"H9999".as_ptr()).is_null());

        let occ: Value = serde_json::from_str(
            &take(plumbline_engine_strongs_occurrences_json(e, c"G2316".as_ptr())).unwrap(),
        )
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
            serde_json::from_str(&take(plumbline_engine_renderings_json(e, c"G25".as_ptr())).unwrap())
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
            &take(plumbline_engine_renderings_json(e, c"H9999".as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(empty["renderings"].as_array().unwrap().len(), 0);

        // Reverse lens: the surface word "God" (normalized) maps to G2316.
        let w: Value =
            serde_json::from_str(&take(plumbline_engine_word_codes_json(e, c"God".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(w["word"], "God");
        let codes = w["codes"].as_array().unwrap();
        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0]["code"], "G2316");
        assert_eq!(codes[0]["count"], 1);

        // A translator-supplied (added) word carries no codes.
        let the: Value =
            serde_json::from_str(&take(plumbline_engine_word_codes_json(e, c"the".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(the["codes"].as_array().unwrap().len(), 0);

        plumbline_engine_free(e);
    }
}

#[test]
fn verse_and_token_lookup() {
    unsafe {
        let e = open();
        let verse: Value =
            serde_json::from_str(&take(plumbline_engine_verse_json(e, c"John 3:16".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(verse["reference"], "John 3:16");
        assert!(verse["body"].as_str().unwrap().contains("God"));
        assert_eq!(verse["tokens"].as_array().unwrap().len(), 6);

        // Token index 4 ("the") carries the KJV-added flag in our sample.
        let tok: Value = serde_json::from_str(
            &take(plumbline_engine_token_json(e, c"John 3:16".as_ptr(), 4)).unwrap(),
        )
        .unwrap();
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
            serde_json::from_str(&take(plumbline_engine_search_json(e, c"loved".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(hits["kind"], "hits");
        assert!(hits["total"].as_u64().unwrap() >= 1);
        assert_eq!(hits["hits"][0]["verse"], "John 3:16");

        // Reference query → goto.
        let goto: Value =
            serde_json::from_str(&take(plumbline_engine_search_json(e, c"John 3".as_ptr())).unwrap())
                .unwrap();
        assert_eq!(goto["kind"], "goto");
        assert_eq!(goto["book"], "John");
        assert_eq!(goto["chapter"], 3);
        assert_eq!(goto["verse"], Value::Null);

        // Bare Strong's code → verses tagged with it.
        let tagged: Value =
            serde_json::from_str(&take(plumbline_engine_search_json(e, c"G2316".as_ptr())).unwrap())
                .unwrap();
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
        assert!(plumbline_engine_tag_add(e, c("Messianic").as_ptr(), c("verse").as_ptr(), c("John 3:16").as_ptr(), ptr::null(), stamp.as_ptr()).is_null());
        assert!(plumbline_engine_tag_add(e, c("Messianic").as_ptr(), c("concept").as_ptr(), c("G2316").as_ptr(), ptr::null(), stamp.as_ptr()).is_null());
        assert!(plumbline_engine_thread_add(e, c("Road").as_ptr(), c("John 3:16").as_ptr(), ptr::null(), stamp.as_ptr()).is_null());
        assert!(plumbline_engine_weave_add_link(e, c("Links").as_ptr(), c("John 3:16").as_ptr(), c("John 3:18").as_ptr(), stamp.as_ptr()).is_null());

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
            serde_json::from_str(&take(plumbline_engine_verse_xrefs_json(e, c("John 3:16").as_ptr())).unwrap()).unwrap();
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
        assert!(take(plumbline_engine_weave_set_notes(e, c("Nope").as_ptr(), c("x").as_ptr())).unwrap().contains("weave"));
        assert!(take(plumbline_engine_thread_entry_set_note(e, c("Road").as_ptr(), 9, ptr::null())).unwrap().contains("entry"));

        // Error paths: a bad target kind, and a bytes-opened engine has no home.
        assert!(take(plumbline_engine_tag_add(e, c("X").as_ptr(), c("bogus").as_ptr(), c("v").as_ptr(), ptr::null(), stamp.as_ptr()))
            .unwrap()
            .contains("kind"));
        let bytes_engine = open();
        assert!(take(plumbline_engine_tag_add(bytes_engine, c("X").as_ptr(), c("verse").as_ptr(), c("John 3:16").as_ptr(), ptr::null(), stamp.as_ptr()))
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
            assert!(plumbline_engine_tag_add(e, c("Belief").as_ptr(), c(kind).as_ptr(), c(value).as_ptr(), ptr::null(), stamp.as_ptr()).is_null());
        }
        assert!(plumbline_engine_weave_from_tag(e, c("belief").as_ptr(), ptr::null(), ptr::null(), stamp.as_ptr()).is_null());

        let weaves: Value = serde_json::from_str(&take(plumbline_engine_weaves_json(e)).unwrap()).unwrap();
        assert_eq!(weaves["weaves"][0]["name"], "Belief"); // null name → the tag's
        let links = weaves["weaves"][0]["links"].as_array().unwrap();
        assert_eq!(links.len(), 1); // canon-ordered chain of the two verses
        assert_eq!(links[0]["a"], "John 3:16");
        assert_eq!(links[0]["b"], "John 3:18");

        // A named subset with one ref is not a weave; an unknown tag errors.
        assert!(take(plumbline_engine_weave_from_tag(e, c("Belief").as_ptr(), c(r#"["John 3:16"]"#).as_ptr(), c("Solo").as_ptr(), stamp.as_ptr()))
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
        let listed: Value =
            serde_json::from_str(&take(plumbline_engine_suggested_weaves_json(e)).unwrap()).unwrap();
        let items = listed["suggested"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["index"], 0);
        assert_eq!(items[0]["links"][0]["aDisplay"], "John 3:16");

        // Approve index 0 → it leaves the suggested queue (one left).
        assert!(plumbline_engine_weave_approve(e, 0).is_null());
        let after: Value =
            serde_json::from_str(&take(plumbline_engine_suggested_weaves_json(e)).unwrap()).unwrap();
        assert_eq!(after["suggested"].as_array().unwrap().len(), 1);
        // The approved weave now asserts its cross-reference from weaves/.
        let xrefs: Value =
            serde_json::from_str(&take(plumbline_engine_verse_xrefs_json(e, c("John 3:16").as_ptr())).unwrap()).unwrap();
        assert!(xrefs["partners"].as_array().unwrap().iter().any(|p| p["verse"] == "John 3:18"));

        // Reject the remaining one (now index 0) → queue empties.
        assert!(plumbline_engine_weave_reject(e, 0).is_null());
        let empty: Value =
            serde_json::from_str(&take(plumbline_engine_suggested_weaves_json(e)).unwrap()).unwrap();
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
        std::fs::write(home.join("data").join("source-priors.json"), r#"{"priors":{"lxx":0.85,"_default":0.5}}"#).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let c = |s: &str| CString::new(s).unwrap();

        // Concept neighbours: same-testament near, Hebrew cross (aligned).
        let n: Value = serde_json::from_str(&take(plumbline_engine_concept_neighbours_json(e, c("G25").as_ptr(), 5)).unwrap()).unwrap();
        assert_eq!(n["code"], "G25");
        assert!(n["near"].as_array().unwrap().iter().all(|x| x["code"].as_str().unwrap().starts_with('G')));
        assert!(n["cross"].as_array().unwrap().iter().any(|x| x["code"] == "H7225"));

        // Fused bridge partner from the external witness.
        let b: Value = serde_json::from_str(&take(plumbline_engine_bridge_partners_json(e, c("G25").as_ptr())).unwrap()).unwrap();
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
        let m: Value = serde_json::from_str(&take(plumbline_engine_morph_json(e, c("John 3:16").as_ptr(), 3)).unwrap()).unwrap();
        assert_eq!(m["code"], "V-AAI-3S");
        assert_eq!(m["gloss"], "aorist active indicative, 3rd singular");
        // A token with no annotation → null.
        assert!(plumbline_engine_morph_json(e, c("John 3:16").as_ptr(), 1).is_null());

        // "Verses like this" (lazy SIF build) → the other Greek verse.
        let s: Value = serde_json::from_str(&take(plumbline_engine_similar_verses_json(e, c("John 3:16").as_ptr(), 5)).unwrap()).unwrap();
        assert_eq!(s["verse"], "John 3:16");
        assert!(s["in"].as_array().unwrap().iter().any(|x| x["verse"] == "John 3:18"));

        // A bytes-opened engine has no embedding/morph → those return null.
        let bytes_engine = open();
        assert!(plumbline_engine_concept_neighbours_json(bytes_engine, c("G25").as_ptr(), 5).is_null());
        assert!(plumbline_engine_morph_json(bytes_engine, c("John 3:16").as_ptr(), 3).is_null());
        plumbline_engine_free(bytes_engine);

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[test]
fn concept_map_bridge_row_lights_up_the_other_testament() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-bridgemap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::create_dir_all(home.join("bridge")).unwrap();
        // A Hebrew verse (H7225 in Genesis) and a Greek verse (G25 in John) — the
        // cross-testament shape that makes "Christ lights up Messiah" meaningful.
        let kjv = concat!(
            r#"{"format":"x","tokenization":"kjv1769-tok2","verses":2}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","In","",[],0],["","the","",[],1],["","beginning","",["H7225"],0]],"v":1}"#,
            "\n",
            r#"{"b":"John","c":3,"t":[["","God","",["G2316"],0],["","loved","",["G25"],0]],"v":16}"#,
        );
        std::fs::write(home.join("data").join("kjv.jsonl"), kjv).unwrap();
        std::fs::write(
            home.join("data").join("strongs.json"),
            r#"{"G25":{"lemma":"ἀγαπάω","strongs_def":"to love"},"H7225":{"lemma":"רֵאשִׁית","kjv_def":"beginning"},"G2316":{"lemma":"θεός","kjv_def":"God"}}"#,
        )
        .unwrap();
        // A bridge witness tying G25 ↔ H7225 (fixture stand-in for Christ↔Messiah).
        std::fs::write(
            home.join("bridge").join("x.json"),
            r#"{"format":"overlay-bridge-sources-v1","links":[{"h":"H7225","g":"G25","source":"lxx"}]}"#,
        )
        .unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let c = |s: &str| CString::new(s).unwrap();

        // G25's concept map carries a bridge row: its Hebrew partner H7225, whose
        // dispersion lights up Genesis (canon index 0) — even though G25 itself
        // occurs only in the NT (its own by_book is 0 at Genesis).
        let m: Value =
            serde_json::from_str(&take(plumbline_engine_concept_map_json(e, c("G25").as_ptr())).unwrap()).unwrap();
        assert_eq!(m["byBook"][0].as_u64().unwrap(), 0, "G25 itself is not in Genesis");
        let bridge = &m["bridge"];
        assert!(bridge.is_object(), "a bridge row exists when a partner does");
        assert!(bridge["partners"].as_array().unwrap().iter().any(|p| p["code"] == "H7225"));
        assert!(
            bridge["byBook"][0].as_u64().unwrap() >= 1,
            "the partner H7225 lights up Genesis in the bridge row"
        );
        assert_eq!(
            bridge["byBook"].as_array().unwrap().len(),
            m["bookCount"].as_u64().unwrap() as usize,
            "the bridge row is canon-length, like by_book"
        );

        // A code with no cross-testament partner omits the bridge row entirely
        // (serde skips the None), so shells draw a single dispersion band.
        let m2: Value =
            serde_json::from_str(&take(plumbline_engine_concept_map_json(e, c("G2316").as_ptr())).unwrap()).unwrap();
        assert!(m2["bridge"].is_null(), "no bridge row without a cross-testament partner");

        // Semantic spokes carry their cosine weight (shells scale distance by
        // it); community spokes omit it (serde skips the None).
        for m in [&m, &m2] {
            for sp in m["spokes"].as_array().unwrap() {
                if sp["semantic"].as_bool().unwrap() {
                    let w = sp["weight"].as_f64().expect("semantic spokes are weighted");
                    assert!((-1.0..=1.0).contains(&w), "a cosine, not a rank: {w}");
                } else {
                    assert!(sp["weight"].is_null(), "community spokes carry no weight");
                }
            }
        }

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
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null());
        let c = |s: &str| CString::new(s).unwrap();

        // Margin notes: present verse → notes; absent verse → null.
        let notes: Value = serde_json::from_str(
            &take(plumbline_engine_verse_notes_json(e, c("John 3:16").as_ptr())).unwrap(),
        )
        .unwrap();
        assert_eq!(notes["notes"][0], "Or, begotten");
        assert!(plumbline_engine_verse_notes_json(e, c("John 3:18").as_ptr()).is_null());

        // TSK: best-voted first, range end carried.
        let xr: Value = serde_json::from_str(
            &take(plumbline_engine_study_xrefs_json(e, c("John 3:16").as_ptr())).unwrap(),
        )
        .unwrap();
        let refs = xr["refs"].as_array().unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0]["votes"], 5);
        assert!(refs[0]["end"].is_null());
        assert_eq!(refs[1]["end"], "John 3:18");

        // Weave library: spans, approval, kind label, resolvability.
        let ws: Value =
            serde_json::from_str(&take(plumbline_engine_weaves_json(e)).unwrap()).unwrap();
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
        let ws: Value =
            serde_json::from_str(&take(plumbline_engine_weaves_json(e)).unwrap()).unwrap();
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
            serde_json::from_str(&take(plumbline_engine_link_pairs_json(e)).unwrap()).unwrap();
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
            serde_json::from_str(&take(plumbline_engine_canon_segments_json(e)).unwrap()).unwrap();
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
            serde_json::from_str(&take(plumbline_engine_chord_map_json(e)).unwrap()).unwrap();
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
            &take(plumbline_engine_concept_map_json(e, c("G2316").as_ptr())).unwrap(),
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
            &take(plumbline_engine_constellation_json(e, 0, ptr::null())).unwrap(),
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
            &take(plumbline_engine_constellation_json(e, 0, pins.as_ptr())).unwrap(),
        )
        .unwrap();
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
        let cj: Value = serde_json::from_str(
            &take(plumbline_engine_concept_json(e, c("G2316").as_ptr())).unwrap(),
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
        let loaded: Value =
            serde_json::from_str(&take(plumbline_config_load_json()).unwrap()).unwrap();
        assert_eq!(loaded["firstRun"], true);
        assert_eq!(loaded["studyMode"], "simple");

        // Save a full-study, two-pane session and read it back.
        let saved = r#"{"studyMode":"full","bodySize":21.0,"openPanes":[{"book":"Gen","chapter":15},{"book":"Rom","chapter":4}],"activePane":1}"#;
        let sc = CString::new(saved).unwrap();
        assert!(plumbline_config_save_json(sc.as_ptr()).is_null());
        let loaded: Value =
            serde_json::from_str(&take(plumbline_config_load_json()).unwrap()).unwrap();
        assert_eq!(loaded["firstRun"], false);
        assert_eq!(loaded["studyMode"], "full");
        assert_eq!(loaded["bodySize"], 21.0);
        assert_eq!(loaded["openPanes"][1]["book"], "Rom");
        assert_eq!(loaded["activePane"], 1);

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
            serde_json::from_str(&take(plumbline_engine_concordance_blocks_json(e, c("G2316").as_ptr())).unwrap()).unwrap();
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
/// tag colour → chapter highlights, the theme palette, guide/about blocks, and
/// index warming. Exercised through a temp home exactly as a shell would.
#[test]
fn tier0_endpoints_via_abi() {
    use std::ffi::CString;
    unsafe {
        // Engine-independent endpoints first (no home needed).
        let c = |s: &str| CString::new(s).unwrap();
        // A null / unknown theme falls back to light; "night" is true-black.
        let light: Value =
            serde_json::from_str(&take(plumbline_theme_palette_json(ptr::null())).unwrap()).unwrap();
        assert_eq!(light["paper"], "#fcf9f4");
        assert_eq!(light["dark"], false);
        let palette: Value =
            serde_json::from_str(&take(plumbline_theme_palette_json(c("night").as_ptr())).unwrap()).unwrap();
        assert_eq!(palette["paper"], "#000000");
        assert_eq!(palette["dark"], true);
        let tones: Value =
            serde_json::from_str(&take(plumbline_theme_highlight_tones_json()).unwrap()).unwrap();
        assert!(tones["tones"].as_array().unwrap().len() >= 5);
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
        assert!(plumbline_engine_user_note_set(e, c("John 3:16").as_ptr(), c("golden text").as_ptr(), stamp.as_ptr()).is_null());
        let note: Value = serde_json::from_str(&take(plumbline_engine_user_note_json(e, c("John 3:16").as_ptr())).unwrap()).unwrap();
        assert_eq!(note["text"], "golden text");
        let all: Value = serde_json::from_str(&take(plumbline_engine_user_notes_json(e)).unwrap()).unwrap();
        assert_eq!(all["notes"].as_array().unwrap().len(), 1);
        assert!(plumbline_engine_user_note_set(e, c("John 3:16").as_ptr(), c("").as_ptr(), stamp.as_ptr()).is_null());
        assert!(plumbline_engine_user_note_json(e, c("John 3:16").as_ptr()).is_null());

        // Highlight: tag a verse, colour the tag, then the chapter reports the wash.
        assert!(plumbline_engine_tag_add(e, c("amber").as_ptr(), c("verse").as_ptr(), c("John 3:16").as_ptr(), ptr::null(), stamp.as_ptr()).is_null());
        assert!(plumbline_engine_tag_set_color(e, c("amber").as_ptr(), c("#f6e0a0").as_ptr()).is_null());
        let hl: Value = serde_json::from_str(&take(plumbline_engine_chapter_highlights_json(e, c("John").as_ptr(), 3)).unwrap()).unwrap();
        assert_eq!(hl["verses"][0]["verse"], "John 3:16");
        assert_eq!(hl["verses"][0]["color"], "#f6e0a0");
        // Clearing the colour clears the wash.
        assert!(plumbline_engine_tag_set_color(e, c("amber").as_ptr(), ptr::null()).is_null());
        let hl2: Value = serde_json::from_str(&take(plumbline_engine_chapter_highlights_json(e, c("John").as_ptr(), 3)).unwrap()).unwrap();
        assert!(hl2["verses"].as_array().unwrap().is_empty());

        // Word-precise cross-verse highlight: "drag" John 3:16 tok2 → 3:18 tok1
        // with a tone. The fixture has 3:16 (6 tokens) and 3:18 (3 tokens), so
        // the range lands as a start-partial run (tok 2..last=5) plus an
        // end-partial run (0..1).
        assert!(plumbline_engine_highlight_add(
            e, c("dragged").as_ptr(), c("#c8b0e0").as_ptr(),
            c("John 3:16").as_ptr(), 2, c("John 3:18").as_ptr(), 1, stamp.as_ptr(),
        ).is_null());
        let hr: Value = serde_json::from_str(
            &take(plumbline_engine_chapter_highlights_json(e, c("John").as_ptr(), 3)).unwrap()).unwrap();
        let runs = hr["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 2, "runs: {runs:?}");
        assert_eq!(runs[0]["verse"], "John 3:16");
        assert_eq!(runs[0]["lo"], 2);
        assert_eq!(runs[0]["hi"], 5);
        assert_eq!(runs[0]["color"], "#c8b0e0");
        assert_eq!(runs[1]["verse"], "John 3:18");
        assert_eq!(runs[1]["lo"], 0);
        assert_eq!(runs[1]["hi"], 1);
        // A backwards drag (end→start) is ordered the same and dedupes.
        assert!(plumbline_engine_highlight_add(
            e, c("dragged").as_ptr(), c("#c8b0e0").as_ptr(),
            c("John 3:18").as_ptr(), 1, c("John 3:16").as_ptr(), 2, stamp.as_ptr(),
        ).is_null());
        let hr_dup: Value = serde_json::from_str(
            &take(plumbline_engine_chapter_highlights_json(e, c("John").as_ptr(), 3)).unwrap()).unwrap();
        assert_eq!(hr_dup["runs"].as_array().unwrap().len(), 2, "backwards drag must dedupe");
        // Remove it → runs gone.
        assert!(plumbline_engine_highlight_remove(
            e, c("dragged").as_ptr(), c("John 3:16").as_ptr(), 2, c("John 3:18").as_ptr(), 1,
        ).is_null());
        let hr2: Value = serde_json::from_str(
            &take(plumbline_engine_chapter_highlights_json(e, c("John").as_ptr(), 3)).unwrap()).unwrap();
        assert!(hr2["runs"].as_array().map(|a| a.is_empty()).unwrap_or(true));

        // Clear-by-verse (WinUI's "Remove highlight"): re-add, then clear on any
        // covered verse drops the whole range.
        assert!(plumbline_engine_highlight_add(
            e, c("dragged").as_ptr(), c("#c8b0e0").as_ptr(),
            c("John 3:16").as_ptr(), 2, c("John 3:18").as_ptr(), 1, stamp.as_ptr(),
        ).is_null());
        assert!(plumbline_engine_highlight_clear_verse(e, c("John 3:16").as_ptr()).is_null());
        let hr4: Value = serde_json::from_str(
            &take(plumbline_engine_chapter_highlights_json(e, c("John").as_ptr(), 3)).unwrap()).unwrap();
        assert!(hr4["runs"].as_array().map(|a| a.is_empty()).unwrap_or(true));

        // Memorization (Tier 2 #15): grade → card, drill, recall, coverage, activity.
        assert!(plumbline_engine_memory_grade(
            e, c("John 3:16").as_ptr(), c("good").as_ptr(), stamp.as_ptr()).is_null());
        let card: Value = serde_json::from_str(
            &take(plumbline_engine_memory_card_json(e, c("John 3:16").as_ptr())).unwrap()).unwrap();
        assert_eq!(card["ref"], "John 3:16");
        assert_eq!(card["reps"], 1);
        assert_eq!(card["mastery"], "young"); // 1-day interval after one Good
        assert_eq!(card["reviews"].as_array().unwrap().len(), 1);
        // An unknown grade is rejected (non-null error).
        assert!(!plumbline_engine_memory_grade(
            e, c("John 3:16").as_ptr(), c("bogus").as_ptr(), stamp.as_ptr()).is_null());

        // Drill: first-letter skeleton + (level-0) unblanked form of the verse.
        let drill: Value = serde_json::from_str(
            &take(plumbline_engine_memory_drill_json(e, c("John 3:16").as_ptr(), 0)).unwrap()).unwrap();
        assert!(drill["text"].as_str().unwrap().starts_with("For God so loved"));
        assert_eq!(drill["firstLetters"], "F G s l t w.");
        assert!(!drill["blanked"].as_str().unwrap().contains('_')); // nothing hidden at level 0

        // Recall scoring: a perfect (case/punctuation-tolerant) recall is 1.0.
        let sc: Value = serde_json::from_str(&take(plumbline_engine_memory_score_json(
            e, c("John 3:16").as_ptr(), c("for god so loved the world").as_ptr())).unwrap()).unwrap();
        assert_eq!(sc["accuracy"], 1.0);

        // Coverage + activity, from the review log.
        let cov: Value = serde_json::from_str(&take(plumbline_engine_memory_coverage_json(
            e, stamp.as_ptr())).unwrap()).unwrap();
        assert_eq!(cov["verses"][0]["ref"], "John 3:16");
        let gospels = cov["sections"].as_array().unwrap().iter()
            .find(|s| s["label"] == "Gospels").unwrap().clone();
        assert_eq!(gospels["cards"], 1);
        let act: Value = serde_json::from_str(
            &take(plumbline_engine_memory_activity_json(e)).unwrap()).unwrap();
        assert_eq!(act["days"].as_array().unwrap().len(), 1);

        // Seed a card without reviewing ("Memorize this verse") → new, reps 0.
        assert!(plumbline_engine_memory_add(e, c("John 3:18").as_ptr(), stamp.as_ptr()).is_null());
        let seeded: Value = serde_json::from_str(
            &take(plumbline_engine_memory_card_json(e, c("John 3:18").as_ptr())).unwrap()).unwrap();
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
            e, c("John 3:16").as_ptr(), c("John 3:18").as_ptr(), stamp.as_ptr()).is_null());
        let pc: Value = serde_json::from_str(
            &take(plumbline_engine_memory_card_json(e, c("John 3:16").as_ptr())).unwrap()).unwrap();
        assert_eq!(pc["ref"], "John 3:16");
        assert_eq!(pc["label"], "John 3:16\u{2013}18");
        assert_eq!(pc["through"], "John 3:18");
        // Only the first verse addresses the card — inner verses are not cards.
        assert!(plumbline_engine_memory_card_json(e, c("John 3:18").as_ptr()).is_null());

        let pd: Value = serde_json::from_str(
            &take(plumbline_engine_memory_drill_json(e, c("John 3:16").as_ptr(), 0)).unwrap()).unwrap();
        assert_eq!(pd["label"], "John 3:16\u{2013}18");
        assert_eq!(pd["verses"], 2, "two of the three verses exist in this fixture");
        let ptext = pd["text"].as_str().unwrap().to_string();
        assert_eq!(ptext, "For God so loved the world. He that believeth");
        assert_eq!(pd["firstLetters"], "F G s l t w. H t b");
        // Typing only the opening verse of a passage cannot score full marks.
        let half: Value = serde_json::from_str(&take(plumbline_engine_memory_score_json(
            e, c("John 3:16").as_ptr(), c("For God so loved the world.").as_ptr())).unwrap()).unwrap();
        let acc = half["accuracy"].as_f64().unwrap();
        assert!(acc > 0.5 && acc < 1.0, "half a passage scores partial, got {acc}");
        // Typing the whole passage back scores it in full.
        let whole: Value = serde_json::from_str(&take(plumbline_engine_memory_score_json(
            e, c("John 3:16").as_ptr(), c(&ptext).as_ptr())).unwrap()).unwrap();
        assert_eq!(whole["accuracy"], 1.0);

        // The hub lists ONE row for the passage; the map shades every verse of it.
        let pcov: Value = serde_json::from_str(&take(plumbline_engine_memory_coverage_json(
            e, stamp.as_ptr())).unwrap()).unwrap();
        assert_eq!(pcov["cards"].as_array().unwrap().len(), 1);
        assert_eq!(pcov["cards"][0]["ref"], "John 3:16");
        assert_eq!(pcov["cards"][0]["label"], "John 3:16\u{2013}18");
        assert_eq!(pcov["cards"][0]["verses"], 3);
        assert_eq!(
            pcov["verses"].as_array().unwrap().iter().map(|v| v["ref"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["John 3:16", "John 3:17", "John 3:18"]
        );
        // Grading the passage keeps it one card, still spanning.
        assert!(plumbline_engine_memory_grade(
            e, c("John 3:16").as_ptr(), c("good").as_ptr(), stamp.as_ptr()).is_null());
        let graded: Value = serde_json::from_str(
            &take(plumbline_engine_memory_card_json(e, c("John 3:16").as_ptr())).unwrap()).unwrap();
        assert_eq!((graded["reps"].as_u64(), graded["through"].as_str()), (Some(1), Some("John 3:18")));
        assert!(plumbline_engine_memory_remove(e, c("John 3:16").as_ptr()).is_null());

        // A backwards end is not a passage — it seeds a plain single-verse card.
        assert!(plumbline_engine_memory_add_passage(
            e, c("John 3:18").as_ptr(), c("John 3:16").as_ptr(), stamp.as_ptr()).is_null());
        let flat: Value = serde_json::from_str(
            &take(plumbline_engine_memory_card_json(e, c("John 3:18").as_ptr())).unwrap()).unwrap();
        assert_eq!(flat["label"], "John 3:18");
        assert!(flat["through"].is_null());
        assert!(plumbline_engine_memory_remove(e, c("John 3:18").as_ptr()).is_null());
        // An end verse that does not exist is refused, not silently flattened.
        assert!(!plumbline_engine_memory_add_passage(
            e, c("John 3:16").as_ptr(), c("John 3:999").as_ptr(), stamp.as_ptr()).is_null());
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

    let t = Instant::now();
    let _ = eng.verse_sim();
    println!("verse_sim SIF:  {:?}", t.elapsed());

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
    let emb = embed::load_embedding(canon::TOKENIZATION_VERSION, data.join("concept-vectors.vec"));
    println!("embedding load:   {:?} (loaded: {})", t.elapsed(), emb.is_some());

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
            let home = std::env::temp_dir()
                .join(format!("plumbline-ffi-akjv-{}-{stamp}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(home.join("data")).unwrap();
            std::fs::write(home.join("data").join("kjv.jsonl"), KJV).unwrap();
            std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();
            std::fs::write(
                home.join("data").join("akjv.jsonl"),
                OVERLAY.replace("kjv1769-tok2", stamp),
            )
            .unwrap();

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
            let dl = plumbline_engine_layout_chapter(
                e, c"John".as_ptr(), 3, cfg(), Some(mono_measure), ptr::null_mut(),
            );
            assert!(!dl.is_null());
            let json = take(plumbline_layout_to_json(dl)).unwrap();
            plumbline_layout_free(dl);
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            v["items"].as_array().unwrap().iter()
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
        assert!(plumbline_engine_concept_neighbours_json(e, c("G25").as_ptr(), 5).is_null());
        assert!(plumbline_engine_morph_json(e, c("John 3:16").as_ptr(), 3).is_null());
        assert!(plumbline_engine_similar_verses_json(e, c("John 3:16").as_ptr(), 5).is_null());

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
        assert!(plumbline_engine_warm_indexes(e).is_null()); // builds the SIF now

        let n: Value = serde_json::from_str(&take(plumbline_engine_concept_neighbours_json(e, c("G25").as_ptr(), 5)).unwrap()).unwrap();
        assert_eq!(n["code"], "G25");
        let m: Value = serde_json::from_str(&take(plumbline_engine_morph_json(e, c("John 3:16").as_ptr(), 3)).unwrap()).unwrap();
        assert_eq!(m["code"], "V-AAI-3S");
        let s: Value = serde_json::from_str(&take(plumbline_engine_similar_verses_json(e, c("John 3:16").as_ptr(), 5)).unwrap()).unwrap();
        assert!(s["in"].as_array().unwrap().iter().any(|x| x["verse"] == "John 3:18"));

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// A corpus of `chapters * per` verses over Psalms, every verse carrying codes
/// the test embedding covers. Deliberately bigger than one warm slice — see
/// `sif_model_is_built_in_slices` for why that is the whole point.
fn generated_kjv(chapters: u16, per: u16) -> String {
    const CODES: [&str; 4] = ["G2316", "G25", "G4100", "H7225"];
    let mut out = format!(
        r#"{{"format":"x","tokenization":"kjv1769-tok2","verses":{}}}"#,
        chapters as usize * per as usize
    );
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

/// The SIF model must be built in SLICES, like every other heavy warm phase.
///
/// It was the one that wasn't. Phase 7 of `warm_next` was a single-shot
/// `verse_sim()` call, and on a real phone that held the engine worker for
/// **54,859 ms in one synchronous block** (maintainer's boot trace, 2026-07-28)
/// — during which the worker answers no layout, no tap and no word study, which
/// is exactly the "it says loading and the first one takes longer, every time I
/// reopen it" report. Every other heavy phase was sliced on 2026-07-27; this one
/// was missed because the same build is ~226 ms on a desktop and vanishes into
/// the noise there. The ratio is ~240x, not the ~6-10x a phone's CPU explains,
/// because the build's cost was allocation churn rather than arithmetic.
///
/// THE CORPUS HERE IS BIGGER THAN ONE SLICE ON PURPOSE. With the two-verse
/// fixture the rest of this file uses, a single call finishes the model whether
/// or not the code slices anything — so that version of this test would pass
/// against the very bug it is named after. Guarding a slicing property requires
/// more work than one slice can do.
///
/// The probe reads `verse_sim` directly rather than calling
/// `similar_verses_json`, because that entry point builds the model on demand:
/// asking "is it built yet?" through the public API would BUILD it and the
/// assertion would be measuring its own side effect.
///
/// It drives `warm_next(WARM_SLICE)` rather than `plumbline_engine_warm_step`,
/// which is compiled only for wasm32. That export is a one-line
/// `guard(0, || e.warm_next(WARM_SLICE))` wrapper over exactly this call, and the
/// slice size is shared, so nothing about the slicing behaviour is bypassed.
#[test]
fn sif_model_is_built_in_slices() {
    use std::ffi::CString;
    unsafe {
        let home = std::env::temp_dir().join(format!("plumbline-ffi-sifslice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("data")).unwrap();
        // 150 x 20 = 3,000 verses: more than the 2,048-verse warm slice.
        std::fs::write(home.join("data").join("kjv.jsonl"), generated_kjv(150, 20)).unwrap();
        std::fs::write(home.join("data").join("strongs.json"), STRONGS).unwrap();

        let home_c = CString::new(home.to_str().unwrap()).unwrap();
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open(home_c.as_ptr(), &mut err);
        assert!(err.is_null() && !e.is_null(), "engine opened");
        let eng = &*e;

        // Warm to completion with NO embedding present. Phase 7 is reached and
        // does nothing, leaving `sif_attempted` false — the same state a web boot
        // is in when the analysis pack has not landed yet.
        let mut calls = 0;
        while eng.warm_next(crate::WARM_SLICE) == 1 {
            calls += 1;
            assert!(calls < 10_000, "warm never terminated");
        }
        assert!(eng.verse_sim.get().is_none(), "no embedding: nothing to build from");

        // The analysis pack arrives.
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
        assert!(plumbline_engine_load_rnd_data(e).is_null());

        // ONE slice. Over 3,000 verses this cannot be the whole model — and if it
        // is, the phase is a monolithic block and a phone pays for it in one go.
        assert_eq!(eng.warm_next(crate::WARM_SLICE), 1, "a slice leaves work behind");
        assert!(
            eng.verse_sim.get().is_none(),
            "one warm slice built the ENTIRE SIF model: phase 7 is not sliced, so the engine \
             worker is held for the whole build and answers no layout or tap while it runs"
        );

        // ...and driving it out finishes, so slicing did not merely defer forever.
        let mut more = 1;
        let mut slices = 1;
        while more == 1 {
            more = eng.warm_next(crate::WARM_SLICE);
            slices += 1;
            assert!(slices < 10_000, "sliced warm never terminated");
        }
        assert!(eng.verse_sim.get().is_some(), "the sliced build completes");

        // And the model it produced actually answers — a builder that terminates
        // with a hollow model would satisfy everything above.
        let s: Value = serde_json::from_str(
            &take(plumbline_engine_similar_verses_json(e, c"Ps 1:2".as_ptr(), 5)).unwrap(),
        )
        .unwrap();
        assert!(
            !s["in"].as_array().unwrap().is_empty(),
            "the sliced SIF model returned no neighbours: {s}"
        );

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
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
        assert!(
            !v["blocks"].as_array().unwrap().is_empty(),
            "the tap built nothing AND answered nothing: {blocks}"
        );

        // Once the warm finishes, the same tap is fully furnished — the sections
        // that were skipped are not skipped forever.
        let mut n = 0;
        while eng.warm_next(crate::WARM_SLICE) == 1 {
            n += 1;
            assert!(n < 10_000, "warm never terminated");
        }
        assert!(eng.occ_ix.get().is_some(), "the warm built the occurrence index");
        assert!(eng.renderings.get().is_some(), "the warm built the rendering lens");
        let after: Value = serde_json::from_str(
            &take(plumbline_engine_word_study_blocks2_json(e, c"Ps 1:2".as_ptr(), 1, 3)).unwrap(),
        )
        .unwrap();
        assert!(
            after["blocks"].as_array().unwrap().len() > v["blocks"].as_array().unwrap().len(),
            "the warm added nothing to the study — the deferred sections never filled in"
        );

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
        assert!(
            eng.occ_ix.get().is_some(),
            "a tap on a shell that does NOT slice must still build what it needs"
        );

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
        let st: Value = serde_json::from_str(&take(plumbline_engine_strongs_json(e, c("G2316").as_ptr())).unwrap()).unwrap();
        assert_eq!(st["code"], "G2316");

        plumbline_engine_free(e);
        let _ = std::fs::remove_dir_all(&home);
    }
}
