<script lang="ts">
  // TAGS — a page of its own under Study, the Android twin being the second
  // MapOverlay in ui/StudyScreen.kt's ExploreScreen.
  //
  // The Tags card used to raise the library panel directly. It is a door now,
  // for the same reason Visualizations became one: there is more than one thing
  // to do with a tag library, and a card that can only ever do the first of them
  // has nowhere to put the rest (maintainer, 2026-08-14).
  //
  // Browse is the library panel it always raised. Rename and Merge are the two
  // operations a tag collection actually accumulates a need for: names drift
  // ("grace", "Grace", "God's grace") and end up wanting to be one tag.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  const tagObjs = $derived((s.q("tags")?.tags ?? []) as any[]);
  const tags = $derived(tagObjs.map((x) => String(x.name)));

  const close = (): void => {
    s.screen = "explore";
  };

  function browse(): void {
    s.panel = { kind: "tags" };
  }

  /** Rename keeps the tag's IDENTITY — the core carries the id across, so the
   *  tag on the other side is the same tag and not a new one wearing the name. */
  async function rename(): Promise<void> {
    const from = await s.askPick(t("tags.renameWhich"), tags);
    if (!from) return;
    const to = await s.askText(t("tags.renameTo", { name: from }), from);
    if (to === null || to.trim() === "" || to.trim() === from) return;
    const err = await s.author("tagRename", from, to.trim());
    s.showToast(err ?? t("tags.renamed", { from, to: to.trim() }));
  }

  /** Merge DELETES the source tag, so it asks first and names both sides in the
   *  question — "merge X into Y" is not a symmetrical sentence and the reader
   *  has to be able to tell which one survives. */
  async function merge(): Promise<void> {
    const from = await s.askPick(t("tags.mergeWhich"), tags);
    if (!from) return;
    const into = await s.askPick(t("tags.mergeInto", { name: from }), tags.filter((x) => x !== from));
    if (!into) return;
    const ok = await s.askConfirm(t("tags.mergeAsk", { from, into }), t("tags.mergeBody", { from, into }), t("tags.mergeVerb"));
    if (!ok) return;
    const err = await s.author("tagMerge", from, into);
    s.showToast(err ?? t("tags.merged", { from, into }));
  }

  /** Category = a grouping heading for the tag LISTS (picker + library panel).
   *  Assigned here and only here — never while reading (maintainer UAT,
   *  2026-08-18: "no need when you're going through the bible"). */
  async function categorize(): Promise<void> {
    const name = await s.askPick(t("tags.categorizeWhich"), tags);
    if (!name) return;
    const current = String(tagObjs.find((x) => x.name === name)?.category ?? "");
    const cat = await s.askText(t("tags.categoryFor", { name }), current);
    if (cat === null || cat.trim() === current) return;
    const err = await s.author("tagSetCategory", name, cat.trim());
    s.showToast(
      err ??
        (cat.trim()
          ? t("tags.categorized", { name, category: cat.trim() })
          : t("tags.categoryCleared", { name })),
    );
  }

  const actions = $derived([
    { id: "browse", go: browse, need: 0 },
    { id: "rename", go: rename, need: 1 },
    { id: "categorize", go: categorize, need: 1 },
    // Two tags, or there is nothing to merge INTO.
    { id: "merge", go: merge, need: 2 },
  ]);
</script>

<section class="screen" aria-label={t("explore.tags")}>
  <ScreenBar title={t("explore.tags")} onBack={close} backLabel={t("nav.study")} onMenu={() => (s.menuOpen = true)} />
  <div class="content">
    <p class="lede">{t("explore.tags.desc")}</p>
    <div class="grid">
      {#each actions as a (a.id)}
        <!-- An action with nothing to act on is DISABLED rather than hidden:
             a menu whose items appear as you acquire data is a menu you cannot
             learn. The reason it is off is in the description. -->
        <button class="ex-card" disabled={tags.length < a.need} onclick={a.go}>
          <span class="ex-name">{t(`tags.${a.id}`)}</span>
          <span class="ex-desc">
            {tags.length < a.need ? t(`tags.${a.id}.needs`) : t(`tags.${a.id}.desc`)}
          </span>
        </button>
      {/each}
    </div>
  </div>
</section>

<style>
  .screen {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--paper, #fcf9f4);
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .lede {
    margin: 0;
    font-size: calc(14.5px * var(--uiScale, 1));
    line-height: 1.45;
    color: var(--faded, #8a8276);
  }
  .grid {
    display: grid;
    gap: 10px;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    align-content: start;
  }
  /* The hub's card, to the pixel — this is the same kind of thing one layer
     down, and a different card here would read as a different kind of choice. */
  .ex-card {
    min-height: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: left;
    padding: 16px 18px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--popupPaper, #f2eee6);
  }
  .ex-card:hover:not(:disabled) {
    border-color: var(--gold, #9e7d38);
  }
  .ex-card:disabled {
    opacity: 0.55;
  }
  .ex-name {
    font-size: calc(17px * var(--uiScale, 1));
    font-weight: 600;
    color: var(--ink, #211f1a);
  }
  .ex-desc {
    font-size: calc(14.5px * var(--uiScale, 1));
    line-height: 1.4;
    color: var(--faded, #8a8276);
  }
</style>
