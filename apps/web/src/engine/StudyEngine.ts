// The shell's engine binding: one method per endpoint of the core's flat C ABI,
// over camelCase wire JSON. Returned JSON is parsed; authoring calls follow the
// choreography write → engine reloads from its home → shell re-fetches, and
// additionally mirror the virtual home to IndexedDB.

import type { WasmEngine } from "./engine";

/** Layout config in logical px. */
export interface LayoutCfg {
  width: number;
  lineHeight: number;
  spaceWidth: number;
  verseNumGap: number;
  paraIndent: number;
  paraSpacing: number;
  versePerLine: boolean;
  /** Paint the leading verse numbers. Optional and defaulted ON at the
   *  marshalling site, so a caller that predates the setting still gets them. */
  verseNumbers?: boolean;
}

export type Grade = "again" | "hard" | "good" | "easy";

/** UTC timestamp in the wire format the authoring endpoints expect. */
export function nowStamp(): string {
  return new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
}

/** TODAY, as the day-keyed reads spell it (`readingBooks`, `readingChapters`,
 *  `memoryDue`) — midday UTC, so the stamp names one calendar day rather than
 *  flipping across a timezone at midnight. One definition, because it is a CACHE
 *  KEY: a warm that derived it differently would miss every time. */
export function dayStamp(): string {
  return nowStamp().slice(0, 10) + "T12:00:00Z";
}

/** TODAY IN THE READER'S OWN TIMEZONE, as `YYYY-MM-DD` — deliberately not
 *  `dayStamp()`, which is midday UTC: the right key for a cache, the wrong answer
 *  for "has a day passed?". A reader at UTC-7 pressing Done at 6pm is already on
 *  the next UTC date and would be handed tomorrow's entry; one at UTC+13 would be
 *  held back a day. session.svelte.ts computes the local date the same way. */
export function localDay(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export class DisplayList {
  #w: WasmEngine;
  #ptr: number;
  #json: unknown | null = null;

  constructor(w: WasmEngine, ptr: number) {
    this.#w = w;
    this.#ptr = ptr;
  }
  get raw(): unknown {
    this.#json ??= JSON.parse(this.#w.takeStr((this.#w.exports.plumbline_layout_to_json as Function)(this.#ptr) as number)!);
    return this.#json;
  }
  get height(): number {
    return (this.#w.exports.plumbline_layout_height as Function)(this.#ptr) as number;
  }
  get width(): number {
    return (this.#w.exports.plumbline_layout_width as Function)(this.#ptr) as number;
  }
  get itemCount(): number {
    return (this.#w.exports.plumbline_layout_item_count as Function)(this.#ptr) as number;
  }
  hitTest(x: number, y: number): any {
    const s = this.#w.takeStr((this.#w.exports.plumbline_layout_hit_test_json as Function)(this.#ptr, x, y) as number);
    return s ? JSON.parse(s) : null;
  }
  free(): void {
    if (this.#ptr) (this.#w.exports.plumbline_layout_free as Function)(this.#ptr);
    this.#ptr = 0;
  }
}

export class StudyEngine {
  #w: WasmEngine;
  #engine: number;
  /** Invoked after any authoring write; boot wires this to home persistence. */
  onAuthored: () => void = () => {};
  /** A reading-map write landed. NOT `onAuthored`: dwell is reported on a timer,
   *  and `onAuthored` runs a full user-subtree diff into IndexedDB — fine for a
   *  note, wasteful every 30 seconds on the one thread that also answers taps. The
   *  worker binds this to `home.persistUserDir("reading")`. */
  onReadingWrite: () => void = () => {};

  private constructor(w: WasmEngine, engine: number) {
    this.#w = w;
    this.#engine = engine;
  }

  static open(w: WasmEngine, home: string): StudyEngine {
    const homePtr = w.inStr(home);
    const [engine, err] = w.withErrSlot((slot) =>
      (w.exports.plumbline_engine_open as Function)(homePtr, slot) as number,
    );
    w.freeStr(homePtr);
    if (!engine) throw new Error(err ?? "engine open failed");
    return new StudyEngine(w, engine);
  }

  /** A second engine on a named language's text, from the same home — what a pane
   *  reading German beside an English one runs on. It shares the reader's data,
   *  every text sitting at the KJV's verse addresses. No fallback to English: a
   *  missing text throws, and the caller offers the download. */
  static openLang(w: WasmEngine, home: string, lang: string): StudyEngine {
    const homePtr = w.inStr(home);
    const langPtr = w.inStr(lang);
    const [engine, err] = w.withErrSlot((slot) =>
      (w.exports.plumbline_engine_open_lang as Function)(homePtr, langPtr, slot) as number,
    );
    w.freeStr(homePtr);
    w.freeStr(langPtr);
    if (!engine) throw new Error(err ?? `no ${lang} text on this device`);
    return new StudyEngine(w, engine);
  }

  get wasm(): WasmEngine {
    return this.#w;
  }

  // ── call helpers ────────────────────────────────────────────────────────────

  /** Call with marshalled string args (null crosses as NULL), free them after. */
  #call<T>(f: (...ptrs: number[]) => T, args: (string | null)[]): T {
    const ptrs = args.map((a) => (a === null ? 0 : this.#w.inStr(a)));
    try {
      return f(...ptrs);
    } finally {
      for (const p of ptrs) if (p) this.#w.freeStr(p);
    }
  }
  #json(name: string, ...args: (string | null)[]): any {
    const s = this.#call(
      (...ptrs) => this.#w.takeStr((this.#w.exports[name] as Function)(this.#engine, ...ptrs) as number),
      args,
    );
    return s === null ? null : JSON.parse(s);
  }
  #text(name: string, ...args: (string | null)[]): string | null {
    return this.#call(
      (...ptrs) => this.#w.takeStr((this.#w.exports[name] as Function)(this.#engine, ...ptrs) as number),
      args,
    );
  }
  /** Authoring call: returns null on success, else the engine's error string. */
  #author(name: string, f: (exp: Function, ...ptrs: number[]) => number, args: (string | null)[]): string | null {
    const err = this.#call((...ptrs) => this.#w.takeStr(f(this.#w.exports[name] as Function, ...ptrs) as number), args);
    if (err === null) this.onAuthored();
    return err;
  }

  // ── lifecycle ───────────────────────────────────────────────────────────────

  free(): void {
    if (this.#engine) (this.#w.exports.plumbline_engine_free as Function)(this.#engine);
    this.#engine = 0;
  }

  // ── reader core ─────────────────────────────────────────────────────────────

  toc(): any {
    return this.#json("plumbline_engine_toc_json");
  }
  chapterCount(book: string): number {
    return this.#call((b) => (this.#w.exports.plumbline_engine_chapter_count as Function)(this.#engine, b) as number, [book]);
  }
  /** Highest verse number in a chapter — the passage picker's range. */
  chapterVerseCount(book: string, chapter: number): number {
    return this.#call(
      (b) =>
        (this.#w.exports.plumbline_engine_chapter_verse_count as Function)(
          this.#engine,
          b,
          chapter,
        ) as number,
      [book],
    );
  }
  verse(refKey: string): any {
    return this.#json("plumbline_engine_verse_json", refKey);
  }
  token(refKey: string, tokenIndex: number): any {
    const s = this.#call(
      (r) => this.#w.takeStr((this.#w.exports.plumbline_engine_token_json as Function)(this.#engine, r, tokenIndex) as number),
      [refKey],
    );
    return s === null ? null : JSON.parse(s);
  }

  /** `PlumblineLayoutConfig` is hand-marshalled at fixed offsets: six f32 then two
   *  u32 flags, 32 bytes. It MUST match the #[repr(C)] struct in crates/ffi/src/lib.rs
   *  field for field — a field added there and not here hands the engine whatever
   *  was in that word of the heap. */
  layoutChapter(book: string, chapter: number, cfg: LayoutCfg): DisplayList | null {
    const CFG_BYTES = 32;
    const cfgPtr = (this.#w.exports.plumbline_web_alloc as Function)(CFG_BYTES) as number;
    const dv = new DataView(this.#w.exports.memory.buffer);
    [cfg.width, cfg.lineHeight, cfg.spaceWidth, cfg.verseNumGap, cfg.paraIndent, cfg.paraSpacing].forEach(
      (v, i) => dv.setFloat32(cfgPtr + i * 4, v, true),
    );
    dv.setUint32(cfgPtr + 24, cfg.versePerLine ? 1 : 0, true);
    // Default ON: an undefined here is a caller that predates the setting, not
    // a reader who turned the numbers off.
    dv.setUint32(cfgPtr + 28, cfg.verseNumbers === false ? 0 : 1, true);
    const dl = this.#call(
      (b) =>
        (this.#w.exports.plumbline_engine_layout_chapter as Function)(
          this.#engine, b, chapter, cfgPtr, this.#w.measureFnptr, 0,
        ) as number,
      [book],
    );
    (this.#w.exports.plumbline_web_free as Function)(cfgPtr, CFG_BYTES);
    return dl ? new DisplayList(this.#w, dl) : null;
  }

  // ── study data ──────────────────────────────────────────────────────────────

  strongs(code: string): any {
    return this.#json("plumbline_engine_strongs_json", code);
  }
  strongsOccurrences(code: string): any {
    return this.#json("plumbline_engine_strongs_occurrences_json", code);
  }
  renderings(code: string): any {
    return this.#json("plumbline_engine_renderings_json", code);
  }
  wordCodes(word: string): any {
    return this.#json("plumbline_engine_word_codes_json", word);
  }
  search(query: string): any {
    return this.#json("plumbline_engine_search_json", query);
  }
  threads(): any {
    return this.#json("plumbline_engine_threads_json");
  }
  tags(): any {
    return this.#json("plumbline_engine_tags_json");
  }
  verseXrefs(refKey: string): any {
    return this.#json("plumbline_engine_verse_xrefs_json", refKey);
  }
  suggestedWeaves(): any {
    return this.#json("plumbline_engine_suggested_weaves_json");
  }
  verseNotes(refKey: string): any {
    return this.#json("plumbline_engine_verse_notes_json", refKey);
  }
  studyXrefs(refKey: string): any {
    return this.#json("plumbline_engine_study_xrefs_json", refKey);
  }
  weaves(): any {
    return this.#json("plumbline_engine_weaves_json");
  }
  linkPairs(): any {
    return this.#json("plumbline_engine_link_pairs_json");
  }
  canonSegments(): any {
    return this.#json("plumbline_engine_canon_segments_json");
  }

  // ── the hymnal ───────────────────────────────────────────────────────────────

  /** Every hymn: `{hymns:[{id,number,titles,firstLines,tune,meter}]}`, in book
   *  order. Empty when this home carries no `data/hymnal.json`. */
  hymnal(): any {
    return this.#json("plumbline_engine_hymnal_json");
  }
  /** One hymn, chords transposed `semis` semitones and split into paintable parts.
   *  Null for an unknown id. Hand-marshalled rather than through `#json`, which
   *  turns every argument into a string pointer — `semis` crosses as an i32. */
  hymn(id: string, semis: number): any {
    const s = this.#call(
      (p) => this.#w.takeStr((this.#w.exports.plumbline_engine_hymn_json as Function)(this.#engine, p, semis) as number),
      [id],
    );
    return s === null ? null : JSON.parse(s);
  }

  // ── the plain-English overlay (the AKJV delta) ───────────────────────────────

  /** Switch the overlay on/off. Reader only — memorize, Present, copy and share
   *  stay KJV whatever this says. */
  setAkjvOverlay(on: boolean): void {
    (this.#w.exports.plumbline_engine_set_akjv_overlay as Function)(this.#engine, on ? 1 : 0);
  }
  /** Whether this home carries a usable overlay — false until stage 2 lands. */
  akjvAvailable(): boolean {
    return !!(this.#w.exports.plumbline_engine_akjv_available as Function)(this.#engine);
  }
  /** `{akjv, kjv}` for a re-rendered token, else null. */
  akjvToken(refKey: string, tokenIndex: number): any {
    const s = this.#call(
      (r) => this.#w.takeStr((this.#w.exports.plumbline_engine_akjv_token_json as Function)(this.#engine, r, tokenIndex) as number),
      [refKey],
    );
    return s === null ? null : JSON.parse(s);
  }

  // ── R&D tier ────────────────────────────────────────────────────────────────

  bridgePartners(code: string): any {
    return this.#json("plumbline_engine_bridge_partners_json", code);
  }
  morph(refKey: string, tokenIndex: number): any {
    const s = this.#call(
      (r) => this.#w.takeStr((this.#w.exports.plumbline_engine_morph_json as Function)(this.#engine, r, tokenIndex) as number),
      [refKey],
    );
    return s === null ? null : JSON.parse(s);
  }
  concept(code: string): any {
    return this.#json("plumbline_engine_concept_json", code);
  }
  gloss(code: string): string | null {
    return this.#text("plumbline_engine_gloss", code);
  }
  warmIndexes(): string | null {
    return this.#text("plumbline_engine_warm_indexes");
  }
  /** Warm ONE lazy index (wasm-only export) — true while `step` named one.
   *  The engine worker warms via this instead of warmIndexes so layout RPCs
   *  interleave with the warm-up (they share its single thread). */
  warmStep(step: number): boolean {
    return ((this.#w.exports.plumbline_engine_warm_step as Function)(this.#engine, step) as number) === 1;
  }
  /** Tell the engine we warm in SLICES, so it must never build an index inside a
   *  reader's request (wasm-only export). Call it right after open: the warm starts
   *  only after stage 2 lands, and a tap in that window would otherwise build
   *  everything at once and freeze this thread. */
  deferBuilds(on: boolean): void {
    (this.#w.exports.plumbline_engine_defer_builds as Function)(this.#engine, on ? 1 : 0);
  }

  /** Load the R&D artifacts from the home if they arrived after open (the
   *  deferred pack); no-op when already loaded or still missing. */
  loadRndData(): string | null {
    return this.#text("plumbline_engine_load_rnd_data");
  }
  /** Load ONE machine-tier artifact (wasm-only export): true while more remain.
   *  Split so a ~17 MB parse cannot hold the worker for seconds. */
  loadRndStep(step: number): boolean {
    return ((this.#w.exports.plumbline_engine_load_rnd_step as Function)(this.#engine, step) as number) === 1;
  }
  /** Load the stage-2 core data (Strong's + margin-note study reload) once
   *  those files land in the home; no-op when nothing is missing. */
  loadCoreData(): string | null {
    return this.#text("plumbline_engine_load_core_data");
  }

  // ── view-models (maps) ──────────────────────────────────────────────────────

  chordMap(): any {
    return this.#json("plumbline_engine_chord_map_json");
  }
  constellation(page: number, pins: number[]): any {
    const s = this.#call(
      (p) => this.#w.takeStr((this.#w.exports.plumbline_engine_constellation_json as Function)(this.#engine, page, p) as number),
      [JSON.stringify(pins)],
    );
    return s === null ? null : JSON.parse(s);
  }

  // ── panel content model ─────────────────────────────────────────────────────

  /** Word study with per-tier gates: bit 0 = human analysis, bit 1 = machine. */
  wordStudyBlocks(refKey: string, tokenIndex: number, gates: number): any {
    const s = this.#call(
      (r) =>
        this.#w.takeStr(
          (this.#w.exports.plumbline_engine_word_study_blocks2_json as Function)(this.#engine, r, tokenIndex, gates) as number,
        ),
      [refKey],
    );
    return s === null ? null : JSON.parse(s);
  }
  /** The word-usage card: totals, distribution and one page of in-context
   *  occurrence lines. Pass either a non-empty `word` (a wusage: link) or
   *  `refKey` + `tokenIndex` (a tap); a non-empty `code` opens the original-word
   *  lens (lusage: links); `scope` is a SearchScope token ("all", "book:Gen", …). */
  wordUsageBlocks(
    word: string,
    code: string,
    refKey: string,
    tokenIndex: number,
    scope: string,
    page: number,
    gates: number,
  ): any {
    const s = this.#call(
      (w, c, r, sc) =>
        this.#w.takeStr(
          (this.#w.exports.plumbline_engine_word_usage_blocks_json as Function)(
            this.#engine,
            w,
            c,
            r,
            tokenIndex,
            sc,
            page,
            gates,
          ) as number,
        ),
      [word, code, refKey, scope],
    );
    return s === null ? null : JSON.parse(s);
  }
  /** Code study card with per-tier gates (see wordStudyBlocks). */
  codeStudyBlocks(code: string, word: string | null, gates: number): any {
    const s = this.#call(
      (c, wd) =>
        this.#w.takeStr(
          (this.#w.exports.plumbline_engine_code_study_blocks2_json as Function)(this.#engine, c, wd, gates) as number,
        ),
      [code, word],
    );
    return s === null ? null : JSON.parse(s);
  }
  concordanceBlocks(code: string): any {
    return this.#json("plumbline_engine_concordance_blocks_json", code);
  }
  renderingConcordanceBlocks(code: string, rendering: string): any {
    return this.#json("plumbline_engine_rendering_concordance_blocks_json", code, rendering);
  }
  threadsBlocks(): any {
    return this.#json("plumbline_engine_threads_blocks_json");
  }
  /** Thread detail; `edit` renders the per-entry reorder/remove/note controls. */
  threadBlocks(index: number, edit?: boolean): any {
    const s = this.#w.takeStr(
      (this.#w.exports.plumbline_engine_thread_blocks2_json as Function)(this.#engine, index, edit ? 1 : 0) as number,
    );
    return s === null ? null : JSON.parse(s);
  }
  tagsBlocks(): any {
    return this.#json("plumbline_engine_tags_blocks_json");
  }
  tagBlocks(index: number): any {
    const s = this.#w.takeStr((this.#w.exports.plumbline_engine_tag_blocks_json as Function)(this.#engine, index) as number);
    return s === null ? null : JSON.parse(s);
  }
  weavesBlocks(): any {
    return this.#json("plumbline_engine_weaves_blocks_json");
  }
  suggestedBlocks(): any {
    return this.#json("plumbline_engine_suggested_blocks_json");
  }
  compareBlocks(index: number, full: boolean): any {
    const s = this.#w.takeStr(
      (this.#w.exports.plumbline_engine_compare_blocks_json as Function)(this.#engine, index, full ? 1 : 0) as number,
    );
    return s === null ? null : JSON.parse(s);
  }
  searchBlocks(query: string): any {
    return this.#json("plumbline_engine_search_blocks_json", query);
  }
  /** `scope` is a `core::search::SearchScope` token — `all` | `ot` | `nt` |
   *  `book:<osis>` | `chapter:<osis>:<ch>`. The search SCREEN's chips. */
  searchBlocksScoped(query: string, scope: string): any {
    return this.#json("plumbline_engine_search_blocks_scoped_json", query, scope);
  }

  // ── authoring (null = success, else error string; home syncs after) ────────

  threadAdd(name: string, refKey: string, note: string | null, added: string): string | null {
    return this.#author("plumbline_engine_thread_add", (f, ...p) => f(this.#engine, ...p), [name, refKey, note, added]);
  }  /** Delete a thread and everything on it. */
  threadRemove(name: string): string | null {
    return this.#author("plumbline_engine_thread_remove", (f, ...p) => f(this.#engine, ...p), [name]);
  }

  tagAdd(name: string, kind: string, value: string, note: string | null, added: string): string | null {
    return this.#author("plumbline_engine_tag_add", (f, ...p) => f(this.#engine, ...p), [name, kind, value, note, added]);
  }
  tagRemove(name: string, kind: string, value: string): string | null {
    return this.#author("plumbline_engine_tag_remove", (f, ...p) => f(this.#engine, ...p), [name, kind, value]);
  }
  /** Delete a whole tag and every member on it. */
  tagDelete(name: string): string | null {
    return this.#author("plumbline_engine_tag_delete", (f, ...p) => f(this.#engine, ...p), [name]);
  }
  /** Rename a tag, keeping its identity. Refuses a blank name, and refuses one
   *  another tag already answers to — that would be a merge. */
  tagRename(from: string, to: string): string | null {
    return this.#author("plumbline_engine_tag_rename", (f, ...p) => f(this.#engine, ...p), [from, to]);
  }
  /** Fold one tag into another and DELETE the source. */
  tagMerge(from: string, into: string): string | null {
    return this.#author("plumbline_engine_tag_merge", (f, ...p) => f(this.#engine, ...p), [from, into]);
  }
  /** Set or clear a tag's category — the management screen's verb. Empty clears. */
  tagSetCategory(name: string, category: string): string | null {
    return this.#author("plumbline_engine_tag_set_category", (f, ...p) => f(this.#engine, ...p), [name, category]);
  }
  weaveAddLink(name: string, aRef: string, bRef: string, added: string): string | null {
    return this.#author("plumbline_engine_weave_add_link", (f, ...p) => f(this.#engine, ...p), [name, aRef, bRef, added]);
  }
  weaveAddLinkSpans(
    name: string, aRef: string, bRef: string,
    aLo: number, aHi: number, bLo: number, bHi: number, added: string,
  ): string | null {
    return this.#author(
      "plumbline_engine_weave_add_link_spans",
      (f, n, a, b, ad) => f(this.#engine, n, a, b, aLo, aHi, bLo, bHi, ad),
      [name, aRef, bRef, added],
    );
  }
  /** Weave a tag's passages (or a refKey subset) into a canon-ordered chain;
   *  null weaveName reuses the tag's name. Re-runs only add new edges. */
  weaveFromTag(tagName: string, refsJson: string | null, weaveName: string | null, added: string): string | null {
    return this.#author("plumbline_engine_weave_from_tag", (f, ...p) => f(this.#engine, ...p), [
      tagName,
      refsJson,
      weaveName,
      added,
    ]);
  }
  weaveApprove(index: number): string | null {
    const err = this.#w.takeStr((this.#w.exports.plumbline_engine_weave_approve as Function)(this.#engine, index) as number);
    if (err === null) this.onAuthored();
    return err;
  }
  weaveReject(index: number): string | null {
    const err = this.#w.takeStr((this.#w.exports.plumbline_engine_weave_reject as Function)(this.#engine, index) as number);
    if (err === null) this.onAuthored();
    return err;
  }
  /** Delete a weave and every link on it — `index` is the flat-library ordinal
   *  (the `weave:i` verb), not `weaveReject`'s suggested ordinal. */
  weaveDelete(index: number): string | null {
    const err = this.#w.takeStr((this.#w.exports.plumbline_engine_weave_delete as Function)(this.#engine, index) as number);
    if (err === null) this.onAuthored();
    return err;
  }
  threadSetNotes(name: string, notes: string): string | null {
    return this.#author("plumbline_engine_thread_set_notes", (f, ...p) => f(this.#engine, ...p), [name, notes]);
  }
  /** The commentary that bookends the passages: `opening` reads before the
   *  first, `closing` after the last. A blank string clears one. */
  threadSetOpening(name: string, opening: string): string | null {
    return this.#author("plumbline_engine_thread_set_opening", (f, ...p) => f(this.#engine, ...p), [name, opening]);
  }
  threadSetClosing(name: string, closing: string): string | null {
    return this.#author("plumbline_engine_thread_set_closing", (f, ...p) => f(this.#engine, ...p), [name, closing]);
  }
  /** Drop entry `index`. The thread survives its last entry — deleting the thread
   *  itself is `threadRemove`. */
  threadEntryRemove(name: string, index: number): string | null {
    // The index rides in the CLOSURE, not in `args`: `#author` marshals strings and
    // passes pointers, so a number in that list would cross as one.
    return this.#author("plumbline_engine_thread_entry_remove", (f, n) => f(this.#engine, n, index), [name]);
  }
  /** Move entry `from` to position `to`; past the end clamps to a no-op. */
  threadEntryMove(name: string, from: number, to: number): string | null {
    return this.#author("plumbline_engine_thread_entry_move", (f, n) => f(this.#engine, n, from, to), [name]);
  }
  threadEntrySetNote(name: string, index: number, note: string): string | null {
    return this.#author(
      "plumbline_engine_thread_entry_set_note",
      (f, n, no) => f(this.#engine, n, index, no),
      [name, note],
    );
  }
  weaveSetNotes(name: string, notes: string): string | null {
    return this.#author("plumbline_engine_weave_set_notes", (f, ...p) => f(this.#engine, ...p), [name, notes]);
  }
  userNoteSet(refKey: string, text: string, stamp: string): string | null {
    return this.#author("plumbline_engine_user_note_set", (f, ...p) => f(this.#engine, ...p), [refKey, text, stamp]);
  }
  // ── user notes / copy ──────────────────────────────────────────────────────

  copyText(refKey: string, kind: string): string | null {
    return this.#text("plumbline_engine_copy_text", refKey, kind);
  }
  userNote(refKey: string): any {
    return this.#json("plumbline_engine_user_note_json", refKey);
  }
  userNotes(): any {
    return this.#json("plumbline_engine_user_notes_json");
  }
  // ── memorization ────────────────────────────────────────────────────────────

  memoryAdd(verseRef: string, now: string): string | null {
    return this.#author("plumbline_engine_memory_add", (f, ...p) => f(this.#engine, ...p), [verseRef, now]);
  }
  /** One card for a whole passage (`startRef`…`throughRef`, same chapter). */
  memoryAddPassage(startRef: string, throughRef: string, now: string): string | null {
    return this.#author("plumbline_engine_memory_add_passage", (f, ...p) => f(this.#engine, ...p), [
      startRef,
      throughRef,
      now,
    ]);
  }
  memoryGrade(verseRef: string, grade: Grade, now: string): string | null {
    return this.#author("plumbline_engine_memory_grade", (f, ...p) => f(this.#engine, ...p), [verseRef, grade, now]);
  }
  memoryRemove(verseRef: string): string | null {
    return this.#author("plumbline_engine_memory_remove", (f, ...p) => f(this.#engine, ...p), [verseRef]);
  }
  memoryCard(verseRef: string): any {
    return this.#json("plumbline_engine_memory_card_json", verseRef);
  }
  memoryDue(now: string): any {
    return this.#json("plumbline_engine_memory_due_json", now);
  }
  memoryCoverage(now: string): any {
    return this.#json("plumbline_engine_memory_coverage_json", now);
  }
  memoryActivity(): any {
    return this.#json("plumbline_engine_memory_activity_json");
  }
  memoryDrill(verseRef: string, level: number): any {
    const s = this.#call(
      (r) => this.#w.takeStr((this.#w.exports.plumbline_engine_memory_drill_json as Function)(this.#engine, r, level) as number),
      [verseRef],
    );
    return s === null ? null : JSON.parse(s);
  }
  memoryScore(verseRef: string, typed: string): any {
    return this.#json("plumbline_engine_memory_score_json", verseRef, typed);
  }

  // ── the reading map ─────────────────────────────────────────────────────────

  /** Every book's reading standing at `now` — `{books,since,spec}`. */
  readingBooks(now: string): any {
    return this.#json("plumbline_engine_reading_books_json", now);
  }
  /** One book's chapters — `{book,chapters,since,spec}`. */
  readingChapters(book: string, now: string): any {
    return this.#json("plumbline_engine_reading_chapters_json", book, now);
  }
  /** One sample of reading time: `stepSeconds` passed with `book` `chapter` in
   *  front of somebody who has scrolled as far as `reached` and touched something
   *  (`interacted`) or not. A null `book` means nothing is being read — a dialog is
   *  up, the tab went hidden, the reader left — and banks the tail.
   *
   *  Counters and thresholds live in the core (`reading::DwellTracker`), so a shell
   *  holds only its clock. Answers null on almost every call, else the same
   *  `{book,chapter,pct,completed,lastRead?}` `readingRecord` gives. */
  readingTick(
    book: string | null,
    chapter: number,
    reached: number,
    stepSeconds: number,
    interacted: boolean,
    now: string,
  ): any {
    const str = this.#call(
      (b, n) =>
        this.#w.takeStr(
          (this.#w.exports.plumbline_engine_reading_tick_json as Function)(
            this.#engine, b, chapter, reached, stepSeconds, interacted ? 1 : 0, n,
          ) as number,
        ),
      [book, now],
    );
    if (str !== null) this.onReadingWrite();
    return str === null ? null : JSON.parse(str);
  }
  /** Credit `seconds` of dwell, having reached verse `reached`. Returns
   *  `{book,chapter,pct,completed,lastRead?}` (null with no writable home). */
  readingRecord(book: string, chapter: number, reached: number, seconds: number, now: string): any {
    const str = this.#call(
      (b, n) =>
        this.#w.takeStr(
          (this.#w.exports.plumbline_engine_reading_record_json as Function)(
            this.#engine, b, chapter, reached, seconds, n,
          ) as number,
        ),
      [book, now],
    );
    if (str !== null) this.onReadingWrite();
    return str === null ? null : JSON.parse(str);
  }
  /** Log a chapter as read on `date` by hand (a paper-Bible read). */
  readingMarkRead(book: string, chapter: number, date: string): string | null {
    return this.#author(
      "plumbline_engine_reading_mark_read",
      (f, b, d) => f(this.#engine, b, chapter, d),
      [book, date],
    );
  }
  /** Drop a chapter's reading record — back to unread. */
  readingForget(book: string, chapter: number): string | null {
    return this.#author("plumbline_engine_reading_forget", (f, b) => f(this.#engine, b, chapter), [book]);
  }

  // ── reading plans + the concept study ──────────────────────────────────────────
  /** `{running:[…], builtins:[…]}` — the reader's plans with derived state, and the
   *  catalogue the picker offers.
   *
   *  `now` dates each schedule's `doneToday`. Call sites pass `""`, meaning "now,
   *  stamped here": it is the read-through cache's KEY, and a key carrying the clock
   *  would mint a fresh entry per call. Staleness across midnight does not matter —
   *  every dwell report invalidates the cache, so the flag re-dates as reading
   *  resumes. */
  plans(now: string): any {
    return this.#json("plumbline_engine_plans_json", now || new Date().toISOString());
  }
  /** Start a built-in schedule by id (replaces its class occupant). */
  planStart(id: string, now: string): string | null {
    return this.#author("plumbline_engine_plan_start", (f, i, n) => f(this.#engine, i, n), [id, now]);
  }
  /** Stop a plan (removes its file; a concept study's tag is untouched). */
  planStop(id: string): string | null {
    return this.#author("plumbline_engine_plan_stop", (f, i) => f(this.#engine, i), [id]);
  }
  /** Pause (true) or resume (false) a plan — set aside, kept whole: file,
   *  progress and class stay put; `today` surfaces stop asking meanwhile. */
  planSetPaused(id: string, paused: boolean): string | null {
    return this.#author("plumbline_engine_plan_set_paused", (f, i) => f(this.#engine, i, paused ? 1 : 0), [id]);
  }
  /** Start or resume a concept study for `tag`; returns the run's id — what the
   *  shell writes into `config.conceptStudy` to enter the mode. An error comes
   *  back prefixed with `!` (no plan id can start with one). Null only if the
   *  engine itself returned nothing. */
  conceptStudyStart(tag: string, now: string): string | null {
    const id = this.#text("plumbline_engine_concept_study_start", tag, now);
    if (id !== null && !id.startsWith("!")) this.onAuthored();
    return id;
  }
  /** Mark a chapter swept in a concept study (generous, any order). */
  conceptStudySweep(id: string, book: string, chapter: number): string | null {
    return this.#author(
      "plumbline_engine_concept_study_sweep",
      (f, i, b) => f(this.#engine, i, b, chapter),
      [id, book],
    );
  }

  // ── sharing ───────────────────────────────────────────────────────────────
  /** What a shared link may carry, for the language it will be READ in:
   *  `{lang, languages, paths, threads, devotionals}`, each option carrying
   *  whether it exists in that language yet (`available`).
   *
   *  TWO languages, and they are almost never the same one: `lang` is the
   *  RECIPIENT's, which every `available` is about; `uiLang` is the SENDER's,
   *  which every `label` comes back in. A picker whose options the person using
   *  it cannot read is not a picker. Asked again whenever the target changes, not
   *  once per session. Unavailable options are RETURNED, not filtered: the
   *  palette shows them as coming soon.
   *
   *  Engine-taking, unlike `shareLink`, because threads and booklets are loaded
   *  data. Building the link itself stays engine-free. */
  shareOptions(lang: string, uiLang: string): any {
    return this.#json("plumbline_engine_share_options_json", lang || "en", uiLang || lang || "en");
  }

  // ── devotionals ───────────────────────────────────────────────────────────
  /** `{running:[…], catalogue:[…]}` — the reader's booklets with their open day,
   *  and the catalogue every picker offers.
   *
   *  `lang` picks the text (per-entry fallback to English lives in the core).
   *  `today` is the reader's LOCAL day and the cache KEY: passed explicitly rather
   *  than stamped here, unlike `plans`, because a key of `""` would never re-ask
   *  across midnight — and midnight is when this answer changes. */
  devotionals(lang: string, today: string): any {
    return this.#json("plumbline_engine_devotionals_json", lang || "en", today || localDay());
  }
  /** One day of a booklet, open or browsed-back-to. Null for a day it has no
   *  entry for. */
  devotionalDay(id: string, day: number, lang: string): any {
    // `#json` marshals STRINGS; `day` crosses the ABI as a plain u32, so it is
    // closed over rather than passed through the pointer list (as in `token()`).
    const raw = this.#call(
      (i, l) =>
        this.#w.takeStr(
          (this.#w.exports.plumbline_engine_devotional_day_json as Function)(this.#engine, i, day, l) as number,
        ),
      [id, lang || "en"],
    );
    return raw === null ? null : JSON.parse(raw);
  }
  /** Start a devotional. Starting one already running keeps its progress. */
  devotionalStart(id: string, now: string): string | null {
    return this.#author("plumbline_engine_devotional_start", (f, i, n) => f(this.#engine, i, n), [id, now]);
  }
  /** Stop a devotional (removes its run file and its banked days). */
  devotionalStop(id: string): string | null {
    return this.#author("plumbline_engine_devotional_stop", (f, i) => f(this.#engine, i), [id]);
  }
  /** Bank a day — the Done at the foot of the page. `today` is the reader's
   *  LOCAL day; it is what holds tomorrow's entry back until tomorrow. */
  devotionalDone(id: string, day: number, today: string): string | null {
    return this.#author(
      "plumbline_engine_devotional_done",
      (f, i, t) => f(this.#engine, i, day, t),
      [id, today || localDay()],
    );
  }
  /** Pause (true) or resume (false) a devotional — set aside, kept whole. */
  devotionalSetPaused(id: string, paused: boolean): string | null {
    return this.#author("plumbline_engine_devotional_set_paused", (f, i) => f(this.#engine, i, paused ? 1 : 0), [id]);
  }
}

// ── engine-independent calls ──────────────────────────────────────────────────

export function routeLink(w: WasmEngine, uri: string): any {
  const p = w.inStr(uri);
  const s = w.takeStr((w.exports.plumbline_route_link_json as Function)(p) as number);
  w.freeStr(p);
  return s === null ? null : JSON.parse(s);
}

export function configLoad(w: WasmEngine): any {
  const s = w.takeStr((w.exports.plumbline_config_load_json as Function)() as number);
  return s === null ? null : JSON.parse(s);
}

export function configSave(w: WasmEngine, config: unknown): string | null {
  const p = w.inStr(JSON.stringify(config));
  const err = w.takeStr((w.exports.plumbline_config_save_json as Function)(p) as number);
  w.freeStr(p);
  return err;
}

/** The share link this reader hands over, plus the church it carries and the two
 *  strings a Church button needs (`core::church`). Engine-independent.
 *
 *  `shell/church.ts` still builds share links itself, because they are read
 *  synchronously out of derived state and this crosses a worker; this binding
 *  covers the whole ABI and lets a test ask the engine what the answer should be. */
export function shareLink(
  w: WasmEngine,
  request: {
    base?: string;
    church?: { name: string; info: string; url: string };
    at?: string;
    lang?: string;
    thread?: string;
    devotional?: string;
    path?: string;
  },
): any {
  const p = w.inStr(JSON.stringify(request));
  const s = w.takeStr((w.exports.plumbline_share_url_json as Function)(p) as number);
  w.freeStr(p);
  return s === null ? null : JSON.parse(s);
}

/** The reading map's tuning (`{wordsPerMinute, completeAt, freshDays,
 *  staleDays, graceSeconds, tickSeconds, idleSeconds}`) without loading the
 *  reader's reading store. Engine-independent. */
export function readingSpec(w: WasmEngine): any {
  const s = w.takeStr((w.exports.plumbline_reading_spec_json as Function)() as number);
  return s === null ? null : JSON.parse(s);
}

/** Every string the shell paints, in the reader's language, in ONE call. Both
 *  arguments, because the core owns the rule that an empty setting means "follow
 *  the device" (`i18n::resolve`); the reply's `lang` says which won.
 *  Engine-independent: the chrome has to exist before an engine does. */
export function i18nCatalog(w: WasmEngine, chosen: string, device: string): any {
  const a = w.inStr(chosen);
  const b = w.inStr(device);
  const s = w.takeStr((w.exports.plumbline_i18n_catalog_json as Function)(a, b) as number);
  w.freeStr(a);
  w.freeStr(b);
  return s === null ? null : JSON.parse(s);
}

/** Tell the ENGINE which language to write in, and get back the code it chose. The
 *  catalogue covers what a shell spells; this covers what the core spells — book
 *  names and references in the TOC, search hits, weave endpoints, note headers, the
 *  reading map. Both are needed, or a German reader gets a German interface listing
 *  a book called Genesis. Must be called BEFORE the boot reply builds the TOC. */
export function i18nSetLanguage(w: WasmEngine, chosen: string, device: string): string {
  const a = w.inStr(chosen);
  const b = w.inStr(device);
  const s = w.takeStr((w.exports.plumbline_i18n_set_language as Function)(a, b) as number);
  w.freeStr(a);
  w.freeStr(b);
  return s ?? "en";
}

export function themePalette(w: WasmEngine, theme: string): any {
  const p = w.inStr(theme);
  const s = w.takeStr((w.exports.plumbline_theme_palette_json as Function)(p) as number);
  w.freeStr(p);
  return s === null ? null : JSON.parse(s);
}

/** Which SEATING a LOCAL date and hour fall in. The date must be the reader's own:
 *  a slot computed in UTC would put a Sunday-evening service in Monday for half the
 *  world. */
export function sessionSlot(w: WasmEngine, date: string, hour: number): string {
  const p = w.inStr(date);
  const s = w.takeStr((w.exports.plumbline_session_slot as Function)(p, hour) as number);
  w.freeStr(p);
  return s ?? "other";
}

/** `sessionSlot` to the minute, honouring a configured Sunday service time:
 *  `minute` is minutes since local midnight, `sundayService` the config's
 *  value or -1 when the reader never set one (the before-noon rule). With a
 *  time set, `sunday-morning` runs from the service start to 1.5h after. */
export function sessionSlotAt(w: WasmEngine, date: string, minute: number, sundayService: number): string {
  const p = w.inStr(date);
  const s = w.takeStr((w.exports.plumbline_session_slot_at as Function)(p, minute, sundayService) as number);
  w.freeStr(p);
  return s ?? "other";
}

export function guideBlocks(w: WasmEngine): any {
  const s = w.takeStr((w.exports.plumbline_panel_guide_blocks_json as Function)() as number);
  return s === null ? null : JSON.parse(s);
}

export function aboutBlocks(w: WasmEngine): any {
  const s = w.takeStr((w.exports.plumbline_panel_about_blocks_json as Function)() as number);
  return s === null ? null : JSON.parse(s);
}

export function engineVersion(w: WasmEngine): string {
  return w.takeStr((w.exports.plumbline_version as Function)() as number) ?? "?";
}
