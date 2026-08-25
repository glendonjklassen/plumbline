<script lang="ts">
  // TAGS — a page of its own under Study, the Android twin being the second
  // MapOverlay in ui/StudyScreen.kt's ExploreScreen.
  //
  // BROWSING IS THE PAGE (maintainer, 2026-08-24): the tags themselves render
  // right here, grouped under their category headings, and tapping one opens
  // its detail card. The organization actions — rename, categorize, merge —
  // are a row of buttons AFTER the list: they are the library's housekeeping,
  // not its front door. (It opened as a card menu whose first card was
  // "Browse tags"; browsing being the basic act, the extra tap was the menu.)
  //
  // Rename and Merge are the two operations a tag collection actually
  // accumulates a need for: names drift ("grace", "Grace", "God's grace") and
  // end up wanting to be one tag.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import { plural, t } from "../lib/i18n.svelte";

  const s = getSession();

  const tagObjs = $derived((s.q("tags")?.tags ?? []) as any[]);
  const tags = $derived(tagObjs.map((x) => String(x.name)));

  /** The list, grouped exactly the way the core groups the library panel
   *  (`panel::tags_list`): headings the moment ANY tag has a category, dead
   *  flat until then; categories alphabetical; the uncategorized bring up the
   *  rear under "No category". `index` is the tag's position in the wire's
   *  own order — the ordinal `{kind:"tag"}` aims at — so grouping can never
   *  re-aim a tap. */
  const grouped = $derived.by(() => {
    const rows = tagObjs.map((x, index) => ({
      name: String(x.name),
      members: ((x.members ?? []) as any[]).length,
      category: String(x.category ?? "").trim(),
      index,
    }));
    if (!rows.some((r) => r.category)) return [{ heading: null as string | null, rows }];
    const cats = [...new Set(rows.map((r) => r.category).filter(Boolean))].sort((a, b) =>
      a.toLowerCase().localeCompare(b.toLowerCase()),
    );
    const groups = cats.map((c) => ({ heading: c as string | null, rows: rows.filter((r) => r.category === c) }));
    const un = rows.filter((r) => !r.category);
    if (un.length) groups.push({ heading: t("tags.uncategorized"), rows: un });
    return groups;
  });

  const close = (): void => {
    s.screen = "explore";
  };

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
   *  2026-08-18: "no need when you're going through the bible").
   *
   *  The picker idiom, not a bare prompt: the categories that exist are a list
   *  you tap, freetext only for a genuinely NEW one — retyping "Doctrine" for
   *  every tag filed under it is how a typo quietly forks a second heading.
   *  With no categories yet there is nothing to pick, so it goes straight to
   *  the prompt, which is where the reader ADDS their first one. */
  async function categorize(): Promise<void> {
    const name = await s.askPick(t("tags.categorizeWhich"), tags);
    if (!name) return;
    const current = String(tagObjs.find((x) => x.name === name)?.category ?? "");
    const existing = [...new Set(tagObjs.map((x) => String(x.category ?? "").trim()).filter(Boolean))].sort((a, b) =>
      a.toLowerCase().localeCompare(b.toLowerCase()),
    );
    let cat: string | null;
    if (existing.length > 0) {
      const newLabel = t("tags.categoryNew");
      const noneLabel = t("tags.uncategorized");
      const picked = await s.askPick(t("tags.categoryFor", { name }), [...existing, newLabel, noneLabel]);
      if (picked === null) return;
      if (picked === noneLabel) cat = "";
      else if (picked === newLabel) cat = await s.askText(t("tags.categoryFor", { name }), current);
      else cat = picked;
    } else {
      cat = await s.askText(t("tags.categoryFor", { name }), current);
    }
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
    {#if tagObjs.length === 0}
      <p class="empty">{t("tags.empty")}</p>
    {:else}
      <div class="list">
        {#each grouped as g (g.heading ?? "")}
          {#if g.heading !== null}
            <p class="ghead">{g.heading}</p>
          {/if}
          {#each g.rows as r (r.index)}
            <button class="tag-row" onclick={() => (s.panel = { kind: "tag", index: r.index })}>
              <span class="tname">{r.name}</span>
              <span class="tcount">{plural("panel.members.one", "panel.members.other", r.members)}</span>
            </button>
          {/each}
        {/each}
      </div>
    {/if}
    <!-- An action with nothing to act on is DISABLED rather than hidden: a menu
         whose items appear as you acquire data is a menu you cannot learn. The
         reason it is off rides the tooltip. -->
    <div class="org">
      {#each actions as a (a.id)}
        <button
          class="org-btn"
          disabled={tags.length < a.need}
          title={tags.length < a.need ? t(`tags.${a.id}.needs`) : t(`tags.${a.id}.desc`)}
          onclick={a.go}
        >
          {t(`tags.${a.id}`)}
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
  .empty {
    margin: 0;
    font-size: calc(14.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .list {
    display: flex;
    flex-direction: column;
    align-content: start;
  }
  .ghead {
    margin: 10px 0 2px;
    font-size: calc(12px * var(--uiScale, 1));
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--sectionGold, #a0894a);
  }
  .tag-row {
    display: flex;
    align-items: baseline;
    gap: 10px;
    text-align: left;
    padding: 10px 6px;
    border: none;
    border-bottom: 1px solid var(--rule, #d8cba8);
    background: none;
  }
  .tag-row:hover .tname {
    text-decoration: underline;
  }
  .tname {
    font-size: calc(16px * var(--uiScale, 1));
    color: var(--gold, #9e7d38);
    font-weight: 600;
  }
  .tcount {
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  .org {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding-top: 2px;
  }
  .org-btn {
    padding: 8px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    background: var(--popupPaper, #f2eee6);
    color: var(--ink, #211f1a);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .org-btn:hover:not(:disabled) {
    border-color: var(--gold, #9e7d38);
  }
  .org-btn:disabled {
    opacity: 0.55;
  }
</style>
