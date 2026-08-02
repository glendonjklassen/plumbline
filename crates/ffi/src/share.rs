//! The share link's C ABI — one call for the link, the church and the two
//! strings a Church button needs.
//!
//! Engine-independent: building a link touches no corpus and no home, and a
//! share surface is often up before anything else is. In its own file rather
//! than `lib.rs` for the same reason `reading_map.rs` is: that file is already
//! past the repo's no-3k-line rule, and cbindgen walks the whole crate, so the
//! generated header does not care where an export lives.

use std::ffi::c_char;
use std::ptr;

use plumbline_core::church;

use crate::{guard, opt_str, out_json, wire};

/// Build the link this reader hands over, from `{base?, church?,
/// startAsNewBeliever?, at?}` (all optional — `{}` is the plain app link).
///
/// Answers `{url, base, church, hasChurch, title, siteUrl}`: the link for the QR
/// and the share sheet, the church as the core normalized it, and the label /
/// site a Church button needs. One call rather than six shell-side helpers —
/// the web and Kotlin copies of those had already drifted on the query encoding
/// and on whether the church was cleaned before it went into a URL.
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
        let opts = church::ShareOpts { start_as_new_believer: req.start_as_new_believer, at: req.at.as_deref() };
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
