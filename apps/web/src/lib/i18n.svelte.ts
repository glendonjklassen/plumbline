// The web shell's half of core::i18n: one reactive string table, filled once at
// boot, read synchronously by every component.
//
// `t()` is a plain function over `$state`, which is what makes a language change
// a repaint rather than a reload — a component that calls `t("nav.read")` in its
// markup re-runs when the table is replaced, with no subscription to write and
// nothing to remember to invalidate.
//
// THE TABLE IS NEVER PARTIAL. The engine resolves the reader's language over
// English key by key (crates/core/src/i18n.rs), so what arrives here answers
// every id that exists. `t()` still has an answer for an id that does not — the
// id itself, which is visible, greppable, and impossible to mistake for copy.
//
// Formatting mirrors `i18n::format` in the core, including the part that looks
// like a bug: a `{placeholder}` with no argument is LEFT ON SCREEN rather than
// blanked, because "Read through {book}" missing its book reads like finished
// copy while the braces name the argument that went missing.

import { BOOT_STRINGS } from "./i18n.generated";
import { shippedBase } from "./locale";
import { DEFAULT_FONT, FONT_SCRIPT, SCRIPT_FACE } from "../engine/fonts.generated";

/** Where the resolved code is remembered between visits — see `seed()`. */
const LAST_LANG = "plumbline.lang";

/** The code this device resolved last time, or "" if it never has.
 *
 *  Read here because `localStorage` is main-thread only — the engine worker has
 *  none, and stage 1 needs this answer before there is an engine to ask. */
export function lastLang(): string {
  try {
    return localStorage.getItem(LAST_LANG) ?? "";
  } catch {
    return "";
  }
}

/** The device's own languages, most-preferred first, as BCP-47 tags. */
export function deviceLocale(): string {
  return navigator.languages?.[0] || navigator.language || "en";
}

/** The shipped code of a locale, if this build ships that language —
 *  `shippedBase` handles the Chinese routing (`zh-TW` → `zht`), so the splash
 *  can speak Chinese before the engine has said anything. */
function shipped(tag: string | null | undefined): string | null {
  const base = shippedBase(tag);
  return base && BOOT_STRINGS[base] ? base : null;
}

/** What to paint with before the engine has said anything.
 *
 *  LAST SESSION'S ANSWER FIRST, the device's locale second. The reader's setting
 *  lives in the config, which the engine has not read yet, so a reader who chose
 *  English on a German phone would watch a German splash on every cold start if
 *  this only asked the hardware. Remembering the resolved code is the same trick
 *  index.html plays with the theme palette, for the same reason: the first frame
 *  has to be right, and last session's answer is the best guess available before
 *  the truth arrives milliseconds later. */
function seed(): { code: string; strings: Record<string, string> } {
  let code = "en";
  try {
    code = shipped(localStorage.getItem(LAST_LANG)) ?? shipped(deviceLocale()) ?? "en";
  } catch {
    // Storage refused (a private window, site data blocked). The device's
    // locale still answers, and if that throws too English is not a failure.
    code = shipped(deviceLocale()) ?? "en";
  }
  return { code, strings: { ...BOOT_STRINGS.en, ...(BOOT_STRINGS[code] ?? {}) } };
}

const initial = seed();
let strings = $state<Record<string, string>>(initial.strings);
let code = $state<string>(initial.code);
/** Whether the first-run prose exists in the painted language — the engine's
 *  answer (`i18n::Lang::has_native_intros`), never derived here.
 *
 *  Seeded from the guessed code rather than defaulting to true, so the one frame
 *  before the boot reply cannot offer a German reader a path into English
 *  paragraphs. English is the language the prose is written in, so the seed is
 *  right for it and conservative for everyone else. */
let nativeIntros = $state<boolean>(initial.code === "en");
export interface LanguageChoice {
  code: string;
  endonym: string;
  name: string;
  /** The Bible a reader of this language gets: "KJV", "Luther", "Reina-Valera". */
  bible: string;
  /** Home paths this language needs that the base pack does not carry. Empty
   *  for English — and the whole answer to "is there anything to download when
   *  the reader picks this", which used to be `code === "de"` in Settings. */
  packFiles: string[];
  /** Whether it has a Strong's dictionary of its own (machine-translated), and
   *  therefore whether the "English definitions instead" escape hatch applies. */
  hasLexicon: boolean;
  /** The manifest role its scripture is filed under — `corpusCache` for the
   *  base pack's own text, `corpus:<code>` for a Bible of its own. Settings
   *  reads it to know whether picking this language must first make sure that
   *  Bible is really on the device (see `hasOwnBible`). */
  corpusRole: string;
  /** Whether this language is written right to left. Straight off the registry
   *  row (`core::i18n::Script::is_rtl`) — the chrome's business only. Direction
   *  INSIDE the text is settled in the engine, which mirrors the display list
   *  and does not consult a shell. */
  rtl: boolean;
  /** The writing system, as `core::i18n::Script`'s token. WHICH FACES CAN SET
   *  THIS LANGUAGE, and a separate question from `rtl` — which is what it was
   *  being asked as while Arabic was the only non-Latin script and the two had
   *  the same answer. Gurmukhi and Devanagari read left to right and no Latin
   *  face has a glyph of either. */
  script: string;
}

let choices = $state<LanguageChoice[]>([]);

/** The catalogue the engine resolved, as it came back over the ABI. Replaces
 *  the boot seed wholesale — including the `boot.*` keys, so a language the
 *  seed guessed wrong is corrected everywhere at once. */
export function setCatalog(
  cat: { lang?: string; strings?: Record<string, string>; languages?: any[]; nativeIntros?: boolean } | null,
): void {
  if (!cat?.strings) return;
  strings = cat.strings;
  code = cat.lang ?? "en";
  nativeIntros = cat.nativeIntros === true;
  choices = (cat.languages ?? []).map((l) => ({
    code: String(l.code),
    endonym: String(l.endonym),
    name: String(l.name ?? ""),
    bible: String(l.bible ?? ""),
    packFiles: Array.isArray(l.packFiles) ? l.packFiles.map(String) : [],
    hasLexicon: typeof l.lexiconRole === "string",
    corpusRole: String(l.corpusRole ?? "corpusCache"),
    rtl: l.rtl === true,
    script: String(l.script ?? "latin"),
  }));
  // THE DOCUMENT'S OWN DIRECTION, set here because this is the one place the
  // interface language becomes current.
  //
  // `dir` is what mirrors the CHROME — every `margin-inline`, the order of a
  // flex row, which side a scrollbar sits on, which way a native `<select>`
  // opens. It is deliberately NOT what decides the reader: scripture is a
  // canvas painted from a display list the engine already mirrored, and it
  // takes its direction from the open corpus (`DisplayList.rtl`), so an Arabic
  // reader whose download has not landed gets English chrome around an English
  // Bible rather than a mirrored shell around a left-to-right text.
  //
  // `lang` alongside it, because it was hardcoded `en` in index.html: it is what
  // a screen reader picks a voice from, and what the browser hyphenates by.
  try {
    const el = document.documentElement;
    el.lang = code;
    el.dir = choices.find((l) => l.code === code)?.rtl ? "rtl" : "ltr";
  } catch {
    /* no document (a test harness importing this): nothing to mirror */
  }
  try {
    localStorage.setItem(LAST_LANG, code);
  } catch {
    // Not being able to remember it costs one frame of the wrong splash next
    // time. It is not worth failing a boot over.
  }
}

/** The language being painted, as a code. */
export function lang(): string {
  return code;
}

/** Every language this build ships, each labelled in ITSELF — a reader looking
 *  for German is looking for "Deutsch". Empty until the boot reply lands. */
export function languages(): LanguageChoice[] {
  return choices;
}

/** How a language is named in a picker: its own name, then the reader's name
 *  for it — "ਪੰਜਾਬੀ (Punjabi)" to an English reader, "English (Englisch)" to a
 *  German one.
 *
 *  EITHER HALF ALONE SERVES ONE PERSON AND FAILS THE OTHER. The endonym alone
 *  is right for someone looking for their own language and useless to someone
 *  handing their phone to a Hindi speaker — six scripts they cannot read, and
 *  no way to tell which row is the one. The reader's own name alone is the
 *  reverse: it puts "Punjabi" in Latin letters in front of the person who is
 *  being handed the phone. This app is built to be handed over, so it shows
 *  both, and the ENDONYM LEADS because the row belongs to the person being
 *  offered it; the bracket is the reader's note to themselves.
 *
 *  IN EVERY LANGUAGE, which is what makes the bracket a catalogue lookup rather
 *  than the registry's `exonym`. That column is the language's ENGLISH name —
 *  it is what the hymnal finder matches on — so it can only ever have served an
 *  English reader. `lang.<code>` is in all six catalogues, so a German reader
 *  gets "Englisch" where an Arabic one gets "الإنجليزية".
 *
 *  No bracket when the two are the same word. That is not a special case for
 *  English: it is what silences the reader's OWN language in every catalogue —
 *  "Deutsch (Deutsch)" in German, "हिन्दी (हिन्दी)" in Hindi. It also covers a
 *  name a language happens to share with its endonym, which is why it compares
 *  the strings rather than the codes.
 */
export function languageLabel(l: LanguageChoice): string {
  const mine = t(`lang.${l.code}`);
  if (!mine || mine === `lang.${l.code}` || mine === l.endonym) return l.endonym;
  return `${l.endonym} (${mine})`;
}

/** Whether the language currently being painted reads right to left. */
export function isRtl(): boolean {
  return choices.find((l) => l.code === code)?.rtl === true;
}

/** The writing system the language currently being painted is set in. Latin
 *  until the boot reply lands, which is what the splash is drawn in anyway. */
export function script(): string {
  return choices.find((l) => l.code === code)?.script ?? "latin";
}

/** The face a token RESOLVES TO under the current language.
 *
 *  A reader of a non-Latin script gets that script's face whatever the config
 *  says, and this is the one rule that says so. The config keeps the reader's
 *  own (Latin) choice untouched — switching back restores it — but every
 *  APPLICATION of a token goes through here, because the alternative was
 *  measured and shipped briefly: scripture painted in Amiri through the
 *  fallback stack at a Latin face's optical scale, smaller than every other
 *  face renders, with the picker hidden and therefore no way for the reader to
 *  correct it. Settings hides the font choice when only one face can set the
 *  language (there is nothing to choose), and hiding the choice is only honest
 *  if the app makes it.
 *
 *  ONE COMPARISON AGAINST THE READER'S SCRIPT, where it used to be `isRtl()`
 *  against one token. That form has no answer for a third script: a Punjabi
 *  reader is not RTL, so the old rule handed them EB Garamond, which has no
 *  Gurmukhi in it at all. */
export function readerFace(token: string): string {
  // Symmetric on purpose. Forcing the script face is the headline; the other
  // half is the cleanup after it: a config can legitimately HOLD a script token
  // (an earlier build's one-entry picker let a reader select it), and applied
  // face-value in English that renders the KJV in naskh Latin while the picker
  // — whose list has no such row — shows a BLANK selection (maintainer's phone,
  // 2026-08-28). An off-script token resolves to the script's own face, or to
  // the default where that script is Latin; the config itself is never
  // rewritten.
  const want = script();
  if (FONT_SCRIPT[token] === want) return token;
  return SCRIPT_FACE[want] ?? DEFAULT_FONT;
}

/** Whether this language reads a Bible of its own rather than the base pack's.
 *
 *  This — not `packFiles` — is what gates the ensure-before-reload in Settings.
 *  The two parted ways when the Bibles started shipping with the app:
 *  `packFiles` now lists only the opt-in dictionary, so Arabic (which has none)
 *  came back empty, the switch skipped the ensure, and the reload raced the
 *  background download of the very text it was switching to. Asked of the
 *  engine's registry rather than decided here, for `LanguageChoice`'s reason. */
export function hasOwnBible(code: string): boolean {
  const role = choices.find((l) => l.code === code)?.corpusRole;
  return role !== undefined && role !== "corpusCache";
}

/** Whether the first-run welcome and the curious path may be OFFERED in the
 *  language being painted.
 *
 *  Those two screens are somebody speaking to a reader about their own life —
 *  which idioms land, which questions are the live ones — so they wait for
 *  someone inside that culture to write them rather than being translated. The
 *  engine decides (`i18n::Lang::has_native_intros`, derived from whether the
 *  words are actually in that language's catalogue); this only carries the
 *  answer. A shell that re-derived it by peeking at `strings` would be a second
 *  copy of the rule, and the two would disagree the first time one moved. */
export function hasNativeIntros(): boolean {
  return nativeIntros;
}

/** Whether the language being painted has a dictionary of its own. */
export function hasOwnLexicon(): boolean {
  return choices.find((l) => l.code === code)?.hasLexicon === true;
}

/** What may be substituted into a placeholder. `null`/`undefined` are allowed
 *  and leave the brace on screen, which is the same outcome as omitting the
 *  argument — a caller with an optional label should not have to decide between
 *  a cast and inventing an empty string. */
export type Arg = string | number | null | undefined;

/** Fill `{placeholders}`; an unfilled one stays visible. Mirrors `i18n::format`. */
export function fill(template: string, args?: Record<string, Arg>): string {
  if (!template.includes("{")) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name) => {
    const v = args?.[name];
    return v === undefined || v === null ? whole : String(v);
  });
}

/** One string, in the reader's language. */
export function t(id: string, args?: Record<string, Arg>): string {
  const s = strings[id];
  return s === undefined ? id : fill(s, args);
}

/** Pick between a one-form and a many-form key, lending both `n`.
 *
 *  Deliberately not a plural engine — see `i18n::plural`. English and German
 *  split exactly one/other; a language with more forms needs CLDR rules and
 *  this function replaced, not extended. */
export function plural(idOne: string, idOther: string, n: number, args?: Record<string, Arg>): string {
  return t(n === 1 ? idOne : idOther, { n, ...args });
}
