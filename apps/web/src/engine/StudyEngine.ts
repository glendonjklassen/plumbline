// The web shell's engine binding — the TS sibling of StudyEngine.kt (Kotlin)
// and Plumbline.cs (C#), method-for-method over the same C ABI and camelCase
// wire JSON. Returned JSON is parsed; authoring calls follow the shared
// choreography (write → engine reloads from its home → shell re-fetches) and
// additionally mirror the virtual home to IndexedDB.

import type { WasmEngine } from "./engine";

/** Layout config in logical px — the shell passes the same shape GTK/WinUI do. */
export interface LayoutCfg {
  width: number;
  lineHeight: number;
  spaceWidth: number;
  verseNumGap: number;
  paraIndent: number;
  paraSpacing: number;
  versePerLine: boolean;
}

export type Grade = "again" | "hard" | "good" | "easy";

/** UTC timestamp in the wire format the authoring endpoints expect. */
export function nowStamp(): string {
  return new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
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
  /** A reading-map write landed. Deliberately NOT `onAuthored`: dwell is
   *  reported on a timer while somebody reads, and `onAuthored` runs a full
   *  user-subtree diff into IndexedDB — fine for a note, wasteful every 30
   *  seconds on the one thread that also answers taps. The worker binds this to
   *  `home.persistUserDir("reading")` instead. */
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

  layoutChapter(book: string, chapter: number, cfg: LayoutCfg): DisplayList | null {
    const cfgPtr = (this.#w.exports.plumbline_web_alloc as Function)(28) as number;
    const dv = new DataView(this.#w.exports.memory.buffer);
    [cfg.width, cfg.lineHeight, cfg.spaceWidth, cfg.verseNumGap, cfg.paraIndent, cfg.paraSpacing].forEach(
      (v, i) => dv.setFloat32(cfgPtr + i * 4, v, true),
    );
    dv.setUint32(cfgPtr + 24, cfg.versePerLine ? 1 : 0, true);
    const dl = this.#call(
      (b) =>
        (this.#w.exports.plumbline_engine_layout_chapter as Function)(
          this.#engine, b, chapter, cfgPtr, this.#w.measureFnptr, 0,
        ) as number,
      [book],
    );
    (this.#w.exports.plumbline_web_free as Function)(cfgPtr, 28);
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
  /** One hymn, chords transposed `semis` semitones and split into paintable
   *  parts. Null for an unknown id.
   *
   *  Hand-marshalled rather than through `#json`, which turns every argument
   *  into a string pointer — `semis` is an i32 and has to cross as one. */
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
  /** Whether this home carries a usable overlay (false until stage 2 lands). */
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
  /** Tell the engine we warm in SLICES, so it must never build an index inside
   *  a reader's request (wasm-only export). Call it right after open — the warm
   *  itself starts only after stage 2 lands, and a tap in that window would
   *  otherwise build everything at once and freeze this thread. */
  deferBuilds(on: boolean): void {
    (this.#w.exports.plumbline_engine_defer_builds as Function)(this.#engine, on ? 1 : 0);
  }
  // NO verseSimSave / verseSimLoad. Nothing the engine builds now is worth
  // persisting between tabs.

  /** Load the R&D artifacts from the home if they arrived after open (the
   *  deferred pack); no-op when already loaded or still missing. */
  loadRndData(): string | null {
    return this.#text("plumbline_engine_load_rnd_data");
  }
  /** Load ONE machine-tier artifact (wasm-only export): true while more
   *  remain. Split so a ~17 MB parse can't hold the worker for seconds. */
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
  threadBlocks(index: number): any {
    const s = this.#w.takeStr((this.#w.exports.plumbline_engine_thread_blocks_json as Function)(this.#engine, index) as number);
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
   *  front of somebody, who has scrolled as far as `reached` and touched
   *  something (`interacted`) or not. A null `book` means nothing is being read
   *  — a dialog is up, the tab went hidden, the reader left — and banks the tail.
   *
   *  The counters and the thresholds live in the core (`reading::DwellTracker`),
   *  so a shell holds only its clock. Answers null on almost every call, and the
   *  same `{book,chapter,pct,completed,lastRead?}` `readingRecord` gives when it
   *  banked a report. */
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

  // ── reading plans + the speedrun ──────────────────────────────────────────
  /** `{running:[…], builtins:[…]}` — the reader's plans with derived state,
   *  and the catalogue the picker offers. */
  plans(now: string): any {
    return this.#json("plumbline_engine_plans_json", now);
  }
  /** Start a built-in schedule by id (replaces its class occupant). */
  planStart(id: string, now: string): string | null {
    return this.#author("plumbline_engine_plan_start", (f, i, n) => f(this.#engine, i, n), [id, now]);
  }
  /** Stop a plan (removes its file; a speedrun's tag is untouched). */
  planStop(id: string): string | null {
    return this.#author("plumbline_engine_plan_stop", (f, i) => f(this.#engine, i), [id]);
  }
  /** Start or resume a speedrun for `tag`; returns the run's id — what the
   *  shell writes into `config.speedrun` to enter the mode. An error comes
   *  back prefixed with `!` (no plan id can start with one). Null only if the
   *  engine itself returned nothing. */
  speedrunStart(tag: string, now: string): string | null {
    const id = this.#text("plumbline_engine_speedrun_start", tag, now);
    if (id !== null && !id.startsWith("!")) this.onAuthored();
    return id;
  }
  /** Mark a chapter swept in a speedrun (generous, any order). */
  speedrunSweep(id: string, book: string, chapter: number): string | null {
    return this.#author(
      "plumbline_engine_speedrun_sweep",
      (f, i, b) => f(this.#engine, i, b, chapter),
      [id, book],
    );
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

/** The share link this reader hands over, plus the church it carries and the
 *  two strings a Church button needs (`core::church`). Engine-independent.
 *
 *  The web shell's own `shell/church.ts` still builds share links, because they
 *  are read synchronously out of derived state and this crosses a worker. This
 *  binding exists so the ABI's TS sibling stays method-for-method with the
 *  Kotlin one, and so a test can ask the engine what the answer should be. */
export function shareLink(
  w: WasmEngine,
  request: { base?: string; church?: { name: string; info: string; url: string }; startAsNewBeliever?: boolean; at?: string },
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

/** Every string the shell paints, in the reader's language, in ONE call.
 *
 *  Both arguments, because the core owns the rule that an empty setting means
 *  "follow the device" — see `i18n::resolve`. The reply's `lang` says which one
 *  won. Engine-independent: the chrome has to exist before an engine does. */
export function i18nCatalog(w: WasmEngine, chosen: string, device: string): any {
  const a = w.inStr(chosen);
  const b = w.inStr(device);
  const s = w.takeStr((w.exports.plumbline_i18n_catalog_json as Function)(a, b) as number);
  w.freeStr(a);
  w.freeStr(b);
  return s === null ? null : JSON.parse(s);
}

/** Tell the ENGINE which language to write in, and get back the code it chose.
 *
 *  The catalogue covers what a shell spells; this covers what the core spells —
 *  book names and references, in the TOC, search hits, weave endpoints, note
 *  headers, the reading map. Both, or a German reader gets a German interface
 *  listing a book called Genesis. Engine-independent, and must be called BEFORE
 *  the boot reply builds the TOC. */
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
