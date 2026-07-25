<script lang="ts">
  // First-run mode chooser (manifest §Simple/Full): two cards; closing
  // without choosing keeps Simple.
  import { getSession } from "../state/session.svelte";

  const s = getSession();

  function choose(mode: "simple" | "full"): void {
    // studyMode round-trips for older readers; the real switches are the
    // per-tier gates (togglable any time under ≡ Analysis).
    s.config.studyMode = mode;
    s.config.humanAnalysis = mode === "full";
    s.config.machineAnalysis = mode === "full";
    s.showFirstRun = false;
    s.saveConfig();
  }
</script>

{#if s.showFirstRun}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="backdrop" onclick={() => choose("simple")}></div>
  <div class="dialog" role="dialog" aria-modal="true">
    <h2>Welcome to pure study</h2>
    <div class="cards">
      <button class="card" onclick={() => choose("simple")}>
        <span class="name">Simple reader</span>
        <span class="desc">Just the text — reading, search, and the words behind the words.</span>
      </button>
      <button class="card" onclick={() => choose("full")}>
        <span class="name">Full study</span>
        <span class="desc">Everything — threads, tags, weaves, maps, memorization, and the research tiers.</span>
      </button>
    </div>
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
    top: 22vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(560px, 94vw);
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 12px;
    padding: 22px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.25);
  }
  h2 {
    font-size: 20px;
    font-weight: 500;
    text-align: center;
    margin-bottom: 16px;
  }
  .cards {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }
  .card {
    flex: 1;
    min-width: 200px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    text-align: left;
    padding: 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--paper, #fcf9f4);
  }
  .card:hover {
    border-color: var(--gold, #9e7d38);
  }
  .name {
    font-weight: 600;
    color: var(--gold, #9e7d38);
  }
  .desc {
    font-size: 13.5px;
    color: var(--faded, #8a8276);
  }
</style>
