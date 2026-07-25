<script lang="ts">
  // First run: pick the analysis tiers (with examples), not a vague
  // Simple/Full choice — people get set up right when they can see what
  // each layer actually is. The text is always on; both tiers default on
  // and can be changed any time in Settings.
  import { getSession } from "../state/session.svelte";

  const s = getSession();

  let human = $state(true);
  let machine = $state(true);

  function start(): void {
    s.config.humanAnalysis = human;
    s.config.machineAnalysis = machine;
    // studyMode round-trips for older readers of the shared config.
    s.config.studyMode = human || machine ? "full" : "simple";
    s.showFirstRun = false;
    s.saveConfig();
  }
</script>

{#if s.showFirstRun}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={start}></div>
  <div class="dialog" role="dialog" aria-modal="true">
    <h2>Welcome to pure study</h2>
    <p class="sub">
      The 1769 King James text is always on — reading, search, and your own tags, notes, and
      threads. Choose which layers of analysis sit alongside it:
    </p>
    <label class="card">
      <input type="checkbox" bind:checked={human} />
      <span class="body">
        <span class="name">Scholars' analysis <span class="mark human">†</span></span>
        <span class="desc">
          Curated scholarship: how the 1769 renders each original word (<i>agapaō</i> → “love”
          ×27 · “beloved” ×13…), word grammar, the same root traced across the testaments, and
          the Treasury's cross-references.
        </span>
      </span>
    </label>
    <label class="card">
      <input type="checkbox" bind:checked={machine} />
      <span class="body">
        <span class="name">Machine analysis <span class="mark machine">≈</span></span>
        <span class="desc">
          Statistical patterns to weigh for yourself: similar concepts, words that appear
          alongside, verses like this one, and the concept maps.
        </span>
      </span>
    </label>
    <p class="note">Every piece of evidence is marked with where it comes from — ✝ the text · † scholarship · ≈ machine.</p>
    <button class="start" onclick={start}>Start reading</button>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 16, 8, 0.35);
    z-index: 40;
  }
  .dialog {
    position: fixed;
    z-index: 41;
    top: 14vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(520px, 94vw);
    max-height: 76vh;
    overflow-y: auto;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    padding: 22px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h2 {
    font-size: 20px;
    font-weight: 500;
    text-align: center;
  }
  .sub {
    font-size: 14px;
    color: var(--faded, #8a8276);
    text-align: center;
  }
  .card {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--paper, #fcf9f4);
    cursor: pointer;
  }
  .card:hover {
    border-color: var(--gold, #9e7d38);
  }
  .card input {
    margin-top: 4px;
    accent-color: var(--gold, #9e7d38);
    width: 17px;
    height: 17px;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .name {
    font-weight: 600;
  }
  .mark.human {
    color: var(--tierHuman, #6f8f6a);
  }
  .mark.machine {
    color: var(--tierMachine, #999);
  }
  .desc {
    font-size: 13.5px;
    color: var(--faded, #8a8276);
    line-height: 1.4;
  }
  .note {
    font-size: 12px;
    color: var(--faded, #8a8276);
    text-align: center;
  }
  .start {
    align-self: center;
    padding: 8px 26px;
    background: var(--gold, #9e7d38);
    color: #fff;
    border-radius: 8px;
    font-size: 16px;
  }
</style>
