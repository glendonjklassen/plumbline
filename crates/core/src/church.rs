//! The home church a shared link carries — the clamps, the link, and the checks.
//!
//! One QR hands over both the Bible and the people who sent it: whoever shares
//! sets their church in Settings, the link they share carries it, and whoever
//! opens that link has it saved locally and sees it in the welcome.
//!
//! Carried as readable query parameters rather than an encoded blob, so someone
//! deciding whether to open a link can see what is in it, and a church that
//! mistyped its own details can fix them by reading the URL.
//!
//! This module is the one implementation of the share-URL builder, written
//! because separate copies drifted. The encoding follows the web: what opens a
//! shared link is always the web app and it parses with `URLSearchParams`, so
//! form encoding is what round-trips a literal `+`.
//!
//! The web shell still keeps a twin, because a share link is read synchronously
//! out of derived state and the engine lives in a worker. Parity is held by
//! `church_vectors.json` beside this file: the tests below check the core
//! against it, and `apps/web/e2e/church-parity.spec.ts` checks the TypeScript
//! against the same rows.

use crate::config::Church;

/// The hosted PWA — what every share hands over.
pub const PWA_URL: &str = "https://plumblinebible.org/";

/// Caps on the three fields. These end up in a URL, in a QR, and on a welcome
/// screen, and the length that still scans is finite.
pub const NAME_MAX: usize = 80;
/// See [`NAME_MAX`].
pub const URL_MAX: usize = 200;

/// The longest a thread name or devotional id may be in a link. Generous for a
/// name someone typed, short enough that a stranger cannot pad a QR with it.
pub const TARGET_MAX: usize = 120;

/// What a shared link says beyond the church itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShareOpts<'a> {
    /// The verse the recipient opens at, as a refKey (`"Ps 23:1"`) — the frozen
    /// compact form, so it travels as-is.
    pub at: Option<&'a str>,
    /// The language the recipient reads in (`?lang=pa`), whatever the sender
    /// reads in. A code, validated on the way back out by [`shared_lang`].
    pub lang: Option<&'a str>,
    /// A thread the recipient lands on (`?thread=Romans+Road`), by name. Only
    /// a name every install has is worth sharing — see [`shared_thread`].
    pub thread: Option<&'a str>,
    /// A devotional booklet the recipient lands on (`?devotional=new-believer-30`).
    pub devotional: Option<&'a str>,
}

/// Whether a church has been set at all — a name is the minimum.
pub fn has(c: &Church) -> bool {
    !c.name.trim().is_empty()
}

/// Normalize whatever came off a query string or a settings field.
///
/// Truncation counts CHARACTERS, not bytes and not UTF-16 code units: a
/// `String.slice` cuts in the middle of a surrogate pair and puts a lone
/// surrogate in the URL, which the recipient reads as `\u{FFFD}`.
pub fn clean(c: &Church) -> Church {
    fn cut(v: &str, max: usize) -> String {
        v.trim().chars().take(max).collect()
    }
    Church {
        name: cut(&c.name, NAME_MAX),
        // A minute outside a day is nonsense, not a truncation problem: dropped,
        // so a bad link produces "never said".
        service: c.service.filter(|m| *m < 24 * 60),
        url: cut(&c.url, URL_MAX),
    }
}

/// What the reader sees on the Church button when there is no site to open: who
/// and when, joined with a colon — a label, not an aside.
///
/// English, because the parity vectors (`church_vectors.json`) are English by
/// construction and that is the form the contract names; [`title_in`] is what a
/// reader actually sees.
pub fn title(c: &Church) -> String {
    title_in(crate::i18n::Lang::En, c)
}

/// [`title`], in the reader's language.
///
/// `lang` is only reached for the EMPTY case: a church that named itself is
/// already in its own language and nothing here should touch it.
pub fn title_in(lang: crate::i18n::Lang, c: &Church) -> String {
    let c = clean(c);
    let parts: Vec<String> = [c.name, c.service.map(|m| service_line(lang, m)).unwrap_or_default()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        crate::i18n::t(lang, "shell.churchFallback", &[])
    } else {
        parts.join(": ")
    }
}

/// "Sundays 10:00 AM" — when the church meets, written the reader's way. The
/// surrounding words come from the catalogue (`church.meets`); only the clock
/// is built here.
///
/// Minutes outside a day never reach this ([`clean`] drops them), but the
/// arithmetic is total rather than relying on that.
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

/// The church's own site, if it is one we are willing to open; `None` is the
/// shell's cue to fall back to [`title`].
///
/// Only http(s) survives: the URL arrives from a stranger's query string, and
/// `javascript:` must never reach an `href`. ASCII control characters are
/// refused for the same reason — a newline inside an href is not something a
/// church typed. The string comes back trimmed but otherwise untouched, so the
/// address bar shows what the reader typed.
pub fn safe_url(url: &str) -> Option<String> {
    let t = url.trim();
    if t.is_empty() || t.len() > URL_MAX * 4 || t.chars().any(|c| c.is_ascii_control()) {
        return None;
    }
    // A scheme is SUPPLIED when the reader did not type one. Churches write their
    // address the way it is on their sign — "gracebible.org", "www.gracebible.org"
    // — and refusing that made the field look broken to the person least likely
    // to know why. `https`, not `http`: guessing the insecure one on a reader's
    // behalf is a guess that can be downgraded, and every host worth linking has
    // had TLS for years.
    //
    // Only where there is no scheme at all. A scheme we do not allow is still
    // refused rather than repaired — prepending to `javascript:alert(1)` would
    // turn a refusal into a link.
    let full = match scheme_of(t) {
        Some(scheme) if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") => t.to_string(),
        Some(_) => return None,
        None => format!("https://{t}"),
    };
    let rest = full.split_once("://")?.1;
    // Authority runs to the first path/query/fragment delimiter; anything before
    // an `@` in it is userinfo, not the host.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    // A DOT and no spaces, which `http://` never had to check because typing the
    // scheme was itself the statement that this is an address. Without it the
    // church NAME typed into the website field ("Grace Bible Church") becomes
    // `https://Grace Bible Church` and is offered to the recipient as a link.
    let host = host.split(':').next().unwrap_or("");
    if host.is_empty() || !host.contains('.') || host.contains(' ') {
        return None;
    }
    Some(full)
}

/// The scheme of `t` if it names one — `Some("https")` for `https://x`, and for
/// an opaque one like `mailto:a@b`. `None` when there is no scheme to speak of,
/// including for a bare `host:port`, which only looks like one.
fn scheme_of(t: &str) -> Option<&str> {
    let (head, rest) = t.split_once(':')?;
    if head.is_empty() || !head.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    if !head.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return None;
    }
    // `example.org:8080` is a host and a port, not a scheme and a path.
    if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(head)
}

/// A share link for `base` carrying `church` (plain `base` when unset).
pub fn share_url(base: &str, church: &Church, opts: &ShareOpts) -> String {
    let c = clean(church);
    let mut pairs: Vec<(&str, &str)> = Vec::new();
    let service: String;
    if has(&c) {
        pairs.push(("church", &c.name));
        // The meeting time travels as a number (minutes since midnight) so the
        // recipient's app writes it their way. Rendered into a String kept alive
        // for the call below, because `pairs` holds borrows.
        if let Some(m) = c.service {
            service = m.to_string();
            pairs.push(("churchService", &service));
        }
        if !c.url.is_empty() {
            pairs.push(("churchUrl", &c.url));
        }
    }
    let at = opts.at.map(str::trim).unwrap_or("");
    if !at.is_empty() {
        pairs.push(("at", at));
    }
    // The palette's three destinations. Each is a name/code the recipient's app
    // resolves against what it actually has — the link asserts nothing about the
    // recipient's install, so a thread they lack falls through to a plain boot.
    let lang = opts.lang.map(str::trim).unwrap_or("");
    if !lang.is_empty() {
        pairs.push(("lang", lang));
    }
    let thread = opts.thread.map(str::trim).unwrap_or("");
    if !thread.is_empty() {
        pairs.push(("thread", thread));
    }
    let devotional = opts.devotional.map(str::trim).unwrap_or("");
    if !devotional.is_empty() {
        pairs.push(("devotional", devotional));
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
        // A time that is not a time reads as "never said": a stranger's query
        // string is not allowed to produce a bad one.
        service: query_get(search, "churchService").and_then(|v| v.trim().parse::<u16>().ok()),
        url: query_get(search, "churchUrl").unwrap_or_default(),
    });
    has(&c).then_some(c)
}

/// The verse a link opens at (`?at=Ps 23:1`), or `None`. Shape-checked here so a
/// stranger's query string can't send the reader somewhere absurd — the engine
/// still has the last word on whether the ref exists.
pub fn shared_at_ref(search: &str) -> Option<String> {
    let raw = query_get(search, "at")?;
    let raw = raw.trim();
    is_ref_shaped(raw).then(|| raw.to_string())
}

/// The language a link asks to be read in (`?lang=pa`), or `None`.
///
/// Validated against the shipped registry rather than passed through: an
/// unknown code must leave the reader in the language they already had, not
/// strand them in a half-applied one. Returns the CANONICAL code, so `?lang=PA`
/// and `?lang=pa-IN` both land on `pa`.
pub fn shared_lang(search: &str) -> Option<&'static str> {
    let raw = query_get(search, "lang")?;
    crate::i18n::Lang::shipped(raw.trim()).map(|l| l.code())
}

/// The thread a link opens on (`?thread=Romans+Road`), or `None`.
///
/// A NAME, not content: the recipient's own install is what has to have it, and
/// the caller resolves it against their loaded threads exactly as
/// `gospelThread` already resolves a configured name. Length-capped here so a
/// stranger's query string cannot pad the address bar.
pub fn shared_thread(search: &str) -> Option<String> {
    shared_target(search, "thread")
}

/// The devotional booklet a link opens on (`?devotional=new-believer-30`), or
/// `None`. An id, resolved by the caller against the booklets they have —
/// including whether it has been translated into the language they read.
pub fn shared_devotional(search: &str) -> Option<String> {
    shared_target(search, "devotional")
}

/// A named destination off a query string: trimmed, length-capped, non-empty.
fn shared_target(search: &str, key: &str) -> Option<String> {
    let raw = query_get(search, key)?;
    let t = raw.trim();
    // Counted in CODE POINTS, like `clean` — a cap that splits a character puts
    // a lone surrogate on screen.
    (!t.is_empty() && t.chars().count() <= TARGET_MAX).then(|| t.to_string())
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
    /// the same rows and holds the TypeScript twin to them, so the two copies
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
                at: row["at"].as_str(),
                // Absent columns are the default, so the rows that predate the
                // share palette keep asserting exactly what they always did.
                lang: row["lang"].as_str(),
                thread: row["thread"].as_str(),
                devotional: row["devotional"].as_str(),
            };
            let cleaned = clean(&c);
            assert_eq!(cleaned.name, row["cleaned"]["name"].as_str().unwrap(), "cleaned name [{name}]");
            assert_eq!(cleaned.service.map(u64::from), row["cleaned"]["service"].as_u64(), "cleaned service [{name}]");
            assert_eq!(cleaned.url, row["cleaned"]["url"].as_str().unwrap(), "cleaned url [{name}]");
            assert_eq!(share_url(PWA_URL, &c, &opts), row["url"].as_str().unwrap(), "url [{name}]");
            assert_eq!(title(&c), row["title"].as_str().unwrap(), "title [{name}]");
            // Against the cleaned url: the only form a shell holds, since a
            // church arrives from the config or a query string, both cleaned.
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

    /// A `.slice(80)` cuts UTF-16 code units, so an emoji straddling the cap
    /// loses half of itself and the URL carries a lone surrogate.
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

    /// A builder that percent-encodes leaves `+` alone, and the recipient's
    /// `URLSearchParams` then reads a bare `+` as a space. Form encoding is what
    /// round-trips it.
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
        let url = share_url(PWA_URL, &c, &ShareOpts { at: Some("Ps 23:1"), ..Default::default() });
        let q = url.split_once('?').unwrap().1;
        assert_eq!(from_query(q), Some(c));
        assert_eq!(shared_at_ref(q).as_deref(), Some("Ps 23:1"));

        // A query string is the least trusted input the app has, and goes
        // through the same clamps a settings field does.
        let long = from_query(&format!("?church=%20%20{}", "n".repeat(200))).expect("a name is a church");
        assert_eq!(long.name, "n".repeat(NAME_MAX));
    }

    /// The share palette's own parameters, out and back.
    ///
    /// Fails against a builder that drops any of them, and against a parser that
    /// passes a stranger's value through: each half is asserted separately, so
    /// "it round-trips" cannot mean "both ends are wrong the same way".
    #[test]
    fn the_palette_parameters_round_trip() {
        let c = church("Grace", None, "");
        let url = share_url(
            PWA_URL,
            &c,
            &ShareOpts {
                lang: Some("pa"),
                thread: Some("Romans Road"),
                devotional: Some("new-believer-30"),
                ..Default::default()
            },
        );
        let q = url.split_once('?').unwrap().1;
        assert_eq!(shared_lang(q), Some("pa"));
        assert_eq!(shared_thread(q).as_deref(), Some("Romans Road"));
        assert_eq!(shared_devotional(q).as_deref(), Some("new-believer-30"));
        // A space in a thread name survives the trip — the form encoding that
        // rescued "Faith + Hope Chapel" is what carries "Romans Road" too.
        assert!(url.contains("thread=Romans+Road"), "{url}");
    }

    /// A stranger's query string cannot strand the reader.
    ///
    /// Every one of these is a value the app must IGNORE rather than half-apply:
    /// an unknown language would leave the reader between two catalogues, and an
    /// unbounded name would pad the address bar of a link someone is deciding
    /// whether to trust.
    #[test]
    fn a_strangers_palette_parameters_are_refused() {
        assert_eq!(shared_lang("?lang=xx"), None);
        assert_eq!(shared_lang("?lang="), None);
        assert_eq!(shared_lang("?church=Grace"), None);
        // Canonicalized, not passed through: the registry's code is what the
        // rest of the app switches on.
        assert_eq!(shared_lang("?lang=PA"), Some("pa"));
        assert_eq!(shared_lang("?lang=pa-IN"), Some("pa"));
        assert_eq!(shared_lang("?lang=de-AT"), Some("de"));

        assert_eq!(shared_thread("?thread=%20%20"), None);
        assert_eq!(shared_thread(&format!("?thread={}", "t".repeat(TARGET_MAX + 1))), None);
        assert_eq!(shared_thread(&format!("?thread={}", "t".repeat(TARGET_MAX))).map(|t| t.len()), Some(TARGET_MAX));
        assert_eq!(shared_devotional("?devotional="), None);
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
        let link = share_url(PWA_URL, &church("Grace", None, ""), &ShareOpts { at: None, ..Default::default() });
        assert_eq!(with_at(&link, "1John 4:8"), format!("{link}&at=1John+4%3A8"));
        assert_eq!(with_at(&link, "  "), link, "no verse, no change");
        // Setting it twice sets it, rather than appending a second one
        // `URLSearchParams.get` would ignore.
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

        // A church writes its address the way it is on the sign. Each of these
        // gets `https://` supplied — the insecure one is never guessed on a
        // reader's behalf — and `www` is just part of the host, not a case.
        for (typed, want) in [
            ("gracebible.org", "https://gracebible.org"),
            ("www.gracebible.org", "https://www.gracebible.org"),
            ("  gracebible.org/welcome  ", "https://gracebible.org/welcome"),
            ("grace.org:8080/x", "https://grace.org:8080/x"),
        ] {
            assert_eq!(safe_url(typed), Some(want.to_string()), "{typed:?}");
        }

        for bad in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            // `javascript://…` is a valid javascript URL: the `//` comments out
            // the first line and everything after the newline runs. The case the
            // scheme check exists for — the others have no `://` at all.
            "javascript://grace.org/%0aalert(1)",
            "JAVASCRIPT://grace.org/\u{000a}alert(1)",
            "data:text/html,<script>",
            "ftp://files.grace.org",
            "mailto:pastor@grace.org",
            "https://",
            "https://@",
            "",
            "   ",
            "https://grace.org\njavascript:alert(1)",
            // Supplying a scheme must not turn the church's NAME, typed into the
            // website field, into a link the recipient is offered.
            "Grace Bible Church",
            "gracebible",
            // Nor may it repair a scheme we refuse: `https://javascript:alert(1)`
            // would be a link where a refusal belongs.
            "javascript:void(0)",
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
