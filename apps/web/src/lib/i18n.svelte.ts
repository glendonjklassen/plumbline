// The web shell's half of core::i18n: one reactive string table, filled once at
// boot, read synchronously by every component.
//
// `t()` is a plain function over `$state`, so a language change is a repaint
// rather than a reload — markup calling `t("nav.read")` re-runs when the table is
// replaced, with nothing to subscribe to or invalidate. The table is never
// partial: the engine resolves the reader's language over English key by key
// (crates/core/src/i18n.rs). An unknown id falls back to the id itself, which is
// greppable and unmistakable for copy.
//
// Formatting mirrors `i18n::format`, including the part that looks like a bug: a
// `{placeholder}` with no argument stays on screen rather than blanking, because
// the braces name the argument that went missing.

import { BOOT_STRINGS } from "./i18n.generated";
import { shippedBase } from "./locale";
import { DEFAULT_FONT, FONT_SCRIPT, SCRIPT_FACE } from "../engine/fonts.generated";

/** Where the resolved code is remembered between visits — see `seed()`. */
const LAST_LANG = "plumbline.lang";

/** The code this device resolved last time, or "" if it never has. Read here
 *  because `localStorage` is main-thread only, and stage 1 needs this answer
 *  before there is an engine to ask. */
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

/** The shipped code of a locale, if this build ships that language.
 *  `shippedBase` handles the Chinese routing (`zh-TW` → `zht`). */
function shipped(tag: string | null | undefined): string | null {
  const base = shippedBase(tag);
  return base && BOOT_STRINGS[base] ? base : null;
}

/** What to paint with before the engine has said anything: last session's
 *  resolved code first, the device's locale second. The reader's setting lives in
 *  the config the engine has not read yet, so asking the hardware alone would
 *  give a reader who chose English on a German phone a German splash on every
 *  cold start. */
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
 *  answer (`i18n::Lang::has_native_intros`), never derived here. Seeded from the
 *  guessed code rather than `true`, so the frame before the boot reply cannot
 *  offer a non-English reader a path into English paragraphs. */
let nativeIntros = $state<boolean>(initial.code === "en");
export interface LanguageChoice {
  code: string;
  endonym: string;
  name: string;
  /** The Bible a reader of this language gets: "KJV", "Luther", "Reina-Valera". */
  bible: string;
  /** Home paths this language needs that the base pack does not carry, and so
   *  the whole answer to "is there anything to download when the reader picks
   *  this". Empty for English. */
  packFiles: string[];
  /** Whether it has a Strong's dictionary of its own (machine-translated), and
   *  therefore whether the "English definitions instead" escape hatch applies. */
  hasLexicon: boolean;
  /** The manifest role its scripture is filed under — `corpusCache` for the
   *  base pack's own text, `corpus:<code>` for a Bible of its own. Settings
   *  reads it to know whether picking this language must first make sure that
   *  Bible is really on the device (see `hasOwnBible`). */
  corpusRole: string;
  /** Whether this language is written right to left (`core::i18n::Script::is_rtl`)
   *  — the chrome's business only. Direction inside the text is settled in the
   *  engine, which mirrors the display list itself. */
  rtl: boolean;
  /** The writing system, as `core::i18n::Script`'s token: which faces can set
   *  this language. Separate from `rtl` — Gurmukhi and Devanagari read left to
   *  right and no Latin face has a glyph of either. */
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
  // The document's own direction, set here because this is the one place the
  // interface language becomes current. `dir` mirrors the chrome only; scripture
  // is a canvas painted from a display list the engine already mirrored, taking
  // its direction from the open corpus (`DisplayList.rtl`) — so an Arabic reader
  // whose download has not landed gets English chrome around an English Bible
  // rather than a mirrored shell around a left-to-right text. `lang` alongside
  // it is what a screen reader picks a voice from, and what the browser
  // hyphenates by.
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
    // Not remembering costs one frame of the wrong splash next time; never a boot.
  }
}

/** The language being painted, as a code. */
export function lang(): string {
  return code;
}

/** Every language this build ships, each labelled in itself. Empty until the
 *  boot reply lands. */
export function languages(): LanguageChoice[] {
  return choices;
}

/** How a language is named in a picker: the endonym, then the reader's name for
 *  it — "ਪੰਜਾਬੀ (Punjabi)" to an English reader, "English (Englisch)" to a German
 *  one. The endonym leads because the row belongs to the person being handed the
 *  phone; the bracket is the reader's note to themselves.
 *
 *  The bracket is the `lang.<code>` catalogue lookup, not the registry's
 *  `exonym` — that column is the language's English name (what the hymnal finder
 *  matches on), so it only ever serves an English reader.
 *
 *  No bracket when the two are the same word, which silences "Deutsch (Deutsch)"
 *  in every catalogue. It compares the strings, not the codes, so a name a
 *  language happens to share with its endonym is covered too.
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

/** The face a token resolves to under the current language.
 *
 *  A reader of a non-Latin script gets that script's face whatever the config
 *  says, and this is the one rule that says so — every application of a token
 *  goes through here. The config keeps the reader's own (Latin) choice untouched,
 *  so switching back restores it. Settings hides the font picker when only one
 *  face can set the language, which is only honest if the app makes the choice.
 *
 *  One comparison against the reader's script, not against `isRtl()`: a Punjabi
 *  reader is not RTL, and EB Garamond has no Gurmukhi in it at all. */
export function readerFace(token: string): string {
  // Symmetric on purpose: a config can legitimately hold a script token, and
  // applied face-value in English that renders the KJV in naskh Latin against a
  // picker with no such row. An off-script token resolves to the script's own
  // face, or the default where that script is Latin; the config is never
  // rewritten.
  const want = script();
  if (FONT_SCRIPT[token] === want) return token;
  return SCRIPT_FACE[want] ?? DEFAULT_FONT;
}

/** Whether this language reads a Bible of its own rather than the base pack's.
 *
 *  This — not `packFiles`, which lists only the opt-in dictionary — gates the
 *  ensure-before-reload in Settings. Using `packFiles` lets a language with no
 *  dictionary (Arabic) skip the ensure, and the reload then races the background
 *  download of the very text it is switching to. */
export function hasOwnBible(code: string): boolean {
  const role = choices.find((l) => l.code === code)?.corpusRole;
  return role !== undefined && role !== "corpusCache";
}

/** Whether the first-run welcome and the curious path may be offered in the
 *  language being painted. Those two screens wait for someone inside the culture
 *  to write them rather than being translated. The engine decides
 *  (`i18n::Lang::has_native_intros`); re-deriving it here by peeking at `strings`
 *  would be a second copy of the rule. */
export function hasNativeIntros(): boolean {
  return nativeIntros;
}

/** Whether the language being painted has a dictionary of its own. */
export function hasOwnLexicon(): boolean {
  return choices.find((l) => l.code === code)?.hasLexicon === true;
}

/** What may be substituted into a placeholder. `null`/`undefined` leave the
 *  brace on screen, the same as omitting the argument, so a caller with an
 *  optional label needs neither a cast nor an invented empty string. */
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
 *  Not a plural engine (see `i18n::plural`): English and German split exactly
 *  one/other. A language with more forms needs CLDR rules and this function
 *  replaced, not extended. */
export function plural(idOne: string, idOther: string, n: number, args?: Record<string, Arg>): string {
  return t(n === 1 ? idOne : idOther, { n, ...args });
}
