<script lang="ts">
  // Renders whichever analytical map popup is open.
  //
  // There were three. The concept map — the radial embedding neighbourhood
  // behind `conceptmap:CODE` — was removed 2026-07-30 along with the concept
  // vectors it drew from; the two that remain are weave visualisations and are
  // not embedding-derived.
  import ChordMap from "./ChordMap.svelte";
  import Constellation from "./Constellation.svelte";
  import { getSession } from "../state/session.svelte";

  const s = getSession();
</script>

{#if s.mapPopup?.kind === "chord"}
  <ChordMap />
{:else if s.mapPopup?.kind === "constellation"}
  <Constellation />
{/if}
