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
    /// What to show for it, already in the chosen language where there is one.
    label: String,
    available: bool,
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
        out_json(&WireShareOptions {
            lang: code.to_string(),
            ui_lang: ui.code().to_string(),
            languages: i18n::Lang::ALL
                .iter()
                .map(|x| WireShareLang {
                    code: x.code().to_string(),
                    endonym: x.spec().endonym.to_string(),
                    exonym: x.spec().exonym.to_string(),
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
                    // A thread is a list of refs, so every corpus resolves it:
                    // there is no language in which Romans Road is missing. What
                    // is not translated yet is the ANNOTATIONS, and the stock
                    // thread carries none in any language.
                    available: true,
                })
                .collect(),
            devotionals: e
                .devotionals()
                .iter()
                .map(|d| WireShareOption {
                    id: d.id.clone(),
                    // Named for the sender, who is the one reading this list.
                    label: crate::devotionals::booklet_name(d, ui.code()),
                    // The one real gate today: `new-believer-30` is written in
                    // English and nothing else.
                    available: d.has_lang(code),
                })
                .collect(),
        })
    })
}
