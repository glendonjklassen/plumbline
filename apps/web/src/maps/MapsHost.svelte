<script lang="ts">
  // Renders whichever analytical map popup is open.
  import ChordMap from "./ChordMap.svelte";
  import ConceptMap from "./ConceptMap.svelte";
  import Constellation from "./Constellation.svelte";
  import { getSession } from "../state/session.svelte";

  const s = getSession();
</script>

{#if s.mapPopup?.kind === "chord"}
  <ChordMap />
{:else if s.mapPopup?.kind === "constellation"}
  <Constellation />
{:else if s.mapPopup?.kind === "conceptMap"}
  <ConceptMap code={s.mapPopup.code} />
{/if}
