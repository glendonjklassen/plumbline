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
// FIRST MATCH WINS, and the order below is deliberate — a failed download on a
// device that is also out of room reports the download, because that is the
// thing the reader is being asked to retry.

/** One bucket: what the raw string has to look like, and what to say instead. */
type Rule = { when: RegExp; say: string };

const RULES: Rule[] = [
  // Already the reader's language. The engine-worker client (worker-client.ts)
  // writes its own copy for a dead or silent worker — "The study engine stopped
  // unexpectedly", "The study engine went quiet for 60s and never finished
  // starting. It got as far as opening the text." — and translating those again
  // would only make them vaguer. e2e/worker-death.spec.ts holds this pass-through.
  { when: /^The study engine\b/, say: "" },

  // The pack format moved under a shell that predates it (pack.ts `checked`).
  // A reload picks up the current bundle, which is a thing the reader can do;
  // the rest of that message is a note to whoever builds the pack.
  {
    when: /data pack format/i,
    say: "This copy of Plumbline is older than the scripture data it just downloaded. Reload the page to pick up the current version.",
  },

  // No room. Two shapes: the Cache API refusing a put, and IndexedDB refusing a
  // transaction. Both mean the same thing to the reader.
  {
    when: /QuotaExceeded|quota|storage is full|no space left/i,
    say: "There is no room left on this device to store the Bible. Free some space and try again — Plumbline needs about 3 MB for the text.",
  },

  // Storage refused outright rather than being full: private windows, a browser
  // set to block site data, an IndexedDB the profile has disowned.
  {
    when: /SecurityError|IDBFactory|access to storage|not allowed to (use|access)/i,
    say: "This browser is not letting Plumbline store anything on the device. Site data (cookies and storage) has to be allowed for this address — a private window blocks it too.",
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
    say: "Plumbline could not finish downloading what it needs to open. Check the connection and try again — once it is on the device it opens with no connection at all.",
  },

  // The engine binary itself would not compile or start.
  {
    when: /WebAssembly|CompileError|LinkError|RuntimeError|out of memory/i,
    say: "Plumbline's engine would not start in this browser. Closing other tabs and reloading is worth trying; if it keeps happening, the browser may be too old for WebAssembly.",
  },

  // The engine opened the home and refused it (StudyEngine.open).
  {
    when: /engine open failed|tokenization|kjv1769/i,
    say: "Plumbline could not read the scripture data on this device. Reloading re-downloads what is missing.",
  },
];

const FALLBACK =
  "Plumbline could not start. Reloading usually clears it; if it does not, the details below are what a bug report needs.";

/** The reader-facing sentence for a boot failure. The raw string stays the
 *  caller's to show — see the <details> on the splash. */
export function bootErrorCopy(raw: string): string {
  const text = (raw ?? "").trim();
  if (!text) return FALLBACK;
  for (const r of RULES) if (r.when.test(text)) return r.say || text;
  return FALLBACK;
}
