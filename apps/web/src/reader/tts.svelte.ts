// Read-aloud over the Web Speech API — WEB ONLY (Android's twin would be the
// platform TextToSpeech; the delta is recorded in the feature manifest).
//
// One utterance PER VERSE, never one per chapter: stopping lands between
// verses, and some engines go silent partway through a very long utterance —
// a chapter is minutes of speech, a verse is seconds. The texts are fetched
// up front by the caller, so a busy engine can never starve the queue.

/** What is being read, or null. `passage` is the display form for the chip. */
let speaking = $state<{ passage: string } | null>(null);

export function ttsSpeaking(): { passage: string } | null {
  return speaking;
}

/** Whether this browser can speak at all — the menu items hide when it can't. */
export function ttsSupported(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window;
}

export function ttsStop(): void {
  speaking = null;
  try {
    window.speechSynthesis.cancel();
  } catch {
    /* no synthesis: nothing was speaking */
  }
}

/** Queue `verses` (plain bodies, in reading order) and start speaking.
 *  `lang` is the pane's text language code ("en" | "de" | "es") — the voice
 *  must match the words, not the UI: a German chapter read with an English
 *  voice is noise. A new read replaces whatever was playing. */
export function ttsSpeak(passage: string, verses: string[], lang: string): void {
  if (!ttsSupported() || verses.length === 0) return;
  ttsStop();
  speaking = { passage };
  const last = verses.length - 1;
  verses.forEach((body, i) => {
    const u = new SpeechSynthesisUtterance(body);
    u.lang = lang || "en";
    // The chip clears when the LAST verse ends — or when anything errors,
    // because a queue that died must not leave a "Reading aloud" chip lying.
    if (i === last) u.onend = () => (speaking = null);
    u.onerror = () => (speaking = null);
    window.speechSynthesis.speak(u);
  });
}
