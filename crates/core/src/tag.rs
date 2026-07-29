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

/// A named collection of targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub tok_version: String,
    pub created: String,
    pub members: Vec<TagMember>,
}

/// The on-disk shape. `overlay-tag-v1` files written before highlights were
/// removed (2026-07-29) may still carry `color` and `highlights` keys; serde
/// ignores unknown fields, so those tags load as ordinary tags and the dead keys
/// drop away the next time the tag is written. Nothing about a tag's MEMBERS
/// changed, so no reader loses a tag.
#[derive(Deserialize)]
struct TagRepr {
    format: String,
    name: String,
    tokenization: String,
    created: String,
    #[serde(default)]
    members: Vec<TagMember>,
}

impl Serialize for Tag {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Tag", 5)?;
        st.serialize_field("format", FORMAT)?;
        st.serialize_field("name", &self.name)?;
        st.serialize_field("tokenization", &self.tok_version)?;
        st.serialize_field("created", &self.created)?;
        st.serialize_field("members", &self.members)?;
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
            tok_version: r.tokenization,
            created: r.created,
            members: r.members,
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
            tok_version: tok_version.to_string(),
            created: added.to_string(),
            members: vec![member],
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
        let home = std::env::temp_dir().join(format!("plumbline-tag-{}", std::process::id()));
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
    fn a_tag_file_from_before_highlights_were_removed_still_loads() {
        // Highlights (and tag colour with them) were removed 2026-07-29. Readers
        // upgrading have `overlay-tag-v1` files on disk that still carry both
        // keys, and losing a reader's TAG because it once had a colour would be
        // unforgivable. Serde ignores unknown fields, so the tag loads whole and
        // the dead keys fall away the next time it is written.
        // r##, not r#: the `"#f6e0a0"` hex would close an r#"…"# string early.
        let old = r##"{
          "format":"overlay-tag-v1",
          "name":"Mercy",
          "color":"#f6e0a0",
          "tokenization":"kjv1769-tok2",
          "created":"2026-01-01T00:00:00Z",
          "members":[{"target":{"kind":"verse","ref":"John 3:16"},"added":"2026-01-01T00:00:00Z"}],
          "highlights":[{"startRef":"John 3:16","startTok":0,"endRef":"John 3:16","endTok":5}]
        }"##;
        let t: Tag = serde_json::from_str(old).unwrap();
        assert_eq!(t.name, "Mercy");
        assert_eq!(t.members.len(), 1, "the members are the tag; they must survive");

        // And writing it back drops the dead keys rather than preserving them.
        let round = to_json(&t).unwrap();
        assert!(!round.contains("color"), "colour is gone: {round}");
        assert!(!round.contains("highlights"), "highlights are gone: {round}");
        let again: Tag = serde_json::from_str(&round).unwrap();
        assert_eq!(again, t);
    }
}
