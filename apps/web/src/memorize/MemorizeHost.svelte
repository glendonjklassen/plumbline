<script lang="ts">
  // Memorization (manifest §Memorization, Android-hub layout): a hub listing
  // every card with Review due / Coverage & activity, the SM-2 review drill
  // (first letters / blank-out slider / typed recall + four grades), and the
  // stats view (8-section coverage rollup + reviews-per-day activity).
  //
  // Its own SCREEN, not a modal (2026-07-29 — "memorize should be its own screen
  // like it is on Android"). It was a 520px centred dialog with a backdrop, which
  // made the app's second-biggest surface feel like a confirmation prompt and left
  // the reader scrolling behind it. It now fills the body between the top bar and
  // the destination nav, exactly as ui/MemorizeScreen.kt does.
  import { untrack } from "svelte";
  import { getSession } from "../state/session.svelte";
  import { nowStamp } from "../engine/StudyEngine";
  import { dispatchLink } from "../study/links";

  const MAX_BLANK_LEVEL = 4; // core memory::MAX_BLANK_LEVEL

  const s = getSession();

  const view = $derived(s.memorize);

  // ── hub data ──
  // ONE timestamp per opening of the dialog. `nowStamp()` is second-granularity
  // and lands in the read-through cache KEY, so computing it inside these reads
  // minted a fresh key every second: the answer went null, `dueRefs` fell back
  // to [], and the reset effect below re-ran — about once a second it threw the
  // reader out of "Type it" and discarded what they had typed, which made typed
  // recall unusable and the e2e drill tests flaky (feedback 2026-07-27).
  // Keyed on the dialog being OPEN, not on `view`, so hub→review keeps the same
  // stamp and the queue snapshot below still sees a resolved due list; and on
  // studyEpoch, so a card just added or graded is scored against now.
  const open = $derived(!!view);
  const stamp = $derived.by(() => {
    void s.studyEpoch;
    return open ? nowStamp() : "";
  });

  const coverage = $derived(open ? s.q("memoryCoverage", stamp) : null);
  const dueRefs = $derived(open ? ((s.q("memoryDue", stamp)?.refs ?? []) as string[]) : []);

  /** Drop a memorization card — the schedule and its whole review log with it,
   *  which is why it asks first. */
  async function removeCard(ref: string, label: string): Promise<void> {
    const ok = await s.askConfirm(
      `Stop memorizing ${label}?`,
      "Do you really want to delete this?",
      "Remove card",
    );
    if (!ok) return;
    const err = await s.author("memoryRemove", ref);
    s.showToast(err ?? `Removed ${label}`);
  }

  function close(): void {
    // A destination closes by going home — leaving `screen` on "memorize" with no
    // view would render an empty screen with no way out.
    s.goRead();
  }
  /** refKey → the core's `go:` verb, split on the LAST space, as core `go_uri`
   *  does. Also in App.svelte and StudyPanel — see App.svelte for why. */
  const goUri = (refKey: string): string => `go:${refKey.replace(/ (?=\S*$)/, ":")}`;

  function goRef(ref: string): void {
    close();
    void dispatchLink(s, goUri(ref));
  }

  // ── review state ──
  let queue = $state<string[]>([]);
  let qi = $state(0);
  let mode = $state<"first" | "blank" | "typed">("first");
  let level = $state(2);
  let typed = $state("");
  let score = $state<any>(null);

  // ENTERING the drill resets it, and nothing else does. The queue is
  // snapshotted once on entry — Android does the same with `remember(only)`
  // ("this session works the queue as it stood on entry"), and grading walks
  // `qi` forward itself. The reads are untracked on purpose: tracking `dueRefs`
  // here meant any study refresh — a grade landing, Strong's arriving, the
  // per-second stamp churn above — wiped the reader's typing, dropped the mode
  // back to "First letters" and sent them to the head of the queue.
  $effect(() => {
    const v = view;
    if (v?.view !== "review") return;
    untrack(() => {
      queue = v.only ? [v.only] : [...dueRefs];
      qi = 0;
      mode = "first";
      typed = "";
      score = null;
    });
  });

  const currentRef = $derived(view?.view === "review" ? queue[qi] : undefined);
  const drill = $derived(currentRef ? s.q("memoryDrill", currentRef, level) : null);

  function grade(g: "again" | "hard" | "good" | "easy"): void {
    if (!currentRef) return;
    void s.author("memoryGrade", currentRef, g, nowStamp()).then((err) => {
      if (err) s.showToast(err);
    });
    typed = "";
    score = null;
    mode = "first";
    qi++;
  }
  // The engine lives in the worker, so scoring is a round trip. `s.engine` is
  // the console/e2e proxy and returns a PROMISE — assigning it straight to
  // `score` made every check read "0% recalled", even a perfect copy/paste
  // (feedback 2026-07-27). Go through the cache like every other read.
  async function check(): Promise<void> {
    if (!currentRef) return;
    score = await s.fetchQ("memoryScore", currentRef, typed);
  }

  const masteryColor: Record<string, string> = {
    new: "var(--faded, #8a8276)",
    learning: "var(--tierResearch, #b04a3a)",
    young: "var(--section, #a0894a)",
    mature: "var(--tierHuman, #6f8f6a)",
  };

  const activity = $derived.by(() => {
    void s.studyEpoch;
    return view?.view === "stats" ? ((s.q("memoryActivity")?.days ?? []) as { day: string; reviews: number }[]) : [];
  });
  const maxReviews = $derived(Math.max(1, ...activity.map((d) => d.reviews)));
</script>

{#if view}
  <section class="screen" aria-label="Memorize">
    <div class="bar">
      <button class="back" onclick={close} aria-label="Back to reading">‹</button>
      <span class="title">
        {view.view === "hub" ? "Memorize" : view.view === "review" ? "Review" : "Coverage & activity"}
      </span>
      {#if view.view !== "hub"}
        <button class="navbtn" onclick={() => (s.memorize = { view: "hub" })}>hub</button>
      {/if}
      <span class="spacer"></span>
    </div>

    {#if view.view === "hub"}
      <div class="content">
        <div class="actions">
          <button
            class="primary"
            disabled={dueRefs.length === 0}
            onclick={() => (s.memorize = { view: "review" })}
          >
            Review due ({dueRefs.length})
          </button>
          <button onclick={() => (s.memorize = { view: "stats" })}>Coverage & activity</button>
        </div>
        <!-- One row per CARD, not per verse: a passage card is a single row
             labelled "Ps 23:1–6" (its `ref` is the first verse, which every
             card endpoint takes). -->
        {#if !coverage?.cards?.length}
          <p class="empty">
            No cards yet — long-press or right-click a verse and choose “Memorize this verse”, or
            “Memorize passage…” for a whole section.
          </p>
        {:else}
          {#each coverage.cards as v (v.ref)}
            <div class="card">
              <button class="ref" onclick={() => goRef(v.ref)}>{v.label ?? v.ref}</button>
              <span class="mastery" style:color={masteryColor[v.mastery] ?? "inherit"}>{v.mastery}</span>
              {#if v.due}<span class="due">due</span>{/if}
              <span class="spacer"></span>
              <button class="drill" onclick={() => (s.memorize = { view: "review", only: v.ref })}>drill</button>
              <button
                class="remove"
                title="Remove card"
                onclick={() => void removeCard(v.ref, v.label ?? v.ref)}>✕</button
              >
            </div>
          {/each}
        {/if}
      </div>
    {:else if view.view === "review"}
      <div class="content">
        {#if !currentRef}
          <p class="empty">
            {queue.length === 0 ? "Nothing due — well kept." : "Done — every card reviewed."}
          </p>
        {:else if drill}
          <p class="drillref">
            {drill.label ?? currentRef}
            <span class="pos">{qi + 1} / {queue.length}</span>
          </p>
          <div class="modes">
            <button class:checked={mode === "first"} onclick={() => (mode = "first")}>First letters</button>
            <button class:checked={mode === "blank"} onclick={() => (mode = "blank")}>Blank out</button>
            <button class:checked={mode === "typed"} onclick={() => (mode = "typed")}>Type it</button>
          </div>
          {#if mode === "first"}
            <p class="drilltext">{drill.firstLetters}</p>
          {:else if mode === "blank"}
            <p class="drilltext">{drill.blanked}</p>
            <input type="range" min="0" max={MAX_BLANK_LEVEL} bind:value={level} aria-label="Blank level" />
          {:else}
            <textarea rows="3" bind:value={typed} placeholder="Type the verse from memory…"></textarea>
            <button class="checkbtn" onclick={() => void check()}>Check</button>
            {#if score}
              <p class="drilltext">
                {#each score.words as w, i (i)}<span
                    class:miss={!w.ok}
                    class:hit={w.ok}>{w.word}
                  </span>{/each}
              </p>
              <p class="accuracy">{Math.round((score.accuracy ?? 0) * 100)}% recalled</p>
            {/if}
          {/if}
          <details class="reveal"><summary>Show the verse</summary><p class="drilltext">{drill.text}</p></details>
          <div class="grades">
            <button onclick={() => grade("again")}>Again</button>
            <button onclick={() => grade("hard")}>Hard</button>
            <button onclick={() => grade("good")}>Good</button>
            <button onclick={() => grade("easy")}>Easy</button>
          </div>
        {/if}
      </div>
    {:else}
      <div class="content">
        <table class="sections">
          <thead><tr><th>Section</th><th>Cards</th><th>Mature</th><th>Reviews</th></tr></thead>
          <tbody>
            {#each coverage?.sections ?? [] as sec (sec.label)}
              <tr>
                <td>{sec.label}</td>
                <td>{sec.cards}</td>
                <td>{sec.mature}</td>
                <td>{sec.reviews}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        <h3>Activity</h3>
        {#if activity.length === 0}
          <p class="empty">No reviews yet.</p>
        {:else}
          {#each [...activity].reverse().slice(0, 30) as d (d.day)}
            <div class="day">
              <span class="date">{d.day}</span>
              <span class="daybar" style:width="{(d.reviews / maxReviews) * 60}%"></span>
              <span class="count">{d.reviews}</span>
            </div>
          {/each}
        {/if}
      </div>
    {/if}
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
    overflow: hidden;
  }
  .back {
    font-size: calc(22px * var(--uiScale, 1));
    line-height: 1;
    padding: 8px 14px;
    border-radius: 6px;
    color: var(--gold, #9e7d38);
  }
  .back:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 14%, transparent);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--rule, #d8cba8);
  }
  .title {
    font-weight: 600;
  }
  .navbtn {
    color: var(--gold, #9e7d38);
    font-size: calc(13.5px * var(--uiScale, 1));
  }
  .spacer {
    flex: 1;
  }
  .content {
    padding: 12px 16px 18px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-bottom: 8px;
  }
  .actions button {
    padding: 6px 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 7px;
  }
  .actions .primary {
    background: var(--gold, #9e7d38);
    color: #fff;
    border-color: var(--gold, #9e7d38);
  }
  .actions .primary:disabled {
    opacity: 0.45;
  }
  .empty {
    color: var(--faded, #8a8276);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 3px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--rule, #d8cba8) 50%, transparent);
  }
  .ref {
    color: var(--gold, #9e7d38);
  }
  .ref:hover {
    text-decoration: underline;
  }
  .mastery {
    font-size: calc(12px * var(--uiScale, 1));
  }
  .due {
    font-size: calc(11px * var(--uiScale, 1));
    color: var(--tierResearch, #b04a3a);
    border: 1px solid currentColor;
    border-radius: 4px;
    padding: 0 4px;
  }
  .drill {
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--gold, #9e7d38);
  }
  .remove {
    color: var(--faded, #8a8276);
    font-size: calc(12px * var(--uiScale, 1));
  }
  .drillref {
    font-weight: 600;
  }
  .pos {
    color: var(--faded, #8a8276);
    font-weight: 400;
    font-size: calc(12.5px * var(--uiScale, 1));
  }
  .modes {
    display: flex;
    gap: 6px;
  }
  .modes button {
    padding: 3px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    font-size: calc(13px * var(--uiScale, 1));
  }
  .modes button.checked {
    border-color: var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
  }
  .drilltext {
    font-size: calc(17px * var(--uiScale, 1));
    line-height: 1.5;
  }
  .drilltext .miss {
    color: var(--tierResearch, #b04a3a);
    text-decoration: underline;
  }
  .drilltext .hit {
    color: var(--tierHuman, #6f8f6a);
  }
  textarea {
    width: 100%;
    background: var(--paper, #fcf9f4);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 6px 8px;
    resize: vertical;
  }
  .checkbtn {
    align-self: flex-start;
    padding: 4px 12px;
    border: 1px solid var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
    border-radius: 6px;
  }
  .accuracy {
    color: var(--faded, #8a8276);
    font-size: calc(13px * var(--uiScale, 1));
  }
  .reveal summary {
    cursor: pointer;
    color: var(--faded, #8a8276);
    font-size: calc(13px * var(--uiScale, 1));
  }
  .grades {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .grades button {
    flex: 1;
    padding: 7px 0;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 7px;
  }
  .grades button:hover {
    border-color: var(--gold, #9e7d38);
  }
  .sections {
    border-collapse: collapse;
    font-size: calc(14px * var(--uiScale, 1));
    width: 100%;
  }
  .sections th,
  .sections td {
    text-align: left;
    padding: 3px 8px 3px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--rule, #d8cba8) 55%, transparent);
  }
  .sections th {
    color: var(--faded, #8a8276);
    font-weight: 500;
    font-size: calc(12px * var(--uiScale, 1));
  }
  h3 {
    font-size: calc(13px * var(--uiScale, 1));
    color: var(--section, #a0894a);
    margin-top: 10px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
  }
  .day {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: calc(12.5px * var(--uiScale, 1));
  }
  .date {
    color: var(--faded, #8a8276);
    width: 6.5em;
  }
  .daybar {
    display: inline-block;
    height: 9px;
    background: var(--gold, #9e7d38);
    border-radius: 3px;
    min-width: 2px;
  }
  .count {
    color: var(--faded, #8a8276);
  }
</style>
