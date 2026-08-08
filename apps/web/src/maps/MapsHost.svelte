<script lang="ts">
  // Renders whichever analytical map popup is open. The two are weave
  // visualisations; neither is embedding-derived.
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
