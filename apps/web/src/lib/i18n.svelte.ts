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

/** The base tag of a locale, if this build ships that language. */
function shipped(tag: string | null | undefined): string | null {
  const base = (tag ?? "").split(/[-_]/)[0].toLowerCase();
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
  }));
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

/** Whether picking this language means fetching its scripture first. Asked of
 *  the engine's own registry rather than decided here — see `LanguageChoice`. */
export function needsPack(code: string): boolean {
  return (choices.find((l) => l.code === code)?.packFiles.length ?? 0) > 0;
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
