<script lang="ts">
  // Memorization (manifest §Memorization, Android-hub layout): a hub listing
  // every card with Review due / Coverage & activity, the SM-2 review drill
  // (first letters / blank-out slider / typed recall + four grades), and the
  // stats view (8-section coverage rollup + reviews-per-day activity).
  import { getSession } from "../state/session.svelte";
  import { nowStamp } from "../engine/StudyEngine";
  import { dispatchLink } from "../study/links";

  const MAX_BLANK_LEVEL = 4; // core memory::MAX_BLANK_LEVEL

  const s = getSession();

  const view = $derived(s.memorize);

  // ── hub data ──
  const coverage = $derived.by(() => {
    void s.studyEpoch;
    return view ? s.engine.memoryCoverage(nowStamp()) : null;
  });
  const dueRefs = $derived.by(() => {
    void s.studyEpoch;
    return view ? ((s.engine.memoryDue(nowStamp())?.refs ?? []) as string[]) : [];
  });

  function close(): void {
    s.memorize = null;
  }
  function goRef(ref: string): void {
    close();
    void dispatchLink(s, `go:${ref.replace(" ", ":")}`);
  }

  // ── review state ──
  let queue = $state<string[]>([]);
  let qi = $state(0);
  let mode = $state<"first" | "blank" | "typed">("first");
  let level = $state(2);
  let typed = $state("");
  let score = $state<any>(null);

  $effect(() => {
    if (view?.view === "review") {
      queue = view.only ? [view.only] : [...dueRefs];
      qi = 0;
      mode = "first";
      typed = "";
      score = null;
    }
  });

  const currentRef = $derived(view?.view === "review" ? queue[qi] : undefined);
  const drill = $derived(currentRef ? s.engine.memoryDrill(currentRef, level) : null);

  function grade(g: "again" | "hard" | "good" | "easy"): void {
    if (!currentRef) return;
    const err = s.engine.memoryGrade(currentRef, g, nowStamp());
    if (err) {
      s.showToast(err);
      return;
    }
    typed = "";
    score = null;
    mode = "first";
    qi++;
  }
  function check(): void {
    if (currentRef) score = s.engine.memoryScore(currentRef, typed);
  }

  const masteryColor: Record<string, string> = {
    new: "var(--faded, #8a8276)",
    learning: "var(--tierResearch, #b04a3a)",
    young: "var(--section, #a0894a)",
    mature: "var(--tierHuman, #6f8f6a)",
  };

  const activity = $derived.by(() => {
    void s.studyEpoch;
    return view?.view === "stats" ? ((s.engine.memoryActivity()?.days ?? []) as { day: string; reviews: number }[]) : [];
  });
  const maxReviews = $derived(Math.max(1, ...activity.map((d) => d.reviews)));
</script>

{#if view}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={close}></div>
  <div class="dialog" role="dialog" aria-modal="true">
    <div class="bar">
      <span class="title">
        {view.view === "hub" ? "Memorize" : view.view === "review" ? "Review" : "Coverage & activity"}
      </span>
      {#if view.view !== "hub"}
        <button class="navbtn" onclick={() => (s.memorize = { view: "hub" })}>‹ hub</button>
      {/if}
      <span class="spacer"></span>
      <button class="close" onclick={close} aria-label="Close">✕</button>
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
        {#if !coverage?.verses?.length}
          <p class="empty">
            No cards yet — long-press or right-click a verse and choose “Memorize this verse”.
          </p>
        {:else}
          {#each coverage.verses as v (v.ref)}
            <div class="card">
              <button class="ref" onclick={() => goRef(v.ref)}>{v.ref}</button>
              <span class="mastery" style:color={masteryColor[v.mastery] ?? "inherit"}>{v.mastery}</span>
              {#if v.due}<span class="due">due</span>{/if}
              <span class="spacer"></span>
              <button class="drill" onclick={() => (s.memorize = { view: "review", only: v.ref })}>drill</button>
              <button
                class="remove"
                title="Remove card"
                onclick={() => {
                  const err = s.engine.memoryRemove(v.ref);
                  if (err) s.showToast(err);
                }}>✕</button
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
          <p class="drillref">{currentRef} <span class="pos">{qi + 1} / {queue.length}</span></p>
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
            <button class="checkbtn" onclick={check}>Check</button>
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
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 38;
  }
  .dialog {
    position: fixed;
    z-index: 39;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(520px, 94vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    box-shadow: 0 16px 64px rgba(0, 0, 0, 0.3);
    overflow: hidden;
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
    font-size: 13.5px;
  }
  .spacer {
    flex: 1;
  }
  .close {
    color: var(--faded, #8a8276);
    padding: 2px 6px;
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
    font-size: 14px;
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
    font-size: 12px;
  }
  .due {
    font-size: 11px;
    color: var(--tierResearch, #b04a3a);
    border: 1px solid currentColor;
    border-radius: 4px;
    padding: 0 4px;
  }
  .drill {
    font-size: 12.5px;
    color: var(--gold, #9e7d38);
  }
  .remove {
    color: var(--faded, #8a8276);
    font-size: 12px;
  }
  .drillref {
    font-weight: 600;
  }
  .pos {
    color: var(--faded, #8a8276);
    font-weight: 400;
    font-size: 12.5px;
  }
  .modes {
    display: flex;
    gap: 6px;
  }
  .modes button {
    padding: 3px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    font-size: 13px;
  }
  .modes button.checked {
    border-color: var(--gold, #9e7d38);
    color: var(--gold, #9e7d38);
  }
  .drilltext {
    font-size: 17px;
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
    font-size: 13px;
  }
  .reveal summary {
    cursor: pointer;
    color: var(--faded, #8a8276);
    font-size: 13px;
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
    font-size: 14px;
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
    font-size: 12px;
  }
  h3 {
    font-size: 13px;
    color: var(--section, #a0894a);
    margin-top: 10px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
  }
  .day {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
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
