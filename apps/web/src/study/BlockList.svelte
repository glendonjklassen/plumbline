<script lang="ts">
  // The per-block painter (manifest P0.1): walks the typed block list from the
  // core producer and renders Section / Para / Rule. It derives nothing — the
  // producer owns tier order, caps, colors (as palette roles), and link URIs.
  interface Run {
    text: string;
    size: number;
    color: string;
    bold: boolean;
    italic: boolean;
    uri?: string;
  }
  interface Block {
    kind: "section" | "para" | "rule";
    title?: string;
    markGlyph?: string;
    markColor?: string;
    runs?: Run[];
    indent?: boolean;
    topGap?: boolean;
  }
  interface Props {
    blocks: Block[];
    onLink: (uri: string, ev: MouseEvent) => void;
  }
  let { blocks, onLink }: Props = $props();

  const color = (role: string | undefined) => `var(--${role ?? "ink"}, var(--ink, #211f1a))`;
</script>

<div class="blocks">
  {#each blocks as b, i (i)}
    {#if b.kind === "rule"}
      <hr />
    {:else if b.kind === "section"}
      <h3>
        <span class="title">{b.title}</span>
        {#if b.markGlyph}
          <span class="mark" style:color={color(b.markColor)}>{b.markGlyph}</span>
        {/if}
      </h3>
    {:else if b.kind === "para"}
      <p class:indent={b.indent} class:gap={b.topGap}>
        {#each b.runs ?? [] as r, j (j)}
          {#if r.uri}
            <button
              class="link"
              style:font-size="calc({r.size}px * var(--uiScale, 1))"
              style:color={color(r.color)}
              style:font-weight={r.bold ? 600 : 400}
              style:font-style={r.italic ? "italic" : "normal"}
              onclick={(e) => onLink(r.uri!, e)}>{r.text}</button
            >
          {:else}
            <span
              style:font-size="calc({r.size}px * var(--uiScale, 1))"
              style:color={color(r.color)}
              style:font-weight={r.bold ? 600 : 400}
              style:font-style={r.italic ? "italic" : "normal"}>{r.text}</span
            >
          {/if}
        {/each}
      </p>
    {/if}
  {/each}
</div>

<style>
  .blocks {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-wrap: break-word;
  }
  hr {
    border: none;
    border-top: 1px solid var(--rule, #d8cba8);
    margin: 10px 0 6px;
  }
  h3 {
    font-size: calc(12px * var(--uiScale, 1));
    font-weight: 600;
    letter-spacing: 0.09em;
    color: var(--section, #a0894a);
    margin: 12px 0 2px;
  }
  h3 .title {
    font-variant: small-caps;
    text-transform: lowercase;
  }
  h3 .mark {
    margin-left: 6px;
    font-size: calc(12px * var(--uiScale, 1));
  }
  p {
    margin: 2px 0;
    line-height: 1.45;
  }
  p.indent {
    padding-left: 14px;
  }
  p.gap {
    margin-top: 10px;
  }
  .link {
    display: inline;
    text-align: left;
    padding: 0;
    text-decoration: none;
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
  }
</style>
