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
      const s = initSession(result);
      // Warm the corpus-derived analytics DURING the splash: the engine runs
      // on the main thread, so warming after first paint froze the UI
      // (menus wouldn't open). Behind the splash the block is invisible —
      // and it's the cheap warm now: the heavy machine-tier artifacts are not
      // in the boot pack at all (TODO #28), so this builds only the
      // concept/leitwort indexes over the corpus.
      if (s.gates & 2) {
        phase = { phase: "warm" };
        await new Promise((r) => requestAnimationFrame(() => setTimeout(r, 30)));
        s.engine.warmIndexes();
      }
      session = s;
      // The deferred machine-tier pack: fetch + load once the reader has been
      // idle a moment. The trailing engine block is synchronous, so this
      // waits out the first interactions rather than competing with them.
      // First-run visitors are still choosing their tiers — FirstRun's start()
      // triggers the load for them if they keep the machine tier on.
      if (s.gates & 2 && !s.showFirstRun) {
        const idle: (cb: () => void) => unknown =
          "requestIdleCallback" in window ? (cb) => requestIdleCallback(cb, { timeout: 8000 }) : (cb) => setTimeout(cb, 250);
        setTimeout(() => idle(() => void s.ensureRnd()), 2500);
      }
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
        : phase.phase === "warm"
          ? "Building the analytics…"
          : "Opening the text…",
  );
</script>

{#if session}
  <Shell />
{:else}
  <div class="splash">
    <div class="mark">✦</div>
    <h1>Plumbline</h1>
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
