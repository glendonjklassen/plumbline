//! The Android bundled-data marker has to be bumped when the bundled data changes.
//!
//! Android extracts `assets/data` into `filesDir` ONCE, guarded by a marker file
//! (`.data-vN`). An install that already holds the current marker skips the whole
//! extraction — so adding a file to the gradle include list reaches new installs
//! only, and for everyone else the feature that reads it is simply missing, with
//! no error anywhere.
//!
//! v0.39.0 shipped exactly that. `hymnal.json` was added to the include list, the
//! marker stayed at `.data-v2`, and every existing reader opened the hymn tab to
//! "The hymnal has not finished loading yet." The comment beside the marker
//! described this precise failure mode, in those words, and it happened anyway —
//! which is the argument for a test rather than a better comment.
//!
//! So: this test pins the two together. The include list is the input, the marker
//! is what carries it to a device, and changing the first without the second is
//! the bug. Both live in files this test reads, so it fails at `cargo test`
//! rather than on a phone.
//!
//! WHEN THIS FAILS, you changed the bundled data. Bump `.data-vN` in
//! MainActivity.kt, then update `EXPECTED_ASSETS` here to match. Two deliberate
//! edits, which is the point: the second one is the device's only notice.

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The data files the APK bundles, as `build.gradle.kts` lists them. Sorted.
const EXPECTED_ASSETS: &[&str] = &[
    "akjv.akjvb",
    "cross-references.tsv",
    // The v3 addition.
    "hymnal.json",
    "kjv-notes.jsonl",
    "kjv.jsonl",
    // The v4 addition: the German corpus.
    "luther1912.jsonl",
    "strongs.json",
];

/// The marker the CURRENT asset set is paired with.
const EXPECTED_MARKER: &str = ".data-v4";

#[test]
fn bundled_data_marker_is_bumped_for_the_current_asset_set() {
    let gradle = std::fs::read_to_string(repo().join("apps/android/app/build.gradle.kts"))
        .expect("build.gradle.kts is readable");
    let kt = std::fs::read_to_string(repo().join("apps/android/app/src/main/java/dev/plumbline/MainActivity.kt"))
        .expect("MainActivity.kt is readable");

    // The `syncData` block's include(...), which may span lines — it grew a file
    // per line when the German corpus was added, and this test used to look at a
    // single line and simply stop finding it.
    let sync = gradle.split("syncData").nth(1).expect("build.gradle.kts still has a syncData task");
    let open = sync.find("include(").expect("syncData still has an include() naming the bundled data");
    let close = sync[open..].find(')').map(|i| open + i).expect("the include( is closed");
    let listing = &sync[open..close];
    assert!(listing.contains("kjv.jsonl"), "the syncData include() no longer names the corpus:\n{listing}");
    let mut listed: Vec<String> =
        listing.split('"').filter(|p| p.contains('.') && !p.contains('(')).map(str::to_string).collect();
    listed.sort();
    listed.dedup();

    let mut expected: Vec<String> = EXPECTED_ASSETS.iter().map(|s| s.to_string()).collect();
    expected.sort();

    assert_eq!(
        listed, expected,
        "the bundled data set changed.\n\
         Bump the `.data-vN` marker in MainActivity.kt so existing installs \
         re-extract, then update EXPECTED_ASSETS and EXPECTED_MARKER in this test.\n\
         Without the bump the new data reaches NEW INSTALLS ONLY and the feature \
         reading it is missing everywhere else — which is how v0.39.0 shipped a \
         hymnal nobody with the app already could open."
    );

    assert!(
        kt.contains(&format!("File(home, \"{EXPECTED_MARKER}\")")),
        "MainActivity.kt no longer uses the marker {EXPECTED_MARKER} this test is paired with; \
         if the bundled data changed, update EXPECTED_ASSETS too"
    );
}

/// The web ships the same files through the pack, so a data file bundled on one
/// shell and absent on the other is a parity break the manifest would not catch.
#[test]
fn every_bundled_data_file_exists() {
    let data = repo().join("data");
    for name in EXPECTED_ASSETS {
        assert!(data.join(name).exists(), "data/{name} is bundled by gradle but not in data/");
    }
}
