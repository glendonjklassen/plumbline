// What the splash says when boot fails.
//
// Everything that can go wrong on the way to first text arrives at App.svelte as
// one string, and until now that string WAS the error screen: a reader who lost
// their connection halfway through the first download read
// "data pack file data/kjv.jsonl.idxcache: HTTP 503", and one on a full phone
// read "QuotaExceededError: Failed to execute 'put' on 'Cache'". Neither says
// the one thing that would help, which is what they can do about it.
//
// So the raw string is translated for the reader — and KEPT. It is what a bug
// report pastes, and it is the only evidence of which rung of the boot ladder
// broke; the splash puts it behind a <details>. This module never invents a
// cause it cannot see: anything unrecognised falls through to a sentence that
// admits as much rather than guessing.
//
// It returns CATALOGUE IDS, not sentences. The words live in
// crates/core/src/i18n/*.json with every other string the reader can meet, and
// the `boot.*` keys are bundled into the shell (scripts/gen-i18n.mjs) precisely
// so that a boot which never reached the engine can still speak the reader's
// language — the one failure where the engine cannot be asked for the words.
//
// FIRST MATCH WINS, and the order below is deliberate — a failed download on a
// device that is also out of room reports the download, because that is the
// thing the reader is being asked to retry.

/** One bucket: what the raw string has to look like, and the catalogue id of
 *  what to say instead. */
type Rule = { when: RegExp; say: string };

const RULES: Rule[] = [
  // The worker died or went silent. worker-client.ts writes those strings for a
  // reader ("The study engine stopped unexpectedly — …", "…went quiet for 60s
  // and never finished starting. It got as far as opening the text.") built at
  // the throw site out of a browser's own error text and a stage name, so there
  // is nothing there to translate. The reader gets one sentence in their
  // language and the whole raw string stays one disclosure away, which is where
  // the detail belonged anyway.
  { when: /^The study engine\b/, say: "boot.error.engine" },

  // The pack format moved under a shell that predates it (pack.ts `checked`).
  // A reload picks up the current bundle, which is a thing the reader can do;
  // the rest of that message is a note to whoever builds the pack.
  {
    when: /data pack format/i,
    say: "boot.error.stale",
  },

  // No room. Two shapes: the Cache API refusing a put, and IndexedDB refusing a
  // transaction. Both mean the same thing to the reader.
  {
    when: /QuotaExceeded|quota|storage is full|no space left/i,
    say: "boot.error.quota",
  },

  // Storage refused outright rather than being full: private windows, a browser
  // set to block site data, an IndexedDB the profile has disowned.
  {
    when: /SecurityError|IDBFactory|access to storage|not allowed to (use|access)/i,
    say: "boot.error.storage",
  },

  // The network. `Failed to fetch` is chromium, `Load failed` is WebKit,
  // `NetworkError` is Firefox, and pack.ts wraps all three with the file it was
  // after. An HTTP status is a reachable server that answered badly, which is
  // still "try again" from here.
  //
  // "what it needs to open", not "the scripture data": this rule also catches a
  // failed download of the ENGINE BINARY (`plumbline_ffi.wasm` — see the 503 case
  // in boot-overlap.spec.ts), and naming the wrong payload at the reader is a
  // small lie told at the one moment they are already stuck. The advice is
  // identical either way, so the sentence does not need to guess.
  {
    when: /Failed to fetch|Load failed|NetworkError|ERR_|HTTP \d{3}|data pack (manifest|file)/i,
    say: "boot.error.network",
  },

  // The engine binary itself would not compile or start.
  {
    when: /WebAssembly|CompileError|LinkError|RuntimeError|out of memory/i,
    say: "boot.error.wasm",
  },

  // The engine opened the home and refused it (StudyEngine.open).
  {
    when: /engine open failed|tokenization|kjv1769/i,
    say: "boot.error.corpus",
  },
];

const FALLBACK = "boot.error.unknown";

/** The catalogue id of the reader-facing sentence for a boot failure. The raw
 *  string stays the caller's to show — see the <details> on the splash. */
export function bootErrorCopy(raw: string): string {
  const text = (raw ?? "").trim();
  if (!text) return FALLBACK;
  for (const r of RULES) if (r.when.test(text)) return r.say;
  return FALLBACK;
}
