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
    /// Chip swatch hex; `None` derives a colour from the name.
    pub color: Option<String>,
    pub tok_version: String,
    pub created: String,
    pub members: Vec<TagMember>,
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
}

impl Serialize for Tag {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Tag", 6)?;
        st.serialize_field("format", FORMAT)?;
        st.serialize_field("name", &self.name)?;
        st.serialize_field("tokenization", &self.tok_version)?;
        st.serialize_field("created", &self.created)?;
        st.serialize_field("members", &self.members)?;
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
}
