// What the splash says when boot fails: a catalogue id for a raw error string. The
// raw string stays the caller's to show (the splash puts it behind a <details>) — it
// is what a bug report pastes, and the only evidence of which rung of the boot ladder
// broke; anything unrecognised falls through to a sentence that admits as much.
//
// Ids, not sentences: the `boot.*` keys are bundled into the shell
// (scripts/gen-i18n.mjs) so a boot that never reached the engine can still speak the
// reader's language. First match wins, and the order below is deliberate — a failed
// download on a device that is also out of room reports the download, because that is
// what the reader is being asked to retry.

/** One bucket: what the raw string has to look like, and the catalogue id of
 *  what to say instead. */
type Rule = { when: RegExp; say: string };

const RULES: Rule[] = [
  // The worker died or went silent. worker-client.ts already builds those strings for
  // a reader out of the browser's own error text and a stage name.
  { when: /^The study engine\b/, say: "boot.error.engine" },

  // The pack format moved under a shell that predates it (pack.ts `checked`); a reload
  // picks up the current bundle.
  {
    when: /data pack format/i,
    say: "boot.error.stale",
  },

  // No room: the Cache API refusing a put, or IndexedDB refusing a transaction.
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

  // The network: `Failed to fetch` is Chromium, `Load failed` is WebKit, `NetworkError`
  // is Firefox. An HTTP status is a reachable server answering badly, still "try
  // again" from here. The sentence says "what it needs to open" rather than "the
  // scripture data" because this also catches a failed engine-binary download.
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
