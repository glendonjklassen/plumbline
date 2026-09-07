//! The share link's C ABI — one call for the link, the church and the two
//! strings a Church button needs.
//!
//! Engine-independent: building a link touches no corpus and no home, so a
//! share surface can be up before anything else is.

use std::ffi::c_char;
use std::ptr;

use plumbline_core::{church, i18n, thread};
use serde::Serialize;

use crate::{guard, opt_str, out_json, wire, PlumblineEngine};

/// Build the link this reader hands over, from `{base?, church?, at?, lang?,
/// thread?, devotional?}` (all optional — `{}` is the plain app link).
///
/// Answers `{url, base, church, hasChurch, title, siteUrl}`: the link for the QR
/// and the share sheet, the church as the core normalized it, and the label /
/// site a Church button needs. Query encoding and church cleaning live here, not
/// in each shell.
///
/// Null only when `request` is null or not JSON.
///
/// # Safety
/// `request` is null or valid NUL-terminated UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn plumbline_share_url_json(request: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(req) = opt_str(request).and_then(|s| serde_json::from_str::<wire::WireShareRequest>(s).ok()) else {
            return ptr::null_mut();
        };
        let base = req.base.as_deref().map(str::trim).filter(|b| !b.is_empty()).unwrap_or(church::PWA_URL);
        let cleaned = church::clean(&req.church.unwrap_or_default().to_core());
        let opts = church::ShareOpts {
            at: req.at.as_deref(),
            lang: req.lang.as_deref(),
            thread: req.thread.as_deref(),
            devotional: req.devotional.as_deref(),
        };
        out_json(&wire::WireShare {
            url: church::share_url(base, &cleaned, &opts),
            base: base.to_string(),
            has_church: church::has(&cleaned),
            title: church::title(&cleaned),
            site_url: church::safe_url(&cleaned.url),
            church: wire::WireChurch::from_core(&cleaned),
        })
    })
}

// ── what a link may offer ─────────────────────────────────────────────────────

/// One selectable language, as a picker shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireShareLang {
    code: String,
    /// What the language calls itself — the only honest label in a picker.
    endonym: String,
    /// Its English name, so a sender can find Punjabi by typing "Punjabi".
    exonym: String,
    /// Whether text in this language runs right to left — for the one place the
    /// palette paints the RECIPIENT's language, the caption under the code.
    rtl: bool,
}

/// One offerable destination or path, and whether it exists in the chosen
/// language yet. `available: false` is NOT a reason to hide the row — the
/// palette shows it as coming soon, because a sender looking for the Arabic
/// booklet should learn that it is being worked on rather than wonder whether
/// they mis-tapped.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireShareOption {
    /// The token the link carries (`sharing`, `new-believer-30`, `Romans Road`).
    id: String,
    /// What to show for it, in the SENDER's language where there is one.
    label: String,
    /// What to show for it in the RECIPIENT's language — the caption under the
    /// code is the one thing on the palette the person being handed the phone
    /// reads. A thread's name is its own name in every language; a booklet's is
    /// its title in `lang`.
    target_label: String,
    available: bool,
    /// Threads only: whether this is one of the per-language gospel walks
    /// (`thread::gospel_default`). The palette swaps such a thread for its
    /// sibling when the recipient's language changes, and leaves any other
    /// choice alone.
    gospel: bool,
}

/// Two strings the palette paints in the RECIPIENT's language, under the code —
/// so a sender who cannot read that language still shows the person in front of
/// them something they can. Templates: `scan_target` carries `{name}` for the
/// shell to fill with the destination's `target_label`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireShareCaptions {
    scan_app: String,
    scan_target: String,
}

/// Everything the share palette may offer, for one chosen language.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireShareOptions {
    /// The language every `available` here is about — the RECIPIENT's. Canonical,
    /// so a shell that asked with a region tag can tell what it actually got.
    lang: String,
    /// The language every `label` here is in — the SENDER's. See the endpoint.
    ui_lang: String,
    languages: Vec<WireShareLang>,
    threads: Vec<WireShareOption>,
    devotionals: Vec<WireShareOption>,
    /// The gospel walk for a reader of `lang` — what the thread box defaults to,
    /// and what a gospel thread becomes when the language changes.
    gospel_default: String,
    captions: WireShareCaptions,
}

/// What a shared link may carry: every shipped language, the four first-run
/// paths, the shareable threads and the devotional booklets — each with whether
/// it exists in the chosen language yet.
///
/// TWO languages, because a share palette has two of them and they are almost
/// never the same one:
///
/// - `lang` is what the RECIPIENT will read in — the sender's choice for someone
///   else, and what every `available` here is about.
/// - `ui_lang` is what the SENDER reads, and every `label` comes back in it.
///
/// Conflating them produces a picker whose own options the person using it
/// cannot read: an English sender aiming a link at Arabic was being offered
/// "متسائل عن الكتاب المقدس — Coming soon". The sender has to understand the
/// choice; the recipient's language is what the choice is ABOUT. `ui_lang` null
/// reads as `lang`, which is the right default for the common case where a
/// reader shares in their own language.
///
/// Engine-taking, unlike [`plumbline_share_url_json`], because threads and
/// booklets are data an engine has loaded. Building a link stays engine-free.
///
/// Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_share_options_json(
    engine: *const PlumblineEngine,
    lang: *const c_char,
    ui_lang: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        // An unknown code reads as English rather than refusing: a palette that
        // returns nothing is worse than one that answers about the base language.
        let l = opt_str(lang).map(i18n::Lang::parse).unwrap_or(i18n::Lang::En);
        let ui = opt_str(ui_lang).map(i18n::Lang::parse).unwrap_or(l);
        let code = l.code();
        // The loaded threads carry the stock set's flags (`lang`, `gospelDefault`);
        // the NAMES offered still come from `STOCK_THREADS` below, because a
        // sender's deletion changes nothing about what the recipient seeds.
        let loaded: Vec<thread::Thread> = e.study_read().threads.iter().map(|t| t.thread.clone()).collect();
        let gospel_default = thread::gospel_default(&loaded, code)
            .map(|t| t.name.clone())
            .unwrap_or_else(|| thread::GOSPEL_DEFAULT.to_string());
        out_json(&WireShareOptions {
            lang: code.to_string(),
            ui_lang: ui.code().to_string(),
            languages: i18n::Lang::ALL
                .iter()
                .map(|x| WireShareLang {
                    code: x.code().to_string(),
                    endonym: x.spec().endonym.to_string(),
                    exonym: x.spec().exonym.to_string(),
                    rtl: x.is_rtl(),
                })
                .collect(),
            // The STOCK set, not this reader's threads: what the RECIPIENT's
            // install seeds is what a shared name will resolve against, and the
            // sender having deleted or renamed their own copy does not change
            // what arrives on the other phone.
            threads: thread::STOCK_THREADS
                .iter()
                .map(|name| WireShareOption {
                    id: (*name).to_string(),
                    label: (*name).to_string(),
                    // A thread's name is data, the same in every language.
                    target_label: (*name).to_string(),
                    // A thread is a list of refs, so every corpus resolves it:
                    // there is no language in which Romans Road is missing. The
                    // annotations are in the language of the thread that carries
                    // them, which is why the walk ships once per language.
                    available: true,
                    gospel: loaded.iter().any(|t| t.name == *name && t.gospel_default),
                })
                .collect(),
            devotionals: e
                .devotionals()
                .iter()
                .map(|d| WireShareOption {
                    id: d.id.clone(),
                    // Named for the sender, who is the one reading this list.
                    label: crate::devotionals::booklet_name(d, ui.code()),
                    target_label: crate::devotionals::booklet_name(d, code),
                    // The one real gate today: `new-believer-30` is written in
                    // English and nothing else.
                    available: d.has_lang(code),
                    gospel: false,
                })
                .collect(),
            gospel_default,
            // In the RECIPIENT's language, on purpose: this is what the sender
            // shows across the table. `{name}` stays for the shell to fill.
            captions: WireShareCaptions {
                scan_app: i18n::t(l, "present.scanToOpenApp", &[]),
                scan_target: i18n::t(l, "share.scanTarget", &[]),
            },
        })
    })
}
