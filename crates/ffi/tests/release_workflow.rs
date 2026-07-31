//! The release path must check what a push checks.
//!
//! CI runs on every push; the release workflow runs on a tag, and a tag can be
//! cut from any commit. Three things had drifted apart between them: the pages
//! job built and deployed the PWA without ever running `npm run check`, so a
//! type error CI would have caught could reach plumblinebible.org; cargo-ndk
//! was installed with a bare `cargo install` while CI pinned it through
//! `taiki-e/install-action`, so the toolchain that cross-compiles the shipped
//! `.so` could differ from the tested one; and there was no `workflow_dispatch`,
//! so the only way to exercise any of this was to cut a tag and delete it again.
//!
//! These tests read `release.yml` and pin the three fixes. The dry-run one is
//! the load-bearing test: a manual run is allowed to build everything and
//! publish nothing, and that promise is spread across five steps in three jobs,
//! so it is exactly the kind of thing a later edit drops one of.

use std::fs;
use std::path::{Path, PathBuf};

const RELEASE: &str = ".github/workflows/release.yml";
const CI: &str = ".github/workflows/ci.yml";

/// What every publishing step in the workflow must be guarded by.
const GUARD: &str = "github.event_name != 'workflow_dispatch'";

/// Steps that reach outside the runner: they create or overwrite a release
/// asset, or they put a bundle on the live domain.
const PUBLISHES: [&str; 5] = [
    "gh release create",
    "gh release upload",
    "actions/configure-pages",
    "actions/upload-pages-artifact",
    "actions/deploy-pages",
];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let p = repo().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

/// The workflow with its comments removed. These tests look for commands and
/// guards, and a comment that talks about one is not one — the header alone
/// names `deploy-pages` and the dispatch guard.
fn code(yml: &str) -> String {
    let kept: Vec<&str> = yml.lines().filter(|l| !l.trim_start().starts_with('#')).collect();
    kept.join("\n")
}

/// The workflow's jobs as (name, body) pairs — same line-based split
/// `version_identity.rs` uses, and for the same reason: enough structure for
/// these checks without pulling a YAML parser into the tree.
fn jobs(yml: &str) -> Vec<(&str, String)> {
    let body = &yml[yml.find("\njobs:\n").expect("no `jobs:` in the workflow") + 1..];
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

/// The `on:` block: from `on:` to the next key at column 0.
fn triggers(yml: &str) -> String {
    let start = yml.find("\non:\n").expect("the workflow has no `on:` block") + 1;
    let rest = &yml[start..];
    let mut out = String::new();
    for (i, line) in rest.lines().enumerate() {
        if i > 0 && !line.starts_with(' ') && !line.is_empty() {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn job(yml: &str, name: &str) -> String {
    jobs(yml).into_iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("release.yml has no `{name}` job")).1
}

/// The type check has to run, and it has to run before anything it could stop:
/// a check after `npm run build` still fails the job, but a check after the
/// deploy has already handed the bundle to readers.
#[test]
fn the_pages_job_type_checks_before_it_builds_or_deploys() {
    let yml = code(&read(RELEASE));
    let pages = job(&yml, "pages");

    let check = pages.find("npm run check").expect(
        "the `pages` job never runs `npm run check` — a tag can ship a type error that CI \
         catches on every push, straight to plumblinebible.org",
    );
    let build = pages.find("npm run build").expect("the `pages` job no longer builds the web bundle at all");
    let deploy = pages
        .find("actions/deploy-pages")
        .expect("the `pages` job no longer deploys — this test is checking the wrong job");
    assert!(
        check < build,
        "`npm run check` runs after `npm run build` in the `pages` job; the check belongs \
         before the bundle it is meant to stop"
    );
    assert!(
        check < deploy,
        "`npm run check` runs after the Pages deploy in the `pages` job, so a type error is \
         reported only once readers already have the build"
    );

    // A check whose failure is swallowed is worse than no check: it reads green.
    let line = pages.lines().find(|l| l.contains("npm run check")).expect("unreachable: found the check above");
    assert!(!line.contains("||"), "the type check's failure is swallowed in the `pages` job: {}", line.trim());
    let step =
        steps(&pages).into_iter().find(|s| s.contains("npm run check")).expect("unreachable: found the check above");
    assert!(
        !step.contains("continue-on-error"),
        "the step that type-checks the web shell is advisory (`continue-on-error`), so a tag \
         still deploys past a type error"
    );
}

/// The release build of the shipped `.so` must use the cargo-ndk CI tested
/// with. `cargo install cargo-ndk --locked` locks this repo's dependency
/// versions, not cargo-ndk's own, so it resolves whatever is newest on the day
/// of the release — a toolchain nothing has run before.
#[test]
fn cargo_ndk_comes_from_the_pinned_action() {
    for wf in [RELEASE, CI] {
        let yml = code(&read(wf));
        assert!(
            !yml.contains("cargo install cargo-ndk"),
            "{wf} installs cargo-ndk with `cargo install`, so the cross-compiler can drift \
             from the one the other workflow pins"
        );
        let mut found = false;
        for (name, body) in jobs(&yml) {
            for step in steps(&body) {
                if !step.contains("cargo-ndk") || step.contains("cargo ndk ") {
                    continue;
                }
                found = true;
                assert!(
                    step.contains("taiki-e/install-action@"),
                    "{wf} job `{name}` installs cargo-ndk without taiki-e/install-action: {}",
                    step.trim()
                );
            }
        }
        assert!(
            found,
            "{wf} no longer installs cargo-ndk at all, yet still cross-compiles the engine \
             for Android"
        );
    }
}

/// A manual run is a dry run. It exists so the release path can be exercised
/// without cutting a tag, which only holds if clicking Run cannot create or
/// overwrite a release asset and cannot put a build on the live domain.
#[test]
fn a_manual_run_builds_but_cannot_publish() {
    // Comments out first, always: the `on:` block carries a comment explaining
    // the dispatch guard, and a `workflow_dispatch` named in prose is not a
    // trigger. Read against the raw text, this assertion passed with the trigger
    // itself deleted.
    let yml = code(&read(RELEASE));
    let on = triggers(&yml);
    assert!(
        on.contains("workflow_dispatch"),
        "release.yml has no `workflow_dispatch`, so the release path can only be exercised by \
         cutting a tag: {on}"
    );

    let mut guarded = 0;
    for (name, body) in jobs(&yml) {
        // A guard on the job covers its steps; the reverse is not true, and
        // `create-release` is guarded per step on purpose (a skipped job skips
        // everything that needs it, and the dry run has to keep building).
        let job_guarded = body.lines().any(|l| l.starts_with("    if:") && l.contains(GUARD));
        for step in steps(&body) {
            let Some(marker) = PUBLISHES.iter().find(|m| step.contains(**m)) else {
                continue;
            };
            guarded += 1;
            let step_guarded = step.lines().any(|l| l.trim_start().starts_with("if:") && l.contains(GUARD));
            assert!(
                job_guarded || step_guarded,
                "job `{name}` publishes ({marker}) without `{GUARD}`, so a manual run can \
                 overwrite a published release or deploy to the live domain: {}",
                step.trim()
            );
        }
    }
    assert_eq!(
        guarded,
        PUBLISHES.len(),
        "expected one guarded step per publishing action {PUBLISHES:?}, found {guarded} — \
         either a publish step was dropped or one grew a second copy"
    );

    // And the dry run has to survive the version gate: `github.ref_name` is a
    // branch name on a dispatch, which the tag-vs-manifests check would reject,
    // so no job downstream of it would ever run.
    let derive = steps(&job(&yml, "version"))
        .into_iter()
        .find(|s| s.contains("id: v"))
        .expect("the `version` job no longer has the `id: v` derivation step")
        .to_string();
    assert!(
        derive.contains("workflow_dispatch"),
        "the `version` job does not know about a manual run, so it reads a branch name where a \
         tag should be and stops every dry run at the manifest gate"
    );
}
