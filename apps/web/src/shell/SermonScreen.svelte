<script lang="ts">
  // The sermon editor: a THREAD written top to bottom — headings, the
  // preacher's own points, and passages — in the order it will be preached.
  // Nothing new is stored: the shape is the thread's `outline` (core
  // thread::OutlineBlock), the passages are its entries, and the notes under
  // them are the entries' notes. Present walks the same thread; the study
  // panel reads the same outline.
  //
  // Desktop first: one wide column, every block a row with its controls on
  // the right, and reordering by buttons (↑ ↓) — a thing a mouse, a keyboard
  // and a thumb all do the same way.
  //
  // Edits save themselves. Text is debounced; a structural change (move, add,
  // remove) saves at once. A save in flight must not clobber what is being
  // typed, so the draft is only re-read from the engine when nothing local is
  // newer (`gen`).
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { nowStamp } from "../engine/StudyEngine";
  import { dispatchLink } from "../study/links";
  import { t } from "../lib/i18n.svelte";
  import { tick, untrack } from "svelte";

  const s = getSession();

  type Block =
    | { uid: number; kind: "heading"; text: string }
    | { uid: number; kind: "point"; text: string }
    | { uid: number; kind: "passage"; ref: string; added: string; entry: number };

  let uidSeq = 0;
  /** The seeded shape — the five places a sermon has, each a heading the
   *  author may rename. Keyed by catalogue id; the hint under one shows while
   *  its heading still reads as seeded and nothing sits under it. */
  const SEED = ["me", "need", "bible", "you", "we"] as const;

  // `qStale`: every authoring write invalidates the cache, and a plain `q`
  // answers null until the worker has re-read — a beat in which the open
  // thread would look gone. The last real answer serves until the fresh one.
  const threads = $derived.by(() => {
    void s.studyEpoch;
    return ((s.qStale("threads")?.threads ?? []) as any[]).slice().sort((a, b) => a.name.localeCompare(b.name));
  });
  const thread = $derived(s.sermonName ? (threads.find((x) => x.name === s.sermonName) ?? null) : null);

  let draft = $state<Block[]>([]);
  /** Bumped by every local edit; a save only marks the draft clean if nothing
   *  was typed while it was in flight. 0 = clean. */
  let gen = $state(0);
  let timer: ReturnType<typeof setTimeout> | null = null;
  let focusUid = $state<number | null>(null);

  function fromEngine(th: any): Block[] {
    const outline = (th.outline ?? []) as any[];
    // A thread that never had a shape is its passages in order; the first save
    // of this editor gives it one.
    if (outline.length === 0) {
      return (th.entries as any[]).map((e, i) => ({ uid: ++uidSeq, kind: "passage", ref: e.verse, added: e.added, entry: i }));
    }
    return outline.map((b) =>
      b.kind === "passage"
        ? { uid: ++uidSeq, kind: "passage", ref: th.entries[b.entry]?.verse ?? "", added: th.entries[b.entry]?.added ?? "", entry: b.entry }
        : { uid: ++uidSeq, kind: b.kind, text: b.text },
    );
  }
  $effect(() => {
    const th = thread;
    if (!s.sermonName) {
      draft = [];
      gen = 0;
      return;
    }
    // A miss while a write lands is not "the thread is gone": the draft stays,
    // or a pending save would write an empty shape over a real one.
    if (!th || gen !== 0) return;
    const next = fromEngine(th);
    // Every save comes back as a fresh thread, and `fromEngine` mints new uids.
    // The list is keyed by uid, so taking them as-is replaced every block's DOM —
    // and with it the field the reader was typing in, which lost focus after each
    // save (the new point after "Add point", and every pause while writing). A
    // block in the same place and of the same kind is the same block: it keeps
    // its uid, and its element.
    const prev = untrack(() => draft);
    for (let i = 0; i < next.length && i < prev.length; i++) {
      if (next[i].kind === prev[i].kind) next[i].uid = prev[i].uid;
    }
    draft = next;
  });

  function wire(blocks: Block[]): string {
    return JSON.stringify(
      blocks.map((b) => (b.kind === "passage" ? { kind: "passage", ref: b.ref, added: b.added } : { kind: b.kind, text: b.text })),
    );
  }
  async function flush(name: string): Promise<void> {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    const g = gen;
    const err = await s.author("threadSetOutline", name, wire(draft));
    if (err) s.showToast(err);
    // Wait for the save's own echo before marking the draft clean. The write
    // clears the read cache, and until the refetch lands `thread` answers with
    // the thread as it was BEFORE this save; a draft marked clean in that window
    // is rebuilt from it, so a block added a moment ago vanishes — cursor and
    // all — and comes back as a new element when the fresh thread arrives. On a
    // slow machine that window is wide (the v0.78.0 and v0.78.1 release runs).
    await s.fetchQ("threads").catch(() => {});
    if (gen === g) gen = 0;
  }
  /** A text edit: saved after a pause. */
  function touch(): void {
    gen++;
    const name = s.sermonName;
    if (!name) return;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void flush(name), 600);
  }
  /** A structural edit: saved now. */
  function change(): Promise<void> {
    gen++;
    const name = s.sermonName;
    return name ? flush(name) : Promise.resolve();
  }
  // Leaving the editor with a pause still pending saves what was typed.
  $effect(() => {
    const name = s.sermonName;
    return () => {
      if (timer && name) void flush(name);
    };
  });
  $effect(() => {
    if (focusUid === null) return;
    void tick().then(() => {
      const el = document.querySelector<HTMLElement>(`[data-uid="${focusUid}"] .text`);
      el?.focus();
      focusUid = null;
    });
  });

  // ── the list ────────────────────────────────────────────────────────────────

  async function newSermon(): Promise<void> {
    const name = (await s.askText(t("sermon.nameAsk")))?.trim();
    if (!name) return;
    const shape = SEED.map((id) => ({ kind: "heading", text: t(`sermon.section.${id}`) }));
    const err = await s.author("threadCreate", name, JSON.stringify(shape), nowStamp());
    if (err) {
      s.showToast(err);
      return;
    }
    gen = 0;
    s.sermonName = name;
  }

  // ── the blocks ──────────────────────────────────────────────────────────────

  /** Whether block `i` ends a section: the last block, or the one before a heading. */
  const endsSection = (i: number): boolean => i === draft.length - 1 || draft[i + 1].kind === "heading";
  /** The seed id a heading still matches, for its hint. */
  function seedOf(text: string): (typeof SEED)[number] | null {
    return SEED.find((id) => t(`sermon.section.${id}`) === text.trim()) ?? null;
  }

  function move(i: number, by: -1 | 1): void {
    const j = i + by;
    if (j < 0 || j >= draft.length) return;
    [draft[i], draft[j]] = [draft[j], draft[i]];
    void change();
  }
  async function remove(i: number): Promise<void> {
    const b = draft[i];
    const name = s.sermonName;
    if (!name) return;
    if (b.kind === "passage") {
      const en = thread?.entries?.[b.entry];
      const ok = await s.askConfirm(
        t("threads.removeEntryAsk", { passage: en?.display ?? b.ref }),
        t("threads.removeEntryBody", { thread: name }),
        t("threads.removeEntryVerb"),
      );
      if (!ok) return;
      // Anything typed first, so the entry removal (which the engine reconciles
      // against the outline) sees the current shape.
      if (gen !== 0) await flush(name);
      const err = await s.author("threadEntryRemove", name, b.entry);
      if (err) s.showToast(err);
      return;
    }
    if (b.text.trim()) {
      const ok = await s.askConfirm(
        t(b.kind === "heading" ? "sermon.removeHeadingAsk" : "sermon.removePointAsk"),
        t("sermon.removeBody"),
        t("sermon.removeVerb"),
      );
      if (!ok) return;
    }
    draft.splice(i, 1);
    void change();
  }
  function addPoint(after: number): void {
    const b: Block = { uid: ++uidSeq, kind: "point", text: "" };
    draft.splice(after + 1, 0, b);
    focusUid = b.uid;
    void change();
  }
  function addHeading(): void {
    const b: Block = { uid: ++uidSeq, kind: "heading", text: "" };
    draft.push(b);
    focusUid = b.uid;
    void change();
  }
  async function addPassage(after: number): Promise<void> {
    const name = s.sermonName;
    if (!name) return;
    const typed = (await s.askText(t("sermon.passageAsk")))?.trim();
    if (!typed) return;
    const found = await s.fetchQ("passageQuery", typed);
    if (!found?.verse) {
      s.showToast(t("sermon.passageUnknown"));
      return;
    }
    const added = nowStamp();
    // The draft is newer than the engine from here until the shape is saved,
    // so the add's reload must not overwrite it.
    gen++;
    const err = await s.author("threadAddRange", name, found.verse, found.end ?? 0, null, added);
    if (err) {
      s.showToast(err);
      gen = 0;
      return;
    }
    draft.splice(after + 1, 0, { uid: ++uidSeq, kind: "passage", ref: found.verse, added, entry: -1 });
    await flush(name);
  }

  // Passage notes are the entries' own, saved through their own call.
  let noteEdits = $state<Record<number, string>>({});
  const noteTimers: Record<number, ReturnType<typeof setTimeout>> = {};
  function noteTouch(entry: number, text: string): void {
    const name = s.sermonName;
    if (!name) return;
    noteEdits[entry] = text;
    clearTimeout(noteTimers[entry]);
    noteTimers[entry] = setTimeout(() => {
      void s.author("threadEntrySetNote", name, entry, text).then((err) => {
        if (err) s.showToast(err);
        else if (noteEdits[entry] === text) delete noteEdits[entry];
      });
    }, 600);
  }

  /** refKey → the core's `go:` verb (see ExploreScreen). */
  const goUri = (refKey: string): string => `go:${refKey.replace(/ (?=\S*$)/, ":")}`;
  function passageLabel(entry: any): string {
    if (!entry) return "";
    return entry.end ? `${entry.display}–${entry.end}` : entry.display;
  }
  function snippet(entry: any): string {
    const joined = ((entry?.text ?? []) as string[]).join(" ");
    return joined.length > 220 ? `${joined.slice(0, 220).trimEnd()}…` : joined;
  }
  /** Grow a textarea to its text. */
  function autosize(el: HTMLTextAreaElement): { destroy(): void } {
    const fit = () => {
      el.style.height = "auto";
      el.style.height = `${el.scrollHeight}px`;
    };
    fit();
    el.addEventListener("input", fit);
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return {
      destroy() {
        el.removeEventListener("input", fit);
        ro.disconnect();
      },
    };
  }
</script>

{#if !s.sermonName}
  <section class="screen" aria-label={t("explore.sermons")}>
    <ScreenBar title={t("explore.sermons")} onBack={() => (s.screen = s.sermonFrom)} onMenu={() => (s.menuOpen = true)} />
    <div class="content list" data-surface="sermons">
      <button class="primary" onclick={() => void newSermon()}>{t("sermon.new")}</button>
      {#each threads as th (th.name)}
        <button class="row" onclick={() => (s.sermonName = th.name)}>
          <span class="name">{th.name}</span>
          {#if (th.outline?.length ?? 0) > 0}<span class="shaped">{t("explore.sermons")}</span>{/if}
        </button>
      {:else}
        <p class="empty">{t("sermon.empty")}</p>
      {/each}
    </div>
  </section>
{:else}
  <section class="screen" aria-label={s.sermonName}>
    <ScreenBar
      title={s.sermonName}
      backLabel={t("sermon.backToList")}
      onBack={() => (s.sermonName = null)}
      onMenu={() => (s.menuOpen = true)}
    >
      {#snippet actions()}
        <button
          class="present"
          onclick={() => {
            s.presentThreadName = s.sermonName;
            s.showPresent = true;
          }}>{t("nav.present")}</button
        >
      {/snippet}
    </ScreenBar>
    <div class="content editor" data-surface="sermon editor">
      {#each draft as b, i (b.uid)}
        <div class="block {b.kind}" data-uid={b.uid} data-kind={b.kind}>
          {#if b.kind === "heading"}
            <input class="text" placeholder={t("sermon.heading")} bind:value={b.text} oninput={touch} onblur={touch} />
          {:else if b.kind === "point"}
            <textarea class="text" rows="1" placeholder={t("sermon.point")} bind:value={b.text} oninput={touch} use:autosize></textarea>
          {:else}
            {@const en = thread?.entries?.[b.entry]}
            <div class="passage-body">
              <button class="ref" onclick={(ev) => void dispatchLink(s, goUri(b.ref), ev)}>{passageLabel(en) || b.ref}</button>
              {#if snippet(en)}<p class="snippet">“{snippet(en)}”</p>{/if}
              <textarea
                class="note"
                rows="1"
                placeholder={t("sermon.note")}
                value={noteEdits[b.entry] ?? en?.note ?? ""}
                oninput={(e) => noteTouch(b.entry, e.currentTarget.value)}
                use:autosize
              ></textarea>
            </div>
          {/if}
          <div class="controls">
            <button class="ctl" disabled={i === 0} title={t("sermon.moveUp")} aria-label={t("sermon.moveUp")} onclick={() => move(i, -1)}>↑</button>
            <button class="ctl" disabled={i === draft.length - 1} title={t("sermon.moveDown")} aria-label={t("sermon.moveDown")} onclick={() => move(i, 1)}>↓</button>
            <button class="ctl del" title={t("sermon.remove")} aria-label={t("sermon.remove")} onclick={() => void remove(i)}>✕</button>
          </div>
        </div>
        {#if b.kind === "heading" && endsSection(i) && seedOf(b.text)}
          <p class="hint">{t(`sermon.hint.${seedOf(b.text)}`)}</p>
        {/if}
        {#if endsSection(i)}
          <div class="adders">
            <button onclick={() => addPoint(i)}>{t("sermon.addPoint")}</button>
            <button onclick={() => void addPassage(i)}>{t("sermon.addPassage")}</button>
          </div>
        {/if}
      {:else}
        <div class="adders">
          <button onclick={() => addPoint(-1)}>{t("sermon.addPoint")}</button>
          <button onclick={() => void addPassage(-1)}>{t("sermon.addPassage")}</button>
        </div>
      {/each}
      <div class="adders tail">
        <button onclick={addHeading}>{t("sermon.addHeading")}</button>
      </div>
    </div>
  </section>
{/if}

<style>
  .screen {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--paper, #fcf9f4);
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 14px 16px 40px;
    width: min(820px, 100%);
    margin: 0 auto;
    box-sizing: border-box;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: stretch;
  }
  .primary {
    align-self: flex-start;
    padding: 8px 16px;
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 8px;
    color: var(--gold, #9e7d38);
    font-weight: 600;
    background: var(--popupPaper, #f2eee6);
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    text-align: start;
    padding: 12px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    background: var(--popupPaper, #f2eee6);
    color: var(--ink, #211f1a);
    font-size: calc(16px * var(--uiScale, 1));
  }
  .row:hover {
    border-color: var(--gold, #9e7d38);
  }
  .shaped {
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .empty,
  .hint {
    color: var(--faded, #8a8276);
    font-size: calc(14px * var(--uiScale, 1));
    margin: 4px 0 6px;
  }
  .hint {
    padding-inline-start: 4px;
  }
  .present {
    padding: 4px 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    color: var(--gold, #9e7d38);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .block {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 4px 0;
  }
  .block.heading {
    margin-top: 22px;
  }
  .block.heading:first-child {
    margin-top: 4px;
  }
  .text,
  .note {
    flex: 1;
    min-width: 0;
    width: 100%;
    box-sizing: border-box;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--ink, #211f1a);
    font: inherit;
    line-height: 1.45;
    padding: 6px 8px;
    resize: none;
    overflow: hidden;
  }
  .text:hover,
  .note:hover {
    border-color: var(--rule, #d8cba8);
  }
  .text:focus,
  .note:focus {
    outline: none;
    border-color: var(--gold, #9e7d38);
    background: var(--popupPaper, #f2eee6);
  }
  .heading .text {
    font-size: calc(19px * var(--uiScale, 1));
    font-weight: 700;
  }
  .point .text {
    font-size: calc(16px * var(--uiScale, 1));
  }
  .passage-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-inline-start: 3px solid var(--gold, #9e7d38);
    border-radius: 6px;
    background: var(--popupPaper, #f2eee6);
  }
  .ref {
    align-self: flex-start;
    min-height: auto;
    padding: 0;
    color: var(--gold, #9e7d38);
    font-weight: 600;
    font-size: calc(16px * var(--uiScale, 1));
  }
  .snippet {
    margin: 0;
    color: var(--faded, #8a8276);
    font-style: italic;
    font-size: calc(14px * var(--uiScale, 1));
    line-height: 1.4;
  }
  .note {
    font-size: calc(14.5px * var(--uiScale, 1));
    padding-inline-start: 0;
  }
  .controls {
    display: flex;
    gap: 2px;
    flex: none;
    padding-top: 2px;
  }
  .ctl {
    min-width: 34px;
    min-height: 34px;
    padding: 0;
    border-radius: 6px;
    color: var(--gold, #9e7d38);
    font-size: 16px;
  }
  .ctl:disabled {
    color: var(--rule, #d8cba8);
  }
  .ctl.del {
    color: var(--faded, #8a8276);
  }
  .ctl:hover:not(:disabled) {
    background: var(--popupPaper, #f2eee6);
  }
  .adders {
    display: flex;
    gap: 6px;
    margin: 2px 0 6px;
    padding-inline-start: 4px;
  }
  .adders button {
    min-height: 30px;
    padding: 3px 10px;
    border: 1px dashed var(--rule, #d8cba8);
    border-radius: 6px;
    color: var(--faded, #8a8276);
    font-size: calc(13.5px * var(--uiScale, 1));
  }
  .adders button:hover {
    color: var(--gold, #9e7d38);
    border-color: var(--gold, #9e7d38);
  }
  .adders.tail {
    margin-top: 18px;
  }
</style>
