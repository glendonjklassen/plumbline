//! The home church a shared link carries — the clamps, the link, and the checks.
//!
//! The point: one QR hands over both the Bible and the people who
//! sent it. Whoever shares sets their church in Settings; the link they share
//! carries it; whoever opens that link has it saved locally and sees it in the
//! welcome, so a card handed out at a service leads back to that service.
//!
//! Carried as READABLE query parameters rather than an encoded blob: someone
//! deciding whether to open a link should be able to see what is in it, and a
//! church that mistypes its own details can fix them by reading the URL.
//!
//! ## Why this is in the core
//!
//! It was written twice — `apps/web/src/shell/church.ts` and Kotlin's
//! `ui/Church.kt` — and the two copies had drifted:
//!
//! * The web built the link with `URLSearchParams`, which form-encodes (a space
//!   becomes `+`); Android built it with `Uri.appendQueryParameter`, which
//!   percent-encodes (a space becomes `%20`). Both decode back to the same
//!   church, so the links worked, but they were not the same link — and a
//!   church whose name contains a literal `+` came out of the Android build as
//!   a SPACE on the recipient's device, because `Uri` leaves `+` alone and the
//!   receiving `URLSearchParams` reads it as a space.
//! * The web's `shareUrl` never cleaned its argument; Android's did. A church
//!   longer than the caps could reach a shared URL from the web and not from
//!   the phone.
//! * `churchTitle` and `visitChurch` existed in Kotlin and, separately, inline
//!   in the web's `Shell.svelte`.
//!
//! This module is now the one implementation. The encoding follows the WEB,
//! because the thing that opens a shared link is always the web app and it
//! parses with `URLSearchParams` — which means form encoding is what round-trips
//! a literal `+`.
//!
//! `church_vectors.json` beside this file is the shared expectation table: the
//! tests below check the core against it, and `apps/web/e2e/church-parity.spec.ts`
//! checks the TypeScript against the same rows. The web shell still needs its own
//! copy because a share link is read synchronously out of derived state and the
//! engine lives in a worker, so parity is held by that table rather than by there
//! being only one body of code.

use crate::config::Church;

/// The hosted PWA — what every share hands over.
pub const PWA_URL: &str = "https://plumblinebible.org/";

/// Caps on the three fields. These end up in a URL, in a QR, and on a welcome
/// screen, and the length that still scans is finite.
pub const NAME_MAX: usize = 80;
/// See [`NAME_MAX`].
pub const URL_MAX: usize = 200;

/// What a shared link says beyond the church itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShareOpts<'a> {
    /// Mark the link as one handed to someone meeting the Bible — the
    /// recipient's welcome opens on the new-believer path instead of asking
    /// them to pick. ONLY the Present screen sets it: an ordinary share goes to
    /// whoever, often someone from the same church, and must stay an ordinary
    /// link.
    pub start_as_new_believer: bool,
    /// The verse the recipient opens at, as a refKey (`"Ps 23:1"`). That is the
    /// frozen compact form, so it travels as-is.
    pub at: Option<&'a str>,
}

/// Whether a church has been set at all — a name is the minimum.
pub fn has(c: &Church) -> bool {
    !c.name.trim().is_empty()
}

/// Normalize whatever came off a query string or a settings field.
///
/// Truncation counts CHARACTERS, not bytes and not UTF-16 code units: the
/// shells' `String.slice`/`take` cut in the middle of a surrogate pair and put a
/// lone surrogate in the URL, which the recipient reads as `\u{FFFD}`.
pub fn clean(c: &Church) -> Church {
    fn cut(v: &str, max: usize) -> String {
        v.trim().chars().take(max).collect()
    }
    Church {
        name: cut(&c.name, NAME_MAX),
        // A minute outside a day is not a truncation problem, it is nonsense —
        // dropped, so "never said" is what a bad link produces.
        service: c.service.filter(|m| *m < 24 * 60),
        url: cut(&c.url, URL_MAX),
    }
}

/// What the reader sees on the Church button when there is no site to open:
/// who and when, which is all we were given.
///
/// Joined with a COLON, because that is what the two parts are: a name, then
/// what it tells you about it — a label, not an aside (an em dash would read as
/// an aside).
///
/// English. The parity vectors (`church_vectors.json`) are English by
/// construction and both shells' twins are checked against them, so this is the
/// form that contract names; [`title_in`] is what a reader actually sees.
pub fn title(c: &Church) -> String {
    title_in(crate::i18n::Lang::En, c)
}

/// [`title`], in the reader's language.
///
/// `lang` is only reached for the EMPTY case: a church that named itself is
/// already in its own language and nothing here should touch it.
pub fn title_in(lang: crate::i18n::Lang, c: &Church) -> String {
    let c = clean(c);
    let parts: Vec<String> =
        [c.name, c.service.map(|m| service_line(lang, m)).unwrap_or_default()].into_iter().filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        crate::i18n::t(lang, "shell.churchFallback", &[])
    } else {
        parts.join(": ")
    }
}

/// "Sundays 10:00 AM" — when the church meets, written the reader's way.
///
/// The CLOCK is the part that differs: English-speaking readers expect 10:00 AM
/// and German and Spanish ones expect 10:00 on a 24-hour clock. The surrounding
/// words come from the catalogue (`church.meets`), so the whole line localizes
/// and nothing here is a sentence.
///
/// Minutes outside a day never reach this — [`clean`] drops them — but the
/// arithmetic is total anyway rather than relying on that.
pub fn service_line(lang: crate::i18n::Lang, minutes: u16) -> String {
    crate::i18n::t(lang, "church.meets", &[("time", &clock(lang, minutes))])
}

/// The clock alone, 12-hour for English and 24-hour otherwise.
pub fn clock(lang: crate::i18n::Lang, minutes: u16) -> String {
    let m = minutes % (24 * 60);
    let (h, min) = (m / 60, m % 60);
    if matches!(lang, crate::i18n::Lang::En) {
        let suffix = if h < 12 { "AM" } else { "PM" };
        let h12 = match h % 12 {
            0 => 12,
            other => other,
        };
        format!("{h12}:{min:02} {suffix}")
    } else {
        format!("{h:02}:{min:02}")
    }
}

/// The church's own site, if it is one we are willing to open. `None` when it
/// is not, and a `None` is the shell's cue to fall back to [`title`].
///
/// Only http(s) survives: a church URL is typed by hand on the phone and
/// arrives from a stranger's query string on the web, and `javascript:` must
/// never reach an `href` or an `Intent`. ASCII control characters are refused
/// for the same reason — a newline inside an href is not something a church
/// typed.
///
/// The string comes back TRIMMED BUT OTHERWISE UNTOUCHED. The reader typed it;
/// the address bar should show what they typed.
pub fn safe_url(url: &str) -> Option<String> {
    let t = url.trim();
    if t.is_empty() || t.len() > URL_MAX * 4 || t.chars().any(|c| c.is_ascii_control()) {
        return None;
    }
    let (scheme, rest) = t.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    // Authority runs to the first path/query/fragment delimiter; anything before
    // an `@` in it is userinfo, not the host.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    (!host.is_empty()).then(|| t.to_string())
}

/// A share link for `base` carrying `church` (plain `base` when unset).
pub fn share_url(base: &str, church: &Church, opts: &ShareOpts) -> String {
    let c = clean(church);
    let mut pairs: Vec<(&str, &str)> = Vec::new();
    let service: String;
    if has(&c) {
        pairs.push(("church", &c.name));
        // The meeting time travels as a NUMBER (minutes since midnight), so the
        // recipient's app writes it their way rather than reading someone
        // else's formatting. Rendered into a String here because `pairs` holds
        // borrows; kept alive for the call below.
        if let Some(m) = c.service {
            service = m.to_string();
            pairs.push(("churchService", &service));
        }
        if !c.url.is_empty() {
            pairs.push(("churchUrl", &c.url));
        }
    }
    if opts.start_as_new_believer {
        pairs.push(("start", "new"));
    }
    let at = opts.at.map(str::trim).unwrap_or("");
    if !at.is_empty() {
        pairs.push(("at", at));
    }
    set_query(base, &pairs)
}

/// Add the opening verse to an ALREADY-built share link, so a caller holding
/// only the finished link (Present) doesn't have to rebuild it from the church.
/// An empty `ref_key` leaves the link alone.
pub fn with_at(link: &str, ref_key: &str) -> String {
    let at = ref_key.trim();
    if at.is_empty() {
        return link.to_string();
    }
    set_query(link, &[("at", at)])
}

/// The church encoded in a query string (`?church=…&churchInfo=…&churchUrl=…`),
/// if any. `search` may keep its leading `?`.
pub fn from_query(search: &str) -> Option<Church> {
    let c = clean(&Church {
        name: query_get(search, "church").unwrap_or_default(),
        // A time that is not a number, or not a time, reads as "never said" —
        // a stranger's query string is not allowed to produce a bad one.
        service: query_get(search, "churchService").and_then(|v| v.trim().parse::<u16>().ok()),
        url: query_get(search, "churchUrl").unwrap_or_default(),
    });
    has(&c).then_some(c)
}

/// Whether this link asks the welcome to open on the new-believer path.
pub fn starts_as_new_believer(search: &str) -> bool {
    query_get(search, "start").as_deref() == Some("new")
}

/// The verse a link opens at (`?at=Ps 23:1`), or `None`. Shape-checked here so a
/// stranger's query string can't send the reader somewhere absurd — the engine
/// still has the last word on whether the ref exists.
pub fn shared_at_ref(search: &str) -> Option<String> {
    let raw = query_get(search, "at")?;
    let raw = raw.trim();
    is_ref_shaped(raw).then(|| raw.to_string())
}

// ── query strings ────────────────────────────────────────────────────────────

/// `[1-3]?[A-Za-z]{2,6} \d{1,3}:\d{1,3}` — the shape of a refKey, without
/// pulling in a regex engine for it.
fn is_ref_shaped(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if b.first().is_some_and(|c| (b'1'..=b'3').contains(c)) {
        i = 1;
    }
    let letters = b[i..].iter().take_while(|c| c.is_ascii_alphabetic()).count();
    if !(2..=6).contains(&letters) {
        return false;
    }
    i += letters;
    if b.get(i) != Some(&b' ') {
        return false;
    }
    i += 1;
    let digits = |from: usize| b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let ch = digits(i);
    if !(1..=3).contains(&ch) {
        return false;
    }
    i += ch;
    if b.get(i) != Some(&b':') {
        return false;
    }
    i += 1;
    let vs = digits(i);
    (1..=3).contains(&vs) && i + vs == b.len()
}

/// The first value for `key`, form-decoded. Mirrors `URLSearchParams.get`,
/// which is what reads these on the receiving end.
fn query_get(search: &str, key: &str) -> Option<String> {
    let q = search.strip_prefix('?').unwrap_or(search);
    let q = q.split('#').next().unwrap_or("");
    q.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (form_decode(k) == key).then(|| form_decode(v))
    })
}

/// Append `pairs` to `url`'s query, dropping any pair already there under a key
/// we are about to set (that is `URLSearchParams.set`, not `append`). A fragment
/// stays at the end, where a URL puts it.
fn set_query(url: &str, pairs: &[(&str, &str)]) -> String {
    let (head, frag) = match url.find('#') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };
    let (path, query) = match head.find('?') {
        Some(i) => (&head[..i], &head[i + 1..]),
        None => (head, ""),
    };
    let mut out = String::with_capacity(url.len() + 64);
    out.push_str(path);
    let mut first = true;
    let mut push = |encoded: &str, out: &mut String| {
        out.push(if first { '?' } else { '&' });
        first = false;
        out.push_str(encoded);
    };
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let k = pair.split('=').next().unwrap_or("");
        if pairs.iter().any(|(key, _)| form_decode(k) == *key) {
            continue;
        }
        push(pair, &mut out);
    }
    for (k, v) in pairs {
        let mut enc = String::new();
        form_encode(k, &mut enc);
        enc.push('=');
        form_encode(v, &mut enc);
        push(&enc, &mut out);
    }
    out.push_str(frag);
    out
}

/// `application/x-www-form-urlencoded`, byte for byte what `URLSearchParams`
/// emits: alphanumerics and `*-._` survive, a space becomes `+`, everything else
/// becomes uppercase percent escapes.
fn form_encode(s: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => out.push(b as char),
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0x0f) as usize] as char);
            }
        }
    }
}

/// The inverse. Invalid escapes are left as typed rather than dropped — a link
/// someone edited by hand should still hand over as much as it can.
///
/// Works on BYTES throughout: a `%` followed by two non-hex bytes may be the
/// middle of a multi-byte character, and slicing the `&str` there would panic.
fn form_decode(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut bytes = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => {
                bytes.push(b' ');
                i += 1;
            }
            b'%' => match (b.get(i + 1).copied().and_then(hex), b.get(i + 2).copied().and_then(hex)) {
                (Some(hi), Some(lo)) => {
                    bytes.push(hi << 4 | lo);
                    i += 3;
                }
                _ => {
                    bytes.push(b'%');
                    i += 1;
                }
            },
            c => {
                bytes.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn church(name: &str, service: Option<u16>, url: &str) -> Church {
        Church { name: name.into(), service, url: url.into() }
    }

    /// The shared expectation table. `apps/web/e2e/church-parity.spec.ts` reads
    /// the same rows and holds the TypeScript copy to them, so the two shells
    /// cannot drift apart.
    #[test]
    fn matches_the_shared_vector_table() {
        let rows: Vec<Value> = serde_json::from_str(include_str!("church_vectors.json")).expect("vector table");
        assert!(rows.len() >= 8, "the table is the parity contract; it should not shrink");
        for row in rows {
            let name = row["name"].as_str().unwrap();
            let c = church(
                row["church"]["name"].as_str().unwrap_or(""),
                row["church"]["service"].as_u64().map(|m| m as u16),
                row["church"]["url"].as_str().unwrap_or(""),
            );
            let opts = ShareOpts {
                start_as_new_believer: row["startAsNewBeliever"].as_bool().unwrap_or(false),
                at: row["at"].as_str(),
            };
            let cleaned = clean(&c);
            assert_eq!(cleaned.name, row["cleaned"]["name"].as_str().unwrap(), "cleaned name [{name}]");
            assert_eq!(
                cleaned.service.map(u64::from),
                row["cleaned"]["service"].as_u64(),
                "cleaned service [{name}]"
            );
            assert_eq!(cleaned.url, row["cleaned"]["url"].as_str().unwrap(), "cleaned url [{name}]");
            assert_eq!(share_url(PWA_URL, &c, &opts), row["url"].as_str().unwrap(), "url [{name}]");
            assert_eq!(title(&c), row["title"].as_str().unwrap(), "title [{name}]");
            // Against the CLEANED url: that is the only form a shell ever holds
            // (a church arrives from the config or a query string, both cleaned).
            assert_eq!(safe_url(&cleaned.url), row["safeUrl"].as_str().map(str::to_string), "safeUrl [{name}]");
        }
    }

    #[test]
    fn clean_trims_and_caps() {
        let c = clean(&church(&"n".repeat(200), Some(600), &"u".repeat(400)));
        assert_eq!(c.name.chars().count(), NAME_MAX);
        assert_eq!(c.service, Some(600));
        assert_eq!(c.url.chars().count(), URL_MAX);
    }

    /// The web's `.slice(80)` and Kotlin's `.take(80)` both cut UTF-16 code
    /// units, so an emoji straddling the cap lost half of itself and the URL
    /// carried a lone surrogate.
    #[test]
    fn clean_never_splits_a_character() {
        let c = clean(&church(&"😀".repeat(NAME_MAX + 10), None, ""));
        assert_eq!(c.name.chars().count(), NAME_MAX);
        assert!(c.name.chars().all(|ch| ch == '😀'), "a character was cut in half");
    }

    #[test]
    fn share_url_is_plain_when_no_church_is_set() {
        assert_eq!(share_url(PWA_URL, &Church::default(), &ShareOpts::default()), PWA_URL);
        // Whitespace is not a church.
        assert!(!has(&church("   ", Some(600), "y")), "a name of spaces is not a name");
        assert_eq!(share_url(PWA_URL, &church("   ", Some(600), "y"), &ShareOpts::default()), PWA_URL);
    }

    /// The Android bug this module was written to end: `Uri.appendQueryParameter`
    /// leaves `+` alone, and the recipient's `URLSearchParams` reads a bare `+`
    /// as a space.
    #[test]
    fn a_literal_plus_survives_the_round_trip() {
        let c = church("Faith + Hope Chapel", None, "");
        let url = share_url(PWA_URL, &c, &ShareOpts::default());
        assert!(url.contains("church=Faith+%2B+Hope+Chapel"), "{url}");
        let q = url.split_once('?').unwrap().1;
        assert_eq!(from_query(q).unwrap().name, "Faith + Hope Chapel");
    }

    #[test]
    fn everything_the_link_carries_comes_back_out() {
        let c = church("Iglesia Bíblica", Some(600), "https://ejemplo.org/a?b=c");
        let url = share_url(PWA_URL, &c, &ShareOpts { start_as_new_believer: true, at: Some("Ps 23:1") });
        let q = url.split_once('?').unwrap().1;
        assert_eq!(from_query(q), Some(c));
        assert!(starts_as_new_believer(q));
        assert_eq!(shared_at_ref(q).as_deref(), Some("Ps 23:1"));

        // Only "new" opens the new-believer welcome. Anything else in `start`
        // is a link we did not write, and an ordinary share must stay ordinary.
        assert!(!starts_as_new_believer("?start=newish"));
        assert!(!starts_as_new_believer("?start="));
        assert!(!starts_as_new_believer("?church=Grace"));

        // What a stranger sent goes through the same clamps a settings field
        // does — a query string is the least trusted input the app has.
        let long = from_query(&format!("?church=%20%20{}", "n".repeat(200))).expect("a name is a church");
        assert_eq!(long.name, "n".repeat(NAME_MAX));
    }

    #[test]
    fn share_url_replaces_rather_than_duplicates() {
        let once = share_url(PWA_URL, &church("A", None, ""), &ShareOpts::default());
        let twice = share_url(&once, &church("B", None, ""), &ShareOpts::default());
        assert_eq!(twice, "https://plumblinebible.org/?church=B");
        // …and an unrelated parameter on the base is not thrown away.
        assert_eq!(
            share_url("https://x/?keep=1", &church("A", None, ""), &ShareOpts::default()),
            "https://x/?keep=1&church=A",
        );
    }

    #[test]
    fn with_at_leaves_the_rest_of_the_link_alone() {
        let link = share_url(PWA_URL, &church("Grace", None, ""), &ShareOpts { start_as_new_believer: true, at: None });
        assert_eq!(with_at(&link, "1John 4:8"), format!("{link}&at=1John+4%3A8"));
        assert_eq!(with_at(&link, "  "), link, "no verse, no change");
        // Setting it twice sets it, rather than appending a second one that
        // `URLSearchParams.get` would then ignore.
        assert_eq!(with_at(&with_at(&link, "Ps 1:1"), "Ps 2:2"), format!("{link}&at=Ps+2%3A2"));
    }

    #[test]
    fn a_fragment_stays_at_the_end() {
        let out = share_url("https://x/#/John/3", &church("Grace", None, ""), &ShareOpts::default());
        assert_eq!(out, "https://x/?church=Grace#/John/3");
    }

    #[test]
    fn only_http_urls_are_offered_as_links() {
        assert_eq!(safe_url("https://gracebible.org"), Some("https://gracebible.org".to_string()));
        assert_eq!(safe_url("  http://gracebible.org/x?y#z  "), Some("http://gracebible.org/x?y#z".to_string()));
        assert_eq!(safe_url("HTTPS://GRACE.ORG"), Some("HTTPS://GRACE.ORG".to_string()));
        for bad in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            // `javascript://…` IS a valid javascript URL — everything after the
            // newline runs, and the `//` makes the first line a comment. It is
            // the case the scheme check exists for, since the others are caught
            // by having no `://` at all.
            "javascript://grace.org/%0aalert(1)",
            "JAVASCRIPT://grace.org/\u{000a}alert(1)",
            "data:text/html,<script>",
            "ftp://files.grace.org",
            "gracebible.org",
            "https://",
            "https://@",
            "",
            "   ",
            "https://grace.org\njavascript:alert(1)",
        ] {
            assert_eq!(safe_url(bad), None, "{bad:?} must not be offered as a link");
        }
    }

    #[test]
    fn a_strangers_at_parameter_has_to_look_like_a_ref() {
        for good in ["Ps 23:1", "1John 4:8", "Gen 1:1", "Song 1:1"] {
            assert_eq!(shared_at_ref(&format!("?at={good}")).as_deref(), Some(good));
        }
        for bad in ["", "Ps23:1", "Ps 23", "Ps 1234:1", "1 Jn 4:8", "../../etc", "Ps 23:1x"] {
            let q = format!("?at={}", bad.replace(' ', "%20"));
            assert_eq!(shared_at_ref(&q), None, "{bad:?} must not be accepted");
        }
    }

    #[test]
    fn title_falls_back_to_something_a_reader_can_read() {
        assert_eq!(title(&Church::default()), "Your church");
        assert_eq!(title(&church("Grace", None, "")), "Grace");
        assert_eq!(title(&church("Grace", Some(600), "")), "Grace: Meets Sundays at 10:00 AM");
        // German writes the same minute on a 24-hour clock.
        assert_eq!(
            title_in(crate::i18n::Lang::De, &church("Grace", Some(600), "")),
            "Grace: Trifft sich sonntags um 10:00"
        );
    }
}
