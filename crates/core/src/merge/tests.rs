use super::*;
use serde_json::json;

fn files(entries: &[(&str, Value)]) -> Files {
    entries.iter().map(|(p, v)| (p.to_string(), pretty(v))).collect()
}

/// The device after the merge is applied.
fn apply(local: &Files, m: &Merged) -> Files {
    let mut out = local.clone();
    for p in &m.delete {
        out.remove(p);
    }
    for (p, b) in &m.write {
        out.insert(p.clone(), b.clone());
    }
    out
}

/// Files compared as JSON, so key order and whitespace do not matter.
fn as_json(f: &Files) -> BTreeMap<String, Value> {
    f.iter().map(|(p, b)| (p.clone(), serde_json::from_slice(b).unwrap_or(Value::Null))).collect()
}

fn note(id: &str, r: &str, text: &str, updated: &str) -> Value {
    json!({"format":"plumbline-note-v2","id":id,"ref":r,"text":text,"created":"2026-01-01T00:00:00Z","updated":updated})
}

fn note_path(id: &str) -> String {
    format!("notes/john-3-16-{id}.json")
}

fn tombs(entries: &[(&str, &str)]) -> (String, Value) {
    let m: Map<String, Value> = entries.iter().map(|(k, v)| (k.to_string(), Value::from(*v))).collect();
    (tomb::REL_PATH.to_string(), json!({"format": tomb::FORMAT, "deleted": m}))
}

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
fn two_notes_on_one_verse_from_two_devices_are_both_kept() {
    let phone = files(&[(&note_path(A), note(A, "John 3:16", "from the phone", "2026-02-01T00:00:00Z"))]);
    let laptop = files(&[(&note_path(B), note(B, "John 3:16", "from the laptop", "2026-02-02T00:00:00Z"))]);
    let after = apply(&phone, &merge(&phone, &laptop));
    assert_eq!(after.len(), 2, "both notes stay: {:?}", after.keys().collect::<Vec<_>>());
}

#[test]
fn the_same_note_edited_keeps_the_newer_edit_either_way_round() {
    let older = files(&[(&note_path(A), note(A, "John 3:16", "first draft", "2026-02-01T00:00:00Z"))]);
    let newer = files(&[(&note_path(A), note(A, "John 3:16", "revised", "2026-03-01T00:00:00Z"))]);
    for (local, incoming) in [(&older, &newer), (&newer, &older)] {
        let after = as_json(&apply(local, &merge(local, incoming)));
        assert_eq!(after.len(), 1);
        assert_eq!(after[&note_path(A)]["text"], "revised");
    }
}

#[test]
fn a_note_deleted_on_one_device_stays_deleted_and_one_written_after_survives() {
    let (tp, tv) = tombs(&[("note:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "2026-04-01T00:00:00Z")]);
    // The device still has it; the backup records its deletion.
    let device = files(&[(&note_path(A), note(A, "John 3:16", "old", "2026-02-01T00:00:00Z"))]);
    let backup = files(&[(&tp, tv.clone())]);
    let after = apply(&device, &merge(&device, &backup));
    assert!(!after.contains_key(&note_path(A)), "the deletion did not reach the device");
    assert!(after.contains_key(tomb::REL_PATH), "the record must travel too, or the next merge resurrects it");

    // The other way round: the device deleted it, the backup is older.
    let device = files(&[(&tp, tv.clone())]);
    let backup = files(&[(&note_path(A), note(A, "John 3:16", "old", "2026-02-01T00:00:00Z"))]);
    let after = apply(&device, &merge(&device, &backup));
    assert!(!after.contains_key(&note_path(A)), "an older backup resurrected a deleted note");

    // Written again after the deletion: it is a new life, and it stays.
    let backup = files(&[(&note_path(A), note(A, "John 3:16", "again", "2026-05-01T00:00:00Z"))]);
    let after = apply(&device, &merge(&device, &backup));
    assert!(after.contains_key(&note_path(A)));
}

#[test]
fn a_v1_note_on_both_devices_is_one_note_and_writes_nothing() {
    let v1 = json!({"format":"pure-note-v1","ref":"John 3:16","text":"old","created":"2025-01-01T00:00:00Z","updated":"2025-01-01T00:00:00Z"});
    let both = files(&[("notes/john-3-16.json", v1)]);
    let m = merge(&both, &both);
    assert_eq!(m, Merged::default(), "identical sides must change nothing");
}

#[test]
fn a_v1_note_meets_its_own_v2_edit_and_the_verse_holds_it_once() {
    let v1 = json!({"format":"pure-note-v1","ref":"John 3:16","text":"old","created":"2025-01-01T00:00:00Z","updated":"2025-01-01T00:00:00Z"});
    let id = crate::usernote::legacy_id("John 3:16", "2025-01-01T00:00:00Z");
    let mut v2 = note(&id, "John 3:16", "edited", "2026-01-01T00:00:00Z");
    v2["created"] = json!("2025-01-01T00:00:00Z");
    let device = files(&[("notes/john-3-16.json", v1)]);
    let backup = files(&[(&note_path(&id), v2)]);
    let after = as_json(&apply(&device, &merge(&device, &backup)));
    assert_eq!(after.len(), 1, "{:?}", after.keys().collect::<Vec<_>>());
    assert_eq!(after[&note_path(&id)]["text"], "edited");
}

#[test]
fn a_backup_v1_note_beside_a_different_device_note_on_the_verse_is_kept_as_v2() {
    // Same verse, same v1 path, different notes (different creation): both stay.
    let mine = json!({"format":"pure-note-v1","ref":"John 3:16","text":"mine","created":"2025-01-01T00:00:00Z","updated":"2025-01-01T00:00:00Z"});
    let theirs = json!({"format":"pure-note-v1","ref":"John 3:16","text":"theirs","created":"2025-06-01T00:00:00Z","updated":"2025-06-01T00:00:00Z"});
    let device = files(&[("notes/john-3-16.json", mine)]);
    let backup = files(&[("notes/john-3-16.json", theirs)]);
    let after = as_json(&apply(&device, &merge(&device, &backup)));
    let texts: BTreeSet<&str> = after.values().filter_map(|v| v["text"].as_str()).collect();
    assert_eq!(texts, BTreeSet::from(["mine", "theirs"]));
    let id = crate::usernote::legacy_id("John 3:16", "2025-06-01T00:00:00Z");
    assert_eq!(after[&note_path(&id)]["format"], crate::usernote::FORMAT);
}

fn tag(id: &str, name: &str, updated: &str, members: &[(&str, &str)]) -> Value {
    let m: Vec<Value> =
        members.iter().map(|(r, added)| json!({"target":{"kind":"verse","ref":r},"added":added})).collect();
    json!({"format":"overlay-tag-v1","name":name,"tokenization":"kjv1769-tok2","created":"2026-01-01T00:00:00Z","id":id,"updated":updated,"members":m})
}

fn refs(v: &Value) -> BTreeSet<String> {
    v["members"].as_array().unwrap().iter().map(|m| m["target"]["ref"].as_str().unwrap().to_string()).collect()
}

#[test]
fn a_tag_on_two_devices_holds_both_devices_verses() {
    let phone = files(&[("tags/grace.json", tag(A, "grace", "2026-02-01T00:00:00Z", &[("John 3:16", "t1")]))]);
    let laptop = files(&[("tags/grace.json", tag(A, "grace", "2026-03-01T00:00:00Z", &[("Eph 2:8", "t2")]))]);
    let after = as_json(&apply(&phone, &merge(&phone, &laptop)));
    assert_eq!(refs(&after["tags/grace.json"]), BTreeSet::from(["John 3:16".into(), "Eph 2:8".into()]));
}

#[test]
fn the_same_name_with_different_ids_is_one_tag_not_two() {
    let phone = files(&[("tags/grace.json", tag(A, "grace", "2026-02-01T00:00:00Z", &[("John 3:16", "t1")]))]);
    let laptop = files(&[("tags/grace.json", tag(B, "grace", "2026-03-01T00:00:00Z", &[("Eph 2:8", "t2")]))]);
    let after = as_json(&apply(&phone, &merge(&phone, &laptop)));
    assert_eq!(after.len(), 1);
    assert_eq!(refs(&after["tags/grace.json"]).len(), 2);
}

#[test]
fn a_renamed_tag_follows_its_id_and_leaves_one_file() {
    let phone = files(&[("tags/grace.json", tag(A, "grace", "2026-02-01T00:00:00Z", &[("John 3:16", "t1")]))]);
    let laptop = files(&[("tags/unmerited-favour.json", tag(A, "unmerited favour", "2026-03-01T00:00:00Z", &[]))]);
    let after = as_json(&apply(&phone, &merge(&phone, &laptop)));
    assert_eq!(after.keys().collect::<Vec<_>>(), ["tags/unmerited-favour.json"]);
    assert_eq!(
        refs(&after["tags/unmerited-favour.json"]),
        BTreeSet::from(["John 3:16".into()]),
        "the members came along"
    );
}

#[test]
fn a_member_removed_on_one_device_stays_removed() {
    let key = tomb::tag_member(A, "verse:John 3:16");
    let (tp, tv) = tombs(&[(key.as_str(), "2026-04-01T00:00:00Z")]);
    let phone = files(&[(
        "tags/grace.json",
        tag(A, "grace", "2026-02-01T00:00:00Z", &[("John 3:16", "2026-01-10T00:00:00Z"), ("Eph 2:8", "t")]),
    )]);
    let laptop =
        files(&[("tags/grace.json", tag(A, "grace", "2026-04-01T00:00:00Z", &[("Eph 2:8", "t")])), (tp.as_str(), tv)]);
    let after = as_json(&apply(&phone, &merge(&phone, &laptop)));
    assert_eq!(refs(&after["tags/grace.json"]), BTreeSet::from(["Eph 2:8".into()]));
}

fn book(chapters: Value) -> Value {
    json!({"format":"plumbline-reading-v1","book":"Gen","chapters":chapters})
}

#[test]
fn reading_in_one_book_on_two_devices_adds_up_by_chapter() {
    let phone = files(&[(
        "reading/gen.json",
        book(
            json!([{"c":1,"lastRead":"2026-01-01","touched":"2026-01-01"},{"c":2,"reached":10,"dwell":30.0,"touched":"2026-01-02"}]),
        ),
    )]);
    let laptop = files(&[(
        "reading/gen.json",
        book(
            json!([{"c":2,"reached":20,"dwell":10.0,"touched":"2026-01-05"},{"c":3,"lastRead":"2026-01-06","touched":"2026-01-06"}]),
        ),
    )]);
    let after = as_json(&apply(&phone, &merge(&phone, &laptop)));
    let ch = after["reading/gen.json"]["chapters"].as_array().unwrap().clone();
    assert_eq!(ch.iter().map(|c| c["c"].as_u64().unwrap()).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(ch[1]["reached"], 20);
    assert_eq!(ch[1]["dwell"], 30.0);
    assert_eq!(ch[1]["touched"], "2026-01-05");
}

#[test]
fn a_forgotten_chapter_stays_forgotten() {
    let (tp, tv) = tombs(&[(tomb::reading("Gen", 1).as_str(), "2026-02-01T00:00:00Z")]);
    let device = files(&[("reading/gen.json", book(json!([]))), (tp.as_str(), tv)]);
    let backup = files(&[("reading/gen.json", book(json!([{"c":1,"lastRead":"2026-01-01","touched":"2026-01-01"}])))]);
    let after = as_json(&apply(&device, &merge(&device, &backup)));
    assert_eq!(after["reading/gen.json"]["chapters"], json!([]));
}

#[test]
fn a_plan_followed_on_two_devices_has_both_devices_days_done() {
    let plan = |done: Value| json!({"format":"plumbline-plan-v1","id":"bible-year","kind":"wholeBible","started":"2026-01-01","done":done});
    let phone = files(&[("plans/bible-year.json", plan(json!([1, 2])))]);
    let laptop = files(&[("plans/bible-year.json", plan(json!([2, 3])))]);
    let after = as_json(&apply(&phone, &merge(&phone, &laptop)));
    assert_eq!(after["plans/bible-year.json"]["done"], json!([1, 2, 3]));
}

#[test]
fn a_memory_card_keeps_the_device_that_reviewed_it_last() {
    let card = |at: &str, ease: f64| json!({"format":"overlay-memory-v1","ref":"Ps 23:1","created":"2026-01-01T00:00:00Z","ease":ease,"reviews":[{"at":at,"grade":"good","intervalDays":1}]});
    let phone = files(&[("memory/ps-23-1.json", card("2026-02-01T00:00:00Z", 2.5))]);
    let laptop = files(&[("memory/ps-23-1.json", card("2026-03-01T00:00:00Z", 2.6))]);
    let after = as_json(&apply(&phone, &merge(&phone, &laptop)));
    assert_eq!(after["memory/ps-23-1.json"]["ease"], 2.6);
}

#[test]
fn settings_come_from_the_backup_and_the_histories_combine() {
    let cfg = |theme: &str, hist: Value| json!({"theme":theme,"history":hist});
    let device = files(&[(
        ".config/plumbline/config.json",
        cfg("night", json!([{"book":"Gen","chapter":1},{"book":"Exod","chapter":2}])),
    )]);
    let backup = files(&[(
        ".config/plumbline/config.json",
        cfg("light", json!([{"book":"Rev","chapter":22},{"book":"Gen","chapter":1}])),
    )]);
    let after = as_json(&apply(&device, &merge(&device, &backup)));
    let c = &after[".config/plumbline/config.json"];
    assert_eq!(c["theme"], "light");
    let books: Vec<&str> = c["history"].as_array().unwrap().iter().map(|e| e["book"].as_str().unwrap()).collect();
    assert_eq!(books, ["Rev", "Gen", "Exod"]);
}

#[test]
fn an_unreadable_backup_file_never_replaces_a_readable_one() {
    let device = files(&[(&note_path(A), note(A, "John 3:16", "fine", "2026-02-01T00:00:00Z"))]);
    let mut backup = Files::new();
    backup.insert(note_path(A), b"{ truncated".to_vec());
    let after = apply(&device, &merge(&device, &backup));
    assert_eq!(after, device);
}

/// The CRDT properties, on a pair of devices that disagree about everything
/// that merges symmetrically (settings excluded: the backup's win by design).
#[test]
fn merging_is_order_free_and_safe_to_repeat() {
    let (tp, tv) = tombs(&[("note:cccccccccccccccccccccccccccccccc", "2026-03-01T00:00:00Z")]);
    let phone = files(&[
        (&note_path(A), note(A, "John 3:16", "phone", "2026-02-01T00:00:00Z")),
        (
            "notes/john-3-16-cccccccccccccccccccccccccccccccc.json",
            note("cccccccccccccccccccccccccccccccc", "John 3:16", "doomed", "2026-02-01T00:00:00Z"),
        ),
        ("tags/grace.json", tag(A, "grace", "2026-02-01T00:00:00Z", &[("John 3:16", "t1")])),
        ("reading/gen.json", book(json!([{"c":1,"reached":5,"touched":"2026-01-01"}]))),
    ]);
    let laptop = files(&[
        (&note_path(B), note(B, "John 3:16", "laptop", "2026-02-02T00:00:00Z")),
        ("tags/grace.json", tag(A, "grace", "2026-03-01T00:00:00Z", &[("Eph 2:8", "t2")])),
        ("reading/gen.json", book(json!([{"c":1,"reached":9,"touched":"2026-01-03"},{"c":2,"touched":"2026-01-04"}]))),
        (tp.as_str(), tv),
    ]);
    let ab = apply(&phone, &merge(&phone, &laptop));
    let ba = apply(&laptop, &merge(&laptop, &phone));
    assert_eq!(as_json(&ab), as_json(&ba), "the result depends on which device restored which");
    assert_eq!(merge(&ab, &laptop), Merged::default(), "restoring the same backup twice changed something");
    assert_eq!(merge(&ab, &ab), Merged::default());
}
