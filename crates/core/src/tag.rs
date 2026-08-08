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
use serde_json::{Map, Value};

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagMember {
    pub target: TagTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub added: String,
    /// Unknown keys on this member, kept — see [`Tag::extra`]. A member is
    /// nested inside the tag file, and a key stripped from it is stripped just
    /// as permanently as one on the tag itself.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A named collection of targets.
#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    pub name: String,
    pub tok_version: String,
    pub created: String,
    /// Stable identity — 32 hex chars, minted once and never derived from the
    /// name (docs/STABLE-IDS.md). `None` for every tag written before ids
    /// existed; [`write_tag`] assigns one on the next save, and an id already
    /// present is never rewritten.
    ///
    /// **The id is the identity; the name is a label.** A rename keeps it, which
    /// is the whole point: the file is `slug(name).json`, so renaming a tag today
    /// leaves a new file and no way to tell it is the same tag the reader had.
    pub id: Option<String>,
    /// UTC stamp of the last mutating save, in the wire format `created` uses.
    /// `None` until this build (or a later one) writes the file.
    ///
    /// This is what makes "which copy is newer" answerable — between a backup
    /// zip and the device it lands on, and between two copies that share an
    /// [`id`](Tag::id). Nothing consumes it yet; it exists now because a
    /// sideloaded APK never auto-updates, so a field absent on the day 1.0 ships
    /// is absent from those devices for ever.
    pub updated: Option<String>,
    pub members: Vec<TagMember>,
    /// Every key in the file this build has never heard of, carried back out
    /// again on save.
    ///
    /// The on-disk formats evolve **additively** (CLAUDE.md §Data formats), and
    /// a sideloaded APK never auto-updates: a build that drops the fields of a
    /// later one drops them for good on that device. So a tag written by a v1.1
    /// and re-saved here comes back whole.
    ///
    /// It holds only keys we have *never heard of*: serde fills it with the
    /// leftovers after the known fields are matched (so a known key can never be
    /// swallowed, and a key a later version promotes to a real field stops
    /// arriving here the moment that field exists), and the two keys highlights
    /// left behind are dropped by name on the way in — retired is not unknown.
    /// Empty for every tag on disk today, and an empty flattened map writes no
    /// key at all, so those files are written exactly as they were.
    pub extra: Map<String, Value>,
}

/// The on-disk shape. `overlay-tag-v1` files written before highlights were
/// removed may still carry `color` and `highlights` keys; those two
/// load as ordinary unknown keys and are then dropped by name (see
/// [`Tag::extra`]), so the dead keys still fall away the next time the tag is
/// written. Nothing about a tag's MEMBERS changed, so no reader loses a tag.
#[derive(Deserialize)]
struct TagRepr {
    format: String,
    name: String,
    tokenization: String,
    created: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    updated: Option<String>,
    #[serde(default)]
    members: Vec<TagMember>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl Serialize for Tag {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // A map, not a struct: serde_json writes the two identically, and only a
        // map can take the unknown keys after the known ones.
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("format", FORMAT)?;
        m.serialize_entry("name", &self.name)?;
        m.serialize_entry("tokenization", &self.tok_version)?;
        m.serialize_entry("created", &self.created)?;
        // Both additive: a tag that has neither writes neither key, so files
        // from before ids existed round-trip byte for byte.
        if let Some(id) = &self.id {
            m.serialize_entry("id", id)?;
        }
        if let Some(updated) = &self.updated {
            m.serialize_entry("updated", updated)?;
        }
        m.serialize_entry("members", &self.members)?;
        for (k, v) in &self.extra {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}

impl<'de> Deserialize<'de> for Tag {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = TagRepr::deserialize(d)?;
        if r.format != FORMAT {
            return Err(D::Error::custom(format!("unknown tag format: {}", r.format)));
        }
        let mut extra = r.extra;
        // Highlights and the tag colour that went with them were removed. We
        // know these two keys and have retired them, so they are dropped rather
        // than preserved for ever.
        extra.remove("color");
        extra.remove("highlights");
        Ok(Tag {
            name: r.name,
            tok_version: r.tokenization,
            created: r.created,
            id: r.id,
            updated: r.updated,
            members: r.members,
            extra,
        })
    }
}

/// A tag plus the file it came from.
#[derive(Debug, Clone, PartialEq)]
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
        Ok(entries) => {
            entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect()
        }
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
    loaded.sort_by_key(|a| a.tag.name.to_lowercase());
    let loaded = crate::store::resolve_duplicate_ids(loaded, |lt| lt.tag.id.as_deref(), |lt| lt.tag.updated.as_deref());
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

/// Atomically write a tag to `path`, stamping it: `updated` becomes `now`, and a
/// tag with no [`id`](Tag::id) is assigned one.
///
/// The stamping lives HERE, in the writer, rather than in each of the callers
/// above — every save of a tag passes through this function, so an id cannot be
/// forgotten by adding a mutation later. `now` is the same UTC stamp the
/// mutation itself carries where the shell sends one (`added`), and the engine's
/// own clock where it does not (see `plumbline-ffi`'s `now_stamp`).
///
/// An id already on the tag is left exactly as it is, whatever its shape: it may
/// have been minted by a later build whose rules we do not know, and identity we
/// rewrite is identity we have destroyed.
pub fn write_tag(path: impl AsRef<Path>, tag: &Tag, now: &str) -> Result<(), Error> {
    let mut stamped = tag.clone();
    stamped.updated = Some(now.to_string());
    if stamped.id.is_none() {
        stamped.id = Some(crate::store::new_id());
    }
    crate::store::write_atomic(path, &to_json(&stamped)?)
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
    let member = TagMember { target: target.clone(), note, added: added.to_string(), extra: Map::new() };
    let wanted = name.trim().to_lowercase();
    if let Some(lt) = loaded.iter().find(|lt| lt.tag.name.to_lowercase() == wanted) {
        if lt.tag.member_of(&target) {
            return Ok(lt.file.clone());
        }
        let mut tag = lt.tag.clone();
        tag.members.push(member);
        write_tag(&lt.file, &tag, added)?;
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
            // Both filled by the writer — a new tag is stamped the same way an
            // edited one is, through one code path.
            id: None,
            updated: None,
            members: vec![member],
            // The one place a new file's provenance can honestly be recorded:
            // its refKeys are being written NOW, in the language the reader is
            // reading. A writer would be the wrong place — it also runs on
            // re-save, where an unstamped older file would gain a confident
            // wrong answer. See `i18n::stamp`.
            extra: crate::i18n::stamped_extra(),
        };
        write_tag(&path, &tag, added)?;
        Ok(path)
    }
}

/// Rewrite a tag's file without `target`. `now` stamps the save (see
/// [`write_tag`]).
pub fn remove_member(lt: &LoadedTag, target: &TagTarget, now: &str) -> Result<(), Error> {
    let mut tag = lt.tag.clone();
    tag.members.retain(|m| &m.target != target);
    write_tag(&lt.file, &tag, now)
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
        add_member(
            &home,
            &loaded,
            "Messianic",
            "kjv1769-tok2",
            concept.clone(),
            Some("Christ".into()),
            "2026-01-03T00:00:00Z",
        )
        .unwrap();

        let (loaded, errs) = load_tags(&home);
        assert!(errs.is_empty());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].tag.members.len(), 2);

        // Remove one member and reload.
        remove_member(&loaded[0], &isa, "2026-07-30T00:00:00Z").unwrap();
        let (loaded, _) = load_tags(&home);
        assert_eq!(loaded[0].tag.members.len(), 1);
        assert_eq!(loaded[0].tag.members[0].target, concept);

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_tag_file_from_before_highlights_were_removed_still_loads() {
        // Highlights (and tag colour with them) were removed. Readers
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

    /// Forward compatibility: the on-disk formats evolve
    /// **additively** (CLAUDE.md §Data formats), and a sideloaded APK never
    /// auto-updates — so a key this build drops is dropped for good on that
    /// device. A tag written by a later build has to come back out whole, on the
    /// tag and on its members alike.
    #[test]
    fn a_tag_keeps_the_keys_of_a_later_build() {
        let home = std::env::temp_dir().join(format!("plumbline-tag-forward-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let path = tag_file(&home, "Messianic");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{
              "format":"overlay-tag-v1","name":"Messianic",
              "tokenization":"kjv1769-tok2","created":"2026-07-01T00:00:00Z",
              "members":[
                {"target":{"kind":"verse","ref":"Isa 53:5"},"added":"2026-07-01T00:00:00Z",
                 "pinned":true,"source":{"by":"reader","at":"2026-08-01"}}
              ],
              "colophon":"9f2c1d","shared":{"with":"study group"},"aliases":["Christ","Messiah"]
            }"#,
        )
        .unwrap();

        // Adding a member rewrites the whole file.
        let (loaded, errs) = load_tags(&home);
        assert!(errs.is_empty(), "{errs:?}");
        add_member(
            &home,
            &loaded,
            "Messianic",
            "kjv1769-tok2",
            TagTarget::Concept("G5547".into()),
            None,
            "2026-09-01T00:00:00Z",
        )
        .unwrap();

        let back: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["members"].as_array().unwrap().len(), 2, "the add itself must land");
        // `colophon`, not `id`: `id` became a real field with stable ids
        // (docs/STABLE-IDS.md), and a key this build knows tests nothing about
        // the keys it doesn't. What `id` does now is asserted separately below.
        assert_eq!(back["colophon"], "9f2c1d", "an unknown scalar was stripped");
        assert_eq!(back["shared"], serde_json::json!({"with":"study group"}), "an unknown object was stripped");
        assert_eq!(back["aliases"], serde_json::json!(["Christ", "Messiah"]), "an unknown array was stripped");
        assert_eq!(back["members"][0]["pinned"], true, "a member's unknown scalar was stripped");
        assert_eq!(
            back["members"][0]["source"],
            serde_json::json!({"by":"reader","at":"2026-08-01"}),
            "a member's unknown object was stripped"
        );
        // And the member this build wrote carries nothing of its own.
        assert_eq!(
            back["members"][1],
            serde_json::json!({"target":{"kind":"concept","strongs":"G5547"},"added":"2026-09-01T00:00:00Z"})
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A tag with nothing unknown in it is written byte for byte as it was before
    /// any of that landed — these files already ship inside backup zips.
    #[test]
    fn a_tag_with_no_unknown_keys_is_written_exactly_as_before() {
        let t: Tag = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(
            to_json(&t).unwrap(),
            r#"{
  "format": "overlay-tag-v1",
  "name": "Messianic",
  "tokenization": "kjv1769-tok2",
  "created": "2026-07-01T00:00:00Z",
  "members": [
    {
      "target": {
        "kind": "verse",
        "ref": "Isa 53:5"
      },
      "added": "2026-07-01T00:00:00Z"
    },
    {
      "target": {
        "kind": "concept",
        "strongs": "G5547"
      },
      "note": "Christ",
      "added": "2026-07-01T00:00:00Z"
    }
  ]
}
"#
        );
    }

    // ── stable ids (docs/STABLE-IDS.md) ──────────────────────────────────────

    fn id_home(tag: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("plumbline-tagid-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        home
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    /// The upgrade case, and the one that has to be right: a tag written before
    /// ids existed gains one the first time this build saves it — and loses
    /// nothing on the way.
    #[test]
    fn a_tag_from_before_ids_gains_one_on_first_save_and_loses_nothing() {
        let home = id_home("upgrade");
        let path = tag_file(&home, "Messianic");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, SAMPLE).unwrap();

        let (loaded, errs) = load_tags(&home);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(loaded[0].tag.id, None, "the file on disk has no id yet");

        add_member(
            &home,
            &loaded,
            "Messianic",
            "kjv1769-tok2",
            TagTarget::Verse(VRef::new("Rom", 1, 3)),
            None,
            "2026-08-01T09:00:00Z",
        )
        .unwrap();

        let back = read_json(&path);
        let id = back["id"].as_str().expect("no id was assigned on save");
        assert_eq!(id.len(), 32, "an id is 32 hex chars: {id}");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "lowercase hex only: {id}");
        assert_eq!(back["updated"], "2026-08-01T09:00:00Z", "updated is the mutation's own stamp");
        // Nothing else moved: created stays put and every member is still there.
        assert_eq!(back["created"], "2026-07-01T00:00:00Z");
        assert_eq!(back["members"].as_array().unwrap().len(), 3);
        assert_eq!(back["name"], "Messianic");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// An id is minted ONCE. A second save moves `updated` and leaves the
    /// identity alone — the whole value of the field is that it doesn't change.
    #[test]
    fn a_second_save_moves_updated_and_keeps_the_id() {
        let home = id_home("stable");
        let target = TagTarget::Verse(VRef::new("Isa", 53, 5));
        let (loaded, _) = load_tags(&home);
        let path =
            add_member(&home, &loaded, "Mercy", "kjv1769-tok2", target.clone(), None, "2026-08-01T00:00:00Z").unwrap();
        let first = read_json(&path);

        let (loaded, _) = load_tags(&home);
        remove_member(&loaded[0], &target, "2026-08-02T00:00:00Z").unwrap();
        let second = read_json(&path);

        assert_eq!(first["id"], second["id"], "the id changed between two saves");
        assert_eq!(first["updated"], "2026-08-01T00:00:00Z");
        assert_eq!(second["updated"], "2026-08-02T00:00:00Z", "updated did not move on a mutating save");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// An id minted by a build whose rules we don't know is left exactly as it
    /// is. Rewriting one — to normalise its shape, say — destroys the identity
    /// it was carrying.
    #[test]
    fn an_id_this_build_did_not_mint_is_left_alone() {
        let home = id_home("foreign");
        let path = tag_file(&home, "Mercy");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"format":"overlay-tag-v1","name":"Mercy","tokenization":"kjv1769-tok2",
                "created":"2026-07-01T00:00:00Z","id":"NOT-32-HEX","members":[]}"#,
        )
        .unwrap();

        let (loaded, _) = load_tags(&home);
        add_member(
            &home,
            &loaded,
            "Mercy",
            "kjv1769-tok2",
            TagTarget::Concept("G26".into()),
            None,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        assert_eq!(read_json(&path)["id"], "NOT-32-HEX");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// The point of an id: it survives the file moving. There is no rename
    /// endpoint yet (nothing in either shell can rename a tag), so this pins the
    /// mechanism a rename will use — write the same tag under a different slug
    /// and it is still the same tag. And the old file is still there, because
    /// only an explicit save may remove one.
    #[test]
    fn a_tag_written_under_a_new_name_keeps_its_id() {
        let home = id_home("rename");
        let (loaded, _) = load_tags(&home);
        let old = add_member(
            &home,
            &loaded,
            "Mercy",
            "kjv1769-tok2",
            TagTarget::Verse(VRef::new("Isa", 53, 5)),
            None,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();

        let (loaded, _) = load_tags(&home);
        let mut renamed = loaded[0].tag.clone();
        renamed.name = "Loving-kindness".to_string();
        let new = tag_file(&home, &renamed.name);
        assert_ne!(new, old, "the slug should change with the name");
        write_tag(&new, &renamed, "2026-08-02T00:00:00Z").unwrap();

        assert_eq!(read_json(&new)["id"], read_json(&old)["id"], "the rename lost the identity");
        assert!(old.exists(), "load-and-save must not delete the old file — that is the reader's data");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Two files, one id — the rename artifact. The newer `updated` is the copy
    /// the reader meant, and **the loader deletes neither file**: a loader that
    /// deleted would turn a bad clock or a half-restored backup into permanent
    /// loss.
    #[test]
    fn duplicate_ids_keep_the_newer_and_load_deletes_nothing() {
        let home = id_home("dup");
        let dir = home.join("tags");
        std::fs::create_dir_all(&dir).unwrap();
        let one = |name: &str, id: &str, updated: &str, members: &str| {
            format!(
                r#"{{"format":"overlay-tag-v1","name":"{name}","tokenization":"kjv1769-tok2",
                    "created":"2026-07-01T00:00:00Z","id":"{id}",
                    "updated":"{updated}","members":[{members}]}}"#
            )
        };
        const A: &str = "aaaaaaaaaaaaaaaabbbbbbbbbbbbbbbb";
        const B: &str = "ccccccccccccccccdddddddddddddddd";
        // Both orderings, deliberately: the loader sorts by name, so a pair whose
        // newer copy sorts FIRST cannot tell "keep the newest" apart from "keep
        // whichever came first". One pair each way can.
        let member = r#"{"target":{"kind":"verse","ref":"Isa 53:5"},"added":"2026-08-02T00:00:00Z"}"#;
        let files = [
            // Pair A — the newer copy sorts LAST.
            ("alms.json", one("Alms", A, "2026-08-01T00:00:00Z", "")),
            ("zeal.json", one("Zeal", A, "2026-08-02T00:00:00Z", member)),
            // Pair B — the newer copy sorts FIRST.
            ("balm.json", one("Balm", B, "2026-08-02T00:00:00Z", member)),
            ("yoke.json", one("Yoke", B, "2026-08-01T00:00:00Z", "")),
        ];
        for (file, body) in &files {
            std::fs::write(dir.join(file), body).unwrap();
        }

        let (loaded, errs) = load_tags(&home);
        assert!(errs.is_empty(), "{errs:?}");
        let names: Vec<&str> = loaded.iter().map(|lt| lt.tag.name.as_str()).collect();
        assert_eq!(names, ["Balm", "Zeal"], "each pair should present as its newer copy alone");
        for lt in &loaded {
            assert_eq!(lt.tag.members.len(), 1, "{} kept the wrong copy's members", lt.tag.name);
        }
        for (file, _) in &files {
            assert!(dir.join(file).exists(), "load deleted {file}");
        }

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Tags with no ids are never confused with each other, however many there
    /// are — the resolution keys on the id, and `None` is not a key.
    #[test]
    fn tags_without_ids_are_all_kept() {
        let home = id_home("noids");
        let dir = home.join("tags");
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["Mercy", "Messianic", "Wisdom"] {
            std::fs::write(
                dir.join(format!("{}.json", crate::store::slug(name, "tag"))),
                format!(
                    r#"{{"format":"overlay-tag-v1","name":"{name}","tokenization":"kjv1769-tok2",
                        "created":"2026-07-01T00:00:00Z","members":[]}}"#
                ),
            )
            .unwrap();
        }
        let (loaded, _) = load_tags(&home);
        assert_eq!(loaded.len(), 3);
        let _ = std::fs::remove_dir_all(&home);
    }
}
