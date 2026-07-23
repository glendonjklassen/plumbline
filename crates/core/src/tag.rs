//! Tags: named collections grouping verses and Strong's concepts under one
//! label (e.g. "Messianic" holding both passages and the concept G5547),
//! regardless of which kind each member is.
//!
//! Unlike a thread, a tag member carries no span or snapshot — just a target (a
//! verse or a concept) plus an optional note and a timestamp. Personal study
//! data: plain JSON, one file per tag under `home/tags`. Ported from overlay
//! `Tag.hs` (read side + membership queries; the writer lands later).

use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::reference::VRef;
use crate::Error;

const FORMAT: &str = "overlay-tag-v1";

/// What a tag member points at: a verse/passage, or a Strong's concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagTarget {
    Verse(VRef),
    /// A Strong's number, e.g. `"H3068"` / `"G2962"`.
    Concept(String),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum TargetRepr {
    Verse {
        #[serde(rename = "ref")]
        ref_key: String,
    },
    Concept {
        strongs: String,
    },
}

impl Serialize for TagTarget {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            TagTarget::Verse(v) => TargetRepr::Verse { ref_key: v.ref_key() },
            TagTarget::Concept(c) => TargetRepr::Concept { strongs: c.clone() },
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for TagTarget {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match TargetRepr::deserialize(d)? {
            TargetRepr::Verse { ref_key } => VRef::parse_ref_key(&ref_key)
                .map(TagTarget::Verse)
                .ok_or_else(|| D::Error::custom(format!("bad target ref: {ref_key}"))),
            TargetRepr::Concept { strongs } => Ok(TagTarget::Concept(strongs)),
        }
    }
}

/// One tag membership: a target with an optional note and a timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagMember {
    pub target: TagTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub added: String,
}

/// A word-precise highlight range that may span verses: from `start`+`start_tok`
/// to `end`+`end_tok` (inclusive token indices under `kjv1769-tok2`). Additive to
/// `overlay-tag-v1` — older readers ignore the `highlights` array and still show
/// whole-verse member washes. Endpoints reuse the frozen refKey; the token
/// offsets follow the same convention as thread/weave spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightRange {
    pub start: VRef,
    pub start_tok: u16,
    pub end: VRef,
    pub end_tok: u16,
    /// Per-range swatch hex; `None` falls back to the owning tag's colour.
    pub color: Option<String>,
    pub note: Option<String>,
    pub added: String,
}

#[derive(Serialize, Deserialize)]
struct HighlightRepr {
    #[serde(rename = "startRef")]
    start_ref: String,
    #[serde(rename = "startTok")]
    start_tok: u16,
    #[serde(rename = "endRef")]
    end_ref: String,
    #[serde(rename = "endTok")]
    end_tok: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    added: String,
}

impl Serialize for HighlightRange {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        HighlightRepr {
            start_ref: self.start.ref_key(),
            start_tok: self.start_tok,
            end_ref: self.end.ref_key(),
            end_tok: self.end_tok,
            color: self.color.clone(),
            note: self.note.clone(),
            added: self.added.clone(),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for HighlightRange {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = HighlightRepr::deserialize(d)?;
        let start = VRef::parse_ref_key(&r.start_ref)
            .ok_or_else(|| D::Error::custom(format!("bad highlight startRef: {}", r.start_ref)))?;
        let end = VRef::parse_ref_key(&r.end_ref)
            .ok_or_else(|| D::Error::custom(format!("bad highlight endRef: {}", r.end_ref)))?;
        Ok(HighlightRange {
            start,
            start_tok: r.start_tok,
            end,
            end_tok: r.end_tok,
            color: r.color,
            note: r.note,
            added: r.added,
        })
    }
}

/// A per-verse wash run produced from highlight ranges: inclusive token indices
/// `[lo, hi]` within this verse, plus the resolved colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightRun {
    pub lo: u16,
    pub hi: u16,
    pub color: String,
}

/// A named collection of targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    /// Chip swatch hex; `None` derives a colour from the name.
    pub color: Option<String>,
    pub tok_version: String,
    pub created: String,
    pub members: Vec<TagMember>,
    /// Word-precise, possibly cross-verse highlight ranges (additive to v1).
    pub highlights: Vec<HighlightRange>,
}

#[derive(Deserialize)]
struct TagRepr {
    format: String,
    name: String,
    #[serde(default)]
    color: Option<String>,
    tokenization: String,
    created: String,
    #[serde(default)]
    members: Vec<TagMember>,
    #[serde(default)]
    highlights: Vec<HighlightRange>,
}

impl Serialize for Tag {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Tag", 7)?;
        st.serialize_field("format", FORMAT)?;
        st.serialize_field("name", &self.name)?;
        st.serialize_field("tokenization", &self.tok_version)?;
        st.serialize_field("created", &self.created)?;
        st.serialize_field("members", &self.members)?;
        if self.highlights.is_empty() {
            st.skip_field("highlights")?;
        } else {
            st.serialize_field("highlights", &self.highlights)?;
        }
        if let Some(c) = &self.color {
            st.serialize_field("color", c)?;
        } else {
            st.skip_field("color")?;
        }
        st.end()
    }
}

impl<'de> Deserialize<'de> for Tag {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = TagRepr::deserialize(d)?;
        if r.format != FORMAT {
            return Err(D::Error::custom(format!("unknown tag format: {}", r.format)));
        }
        Ok(Tag {
            name: r.name,
            color: r.color,
            tok_version: r.tokenization,
            created: r.created,
            members: r.members,
            highlights: r.highlights,
        })
    }
}

/// A tag plus the file it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedTag {
    pub file: PathBuf,
    pub tag: Tag,
}

impl Tag {
    /// Whether `target` is a member of this tag (drives a chip's on/off state).
    pub fn member_of(&self, target: &TagTarget) -> bool {
        self.members.iter().any(|m| &m.target == target)
    }
}

/// Which of the loaded tags hold this target.
pub fn tags_with<'a>(target: &TagTarget, tags: &'a [LoadedTag]) -> Vec<&'a LoadedTag> {
    tags.iter().filter(|lt| lt.tag.member_of(target)).collect()
}

/// Load every `home/tags/*.json`, sorted by (lowercased) name. Ported from
/// `loadTags`.
pub fn load_tags(home: impl AsRef<Path>) -> (Vec<LoadedTag>, Vec<String>) {
    let dir = home.as_ref().join("tags");
    let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();

    let mut loaded = Vec::new();
    let mut errors = Vec::new();
    for path in files {
        match std::fs::read(&path) {
            Err(e) => errors.push(format!("{}: {e}", path.display())),
            Ok(bytes) => match serde_json::from_slice::<Tag>(&bytes) {
                Ok(tag) => loaded.push(LoadedTag { file: path, tag }),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            },
        }
    }
    loaded.sort_by(|a, b| a.tag.name.to_lowercase().cmp(&b.tag.name.to_lowercase()));
    (loaded, errors)
}

/// Serialize a tag to pretty JSON with a trailing newline.
pub fn to_json(tag: &Tag) -> Result<String, Error> {
    serde_json::to_string_pretty(tag).map(|s| s + "\n").map_err(|e| Error::Parse(e.to_string()))
}

/// The file a tag named `name` lives in, under `home/tags`.
pub fn tag_file(home: impl AsRef<Path>, name: &str) -> PathBuf {
    home.as_ref().join("tags").join(format!("{}.json", crate::store::slug(name, "tag")))
}

/// Atomically write a tag to `path`.
pub fn write_tag(path: impl AsRef<Path>, tag: &Tag) -> Result<(), Error> {
    crate::store::write_atomic(path, &to_json(tag)?)
}

/// Add `target` to the tag named `name` (case-insensitive match among
/// `loaded`), creating its file on first use. A target already present is left
/// as-is (no duplicate). Returns the file written. Ported from `addMember`
/// (caller supplies the `added` timestamp).
pub fn add_member(
    home: impl AsRef<Path>,
    loaded: &[LoadedTag],
    name: &str,
    tok_version: &str,
    target: TagTarget,
    note: Option<String>,
    added: &str,
) -> Result<PathBuf, Error> {
    let member = TagMember { target: target.clone(), note, added: added.to_string() };
    let wanted = name.trim().to_lowercase();
    if let Some(lt) = loaded.iter().find(|lt| lt.tag.name.to_lowercase() == wanted) {
        if lt.tag.member_of(&target) {
            return Ok(lt.file.clone());
        }
        let mut tag = lt.tag.clone();
        tag.members.push(member);
        write_tag(&lt.file, &tag)?;
        Ok(lt.file.clone())
    } else {
        let path = tag_file(&home, name);
        if path.exists() {
            return Err(Error::Corpus(format!(
                "{} exists but could not be read — refusing to overwrite",
                path.display()
            )));
        }
        let tag = Tag {
            name: name.trim().to_string(),
            color: None,
            tok_version: tok_version.to_string(),
            created: added.to_string(),
            members: vec![member],
            highlights: Vec::new(),
        };
        write_tag(&path, &tag)?;
        Ok(path)
    }
}

/// Rewrite a tag's file without `target`.
pub fn remove_member(lt: &LoadedTag, target: &TagTarget) -> Result<(), Error> {
    let mut tag = lt.tag.clone();
    tag.members.retain(|m| &m.target != target);
    write_tag(&lt.file, &tag)
}

/// Set (or clear, with `None`) the swatch colour of the tag named `name`
/// (case-insensitive). Drives the highlighting feature (Tier 0 #4): a verse's
/// wash is the colour of a colour-bearing tag it belongs to. Errors if no such
/// tag is loaded.
pub fn set_color(loaded: &[LoadedTag], name: &str, color: Option<&str>) -> Result<(), Error> {
    let wanted = name.trim().to_lowercase();
    match loaded.iter().find(|lt| lt.tag.name.to_lowercase() == wanted) {
        Some(lt) => {
            let mut tag = lt.tag.clone();
            tag.color = color.map(str::to_string);
            write_tag(&lt.file, &tag)
        }
        None => Err(Error::Corpus(format!("no tag named {name}"))),
    }
}

/// The highlight colour for a verse: the swatch of the first colour-bearing tag
/// (in load order) that holds it, or `None`. "The tags browser doubles as the
/// highlight browser" — any coloured tag paints a wash behind its verses.
pub fn verse_color<'a>(tags: &'a [LoadedTag], vref: &VRef) -> Option<&'a str> {
    let target = TagTarget::Verse(vref.clone());
    tags.iter().find_map(|lt| {
        lt.tag.color.as_deref().filter(|_| lt.tag.member_of(&target))
    })
}

/// Append a word-precise highlight `range` to the tag named `name`
/// (case-insensitive), creating its file on first use. An identical range (same
/// endpoints) is left as-is (no duplicate). On create the tag takes the range's
/// colour, so it shows as a colour-bearing tone in the browser. The token
/// offsets are only meaningful under `tok_version` (`kjv1769-tok2`).
pub fn add_highlight(
    home: impl AsRef<Path>,
    loaded: &[LoadedTag],
    name: &str,
    tok_version: &str,
    range: HighlightRange,
    added: &str,
) -> Result<PathBuf, Error> {
    let same = |h: &HighlightRange| {
        h.start == range.start
            && h.start_tok == range.start_tok
            && h.end == range.end
            && h.end_tok == range.end_tok
    };
    let wanted = name.trim().to_lowercase();
    if let Some(lt) = loaded.iter().find(|lt| lt.tag.name.to_lowercase() == wanted) {
        if lt.tag.highlights.iter().any(same) {
            return Ok(lt.file.clone());
        }
        let mut tag = lt.tag.clone();
        tag.highlights.push(range);
        write_tag(&lt.file, &tag)?;
        Ok(lt.file.clone())
    } else {
        let path = tag_file(&home, name);
        if path.exists() {
            return Err(Error::Corpus(format!(
                "{} exists but could not be read — refusing to overwrite",
                path.display()
            )));
        }
        let tag = Tag {
            name: name.trim().to_string(),
            color: range.color.clone(),
            tok_version: tok_version.to_string(),
            created: added.to_string(),
            members: Vec::new(),
            highlights: vec![range],
        };
        write_tag(&path, &tag)?;
        Ok(path)
    }
}

/// Rewrite a tag's file without the highlight range whose endpoints match.
pub fn remove_highlight(lt: &LoadedTag, range: &HighlightRange) -> Result<(), Error> {
    let mut tag = lt.tag.clone();
    tag.highlights.retain(|h| {
        !(h.start == range.start
            && h.start_tok == range.start_tok
            && h.end == range.end
            && h.end_tok == range.end_tok)
    });
    write_tag(&lt.file, &tag)
}

/// The word-precise wash runs for a single verse (`verse_len` = its token
/// count), decomposed from every highlight range that covers it: a range fully
/// washes its interior verses and its partial first/last verse by token index.
/// The colour is the range's own, else the owning tag's; ranges with neither are
/// skipped (nothing to paint). Complements [`verse_color`]'s whole-verse washes.
pub fn verse_highlight_runs(tags: &[LoadedTag], vref: &VRef, verse_len: u16) -> Vec<HighlightRun> {
    let rk = vref.reading_key();
    let mut runs = Vec::new();
    for lt in tags {
        for h in &lt.tag.highlights {
            let (s, e) = (h.start.reading_key(), h.end.reading_key());
            if rk < s || rk > e {
                continue;
            }
            let Some(color) = h.color.as_deref().or(lt.tag.color.as_deref()) else {
                continue;
            };
            let lo = if rk == s { h.start_tok } else { 0 };
            let hi = if rk == e { h.end_tok } else { verse_len.saturating_sub(1) };
            if lo <= hi {
                runs.push(HighlightRun { lo, hi, color: color.to_string() });
            }
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"{
      "format":"overlay-tag-v1","name":"Messianic","color":"#8844aa",
      "tokenization":"kjv1769-tok2","created":"2026-07-01T00:00:00Z",
      "members":[
        {"target":{"kind":"verse","ref":"Isa 53:5"},"added":"2026-07-01T00:00:00Z"},
        {"target":{"kind":"concept","strongs":"G5547"},"note":"Christ","added":"2026-07-01T00:00:00Z"}
      ]}"##;

    #[test]
    fn parses_tag_with_both_target_kinds() {
        let t: Tag = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(t.name, "Messianic");
        assert_eq!(t.color.as_deref(), Some("#8844aa"));
        assert_eq!(t.members.len(), 2);
        assert_eq!(t.members[0].target, TagTarget::Verse(VRef::new("Isa", 53, 5)));
        assert_eq!(t.members[1].target, TagTarget::Concept("G5547".into()));
    }

    #[test]
    fn membership_queries() {
        let t: Tag = serde_json::from_str(SAMPLE).unwrap();
        assert!(t.member_of(&TagTarget::Verse(VRef::new("Isa", 53, 5))));
        assert!(!t.member_of(&TagTarget::Verse(VRef::new("Isa", 53, 6))));
        let lt = LoadedTag { file: "x".into(), tag: t };
        let hits = tags_with(&TagTarget::Concept("G5547".into()), std::slice::from_ref(&lt));
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn roundtrips_through_json() {
        let t: Tag = serde_json::from_str(SAMPLE).unwrap();
        let back: Tag = serde_json::from_str(&to_json(&t).unwrap()).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn add_dedupes_remove_and_reload() {
        let home = std::env::temp_dir().join(format!("pure-tag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let isa = TagTarget::Verse(VRef::new("Isa", 53, 5));
        let concept = TagTarget::Concept("G5547".into());

        let (loaded, _) = load_tags(&home);
        add_member(&home, &loaded, "Messianic", "kjv1769-tok2", isa.clone(), None, "2026-01-01T00:00:00Z").unwrap();
        // Adding the same target again does not duplicate it.
        let (loaded, _) = load_tags(&home);
        add_member(&home, &loaded, "messianic", "kjv1769-tok2", isa.clone(), None, "2026-01-02T00:00:00Z").unwrap();
        // A second, different target joins the same tag.
        let (loaded, _) = load_tags(&home);
        add_member(&home, &loaded, "Messianic", "kjv1769-tok2", concept.clone(), Some("Christ".into()), "2026-01-03T00:00:00Z").unwrap();

        let (loaded, errs) = load_tags(&home);
        assert!(errs.is_empty());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].tag.members.len(), 2);

        // Remove one member and reload.
        remove_member(&loaded[0], &isa).unwrap();
        let (loaded, _) = load_tags(&home);
        assert_eq!(loaded[0].tag.members.len(), 1);
        assert_eq!(loaded[0].tag.members[0].target, concept);

        let _ = std::fs::remove_dir_all(&home);
    }

    const HL_SAMPLE: &str = r##"{
      "format":"overlay-tag-v1","name":"Amber","color":"#d8a24a",
      "tokenization":"kjv1769-tok2","created":"2026-07-01T00:00:00Z",
      "members":[],
      "highlights":[
        {"startRef":"John 3:16","startTok":3,"endRef":"John 3:18","endTok":5,"added":"2026-07-01T00:00:00Z"}
      ]}"##;

    #[test]
    fn parses_and_roundtrips_highlight_range() {
        let t: Tag = serde_json::from_str(HL_SAMPLE).unwrap();
        assert_eq!(t.highlights.len(), 1);
        let h = &t.highlights[0];
        assert_eq!(h.start, VRef::new("John", 3, 16));
        assert_eq!(h.start_tok, 3);
        assert_eq!(h.end, VRef::new("John", 3, 18));
        assert_eq!(h.end_tok, 5);
        let back: Tag = serde_json::from_str(&to_json(&t).unwrap()).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn old_v1_file_without_highlights_defaults_empty() {
        // SAMPLE has no `highlights` array — additive default keeps it loading.
        let t: Tag = serde_json::from_str(SAMPLE).unwrap();
        assert!(t.highlights.is_empty());
    }

    #[test]
    fn highlight_runs_decompose_across_verses() {
        let t: Tag = serde_json::from_str(HL_SAMPLE).unwrap();
        let lt = LoadedTag { file: "x".into(), tag: t };
        let tags = std::slice::from_ref(&lt);
        // start verse (len 9 → last index 8): from start_tok to the end
        assert_eq!(
            verse_highlight_runs(tags, &VRef::new("John", 3, 16), 9),
            vec![HighlightRun { lo: 3, hi: 8, color: "#d8a24a".into() }]
        );
        // interior verse (len 6 → 0..5): whole verse
        assert_eq!(
            verse_highlight_runs(tags, &VRef::new("John", 3, 17), 6),
            vec![HighlightRun { lo: 0, hi: 5, color: "#d8a24a".into() }]
        );
        // end verse: 0..end_tok
        assert_eq!(
            verse_highlight_runs(tags, &VRef::new("John", 3, 18), 20),
            vec![HighlightRun { lo: 0, hi: 5, color: "#d8a24a".into() }]
        );
        // outside the range on either side: nothing
        assert!(verse_highlight_runs(tags, &VRef::new("John", 3, 19), 10).is_empty());
        assert!(verse_highlight_runs(tags, &VRef::new("John", 3, 15), 10).is_empty());
    }

    #[test]
    fn single_verse_highlight_run() {
        let t: Tag = serde_json::from_str(
            r##"{"format":"overlay-tag-v1","name":"A","color":"#111111",
            "tokenization":"kjv1769-tok2","created":"t","members":[],
            "highlights":[{"startRef":"Gen 1:1","startTok":2,"endRef":"Gen 1:1","endTok":4,"added":"t"}]}"##,
        )
        .unwrap();
        let lt = LoadedTag { file: "x".into(), tag: t };
        assert_eq!(
            verse_highlight_runs(std::slice::from_ref(&lt), &VRef::new("Gen", 1, 1), 10),
            vec![HighlightRun { lo: 2, hi: 4, color: "#111111".into() }]
        );
    }

    #[test]
    fn add_and_remove_highlight_roundtrip() {
        let home = std::env::temp_dir().join(format!("pure-hl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let range = HighlightRange {
            start: VRef::new("John", 3, 16),
            start_tok: 1,
            end: VRef::new("John", 3, 17),
            end_tok: 2,
            color: Some("#d8a24a".into()),
            note: None,
            added: "t".into(),
        };
        let (loaded, _) = load_tags(&home);
        add_highlight(&home, &loaded, "Amber", "kjv1769-tok2", range.clone(), "t").unwrap();
        // dedup: the same range again is a no-op (case-insensitive tag match)
        let (loaded, _) = load_tags(&home);
        add_highlight(&home, &loaded, "amber", "kjv1769-tok2", range.clone(), "t").unwrap();
        let (loaded, errs) = load_tags(&home);
        assert!(errs.is_empty());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].tag.highlights.len(), 1);
        assert_eq!(loaded[0].tag.color.as_deref(), Some("#d8a24a"));

        remove_highlight(&loaded[0], &range).unwrap();
        let (loaded, _) = load_tags(&home);
        assert!(loaded[0].tag.highlights.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }
}
