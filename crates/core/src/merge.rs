//! Merge a backup into a device, so two devices' study data add together.
//!
//! A restore used to write the backup's files over the device's, so the same
//! note or tag on both kept only the backup's copy. [`merge`] combines them
//! instead, per kind of data:
//!
//! | Data | Same thing on both sides |
//! |---|---|
//! | notes | matched by id; the newer edit wins; different ids are all kept |
//! | tags | matched by id (else name); members are a union; the newer tag names it |
//! | weaves | matched by id (else name); links are a union |
//! | threads, memory cards | the newer one wins, whole |
//! | plans, devotional runs | done days are a union |
//! | reading | per chapter: furthest reached, longest dwell, latest dates |
//! | settings | the backup's, with both reading histories combined |
//!
//! Deletion records ([`crate::tombstone`]) from both sides are combined first,
//! and anything a record is at least as new as is dropped — from the device as
//! much as from the backup — so a restore neither resurrects what was deleted
//! nor keeps what the other device deleted later.
//!
//! "Newer" compares the frozen `YYYY-MM-DDThh:mm:ssZ` stamps as strings, so it
//! trusts each device's clock. Ties keep the device's copy.
//!
//! Works on raw JSON, so every field this build does not know survives a merge.
//! A file it cannot read is never written over a readable one, and is kept
//! where it is. Paths are home-relative (`notes/john-3-16-<id>.json`).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::tombstone as tomb;

/// Home-relative path → bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

/// One side's files of one kind, borrowed.
type Side<'a> = Vec<(&'a str, &'a [u8])>;

/// How an array's elements are keyed for a union.
type KeyOf = fn(&Value) -> Option<String>;

/// What to do to the device's files.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Merged {
    pub write: Files,
    pub delete: Vec<String>,
}

/// Merge `incoming` (a backup) into `local` (the device). See the module docs.
pub fn merge(local: &Files, incoming: &Files) -> Merged {
    let records_local = local.get(tomb::REL_PATH).map(|b| tomb::parse(b)).unwrap_or_default();
    let records_in = incoming.get(tomb::REL_PATH).map(|b| tomb::parse(b)).unwrap_or_default();
    let records = tomb::union(&records_local, &records_in);

    let mut out = Out { local, write: Files::new(), delete: BTreeSet::new() };
    if records != records_local {
        out.put(tomb::REL_PATH.to_string(), tomb::to_bytes(&records));
    }

    // Partition both sides by kind; anything unparsed falls to `other`.
    let mut buckets: BTreeMap<Kind, (Side, Side)> = BTreeMap::new();
    for (side, files) in [(0, local), (1, incoming)] {
        for (p, b) in files {
            if p == tomb::REL_PATH {
                continue;
            }
            let e = buckets.entry(kind_of(p)).or_default();
            if side == 0 {
                e.0.push((p, b))
            } else {
                e.1.push((p, b))
            }
        }
    }
    for (kind, (l, i)) in &buckets {
        match kind {
            Kind::Notes => notes(&mut out, &records, l, i),
            Kind::Tags => named(&mut out, &records, l, i, &NAMED_TAG),
            Kind::Weaves => named(&mut out, &records, l, i, &NAMED_WEAVE),
            Kind::Threads => named(&mut out, &records, l, i, &NAMED_THREAD),
            Kind::Suggested => suggested(&mut out, &records, l, i),
            Kind::Memory => memory(&mut out, &records, l, i),
            Kind::Plans => runs(&mut out, &records, l, i, crate::plan::FORMAT, tomb::plan, "doneOn"),
            Kind::Devotionals => runs(&mut out, &records, l, i, crate::devotional::RUN_FORMAT, tomb::devotional, ""),
            Kind::Reading => reading(&mut out, &records, l, i),
            Kind::Config => other(&mut out, l, i, true),
            Kind::Other => other(&mut out, l, i, false),
        }
    }
    out.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Notes,
    Tags,
    Weaves,
    Suggested,
    Threads,
    Memory,
    Plans,
    Devotionals,
    Reading,
    Config,
    Other,
}

fn kind_of(path: &str) -> Kind {
    let json = path.ends_with(".json");
    let in_dir = |d: &str| path.strip_prefix(d).is_some_and(|rest| !rest.contains('/'));
    match () {
        _ if path.starts_with(".config/") => Kind::Config,
        _ if !json => Kind::Other,
        _ if in_dir("notes/") => Kind::Notes,
        _ if in_dir("tags/") => Kind::Tags,
        _ if in_dir("weaves/suggested/") => Kind::Suggested,
        _ if in_dir("weaves/") => Kind::Weaves,
        _ if in_dir("threads/") => Kind::Threads,
        _ if in_dir("memory/") => Kind::Memory,
        _ if in_dir("plans/") => Kind::Plans,
        _ if in_dir("devotionals/") => Kind::Devotionals,
        _ if in_dir("reading/") => Kind::Reading,
        _ => Kind::Other,
    }
}

// ── output ───────────────────────────────────────────────────────────────────

struct Out<'a> {
    local: &'a Files,
    write: Files,
    delete: BTreeSet<String>,
}

impl Out<'_> {
    /// Want `path` to hold `bytes`. Skipped when the device already holds the
    /// same bytes or the same JSON, so a merge of identical sides writes nothing.
    fn put(&mut self, path: String, bytes: Vec<u8>) {
        self.delete.remove(&path);
        if let Some(have) = self.local.get(&path) {
            if *have == bytes || same_json(have, &bytes) {
                return;
            }
        }
        self.write.insert(path, bytes);
    }
    fn put_json(&mut self, path: String, v: &Value) {
        self.put(path, pretty(v));
    }
    /// Want the device's `path` gone (a no-op if it does not have one).
    fn drop_local(&mut self, path: &str) {
        if self.local.contains_key(path) && !self.write.contains_key(path) {
            self.delete.insert(path.to_string());
        }
    }
    fn finish(self) -> Merged {
        let delete = self.delete.into_iter().filter(|p| !self.write.contains_key(p)).collect();
        Merged { write: self.write, delete }
    }
}

fn pretty(v: &Value) -> Vec<u8> {
    (serde_json::to_string_pretty(v).unwrap_or_default() + "\n").into_bytes()
}

fn same_json(a: &[u8], b: &[u8]) -> bool {
    match (serde_json::from_slice::<Value>(a), serde_json::from_slice::<Value>(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn str_of<'v>(v: &'v Value, key: &str) -> &'v str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// A JSON object with format `format`, or `None`.
fn object_of(bytes: &[u8], format: &str) -> Option<Value> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    (v.is_object() && str_of(&v, "format") == format).then_some(v)
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

// ── fallback: whole files ────────────────────────────────────────────────────

/// Whole-file union: a file on one side only is kept or taken; on both, the
/// backup's copy wins, as a restore always has. `config` merges the reading
/// history inside `config.json`.
fn other(out: &mut Out, l: &[(&str, &[u8])], i: &[(&str, &[u8])], config: bool) {
    let lm: BTreeMap<&str, &[u8]> = l.iter().copied().collect();
    for (p, ib) in i {
        match lm.get(p) {
            None => out.put(p.to_string(), ib.to_vec()),
            Some(lb) => {
                if config && p.ends_with("/config.json") {
                    if let Some(v) = config_json(lb, ib) {
                        out.put_json(p.to_string(), &v);
                        continue;
                    }
                }
                out.put(p.to_string(), ib.to_vec());
            }
        }
    }
}

/// The backup's settings, with both reading histories combined: the backup's
/// first, then the device's entries it lacks, deduplicated by book and chapter
/// and capped like the app caps it.
fn config_json(local: &[u8], incoming: &[u8]) -> Option<Value> {
    let lv: Value = serde_json::from_slice(local).ok()?;
    let mut iv: Value = serde_json::from_slice(incoming).ok()?;
    let lh = lv.get("history").and_then(Value::as_array).cloned().unwrap_or_default();
    let ih = iv.get("history").and_then(Value::as_array).cloned().unwrap_or_default();
    let key =
        |e: &Value| format!("{}|{}", str_of(e, "book"), e.get("chapter").map(Value::to_string).unwrap_or_default());
    let mut seen = BTreeSet::new();
    let mut hist = Vec::new();
    for e in ih.iter().chain(lh.iter()) {
        if seen.insert(key(e)) {
            hist.push(e.clone());
        }
    }
    hist.truncate(crate::config::HISTORY_CAP);
    if let Some(o) = iv.as_object_mut() {
        if !hist.is_empty() || o.contains_key("history") {
            o.insert("history".to_string(), Value::Array(hist));
        }
    }
    Some(iv)
}

/// Files of a known kind that did not parse go through here: kept where they
/// are, never written over a readable file, taken from the backup only into an
/// empty place.
fn unparsed(out: &mut Out, local_paths: &BTreeSet<&str>, path: &str, bytes: &[u8]) {
    if !local_paths.contains(path) {
        out.put(path.to_string(), bytes.to_vec());
    }
}

// ── notes ────────────────────────────────────────────────────────────────────

struct NoteItem<'a> {
    path: &'a str,
    bytes: &'a [u8],
    note: crate::usernote::UserNote,
    repr: crate::usernote::NoteRepr,
}

fn notes(out: &mut Out, records: &BTreeMap<String, String>, l: &[(&str, &[u8])], i: &[(&str, &[u8])]) {
    let local_paths: BTreeSet<&str> = l.iter().map(|(p, _)| *p).collect();
    let mut by_id: BTreeMap<String, (Vec<NoteItem>, Vec<NoteItem>)> = BTreeMap::new();
    for (side, list) in [(0, l), (1, i)] {
        for (p, b) in list {
            match crate::usernote::parse(b) {
                Ok((note, repr)) => {
                    let e = by_id.entry(note.id.clone()).or_default();
                    let item = NoteItem { path: p, bytes: b, note, repr };
                    if side == 0 {
                        e.0.push(item)
                    } else {
                        e.1.push(item)
                    }
                }
                Err(_) if side == 1 => unparsed(out, &local_paths, p, b),
                Err(_) => {}
            }
        }
    }
    for (id, (ls, is)) in by_id {
        let newest = |v: &[NoteItem]| {
            v.iter().enumerate().max_by(|a, b| a.1.note.updated.cmp(&b.1.note.updated)).map(|(k, _)| k)
        };
        let lw = newest(&ls);
        let iw = newest(&is);
        let take_incoming = match (lw, iw) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(a), Some(b)) => is[b].note.updated > ls[a].note.updated,
        };
        let winner = if take_incoming { &is[iw.unwrap()] } else { &ls[lw.unwrap()] };
        if tomb::kills(records, &tomb::note(&id), &winner.note.updated) {
            for n in &ls {
                out.drop_local(n.path);
            }
            continue;
        }
        let keep = if take_incoming {
            let path = format!("notes/{}", crate::usernote::note_file_name(&winner.note.vref, &id));
            if winner.repr.format == crate::usernote::FORMAT && winner.path == path {
                out.put(path.clone(), winner.bytes.to_vec());
            } else {
                // A v1 note, or one at a stray path: written as v2 where it belongs.
                let mut v: Value = serde_json::from_slice(winner.bytes).unwrap_or(Value::Null);
                if let Some(o) = v.as_object_mut() {
                    o.insert("format".into(), Value::from(crate::usernote::FORMAT));
                    o.insert("id".into(), Value::from(id.as_str()));
                }
                out.put_json(path.clone(), &v);
            }
            path
        } else {
            winner.path.to_string()
        };
        for n in &ls {
            if n.path != keep {
                out.drop_local(n.path);
            }
        }
    }
}

// ── tags, weaves, threads: named objects with an id ──────────────────────────

/// How one kind of named object is merged.
struct Named {
    dir: &'static str,
    format: &'static str,
    fallback_slug: &'static str,
    record: fn(&str) -> String,
    /// The array whose elements are a union, and how an element is keyed.
    /// `None`: the newer object wins whole.
    union: Option<(&'static str, KeyOf)>,
    /// For tags: an element removed is recorded, and the record checked
    /// against the element's `added`.
    element_record: Option<fn(&str, &str) -> String>,
}

const NAMED_TAG: Named = Named {
    dir: "tags",
    format: "overlay-tag-v1",
    fallback_slug: "tag",
    record: tomb::tag,
    union: Some(("members", |m| m.get("target").and_then(tomb::member_target_json))),
    element_record: Some(tomb::tag_member),
};
const NAMED_WEAVE: Named = Named {
    dir: "weaves",
    format: "overlay-weave-v2",
    fallback_slug: "weave",
    record: tomb::weave,
    union: Some(("links", link_key)),
    element_record: None,
};
const NAMED_THREAD: Named = Named {
    dir: "threads",
    format: "overlay-thread-v1",
    fallback_slug: "thread",
    record: tomb::thread,
    union: None,
    element_record: None,
};

/// An undirected link: its two ends in order, with their spans, and its label.
fn link_key(l: &Value) -> Option<String> {
    let (a, b) = (str_of(l, "a"), str_of(l, "b"));
    let (sa, sb) = (l.get("spanA").cloned().unwrap_or(Value::Null), l.get("spanB").cloned().unwrap_or(Value::Null));
    let ((a, sa), (b, sb)) = if a <= b { ((a, sa), (b, sb)) } else { ((b, sb), (a, sa)) };
    if a.is_empty() || b.is_empty() {
        return None;
    }
    Some(format!("{a}|{sa}|{b}|{sb}|{}", str_of(l, "label")))
}

struct Obj<'a> {
    path: &'a str,
    bytes: &'a [u8],
    v: Value,
}

impl Obj<'_> {
    fn identity(&self) -> String {
        tomb::identity(self.v.get("id").and_then(Value::as_str), str_of(&self.v, "name"))
    }
    fn name_identity(&self) -> String {
        tomb::identity(None, str_of(&self.v, "name"))
    }
    fn updated(&self) -> &str {
        str_of(&self.v, "updated")
    }
}

fn named(out: &mut Out, records: &BTreeMap<String, String>, l: &[(&str, &[u8])], i: &[(&str, &[u8])], k: &Named) {
    let local_paths: BTreeSet<&str> = l.iter().map(|(p, _)| *p).collect();
    let mut locals: Vec<Obj> = Vec::new();
    for (p, b) in l {
        if let Some(v) = object_of(b, k.format) {
            locals.push(Obj { path: p, bytes: b, v });
        }
    }
    let mut incomings: Vec<Obj> = Vec::new();
    for (p, b) in i {
        match object_of(b, k.format) {
            Some(v) => incomings.push(Obj { path: p, bytes: b, v }),
            None => unparsed(out, &local_paths, p, b),
        }
    }
    // Pair each incoming object with a local one: by identity, else by the file
    // it would occupy (the same name, so the same thing to a reader).
    let mut pairs: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let mut taken = vec![false; locals.len()];
    for (ii, io) in incomings.iter().enumerate() {
        let id = io.identity();
        let found = locals
            .iter()
            .enumerate()
            .find(|(li, lo)| !taken[*li] && lo.identity() == id)
            .or_else(|| {
                locals.iter().enumerate().find(|(li, lo)| !taken[*li] && file_name(lo.path) == file_name(io.path))
            })
            .map(|(li, _)| li);
        if let Some(li) = found {
            taken[li] = true;
        }
        pairs.push((found, Some(ii)));
    }
    for (li, t) in taken.iter().enumerate() {
        if !t {
            pairs.push((Some(li), None));
        }
    }

    for (li, ii) in pairs {
        let lo = li.map(|x| &locals[x]);
        let io = ii.map(|x| &incomings[x]);
        let take_incoming = match (lo, io) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(a), Some(b)) => b.updated() > a.updated(),
        };
        let base = if take_incoming { io.unwrap() } else { lo.unwrap() };
        // Deleted, under its id or (written before ids) its name?
        let ids: BTreeSet<String> =
            [lo, io].into_iter().flatten().flat_map(|o| [o.identity(), o.name_identity()]).collect();
        if ids.iter().any(|id| tomb::kills(records, &(k.record)(id), base.updated())) {
            if let Some(lo) = lo {
                out.drop_local(lo.path);
            }
            continue;
        }
        let mut v = base.v.clone();
        let mut changed = take_incoming;
        if let Some((field, key_of)) = k.union {
            let identity = base.identity();
            let other = if take_incoming { lo } else { io };
            let mut seen = BTreeSet::new();
            let mut elems: Vec<Value> = Vec::new();
            let from_base = base.v.get(field).and_then(Value::as_array).cloned().unwrap_or_default();
            let from_other = other.and_then(|o| o.v.get(field).and_then(Value::as_array).cloned()).unwrap_or_default();
            for e in from_base.iter().chain(from_other.iter()) {
                let Some(key) = key_of(e) else {
                    elems.push(e.clone());
                    continue;
                };
                if !seen.insert(key.clone()) {
                    continue;
                }
                if let Some(rec) = k.element_record {
                    if tomb::kills(records, &rec(&identity, &key), str_of(e, "added")) {
                        continue;
                    }
                }
                elems.push(e.clone());
            }
            if base.v.get(field).and_then(Value::as_array) != Some(&elems) {
                changed = true;
            }
            if let Some(o) = v.as_object_mut() {
                o.insert(field.to_string(), Value::Array(elems));
            }
        }
        let path = if take_incoming {
            // The backup's name for it, placed as this build would name the file.
            format!("{}/{}.json", k.dir, crate::store::slug(str_of(&v, "name"), k.fallback_slug))
        } else {
            lo.unwrap().path.to_string()
        };
        if changed {
            if take_incoming && base.v == v && dir_of(base.path) == k.dir && file_name(base.path) == file_name(&path) {
                out.put(path.clone(), base.bytes.to_vec());
            } else {
                out.put_json(path.clone(), &v);
            }
        }
        if let Some(lo) = lo {
            if lo.path != path {
                out.drop_local(lo.path);
            }
        }
    }
}

/// Suggested weaves: whole files, the device's copy when both have one, and a
/// rejected (or approved) suggestion stays gone.
fn suggested(out: &mut Out, records: &BTreeMap<String, String>, l: &[(&str, &[u8])], i: &[(&str, &[u8])]) {
    for (p, _) in l {
        if records.contains_key(&tomb::suggested(file_name(p))) {
            out.drop_local(p);
        }
    }
    let lm: BTreeSet<&str> = l.iter().map(|(p, _)| *p).collect();
    for (p, b) in i {
        if !lm.contains(p) && !records.contains_key(&tomb::suggested(file_name(p))) {
            out.put(p.to_string(), b.to_vec());
        }
    }
}

// ── memory cards ─────────────────────────────────────────────────────────────

/// When a card last saw any activity: its latest review, else its creation.
fn card_activity(v: &Value) -> String {
    let mut latest = str_of(v, "created").to_string();
    for r in v.get("reviews").and_then(Value::as_array).into_iter().flatten() {
        let at = str_of(r, "at");
        if at > latest.as_str() {
            latest = at.to_string();
        }
    }
    latest
}

fn memory(out: &mut Out, records: &BTreeMap<String, String>, l: &[(&str, &[u8])], i: &[(&str, &[u8])]) {
    const FORMAT: &str = "overlay-memory-v1";
    let local_paths: BTreeSet<&str> = l.iter().map(|(p, _)| *p).collect();
    let lm: BTreeMap<&str, (&[u8], Option<Value>)> = l.iter().map(|(p, b)| (*p, (*b, object_of(b, FORMAT)))).collect();
    let im: BTreeMap<&str, (&[u8], Option<Value>)> = i.iter().map(|(p, b)| (*p, (*b, object_of(b, FORMAT)))).collect();
    let paths: BTreeSet<&str> = lm.keys().chain(im.keys()).copied().collect();
    for p in paths {
        let lv = lm.get(p).and_then(|x| x.1.as_ref());
        let iv = im.get(p).and_then(|x| x.1.as_ref());
        if iv.is_none() {
            if let Some((b, None)) = im.get(p) {
                unparsed(out, &local_paths, p, b);
            }
        }
        let reviews = |v: &Value| v.get("reviews").and_then(Value::as_array).map_or(0, Vec::len);
        let take_incoming = match (lv, iv) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(a), Some(b)) => (card_activity(b), reviews(b)) > (card_activity(a), reviews(a)),
        };
        let Some(winner) = (if take_incoming { iv } else { lv }) else { continue };
        if tomb::kills(records, &tomb::card(file_name(p)), &card_activity(winner)) {
            out.drop_local(p);
            continue;
        }
        if take_incoming {
            out.put(p.to_string(), im[p].0.to_vec());
        }
    }
}

// ── plans and devotional runs: done days are a union ─────────────────────────

fn runs(
    out: &mut Out,
    records: &BTreeMap<String, String>,
    l: &[(&str, &[u8])],
    i: &[(&str, &[u8])],
    format: &str,
    record: fn(&str) -> String,
    done_on: &str,
) {
    let local_paths: BTreeSet<&str> = l.iter().map(|(p, _)| *p).collect();
    let lm: BTreeMap<&str, &[u8]> = l.iter().copied().collect();
    let im: BTreeMap<&str, &[u8]> = i.iter().copied().collect();
    let paths: BTreeSet<&str> = lm.keys().chain(im.keys()).copied().collect();
    for p in paths {
        let lv = lm.get(p).and_then(|b| object_of(b, format));
        let iv = im.get(p).and_then(|b| object_of(b, format));
        match (lv, iv) {
            (None, None) => {
                // Neither side is a run this build reads — a devotional booklet,
                // say. The device's copy stays; the backup's fills a gap.
                if let Some(b) = im.get(p) {
                    unparsed(out, &local_paths, p, b);
                }
            }
            (lv, iv) => {
                let base = lv.clone().or_else(|| iv.clone()).unwrap();
                if tomb::kills(records, &record(str_of(&base, "id")), str_of(&base, "started")) {
                    out.drop_local(p);
                    continue;
                }
                let (Some(lv), Some(iv)) = (lv, iv) else {
                    if !lm.contains_key(p) {
                        out.put(p.to_string(), im[p].to_vec());
                    }
                    continue;
                };
                let mut v = lv.clone();
                let mut days: Vec<u64> = [&lv, &iv]
                    .iter()
                    .flat_map(|x| x.get("done").and_then(Value::as_array).cloned().unwrap_or_default())
                    .filter_map(|d| d.as_u64())
                    .collect();
                days.sort_unstable();
                days.dedup();
                let o = v.as_object_mut().unwrap();
                o.insert("done".into(), Value::from(days));
                if !done_on.is_empty() {
                    let mut m: Map<String, Value> = Map::new();
                    for x in [&iv, &lv] {
                        for (day, date) in x.get(done_on).and_then(Value::as_object).cloned().unwrap_or_default() {
                            let keep_earlier = m
                                .get(&day)
                                .and_then(Value::as_str)
                                .is_some_and(|have| have <= date.as_str().unwrap_or(""));
                            if !keep_earlier {
                                m.insert(day, date);
                            }
                        }
                    }
                    if !m.is_empty() {
                        o.insert(done_on.into(), Value::Object(m));
                    }
                    let mut swept: Map<String, Value> = Map::new();
                    for x in [&lv, &iv] {
                        for (key, arr) in x.get("swept").and_then(Value::as_object).cloned().unwrap_or_default() {
                            let mut all: Vec<u64> = swept
                                .get(&key)
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default()
                                .iter()
                                .filter_map(Value::as_u64)
                                .collect();
                            all.extend(arr.as_array().cloned().unwrap_or_default().iter().filter_map(Value::as_u64));
                            all.sort_unstable();
                            all.dedup();
                            swept.insert(key, Value::from(all));
                        }
                    }
                    if !swept.is_empty() {
                        o.insert("swept".into(), Value::Object(swept));
                    }
                } else {
                    let latest =
                        [str_of(&lv, "lastDone"), str_of(&iv, "lastDone")].into_iter().max().unwrap_or("").to_string();
                    if !latest.is_empty() {
                        o.insert("lastDone".into(), Value::from(latest));
                    }
                }
                out.put_json(p.to_string(), &v);
            }
        }
    }
}

// ── reading ──────────────────────────────────────────────────────────────────

fn reading(out: &mut Out, records: &BTreeMap<String, String>, l: &[(&str, &[u8])], i: &[(&str, &[u8])]) {
    let fmt = crate::reading::FORMAT;
    let local_paths: BTreeSet<&str> = l.iter().map(|(p, _)| *p).collect();
    let lm: BTreeMap<&str, &[u8]> = l.iter().copied().collect();
    let im: BTreeMap<&str, &[u8]> = i.iter().copied().collect();
    let paths: BTreeSet<&str> = lm.keys().chain(im.keys()).copied().collect();
    for p in paths {
        let lv: Option<Value> = lm.get(p).and_then(|b| serde_json::from_slice(b).ok());
        let iv: Option<Value> = im.get(p).and_then(|b| serde_json::from_slice(b).ok());
        if file_name(p) == "_since.json" {
            // When the reader started: the earlier of the two.
            match (&lv, &iv) {
                (Some(a), Some(b)) if str_of(b, "since") < str_of(a, "since") && !str_of(b, "since").is_empty() => {
                    out.put(p.to_string(), im[p].to_vec())
                }
                (None, Some(_)) => out.put(p.to_string(), im[p].to_vec()),
                _ => {}
            }
            continue;
        }
        let is_book = |v: &Option<Value>| v.as_ref().is_some_and(|v| str_of(v, "format") == fmt);
        if !is_book(&lv) && !is_book(&iv) {
            if let Some(b) = im.get(p) {
                unparsed(out, &local_paths, p, b);
            }
            continue;
        }
        let base = if is_book(&lv) { lv.clone().unwrap() } else { iv.clone().unwrap() };
        let book = str_of(&base, "book").to_string();
        let mut by_c: BTreeMap<u64, Value> = BTreeMap::new();
        for side in [&lv, &iv] {
            let Some(v) = side.as_ref().filter(|v| str_of(v, "format") == fmt) else { continue };
            for r in v.get("chapters").and_then(Value::as_array).into_iter().flatten() {
                let Some(c) = r.get("c").and_then(Value::as_u64) else { continue };
                let merged = match by_c.remove(&c) {
                    None => r.clone(),
                    Some(have) => merge_chapter(&have, r),
                };
                by_c.insert(c, merged);
            }
        }
        let chapters: Vec<Value> = by_c
            .into_iter()
            .filter(|(c, r)| {
                let seen = [str_of(r, "touched"), str_of(r, "lastRead")].into_iter().max().unwrap_or("");
                !tomb::kills(records, &tomb::reading(&book, *c as u16), seen)
            })
            .map(|(_, r)| r)
            .collect();
        let mut v = base;
        v.as_object_mut().unwrap().insert("chapters".into(), Value::Array(chapters));
        out.put_json(p.to_string(), &v);
    }
}

/// One chapter read on two devices: the furthest point and the longest dwell of
/// the pass under way, the most passes, the latest dates. Other keys come from
/// the record touched later.
fn merge_chapter(a: &Value, b: &Value) -> Value {
    let later = if (str_of(b, "touched"), str_of(b, "lastRead")) > (str_of(a, "touched"), str_of(a, "lastRead")) {
        b
    } else {
        a
    };
    let mut out = later.clone();
    let o = out.as_object_mut().unwrap();
    for k in ["reached", "reads"] {
        let m = a.get(k).and_then(Value::as_u64).unwrap_or(0).max(b.get(k).and_then(Value::as_u64).unwrap_or(0));
        if m > 0 || o.contains_key(k) {
            o.insert(k.into(), Value::from(m));
        }
    }
    let d = a
        .get("dwell")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .max(b.get("dwell").and_then(Value::as_f64).unwrap_or(0.0));
    if d > 0.0 || o.contains_key("dwell") {
        o.insert("dwell".into(), Value::from(d));
    }
    for k in ["lastRead", "touched"] {
        let m = [str_of(a, k), str_of(b, k)].into_iter().max().unwrap_or("");
        if !m.is_empty() {
            o.insert(k.into(), Value::from(m));
        }
    }
    out
}

#[cfg(test)]
mod tests;
