<script lang="ts">
  // Boot: fetch pack → build home → instantiate wasm → open engine, with a
  // progress screen; then hand over to the Shell. Fonts load before the first
  // layout so the measure callback sees real EB Garamond metrics.
  import { boot, type BootPhase } from "./engine/boot";
  import { initSession, type Session } from "./state/session.svelte";
  import Shell from "./shell/Shell.svelte";

  let phase = $state<BootPhase>({ phase: "download", fraction: 0 });
  let error = $state<string | null>(null);
  let session = $state<Session | null>(null);

  async function start(): Promise<void> {
    try {
      await Promise.all([
        document.fonts.load('18px "EB Garamond"'),
        document.fonts.load('italic 18px "EB Garamond"'),
        document.fonts.load('bold 18px "EB Garamond"'),
      ]);
      const result = await boot((p) => (phase = p));
      session = initSession(result);
      // Kill the first-study-click pause off the critical path (Tier-0 #6),
      // GTK-style: proactive, after first paint.
      if (session.gates & 2) setTimeout(() => session!.engine.warmIndexes(), 400);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  start();

  const phaseLabel = $derived(
    phase.phase === "download"
      ? `Fetching scripture data — ${Math.round((phase.fraction ?? 0) * 100)}%`
      : phase.phase === "prepare"
        ? "Preparing the study engine…"
        : "Opening the text…",
  );
</script>

{#if session}
  <Shell />
{:else}
  <div class="splash">
    <div class="mark">✦</div>
    <h1>pure study</h1>
    <p class="sub">1769 King James Version</p>
    {#if error}
      <p class="error">{error}</p>
      <button onclick={() => location.reload()}>Retry</button>
    {:else}
      <div class="bar">
        <div
          class="fill"
          class:indeterminate={phase.phase !== "download"}
          style:width={phase.phase === "download" ? `${(phase.fraction ?? 0) * 100}%` : "100%"}
        ></div>
      </div>
      <p class="detail">{phaseLabel}</p>
    {/if}
  </div>
{/if}

<style>
  .splash {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    background: #fcf9f4;
    color: #211f1a;
  }
  .mark {
    font-size: 28px;
    color: #9e7d38;
  }
  h1 {
    font-weight: 500;
    font-size: 30px;
    letter-spacing: 0.04em;
  }
  .sub {
    color: #8a8276;
    font-style: italic;
  }
  .bar {
    width: min(340px, 70vw);
    height: 5px;
    margin-top: 18px;
    border-radius: 3px;
    background: #ece5d8;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: #9e7d38;
    border-radius: 3px;
    transition: width 0.15s ease;
  }
  .fill.indeterminate {
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 0.45;
    }
    50% {
      opacity: 1;
    }
  }
  .detail {
    font-size: 13px;
    color: #8a8276;
  }
  .error {
    color: #b04a3a;
    max-width: 40em;
    text-align: center;
  }
  button {
    margin-top: 8px;
    padding: 6px 18px;
    border: 1px solid #9e7d38;
    border-radius: 6px;
    color: #9e7d38;
  }
</style>
