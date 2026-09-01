<script lang="ts">
  // SHARE, as a destination — the evangelism role's home. What was a header
  // QR dialog plus three church fields buried at the bottom of Settings is one
  // screen: the QR and link that hand the app over, and the church that a
  // shared link carries — set where its effect is visible, not in Settings.
  // The Android twin is ui/ShareScreen.kt.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import QrCode from "./QrCode.svelte";
  import {
    churchFromQuery,
    PRESET_CAP,
    churchTitle,
    cleanChurch,
    hasChurch,
    REF_SHAPE,
    sharedAtRef,
    sharedDevotional,
    sharedLang,
    sharedThread,
    visitChurch,
  } from "./church";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  // What we actually hand over: whatever the palette below has been set to,
  // which with an untouched palette is exactly the plain app link plus this
  // reader's church. ONE derived link — the code, the button and the readout
  // cannot show three different things.
  const link = $derived(s.customShareLink);

  // What this link may carry, for the language it is aimed at. Re-asked when
  // that language changes, because that is when the answers change.
  const options = $derived(s.shareOptions());
  const draft = $derived(s.shareDraft);
  const langRows = $derived((options?.languages ?? []) as { code: string; endonym: string; exonym: string }[]);
  const threadRows = $derived((options?.threads ?? []) as Opt[]);
  const devotionalRows = $derived((options?.devotionals ?? []) as Opt[]);
  interface Opt {
    id: string;
    label: string;
    available: boolean;
  }

  /** A language's name IN THE SENDER'S language — "Punjabi" for an English
   *  reader, "Arabisch" for a German one.
   *
   *  NOT the endonym, which is what Settings shows: there the reader is picking
   *  their OWN language and is looking for the word they would recognise
   *  ("Deutsch"). Here they are naming someone ELSE's language while reading
   *  their own, so their own word for it is the one that helps. Falls back to
   *  the endonym if a catalogue has no name for it. */
  function langName(l: { code: string; endonym: string }): string {
    const mine = t(`lang.${l.code}`);
    return !mine || mine === `lang.${l.code}` ? l.endonym : mine;
  }

  /** A verse is only offered once it LOOKS like one, and the field says so
   *  rather than silently dropping it from the link. */
  const verseBad = $derived(draft.target === "verse" && draft.at.trim() !== "" && !REF_SHAPE.test(draft.at.trim()));

  /** Whether the engine has answered for the CURRENT language yet.
   *
   *  Load-bearing, not cosmetic. `shareOptions` is a read-through query, so the
   *  frame after the language changes has no answer for the new one — and an
   *  empty answer is indistinguishable from "nothing is available" unless this is
   *  asked first. Without it, switching the link to Arabic greys out Thread,
   *  which is available in every language, and the effect below then resets a
   *  destination the reader had deliberately chosen. Not-yet-known is not the
   *  same as not-there. */
  const loaded = $derived(options?.lang === (draft.lang || options?.lang));

  /** Whether a destination has anything to offer in the chosen language. A
   *  thread is refs, so every corpus resolves it; a booklet has to have been
   *  written. Both are asked of the ENGINE rather than assumed, so a stock set
   *  that grows or a booklet that gets translated needs no change here.
   *  Optimistic while loading, so nothing flickers into "Coming soon". */
  const threadsOpen = $derived(!loaded || threadRows.some((th) => th.available));
  const devotionalsOpen = $derived(!loaded || devotionalRows.some((d) => d.available));

  /** The first thing a destination can actually offer, for defaulting into. */
  const firstOpen = (rows: Opt[]): string => rows.find((r) => r.available)?.id ?? "";

  // Keeping the draft coherent with what the chosen language actually has.
  //
  // Two directions, and both matter. Choosing a destination must FILL it — a
  // palette showing "Destination: Thread" with an empty thread box is asking the
  // reader to answer a question it could have answered itself, and "none" is not
  // one of the answers. And a language change must EMPTY what it strands: pick
  // the English booklet, aim the link at Arabic, and the booklet it names is not
  // written there. Left alone, the link would quietly carry a destination the
  // recipient cannot reach, which is the one thing this palette exists to prevent.
  $effect(() => {
    // Never on a stale or absent answer: correcting the draft against the
    // PREVIOUS language's availability is how a deliberate choice gets silently
    // undone mid-switch.
    if (!loaded) return;
    if (draft.target === "thread" && !threadRows.some((th) => th.id === draft.thread && th.available)) {
      draft.thread = firstOpen(threadRows);
    }
    if (draft.target === "devotional" && !devotionalRows.some((d) => d.id === draft.devotional && d.available)) {
      draft.devotional = firstOpen(devotionalRows);
    }
    // A destination with nothing left in it falls back to the plain app link
    // rather than sitting on a choice the language cannot honour.
    if (draft.target === "thread" && threadRows.length && !threadsOpen) draft.target = "app";
    if (draft.target === "devotional" && devotionalRows.length && !devotionalsOpen) draft.target = "app";
  });

  /** The readout: what the recipient actually gets, in plain rows rather than a
   *  composed sentence. Rows because this is nine languages — a sentence with
   *  three things slotted into it reads as machine translation in most of them,
   *  and a label beside a value reads the same everywhere.
   *
   *  Read back OUT OF THE BUILT LINK rather than off the draft, so the two cannot
   *  disagree. The draft can hold things the link does not carry — a devotional
   *  picked and then stranded by a language change, a verse still half typed —
   *  and a readout describing the draft would promise a destination the recipient
   *  is not going to get. This describes the URL behind the QR, which is the only
   *  thing actually being handed over. */
  const summary = $derived.by(() => {
    const q = new URL(link).search;
    const thread = sharedThread(q);
    const devotional = sharedDevotional(q);
    const opensAs =
      thread ??
      (devotional ? (devotionalRows.find((d) => d.id === devotional)?.label ?? devotional) : null) ??
      sharedAtRef(q) ??
      t("share.opensApp");
    const rows: [string, string][] = [[t("share.opens"), opensAs]];
    const linkLang = sharedLang(q);
    const chosenLang = linkLang ? langRows.find((l) => l.code === linkLang) : null;
    rows.push([t("share.languageLabel"), chosenLang ? langName(chosenLang) : t("share.languageDevice")]);
    const church = churchFromQuery(q);
    if (church) rows.push([t("settings.church"), church.name]);
    return rows;
  });
  async function shareLink(): Promise<void> {
    const title = hasChurch(s.church) ? t("share.fromChurch", { church: s.church.name }) : "Plumbline";
    if (navigator.share) {
      try {
        await navigator.share({ title, url: link });
        return;
      } catch (e) {
        // A dismissed sheet is an answer, not a failure — falling through would
        // overwrite the reader's clipboard for a share they just cancelled (and
        // writeText throws anyway: the closing sheet still holds the focus).
        // Every other rejection still gets the fallback. (ContextMenu's rule.)
        if ((e as DOMException | undefined)?.name === "AbortError") return;
      }
    }
    try {
      await navigator.clipboard.writeText(link);
      s.showToast(t("share.copied"));
    } catch {
      s.showToast(t("settings.copyBlocked"));
    }
  }

  /** Save the palette's current state under a name the reader gives.
   *
   *  Named rather than auto-labelled: "Romans Road · ਪੰਜਾਬੀ" is what the settings
   *  ARE, and the reader wants what the settings are FOR ("Tuesday outreach").
   *  Re-using an existing name overwrites that preset, which is how you amend
   *  one without deleting it first. */
  async function savePreset(): Promise<void> {
    const name = (await s.askText(t("share.presetName")))?.trim();
    if (!name) return;
    // Only a NEW name can run out of room: re-using one overwrites in place, and
    // refusing that would strand a reader at the cap with no way to amend.
    const isNew = !s.sharePresets.some((p) => p.name === name);
    if (isNew && s.sharePresets.length >= PRESET_CAP) {
      s.showToast(t("share.presetFull"));
      return;
    }
    await s.savePreset(name);
    s.showToast(t("share.presetSaved"));
  }

  /** Delete a preset, behind the shared confirmation every other destructive
   *  action in the app uses — a preset is a thing the reader made. */
  async function removePreset(name: string): Promise<void> {
    const ok = await s.askConfirm(t("share.presetDelete"), t("share.presetDeleteBody", { name }));
    if (ok) await s.deletePreset(name);
  }

  /** Copy without the share sheet — the palette's own button. Building a link is
   *  a deliberate act, and the reader usually wants it on the clipboard to paste
   *  somewhere specific rather than handed to an OS picker. */
  async function copyLink(): Promise<void> {
    try {
      await navigator.clipboard.writeText(link);
      s.showToast(t("share.copied"));
    } catch {
      s.showToast(t("settings.copyBlocked"));
    }
  }

  // The reader's home church. Loaded once per visit to the screen; every field
  // saves on change, exactly as Settings did.
  let churchName = $state("");
  let churchUrl = $state("");
  let churchLoaded = false;
  $effect(() => {
    if (s.screen !== "share" || churchLoaded) return;
    churchLoaded = true;
    const c = s.church;
    churchName = c.name;
    churchUrl = c.url;
  });
  function saveChurch(): void {
    s.setChurch(cleanChurch({ name: churchName, service: s.config.sundayService ?? null, url: churchUrl }));
  }

  /** THE SAME VALUE Settings edits — `config.sundayService`, not a second copy
   *  (maintainer, 2026-08-26). The reader had already given their service time
   *  there and was being asked to type it again into a free-text line the share
   *  link then carried as prose. It is one number now: Settings and this field
   *  write it, the Sunday bookmark reads it, and the link carries it as minutes
   *  so the recipient's app writes the time their own way. */
  function serviceTimeValue(): string {
    const m = s.config.sundayService;
    if (typeof m !== "number") return "";
    return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
  }
  function setServiceTime(e: Event): void {
    const v = (e.currentTarget as HTMLInputElement).value;
    if (!v) {
      s.config.sundayService = undefined;
    } else {
      const [h, m] = v.split(":").map(Number);
      s.config.sundayService = h * 60 + m;
    }
    s.saveConfig();
    saveChurch(); // the church carries the time into the link
  }
</script>

<section class="screen" aria-label={t("nav.share")}>
  <ScreenBar title={t("nav.share")} onBack={() => s.goRead()} onMenu={() => (s.menuOpen = true)} />
  <div class="content">
    <div class="card qr-card" data-surface="share app">
      <h3>{t("share.title")}</h3>
      <p class="sub">{hasChurch(s.church) ? t("share.subChurch") : t("share.sub")}</p>
      <QrCode size={220} text={link} />
      <p class="sub">plumblinebible.org</p>
      {#if hasChurch(s.church)}
        <p class="with">{t("share.with", { church: s.church.name })}</p>
      {/if}
      <!-- What the code above hands over. Rows, not a composed sentence: three
           values slotted into one sentence reads as machine translation in most
           of the nine. It belongs HERE rather than under the palette, where it
           would only restate the controls the reader is already looking at. -->
      <div class="preview">
        <p class="preview-title">{t("share.preview")}</p>
        {#each summary as [label, value] (label)}
          <p class="preview-row"><span class="k">{label}</span><span class="v">{value}</span></p>
        {/each}
      </div>
      <button class="primary" onclick={shareLink}>{t("share.action")}</button>
    </div>
    <!-- Share is the app AND the Gospel (maintainer direction, 2026-08-11):
         the same Present that Preach raises, opened straight onto the Romans
         Road — the first-run "Sharing the gospel" path, now living where the
         sharing happens. -->
    <div class="card" data-surface="share gospel">
      <h3>{t("share.gospel")}</h3>
      <p class="desc">{t("share.gospelDesc")}</p>
      <button
        class="visit"
        onclick={() => {
          s.presentThreadName = s.gospelThread();
          s.showPresent = true;
        }}
      >
        {t("share.gospelGo")}
      </button>
    </div>
    <!-- The palette: one link, built from what the recipient will actually get.
         Every control here only offers what EXISTS in the chosen language — the
         engine answers that per language (`shareOptions`), and what is not
         written yet is shown as coming soon rather than hidden, so a sender
         learns it is on the way instead of wondering whether they mis-tapped. -->
    <div class="card" data-surface="share custom">
      <h3>{t("share.custom")}</h3>
      <p class="desc">{t("share.customDesc")}</p>

      <!-- Presets first: this is the one-click path, and the controls below are
           what you reach for when no saved one fits. Applying a preset does not
           save anything — the palette still opens on the plain app link every
           time, and a preset is only ever loaded by tapping it. -->
      {#if s.sharePresets.length > 0}
        <div class="presets" aria-label={t("share.presets")}>
          {#each s.sharePresets as p (p.name)}
            <span class="chip">
              <button class="chip-use" onclick={() => s.applyPreset(p)}>{p.name}</button>
              <button
                class="chip-del"
                aria-label={t("share.presetDelete")}
                title={t("share.presetDelete")}
                onclick={() => removePreset(p.name)}>×</button
              >
            </span>
          {/each}
        </div>
      {/if}

      <!-- One grid for the whole form: `.row` is `display: contents`, so every
           label lands in the same column and every control in the other. That is
           what makes the boxes line up AND come out the same width — a per-row
           flex sizes each control to whatever its own label left over, which is
           different in every row and different again in every language. -->
      <div class="fields">
        <label class="row">
          <span>{t("share.opens")}</span>
          <select bind:value={draft.target}>
            <option value="app">{t("share.opensApp")}</option>
            <option value="thread" disabled={!threadsOpen}>
              {t("share.opensThread")}{threadsOpen ? "" : ` — ${t("share.comingSoon")}`}
            </option>
            <!-- Disabled where the language has no booklet, rather than offering
                 the destination and then an empty list behind it. Coming soon is
                 said HERE, on the choice itself, which is where the reader is
                 deciding. -->
            <option value="devotional" disabled={!devotionalsOpen}>
              {t("share.opensDevotional")}{devotionalsOpen ? "" : ` — ${t("share.comingSoon")}`}
            </option>
            <option value="verse">{t("share.opensVerse")}</option>
          </select>
        </label>

        {#if draft.target === "thread"}
          <label class="row">
            <span>{t("share.threadLabel")}</span>
            <select bind:value={draft.thread}>
              {#each threadRows as th (th.id)}
                <option value={th.id} disabled={!th.available}>
                  {th.label}{th.available ? "" : ` — ${t("share.comingSoon")}`}
                </option>
              {/each}
            </select>
          </label>
        {:else if draft.target === "devotional"}
          <label class="row">
            <span>{t("share.devotionalLabel")}</span>
            <select bind:value={draft.devotional}>
              {#each devotionalRows as d (d.id)}
                <option value={d.id} disabled={!d.available}>
                  {d.label}{d.available ? "" : ` — ${t("share.comingSoon")}`}
                </option>
              {/each}
            </select>
          </label>
        {:else if draft.target === "verse"}
          <label class="row">
            <span>{t("share.verseLabel")}</span>
            <input class="field" placeholder={t("share.versePlaceholder")} bind:value={draft.at} />
          </label>
          {#if verseBad}
            <p class="soon">{t("share.verseInvalid")}</p>
          {/if}
        {/if}

        <!-- The language the RECIPIENT reads in, which is the whole point: a
             sender reading English can hand over a Punjabi Bible. -->
        <label class="row">
          <span>{t("share.languageLabel")}</span>
          <select bind:value={draft.lang}>
            <option value="">{t("share.languageDevice")}</option>
            {#each langRows as l (l.code)}
              <option value={l.code}>{langName(l)}</option>
            {/each}
          </select>
        </label>

          {#if hasChurch(s.church)}
            <label class="check">
              <input type="checkbox" bind:checked={draft.church} />
              <span>{t("share.includeChurch")}</span>
            </label>
          {/if}
      </div>

      <div class="acts">
        <button class="visit" onclick={savePreset}>{t("share.savePreset")}</button>
        <button class="visit" onclick={copyLink}>{t("share.copyLink")}</button>
      </div>
    </div>
    <div class="card">
      <h3>{t("settings.church")}</h3>
      <p class="desc">{t("settings.churchDesc")}</p>
      <input class="field" placeholder={t("settings.churchName")} bind:value={churchName} onchange={saveChurch} />
      <label class="svc">
        <span>{t("settings.churchService")}</span>
        <input type="time" value={serviceTimeValue()} onchange={setServiceTime} />
      </label>
      <input class="field" placeholder={t("settings.churchUrl")} bind:value={churchUrl} onchange={saveChurch} />
      {#if hasChurch(s.church)}
        <!-- The recipient's path to the congregation a shared link named —
             this button was the header's Church chip before Share was a role. -->
        <button
          class="visit"
          title={churchTitle(s.church, t("shell.churchFallback"), s.churchMeets(s.church))}
          onclick={() => visitChurch(s.church, s.showToast, t("shell.churchFallback"))}
        >
          {t("shell.church")}
        </button>
      {/if}
    </div>
  </div>
</section>

<style>
  /* The service time reads as a labelled control, not a third text box: it is
     the one field here that is a number rather than something typed. */
  .svc {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 8px 0;
    color: var(--ink, #211f1a);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .svc input {
    font: inherit;
    color: var(--ink, #211f1a);
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 6px 8px;
  }
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
    display: grid;
    gap: 12px;
    grid-template-columns: repeat(auto-fit, minmax(280px, 380px));
    align-content: start;
    justify-content: center;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 18px 20px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 10px;
    background: var(--popupPaper, #f2eee6);
  }
  /* The QR needs its white field whatever the theme, as the dialog always had. */
  .qr-card {
    align-items: center;
    background: #ffffff;
    color: #101010;
  }
  h3 {
    margin: 0;
    font-size: calc(17px * var(--uiScale, 1));
    font-weight: 600;
  }
  .card:not(.qr-card) h3 {
    color: var(--ink, #211f1a);
  }
  .sub {
    margin: 0;
    color: #5a564e;
    font-size: calc(13px * var(--uiScale, 1));
  }
  .with {
    margin: 0;
    font-size: calc(13px * var(--uiScale, 1));
    font-weight: 600;
    color: #9e7d38;
  }
  .desc {
    margin: 0;
    font-size: calc(13.5px * var(--uiScale, 1));
    line-height: 1.4;
    color: var(--faded, #8a8276);
  }
  .primary {
    margin-top: 6px;
    padding: 6px 16px;
    border: 1px solid #9e7d38;
    border-radius: 6px;
    background: #9e7d38;
    color: #ffffff;
  }
  .visit {
    align-self: flex-start;
    margin-top: 4px;
    padding: 5px 14px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    color: var(--gold, #9e7d38);
  }
  .visit:hover {
    border-color: var(--gold, #9e7d38);
  }
  /* The palette's controls, as ONE grid rather than a stack of independent rows.
     `.row` is `display: contents`, so its label and its control become children
     of this grid: every label lands in the first column, sized to the widest of
     them, and every control in the second. That is what makes them line up and
     come out the same width. Per-row flex cannot — each control would take
     whatever its own label left over, which differs by row and again by
     language ("Language" / "Sprache" / "لغة الجهاز"). */
  .fields {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: 10px;
  }
  .row {
    display: contents;
    color: var(--ink, #211f1a);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .row > span {
    color: var(--ink, #211f1a);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .row select,
  .row input {
    min-width: 0;
    width: 100%;
    font: inherit;
    color: var(--ink, #211f1a);
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    padding: 6px 8px;
  }
  /* Full width: these are not label/control pairs, so they take the whole row
     rather than being squeezed into the control column. */
  .fields .soon,
  .check {
    grid-column: 1 / -1;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--ink, #211f1a);
    font-size: calc(14px * var(--uiScale, 1));
  }
  /* Coming-soon and malformed-verse notes: the same quiet register, because both
     say "not this, yet" rather than "you did something wrong". */
  .soon {
    margin: 0;
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
  }
  /* Saved presets: a wrapping row of chips, each a load button with its own
     delete. Two buttons rather than one with a swipe or a long-press, because
     this row is read on a desktop as often as a phone. */
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 2px;
  }
  .chip {
    display: inline-flex;
    align-items: stretch;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 999px;
    overflow: hidden;
    background: var(--paper, #fcf9f4);
  }
  .chip-use {
    padding: 4px 10px;
    color: var(--gold, #9e7d38);
    font-size: calc(13px * var(--uiScale, 1));
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip-use:hover {
    background: var(--popupPaper, #f2eee6);
  }
  /* The × is the destructive half, so it is visually quieter than the name and
     separated from it — a mis-tap here deletes something the reader made. */
  .chip-del {
    padding: 4px 8px;
    border-inline-start: 1px solid var(--rule, #d8cba8);
    color: var(--faded, #8a8276);
    font-size: calc(13px * var(--uiScale, 1));
    line-height: 1;
  }
  .chip-del:hover {
    color: var(--ink, #211f1a);
    background: var(--popupPaper, #f2eee6);
  }
  /* Right-aligned, like every other card's actions: a bunch in the left corner
     reads as leftovers rather than as the things you press when you are done. */
  .acts {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 2px;
  }
  .acts .visit {
    margin-top: 0;
  }

  /* Inside the QR card, which keeps its white field in every theme — so these
     are fixed tones, not palette roles. A `var(--paper)` here would paint a dark
     box inside a white card the moment the reader is on a dark theme.
     `align-self: stretch` because the card centres its children. */
  .preview {
    align-self: stretch;
    margin-top: 4px;
    padding: 10px 12px;
    border: 1px solid #e4ddcb;
    border-radius: 8px;
    background: #faf7f1;
  }
  .preview-title {
    margin: 0 0 6px;
    font-size: calc(12px * var(--uiScale, 1));
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: #7a746a;
  }
  .preview-row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin: 0;
    padding: 2px 0;
    font-size: calc(13.5px * var(--uiScale, 1));
  }
  .preview-row .k {
    color: #7a746a;
  }
  .preview-row .v {
    color: #101010;
    text-align: end;
  }
  .field {
    padding: 7px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    font-size: calc(14.5px * var(--uiScale, 1));
  }
</style>
