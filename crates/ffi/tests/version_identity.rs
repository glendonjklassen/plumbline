//! One release, one version number.
//!
//! Two things name the build a reader sees. `plumbline_version()` hands the
//! shell `CARGO_PKG_VERSION` — the workspace version, shown under About as
//! `engine ` + that number — while the web shell's `PLUMBLINE_VERSION` is
//! derived from the release tag. They have drifted twice: the manifests sat at
//! 0.1.0 through every 0.x release, so About read `engine 0.1.0` on a 0.3.x
//! build; and the release workflow once stripped the tag's leading `v` for one
//! consumer and not another, so one release called itself both "v1.0.0" and
//! "1.0.0".
//!
//! These tests pin what a build can check with no tag in hand: the manifests
//! agree with the engine, and the workflow derives ONE displayed version. The
//! other half — tag vs. manifests — is unknowable here, so the workflow's
//! `version` job asserts it at release time and the last two tests pin that
//! guard's wiring against quiet removal.

use std::fs;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let p = repo().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

/// The value of the first `version = "…"` anchored at column 0 — TOML's
/// `[workspace.package]` version. Every dependency version in the root manifest
/// is inline (`serde = { version = … }`), so none of them anchor there.
fn manifest_version(toml: &str) -> &str {
    toml.lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|r| r.split('"').next())
        .expect("no `version = \"…\"` at column 0 in the root Cargo.toml")
}

/// The workflow's jobs as (name, body) pairs: a job starts at a two-space
/// indented `key:` line after `jobs:` and runs to the next one. Enough structure
/// for the checks below without pulling a YAML parser into the tree.
fn jobs(yml: &str) -> Vec<(&str, String)> {
    let body = &yml[yml.find("\njobs:\n").expect("release.yml has no `jobs:`") + 1..];
    let mut out: Vec<(&str, String)> = Vec::new();
    for line in body.lines() {
        let key = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');
        if key {
            out.push((line.trim().trim_end_matches(':'), String::new()));
        } else if let Some(last) = out.last_mut() {
            last.1.push_str(line);
            last.1.push('\n');
        }
    }
    out
}

/// A job's steps, split on the six-space `- ` that begins each one.
fn steps(job: &str) -> Vec<&str> {
    job.split("\n      - ").collect()
}

#[test]
fn the_engine_reports_the_workspace_version() {
    let root = read("Cargo.toml");
    assert_eq!(
        manifest_version(&root),
        env!("CARGO_PKG_VERSION"),
        "the root Cargo.toml version and plumbline-ffi's disagree — `plumbline_version()` \
         is what About prints as `engine …`"
    );
    // The inheritance is the whole point: a version pinned in the crate would
    // let the workspace bump without the number the shells actually display.
    assert!(
        read("crates/ffi/Cargo.toml").contains("version.workspace = true"),
        "crates/ffi no longer inherits the workspace version, so bumping the workspace \
         no longer changes what `plumbline_version()` reports"
    );
}

#[test]
fn the_web_package_version_matches_the_engine() {
    let pkg: serde_json::Value = serde_json::from_str(&read("apps/web/package.json")).unwrap();
    let web = pkg["version"].as_str().expect("apps/web/package.json has no version");
    assert_eq!(
        web,
        env!("CARGO_PKG_VERSION"),
        "apps/web/package.json says {web} and the engine says {} — the release workflow \
         checks the tag against BOTH, so this drift stops the next release",
        env!("CARGO_PKG_VERSION")
    );
}

#[test]
fn the_shell_displays_one_derived_version() {
    let yml = read(".github/workflows/release.yml");

    // Every consumer of a displayed version reads the one job output. This is
    // the exact bug: `PLUMBLINE_VERSION: ${{ github.ref_name }}` kept the tag's
    // `v` while a second derivation dropped it.
    let key = "PLUMBLINE_VERSION:";
    let want = "needs.version.outputs.name";
    let uses: Vec<&str> = yml.lines().filter(|l| l.contains(key)).collect();
    assert!(!uses.is_empty(), "release.yml no longer sets {key} at all");
    for line in uses {
        assert!(
            line.contains(want),
            "release.yml feeds {key} from something other than {want}, so a future second \
             consumer can disagree about the same release again: {}",
            line.trim()
        );
    }

    // ...and there is exactly one place that turns the tag into that string.
    // Two derivations is how the last one drifted.
    let strips: Vec<&str> = yml.lines().filter(|l| !l.trim_start().starts_with('#') && l.contains("#v}")).collect();
    assert_eq!(
        strips.len(),
        1,
        "the tag's leading 'v' is stripped in {} places in release.yml — derive the \
         displayed version once, in the `version` job: {strips:?}",
        strips.len()
    );

    // An expression against a job that is not a dependency resolves to the empty
    // string, which would ship "Plumbline " with no number at all.
    for (name, body) in jobs(&yml) {
        if !body.contains("needs.version.outputs") {
            continue;
        }
        let needs = body
            .lines()
            .find(|l| l.trim_start().starts_with("needs:"))
            .unwrap_or_else(|| panic!("job `{name}` reads needs.version.outputs but declares no `needs:`"));
        assert!(
            needs.contains("version"),
            "job `{name}` reads needs.version.outputs without needing `version`, so the \
             expression resolves to nothing and About shows a blank version: {}",
            needs.trim()
        );
    }
}

/// Every `d.d.d` in `s`, with any leading `v` dropped.
fn version_literals(s: &str) -> Vec<String> {
    let b: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() || (i > 0 && (b[i - 1].is_ascii_digit() || b[i - 1] == '.')) {
            i += 1;
            continue;
        }
        let start = i;
        let mut dots = 0;
        while i < b.len() && (b[i].is_ascii_digit() || (b[i] == '.' && dots < 2)) {
            if b[i] == '.' {
                // A trailing dot ends the run — "1.0." is prose, not a version.
                if i + 1 >= b.len() || !b[i + 1].is_ascii_digit() {
                    break;
                }
                dots += 1;
            }
            i += 1;
        }
        if dots == 2 {
            out.push(b[start..i].iter().collect());
        }
    }
    out
}

#[test]
fn the_readme_names_no_version_but_the_one_it_ships() {
    // The README is the download page's instructions. It told readers to verify
    // `plumbline-v1.0.0-android.apk` for a year in which no such release existed
    // — the version-identity work above bumped three manifests and left the one
    // file a sideloader actually reads. The durable fix is that the sideload
    // block names no version at all (`releases/latest`, `plumbline-v*`), so this
    // test's real job is to keep a literal from creeping back in: any version in
    // the README has to be the one this tree builds.
    let want = env!("CARGO_PKG_VERSION");
    let readme = read("README.md");
    for (n, line) in readme.lines().enumerate() {
        for found in version_literals(line) {
            assert_eq!(
                found,
                want,
                "README.md:{} names version {found} but this tree builds {want} — a reader \
                 following these steps downloads or hashes the wrong file. Prefer no literal \
                 at all (`releases/latest`, `plumbline-v*-android.apk`): {}",
                n + 1,
                line.trim()
            );
        }
    }
}

#[test]
fn the_release_workflow_checks_the_tag_against_the_manifests() {
    let yml = read(".github/workflows/release.yml");
    let (_, version_job) = jobs(&yml)
        .into_iter()
        .find(|(n, _)| *n == "version")
        .expect("release.yml has no `version` job to derive the one displayed version");
    let guard = steps(&version_job)
        .into_iter()
        .find(|s| s.contains("id: v"))
        .expect("the `version` job no longer has the `id: v` derivation step")
        .to_string();

    // About prints the engine version from Cargo.toml, not from the tag, so a
    // tag off an unbumped tree ships "Plumbline 1.1.0 · engine 1.0.0".
    for want in ["Cargo.toml", "apps/web/package.json", "exit 1"] {
        assert!(
            guard.contains(want),
            "the `version` job no longer mentions {want}: nothing stops a tag from shipping \
             a version the engine does not report"
        );
    }
}
