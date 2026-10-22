//! Encrypted backups: the reader's study data, sealed under a phrase.
//!
//! The file is an **age** file (<https://age-encryption.org/v1>) with a single
//! passphrase (`scrypt`) recipient — a published, stable format with several
//! independent implementations, so a backup is readable without Plumbline:
//! `age -d plumbline-backup.age` and the phrase recover the zip inside. That is
//! the point of choosing it over a private container: the reader's data stays
//! theirs even if this app is gone.
//!
//! This module implements exactly the subset a backup needs — the v1 header,
//! the scrypt recipient, and the STREAM payload — over the RustCrypto
//! primitives, and is checked against the age test vectors in `age-vectors/`
//! (from <https://github.com/C2SP/CCTV>, which permits copying without
//! attribution). Nothing here generates randomness: the caller supplies the
//! entropy, as it supplies every timestamp, so the core stays pure and the
//! tests deterministic.
//!
//! The **phrase** is five words from the BIP39 English list (2048 words, MIT
//! licensed — `bip39-english.txt`): 55 bits, which under scrypt is out of reach
//! of a brute-force search even with the file in hand, and still something a
//! person can write inside a Bible cover. The app makes the phrase; it never
//! asks the reader to invent one, because invented ones are guessable.

use std::fmt::Write as _;

use base64::engine::general_purpose::STANDARD_NO_PAD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit as _};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// The first bytes of every age file — what tells a backup apart from a plain zip.
pub const MAGIC: &[u8] = b"age-encryption.org/v1\n";

/// Random bytes an [`encrypt`] needs: file key, scrypt salt, payload nonce.
pub const ENTROPY_LEN: usize = 48;

/// Random bytes a [`phrase`] needs: 55 bits, read from the first 8.
pub const PHRASE_ENTROPY_LEN: usize = 8;

/// Words in a phrase. 5 × 11 bits = 55 bits.
pub const PHRASE_WORDS: usize = 5;

/// log2 of the scrypt work factor we write. 2^15 × 128 × 8 = 32 MiB of memory
/// and a few hundred ms in wasm — cheap for the reader, and with a 55-bit phrase
/// a search still costs ~2^55 × that. (age's own CLI defaults to 18; both read
/// the other's files, since the factor is carried in the header.)
pub const WORK_FACTOR: u8 = 15;

/// The largest work factor [`decrypt`] will attempt: 2^20 is a gigabyte and
/// seconds; anything above is refused as a header error, as age does.
const MAX_WORK_FACTOR: u8 = 20;

const CHUNK: usize = 64 * 1024;
const TAG: usize = 16;

/// Why a backup could not be sealed or opened. Which of these it is matters
/// to the shell: a wrong phrase is asked again, the others are reported.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("not enough entropy supplied")]
    Entropy,
    #[error("not an encrypted backup")]
    NotAge,
    #[error("damaged backup header: {0}")]
    Header(String),
    /// Parsed fine, but no scrypt recipient / the phrase does not unwrap it.
    #[error("the phrase does not unlock this backup")]
    WrongPhrase,
    #[error("the backup header has been altered")]
    HeaderMac,
    #[error("the backup is damaged: {0}")]
    Payload(String),
}

// ── phrase ────────────────────────────────────────────────────────────────────

fn wordlist() -> &'static [&'static str] {
    static WORDS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| include_str!("backup/bip39-english.txt").lines().filter(|l| !l.is_empty()).collect())
}

/// Five words from `entropy` (≥ 8 bytes), space-separated, lowercase.
pub fn phrase(entropy: &[u8]) -> Result<String, Error> {
    if entropy.len() < PHRASE_ENTROPY_LEN {
        return Err(Error::Entropy);
    }
    let words = wordlist();
    debug_assert_eq!(words.len(), 2048);
    let mut bits = u64::from_be_bytes(entropy[..8].try_into().expect("8 bytes"));
    let mut out = Vec::with_capacity(PHRASE_WORDS);
    for _ in 0..PHRASE_WORDS {
        out.push(words[(bits & 0x7ff) as usize]);
        bits >>= 11;
    }
    Ok(out.join(" "))
}

/// The one spelling of a phrase: trimmed, lowercase, single spaces. Applied
/// before every seal and unseal, so "Harbor  Pencil" typed on a phone opens a
/// file sealed with "harbor pencil". A phrase from the `age` CLI goes through
/// the same, so the reader types the words, not the whitespace.
pub fn normalize_phrase(s: &str) -> String {
    s.split_whitespace().map(|w| w.to_lowercase()).collect::<Vec<_>>().join(" ")
}

/// Whether `bytes` are an age file (as opposed to a plain zip).
pub fn is_encrypted(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

// ── primitives ────────────────────────────────────────────────────────────────

fn hkdf(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let mut okm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(salt), ikm).expand(info, &mut okm).expect("32 bytes is a valid HKDF length");
    okm
}

fn scrypt_key(phrase: &str, salt: &[u8], log_n: u8) -> [u8; 32] {
    let mut s = Vec::with_capacity(28 + salt.len());
    s.extend_from_slice(b"age-encryption.org/v1/scrypt");
    s.extend_from_slice(salt);
    let params = scrypt::Params::new(log_n, 8, 1, 32).expect("valid scrypt params");
    let mut key = [0u8; 32];
    scrypt::scrypt(phrase.as_bytes(), &s, &params, &mut key).expect("32 bytes is a valid scrypt length");
    key
}

fn header_mac(file_key: &[u8; 16], header: &[u8]) -> [u8; 32] {
    let key = hkdf(file_key, &[], b"header");
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&key).expect("any key length");
    mac.update(header);
    mac.finalize().into_bytes().into()
}

fn chunk_nonce(counter: u64, last: bool) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[3..11].copy_from_slice(&counter.to_be_bytes());
    n[11] = last as u8;
    n
}

// ── encrypt ───────────────────────────────────────────────────────────────────

/// Seal `plain` under `phrase` as an age file. `entropy` must be at least
/// [`ENTROPY_LEN`] bytes from a CSPRNG and never reused.
pub fn encrypt(plain: &[u8], phrase: &str, entropy: &[u8]) -> Result<Vec<u8>, Error> {
    if entropy.len() < ENTROPY_LEN {
        return Err(Error::Entropy);
    }
    let phrase = normalize_phrase(phrase);
    let file_key: [u8; 16] = entropy[0..16].try_into().expect("16 bytes");
    let salt: [u8; 16] = entropy[16..32].try_into().expect("16 bytes");
    let nonce: [u8; 16] = entropy[32..48].try_into().expect("16 bytes");

    // Header: the one scrypt stanza wrapping the file key, then the MAC.
    let wrap_key = scrypt_key(&phrase, &salt, WORK_FACTOR);
    let wrapped = ChaCha20Poly1305::new((&wrap_key).into())
        .encrypt((&[0u8; 12]).into(), &file_key[..])
        .expect("in-memory AEAD cannot fail");
    let mut header = String::new();
    header.push_str(std::str::from_utf8(MAGIC).expect("ascii"));
    let _ = writeln!(header, "-> scrypt {} {}", B64.encode(salt), WORK_FACTOR);
    header.push_str(&B64.encode(wrapped));
    header.push('\n');
    header.push_str("---");
    let mac = header_mac(&file_key, header.as_bytes());
    header.push(' ');
    header.push_str(&B64.encode(mac));
    header.push('\n');

    // Payload: the nonce, then STREAM chunks.
    let mut out = header.into_bytes();
    out.extend_from_slice(&nonce);
    let payload_key = hkdf(&file_key, &nonce, b"payload");
    let cipher = ChaCha20Poly1305::new((&payload_key).into());
    let mut chunks = plain.chunks(CHUNK).peekable();
    let mut counter = 0u64;
    if chunks.peek().is_none() {
        // An empty payload is one empty final chunk — a tag and nothing else.
        out.extend(cipher.encrypt((&chunk_nonce(0, true)).into(), &[][..]).expect("AEAD"));
    }
    while let Some(c) = chunks.next() {
        let last = chunks.peek().is_none();
        out.extend(cipher.encrypt((&chunk_nonce(counter, last)).into(), c).expect("AEAD"));
        counter += 1;
    }
    Ok(out)
}

// ── decrypt ───────────────────────────────────────────────────────────────────

struct Stanza {
    args: Vec<String>,
    body: Vec<u8>,
}

struct Header {
    stanzas: Vec<Stanza>,
    /// Bytes the MAC covers: from the version line through `---`.
    signed: Vec<u8>,
    mac: [u8; 32],
    /// Where the payload begins.
    end: usize,
}

fn is_b64_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'+' || b == b'/'
}

/// One line, without its LF; `None` at end of input. CRLF is not a line ending here.
fn take_line<'a>(bytes: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
    if *at >= bytes.len() {
        return None;
    }
    let rest = &bytes[*at..];
    let nl = rest.iter().position(|&b| b == b'\n')?;
    *at += nl + 1;
    Some(&rest[..nl])
}

fn parse_header(bytes: &[u8]) -> Result<Header, Error> {
    let bad = |what: &str| Error::Header(what.to_string());
    if !bytes.starts_with(MAGIC) {
        return Err(Error::NotAge);
    }
    let mut at = MAGIC.len();
    let mut stanzas = Vec::new();
    loop {
        let line = take_line(bytes, &mut at).ok_or_else(|| bad("the header never ends"))?;
        if let Some(rest) = line.strip_prefix(b"-> ") {
            let rest = std::str::from_utf8(rest).map_err(|_| bad("a stanza argument is not text"))?;
            let args: Vec<String> = rest.split(' ').map(str::to_string).collect();
            if args.iter().any(|a| a.is_empty() || !a.bytes().all(|b| (0x21..=0x7e).contains(&b))) {
                return Err(bad("a stanza argument is empty or not printable"));
            }
            // Body: full 64-column lines, then one shorter (possibly empty) line.
            let mut b64 = Vec::new();
            loop {
                let l = take_line(bytes, &mut at).ok_or_else(|| bad("a stanza body never ends"))?;
                if l.len() > 64 || !l.iter().all(|&b| is_b64_char(b)) {
                    return Err(bad("a stanza body line is not base64"));
                }
                b64.extend_from_slice(l);
                if l.len() < 64 {
                    break;
                }
            }
            let body = B64.decode(&b64).map_err(|_| bad("a stanza body is not canonical base64"))?;
            stanzas.push(Stanza { args, body });
        } else if let Some(rest) = line.strip_prefix(b"---") {
            if stanzas.is_empty() {
                return Err(bad("no recipients"));
            }
            let Some(mac_b64) = rest.strip_prefix(b" ") else {
                return Err(bad("the MAC line is malformed"));
            };
            if mac_b64.len() != 43 || !mac_b64.iter().all(|&b| is_b64_char(b)) {
                return Err(bad("the MAC is not 43 base64 characters"));
            }
            let mac = B64.decode(mac_b64).map_err(|_| bad("the MAC is not canonical base64"))?;
            let mac: [u8; 32] = mac.try_into().map_err(|_| bad("the MAC is not 32 bytes"))?;
            // `at` is past the MAC line's LF; the line is "--- " + 43 + LF = 48
            // bytes, and the signed region ends after its "---".
            let signed_end = at - 48 + 3;
            let signed = bytes[..signed_end].to_vec();
            debug_assert!(signed.ends_with(b"---"));
            return Ok(Header { stanzas, signed, mac, end: at });
        } else {
            return Err(bad("an unexpected line in the header"));
        }
    }
}

/// The scrypt work factor argument: decimal, no sign, no leading zero, in range.
fn parse_work_factor(s: &str) -> Result<u8, Error> {
    let bad = |what: &str| Error::Header(what.to_string());
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0')) {
        return Err(bad("the scrypt work factor is not a plain decimal"));
    }
    let n: u8 = s.parse().map_err(|_| bad("the scrypt work factor is out of range"))?;
    if n == 0 {
        return Err(bad("the scrypt work factor is zero"));
    }
    if n > MAX_WORK_FACTOR {
        return Err(bad("the scrypt work factor is too expensive to try"));
    }
    Ok(n)
}

/// Unwrap the file key with `phrase`. An scrypt stanza must be the only one.
fn unwrap_scrypt(h: &Header, phrase: &str) -> Result<[u8; 16], Error> {
    let bad = |what: &str| Error::Header(what.to_string());
    let scrypts: Vec<&Stanza> =
        h.stanzas.iter().filter(|s| s.args.first().map(String::as_str) == Some("scrypt")).collect();
    let Some(st) = scrypts.first() else {
        return Err(Error::WrongPhrase); // no passphrase recipient at all
    };
    if h.stanzas.len() != 1 {
        return Err(bad("an scrypt recipient must be alone"));
    }
    if st.args.len() != 3 {
        return Err(bad("the scrypt stanza needs a salt and a work factor"));
    }
    let salt = B64.decode(&st.args[1]).map_err(|_| bad("the scrypt salt is not canonical base64"))?;
    if salt.len() != 16 {
        return Err(bad("the scrypt salt is not 16 bytes"));
    }
    let log_n = parse_work_factor(&st.args[2])?;
    if st.body.len() != 32 {
        return Err(bad("the wrapped file key is not 32 bytes"));
    }
    let key = scrypt_key(phrase, &salt, log_n);
    let fk = ChaCha20Poly1305::new((&key).into())
        .decrypt((&[0u8; 12]).into(), &st.body[..])
        .map_err(|_| Error::WrongPhrase)?;
    Ok(fk.try_into().expect("32 - 16 = 16"))
}

/// Decrypt the STREAM payload into `out`. What was verified before an error is
/// left in `out` — a caller that wants all-or-nothing clears it on `Err`.
///
/// A full-size chunk is ambiguous until opened: it is a middle chunk unless it
/// fails as one and succeeds as the final chunk — the reference implementation
/// tries it both ways, and the test vectors pin what it releases.
fn decrypt_payload(file_key: &[u8; 16], payload: &[u8], out: &mut Vec<u8>) -> Result<(), Error> {
    let bad = |what: &str| Error::Payload(what.to_string());
    if payload.len() < 16 {
        // The reference counts the nonce as the header's tail: a header failure.
        return Err(Error::Header("no payload nonce".to_string()));
    }
    let (nonce, mut rest) = payload.split_at(16);
    let key = hkdf(file_key, nonce, b"payload");
    let cipher = ChaCha20Poly1305::new((&key).into());
    if rest.is_empty() {
        return Err(bad("no payload"));
    }
    let mut counter = 0u64;
    loop {
        let full = rest.len() >= CHUNK + TAG;
        let (c, after) = rest.split_at(if full { CHUNK + TAG } else { rest.len() });
        if c.len() < TAG {
            return Err(bad("a short final chunk"));
        }
        let open = |last: bool| cipher.decrypt((&chunk_nonce(counter, last)).into(), c);
        if !full {
            // Short, so final by construction.
            let plain = open(true).map_err(|_| bad("the final chunk failed to authenticate"))?;
            if plain.is_empty() && counter != 0 {
                return Err(bad("an empty final chunk"));
            }
            out.extend_from_slice(&plain);
            return Ok(());
        }
        match open(false) {
            Ok(plain) => {
                out.extend_from_slice(&plain);
                if after.is_empty() {
                    return Err(bad("the stream ended without a final chunk"));
                }
            }
            Err(_) => {
                let plain = open(true).map_err(|_| bad("a chunk failed to authenticate"))?;
                out.extend_from_slice(&plain);
                return if after.is_empty() { Ok(()) } else { Err(bad("data after the final chunk")) };
            }
        }
        rest = after;
        counter += 1;
    }
}

/// Decrypt an age file given its file key — the header still has to parse and
/// authenticate. What [`decrypt`] does once the phrase has unwrapped the key.
fn decrypt_with_file_key(file: &[u8], h: &Header, file_key: &[u8; 16], out: &mut Vec<u8>) -> Result<(), Error> {
    let expect = header_mac(file_key, &h.signed);
    // Constant-time compare, as a MAC check must be.
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&hkdf(file_key, &[], b"header")).expect("any key length");
    mac.update(&h.signed);
    if mac.verify_slice(&h.mac).is_err() {
        debug_assert_ne!(expect, h.mac);
        return Err(Error::HeaderMac);
    }
    decrypt_payload(file_key, &file[h.end..], out)
}

/// Open an age file sealed with `phrase`. All or nothing: on any error the
/// plaintext is discarded.
pub fn decrypt(file: &[u8], phrase: &str) -> Result<Vec<u8>, Error> {
    let phrase = normalize_phrase(phrase);
    let h = parse_header(file)?;
    let file_key = unwrap_scrypt(&h, &phrase)?;
    let mut out = Vec::new();
    decrypt_with_file_key(file, &h, &file_key, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    fn entropy(seed: u8) -> Vec<u8> {
        (0..ENTROPY_LEN as u8).map(|i| i.wrapping_mul(31).wrapping_add(seed)).collect()
    }

    #[test]
    fn a_phrase_is_five_list_words_and_deterministic() {
        let p = phrase(&[0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(p, "abandon abandon abandon abandon abandon");
        let p = phrase(&[0xff; 8]).unwrap();
        assert_eq!(p, "zoo zoo zoo zoo zoo");
        let p = phrase(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        assert_eq!(p.split(' ').count(), PHRASE_WORDS);
        for w in p.split(' ') {
            assert!(wordlist().contains(&w), "{w} is not on the list");
        }
        assert_eq!(phrase(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap(), p, "same entropy, same phrase");
        assert_eq!(phrase(&[1, 2, 3]), Err(Error::Entropy));
        assert_eq!(wordlist().len(), 2048);
    }

    #[test]
    fn spelling_of_a_phrase_does_not_matter() {
        assert_eq!(normalize_phrase("  Harbor\tPencil  olive\nDRUM  "), "harbor pencil olive drum");
        let sealed = encrypt(b"hello", "harbor pencil", &entropy(1)).unwrap();
        assert_eq!(decrypt(&sealed, " HARBOR  pencil ").unwrap(), b"hello");
    }

    #[test]
    fn round_trip_at_the_chunk_edges() {
        for len in [0usize, 1, CHUNK - 1, CHUNK, CHUNK + 1, 2 * CHUNK, 2 * CHUNK + 7] {
            let plain: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            let sealed = encrypt(&plain, "olive drum", &entropy(len as u8)).unwrap();
            assert!(is_encrypted(&sealed));
            assert!(!is_encrypted(&plain));
            assert_eq!(decrypt(&sealed, "olive drum").unwrap(), plain, "len {len}");
        }
    }

    #[test]
    fn the_wrong_phrase_is_told_apart_from_damage() {
        let sealed = encrypt(b"the waters", "olive drum", &entropy(3)).unwrap();
        assert_eq!(decrypt(&sealed, "olive drums"), Err(Error::WrongPhrase));
        // A flipped payload byte: the chunk fails to authenticate.
        let mut bad = sealed.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(matches!(decrypt(&bad, "olive drum"), Err(Error::Payload(_))));
        // A flipped header byte (the salt): the stanza no longer unwraps.
        let mut bad = sealed.clone();
        bad[MAGIC.len() + 12] = if bad[MAGIC.len() + 12] == b'A' { b'B' } else { b'A' };
        assert!(matches!(decrypt(&bad, "olive drum"), Err(Error::WrongPhrase | Error::Header(_))));
        assert_eq!(decrypt(b"PK\x03\x04 a zip", "olive drum"), Err(Error::NotAge));
        assert_eq!(encrypt(b"x", "p", &[0u8; 10]), Err(Error::Entropy));
    }

    #[test]
    fn the_header_we_write_is_what_age_specifies() {
        let sealed = encrypt(b"x", "olive drum", &entropy(9)).unwrap();
        let text = String::from_utf8_lossy(&sealed[..sealed.len() - 16 - 1 - 16]);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "age-encryption.org/v1");
        assert!(lines[1].starts_with("-> scrypt "), "{}", lines[1]);
        assert_eq!(lines[1].split(' ').count(), 4, "-> scrypt <salt> <log N>");
        assert_eq!(lines[1].split(' ').nth(3), Some("15"));
        assert_eq!(lines[2].len(), 43, "32 wrapped bytes");
        assert!(lines[3].starts_with("--- ") && lines[3].len() == 4 + 43);
    }

    // ── the age test vectors (C2SP/CCTV) ─────────────────────────────────────
    //
    // Each file: a key/value preamble, a blank line, then the age file (zlib
    // compressed when `compressed: zlib`). A vector with a `passphrase` goes
    // through `decrypt`; one with only an identity we cannot hold goes through
    // the file key the vector prints, which still exercises the header parser,
    // the MAC and the STREAM payload. `payload` is the SHA-256 of everything
    // that was released — on a payload failure, of what was verified before it.

    struct Vector {
        name: String,
        expect: String,
        payload: Option<String>,
        file_key: Option<[u8; 16]>,
        passphrase: Option<String>,
        file: Vec<u8>,
    }

    fn vectors() -> Vec<Vector> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/backup/age-vectors");
        let mut out = Vec::new();
        let mut names: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        names.sort();
        for path in names {
            let raw = std::fs::read(&path).unwrap();
            let split = raw.windows(2).position(|w| w == b"\n\n").expect("preamble ends") + 2;
            let pre = std::str::from_utf8(&raw[..split]).unwrap();
            let mut v = Vector {
                name: path.file_name().unwrap().to_string_lossy().into_owned(),
                expect: String::new(),
                payload: None,
                file_key: None,
                passphrase: None,
                file: raw[split..].to_vec(),
            };
            let mut zlib = false;
            for line in pre.lines() {
                let Some((k, val)) = line.split_once(": ") else { continue };
                match k {
                    "expect" => v.expect = val.to_string(),
                    "payload" => v.payload = Some(val.to_string()),
                    "passphrase" if v.passphrase.is_none() => v.passphrase = Some(val.to_string()),
                    "file key" => {
                        let bytes: Vec<u8> = (0..val.len())
                            .step_by(2)
                            .map(|i| u8::from_str_radix(&val[i..i + 2], 16).unwrap())
                            .collect();
                        v.file_key = bytes.try_into().ok();
                    }
                    "compressed" => zlib = val == "zlib",
                    _ => {}
                }
            }
            if zlib {
                use std::io::Read;
                let mut plain = Vec::new();
                flate2::read::ZlibDecoder::new(&v.file[..]).read_to_end(&mut plain).unwrap();
                v.file = plain;
            }
            out.push(v);
        }
        assert!(out.len() > 70, "the vectors are present");
        out
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn the_age_test_vectors_pass() {
        for v in vectors() {
            let mut released = Vec::new();
            let result: Result<(), Error> = (|| {
                let h = parse_header(&v.file)?;
                let fk = match &v.passphrase {
                    Some(p) => unwrap_scrypt(&h, &normalize_phrase(p))?,
                    None => v.file_key.expect("a vector without a passphrase prints its file key"),
                };
                decrypt_with_file_key(&v.file, &h, &fk, &mut released)
            })();
            let got = match &result {
                Ok(()) => "success",
                Err(Error::NotAge | Error::Header(_)) => "header failure",
                Err(Error::WrongPhrase) => "no match",
                Err(Error::HeaderMac) => "HMAC failure",
                Err(Error::Payload(_)) => "payload failure",
                Err(Error::Entropy) => unreachable!(),
            };
            assert_eq!(got, v.expect, "{}: {:?}", v.name, result);
            if let Some(want) = &v.payload {
                // Everything released must match — a payload failure included.
                if got == "success" || got == "payload failure" {
                    assert_eq!(&hex(&Sha256::digest(&released)), want, "{}: released payload", v.name);
                }
            }
            // And the public entry point agrees with the vector when it can be used at all.
            if let Some(p) = &v.passphrase {
                match decrypt(&v.file, p) {
                    Ok(_) => assert_eq!(got, "success", "{}", v.name),
                    Err(_) => assert_ne!(got, "success", "{}", v.name),
                }
                if got == "success" {
                    assert_eq!(hex(&Sha256::digest(decrypt(&v.file, p).unwrap())), v.payload.clone().unwrap());
                }
            }
        }
    }

    #[test]
    fn what_we_write_passes_the_vector_reader_too() {
        // The vectors can only check decryption, so this closes the loop: a file
        // we wrote parses and authenticates through the same path they do.
        let plain = b"\x50\x4b\x03\x04 a backup zip";
        let sealed = encrypt(plain, "harbor pencil olive drum acid", &entropy(7)).unwrap();
        let h = parse_header(&sealed).unwrap();
        assert_eq!(h.stanzas.len(), 1);
        assert_eq!(h.stanzas[0].args, ["scrypt", &B64.encode(&entropy(7)[16..32]), "15"]);
        let fk: [u8; 16] = entropy(7)[0..16].try_into().unwrap();
        let mut out = Vec::new();
        decrypt_with_file_key(&sealed, &h, &fk, &mut out).unwrap();
        assert_eq!(out, plain);
    }
}
