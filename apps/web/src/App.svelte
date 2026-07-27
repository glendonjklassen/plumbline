<script lang="ts">
  // Boot (TODO #28): the ENGINE WORKER does everything — pack fetch, home,
  // wasm, open, warm, and later the deferred R&D pack — while this thread
  // paints the splash from its progress messages. Fonts load here too, for
  // PAINTING; the worker loads its own copy for layout measurement.
  //
  // The splash is a SPLASH. A previous build painted last session's chapter
  // here as a live mini-reader; it looked like the app but answered nothing,
  // and reading text you can't touch reads as broken, not fast (feedback
  // 2026-07-26). Honest progress beats a decoy — the work now goes into
  // making the wait short rather than disguising it.
  import { EngineRpc, type WorkerProgress } from "./engine/worker-client";
  import { precacheShell } from "./engine/precache";
  import { churchFromQuery, hasChurch, sharedAtRef, startsAsNewBeliever } from "./shell/church";
  import { initSession, type Session } from "./state/session.svelte";
  import { dispatchLink } from "./study/links";
  import FirstRun from "./shell/FirstRun.svelte";
  import Shell from "./shell/Shell.svelte";

  let phase = $state<WorkerProgress>({ phase: "download", fraction: 0 });
  let error = $state<string | null>(null);
  let session = $state<Session | null>(null);

  async function start(): Promise<void> {
    try {
      const rpc = new EngineRpc();
      rpc.onProgress = (p) => (phase = p);
      // Phones defer the machine-tier auto-download (2026-07-26): the shell
      // offers an explicit "load analysis" action instead of spending ~4 MB
      // and worker time behind the reader's back.
      const deferRnd = matchMedia("(max-width: 700px)").matches;
      const [info] = await Promise.all([
        rpc.boot({ deferRnd }),
        document.fonts.load('18px "EB Garamond"'),
        document.fonts.load('italic 18px "EB Garamond"'),
        document.fonts.load('bold 18px "EB Garamond"'),
      ]);
      // Prime what synchronous readers need on their first frame: the theme
      // palettes, highlight tones, and the TOC/canon shape.
      const [light, dark, night, tones] = await Promise.all([
        rpc.static("themePalette", "light"),
        rpc.static("themePalette", "dark"),
        rpc.static("themePalette", "night"),
        rpc.static("highlightTones"),
      ]);
      const s = initSession(rpc, info, { light, dark, night }, info.bundledOn);
      // A shared link can carry the sender's church. Save it as this reader's
      // own (theirs wins if they've already set one), then strip it from the
      // address bar so a bookmark or a reload isn't a link about a church.
      const shared = churchFromQuery(location.search);
      s.startAsNewBeliever = startsAsNewBeliever(location.search);
      if (shared && !s.config.church?.name) {
        s.config.church = shared;
        s.sharedByChurch = shared;
        s.saveConfig();
      } else if (shared) {
        s.sharedByChurch = shared; // shown in the welcome, not saved over theirs
      }
      // A shared PASSAGE opens where it points (`?at=Ps 23:1`) — the QR on the
      // Present end card hands over the weave, not just the app (2026-07-27).
      const at = sharedAtRef(location.search);
      if (shared || s.startAsNewBeliever || at) {
        history.replaceState(null, "", location.pathname + location.hash);
      }
      // Returning readers never see the welcome, so without this a link's
      // church would be saved with no sign it happened (feedback 2026-07-27).
      if (hasChurch(shared) && !s.showFirstRun) {
        s.showToast(`Home church set to ${shared.name} — tap Church to visit them`);
      }
      s.rndDeferred = deferRnd;
      s.tones = tones?.tones ?? [];
      await Promise.all([s.fetchQ("toc"), s.fetchQ("canonSegments")]);
      session = s;
      // After the TOC is in, so navigation clamps against a real canon.
      if (at) void dispatchLink(s, `go:${at.replace(" ", ":")}`);
      // The on-device boot numbers (also under Settings → boot diagnostics).
      void rpc.bootTrace().then((t) => {
        s.bootTrace = t;
        console.table(t.map(([stage, ms]) => ({ stage, ms })));
      });
      // Idle work: make this visit enough to run offline next time.
      const idle = globalThis.requestIdleCallback ?? ((f: () => void) => setTimeout(f, 1200));
      idle(() => void precacheShell());
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
  {#if session.showFirstRun}
    <!-- First launch: the welcome owns the screen, straight off the loader —
         the reader mounts only after a path is chosen (no John 3 flashing
         under a question, feedback 2026-07-26). -->
    <div class="firstrun-stage">
      <FirstRun />
    </div>
  {:else}
    <Shell />
    <!-- Also mounted over the app: the welcome is re-openable from the top
         bar, and it renders as its own dialog (feedback 2026-07-27). -->
    <FirstRun />
  {/if}
{:else}
  <div class="splash">
    <div class="mark">✦</div>
    <h1>Plumbline</h1>
    <p class="sub">The Holy Bible</p>
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
  .firstrun-stage {
    position: fixed;
    inset: 0;
    background: var(--paper, #fcf9f4);
  }
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
