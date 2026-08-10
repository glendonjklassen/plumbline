<script lang="ts">
  // The hymnal, as its own destination — the fifth alongside Read, Explore,
  // Present and Memorize. Two views in one screen: the index, and one hymn.
  //
  // The engine does the work. It hands over stanzas already split into
  // (chord?, text) parts and already transposed, so nothing here parses a
  // bracket or knows that G+3 is Bb. What lives here is what a browser knows and
  // the core does not: which language this reader wants, whether the chords are
  // showing, and how fast the page should scroll while they sing.
  import { getSession } from "../state/session.svelte";
  import { modal } from "../lib/modal";
  import { languages, t } from "../lib/i18n.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";

  const s = getSession();

  const index = $derived(s.q("hymnal")?.hymns ?? []);
  const open = $derived(s.hymn);
  const hymn = $derived(open ? s.q("hymn", open.id, open.semis) : null);

  let filter = $state("");

  /** If a search token names a language this hymnal knows — its code ("de"),
   *  English name ("German") or endonym ("Deutsch") — the code it names, else
   *  null. Empty before the catalogue lands, which simply recognises no
   *  language tokens yet. */
  function langToken(tok: string): string | null {
    for (const l of languages()) {
      if (tok === l.code.toLowerCase() || tok === l.endonym.toLowerCase() || tok === l.name.toLowerCase())
        return l.code;
    }
    return null;
  }

  /** Number, title or first line, in any of the hymn's languages — a singer
   *  looking for "Amazing grace" should not have to know it is number 14, and
   *  someone who only knows the tune's opening words should find it by those.
   *  A token that NAMES a language ("de", "German", "Deutsch") narrows the book
   *  to hymns carrying it, on top of the rest of the query — so "de" is every
   *  German hymn and "de jesu" the German ones whose text says "jesu". */
  const shown = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return index;
    const langCodes: string[] = [];
    const textTokens: string[] = [];
    for (const tok of q.split(/\s+/)) {
      const code = langToken(tok);
      if (code) langCodes.push(code);
      else textTokens.push(tok);
    }
    const textQ = textTokens.join(" ");
    return index.filter((h: any) => {
      const have = Object.keys(h.titles ?? {});
      if (!langCodes.every((c) => have.includes(c))) return false;
      if (!textQ) return true;
      if (String(h.number) === textQ) return true;
      const texts = [...Object.values(h.titles ?? {}), ...Object.values(h.firstLines ?? {})];
      return texts.some((t) => String(t).toLowerCase().includes(textQ));
    });
  });

  /** The language to show, given what this hymn actually has. The reader's
   *  preference is a preference, not a promise: a German-only hymn shows German
   *  to an English reader rather than showing nothing. */
  function pick(langs: string[], want: string): string {
    return langs.includes(want) ? want : (langs[0] ?? "en");
  }

  const langs = $derived(hymn ? Object.keys(hymn.texts) : []);
  const lang = $derived(pick(langs, s.hymnLang));
  const text = $derived(hymn ? hymn.texts[lang] : null);

  /** A language chip's label. The endonym from the central list — a singer
   *  looking for the German text is looking for "Deutsch" — and the bare code
   *  upper-cased only for a language the hymnal has but the app does not ship
   *  an interface in, which is a thing the hymn files are allowed to do. */
  function endonym(code: string): string {
    return languages().find((l) => l.code === code)?.endonym ?? code.toUpperCase();
  }

  function openHymn(id: string): void {
    s.hymn = { id, semis: 0 };
  }

  // ── the automatic scroll ─────────────────────────────────────────────────
  //
  // A CONTINUOUS CREEP, not a jump per line. Singing is continuous, and a page
  // that steps every few seconds makes everyone find their place again each
  // time. Speeds are 1–9 with 0 meaning hold still, which is the setting a
  // player who is also fretting chords actually wants for a short hymn.
  //
  // Driven by rAF and scaled by the FRAME'S OWN elapsed time, so the same
  // setting creeps at the same rate on a 60Hz phone and a 120Hz one. A
  // per-frame constant would run at double speed on the good screen, which is
  // exactly the device most likely to be the one held up.
  let scroller = $state<HTMLElement | null>(null);

  /** Pixels per second at speed 1..9. Slow end first: 12 px/s is about a line
   *  every four seconds at sing-mode type sizes, which is a hymn taken gently. */
  const SPEED_PX = [0, 12, 18, 26, 36, 48, 62, 80, 104, 135];

  $effect(() => {
    const el = scroller;
    const speed = s.hymnScroll;
    if (!el || !s.hymnSinging || speed <= 0) return;
    let raf = 0;
    let last = performance.now();
    // Fractional pixels accumulate here: at 12 px/s a 120Hz frame is 0.1px, and
    // assigning that to scrollTop rounds it to zero every frame — the page would
    // simply never move at the slowest and most useful speeds.
    let carry = 0;
    const step = (now: number): void => {
      const dt = Math.min(now - last, 250) / 1000; // a backgrounded tab must not lurch
      last = now;
      carry += SPEED_PX[speed] * dt;
      const whole = Math.floor(carry);
      if (whole > 0) {
        carry -= whole;
        el.scrollTop += whole;
      }
      raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  });
</script>

<section class="screen" aria-label={t("hymnal.title")}>
  <ScreenBar
    title={open && text ? text.title : t("hymnal.title")}
    onBack={() => (open ? (s.hymn = null) : s.goRead())}
    backLabel={open ? t("hymnal.backToList") : t("bar.backToReading")}
    onMenu={() => (s.menuOpen = true)}
  >
    {#snippet actions()}
      {#if open && hymn}
        {#if langs.length > 1}
          <!-- One hymn, two texts: the same tune sung in either language. This is
               the toggle the German release grows out of. -->
          <div class="langs" role="group" aria-label={t("hymnal.language")}>
            {#each langs as l (l)}
              <button class="chip" class:on={l === lang} onclick={() => (s.hymnLang = l)}>
                {endonym(l)}
              </button>
            {/each}
          </div>
        {/if}
        <button class="chip" class:on={s.hymnChords} onclick={() => (s.hymnChords = !s.hymnChords)}>
          {t("hymnal.chords")}
        </button>
        {#if s.hymnChords}
          <div class="transpose" role="group" aria-label={t("hymnal.transpose")}>
            <button onclick={() => (s.hymn = { id: open.id, semis: open.semis - 1 })} aria-label={t("hymnal.transposeDown")}
              >−</button
            >
            <span class="key">{hymn.transposedKey}</span>
            <button onclick={() => (s.hymn = { id: open.id, semis: open.semis + 1 })} aria-label={t("hymnal.transposeUp")}
              >+</button
            >
          </div>
        {/if}
        <button class="sing" onclick={() => (s.hymnSinging = true)}>{t("hymnal.sing")}</button>
      {/if}
    {/snippet}
  </ScreenBar>

  {#if !open}
    <div class="find">
      <input
        type="search"
        placeholder={t("hymnal.find")}
        bind:value={filter}
        aria-label={t("hymnal.findLabel")}
      />
    </div>
    <div class="content">
      {#if index.length === 0}
        <p class="empty">{t("hymnal.loading")}</p>
      {:else if shown.length === 0}
        <p class="empty">{t("hymnal.noMatch", { query: filter })}</p>
      {:else}
        {#each shown as h (h.id)}
          {@const l = pick(Object.keys(h.titles ?? {}), s.hymnLang)}
          <button class="row" onclick={() => openHymn(h.id)}>
            <span class="num">{h.number}</span>
            <span class="rt">
              <span class="rtitle">{h.titles[l]}</span>
              <span class="rsub">{h.firstLines?.[l] ?? ""}</span>
            </span>
            <span class="tune">{h.tune}</span>
          </button>
        {/each}
      {/if}
    </div>
  {:else if !hymn || !text}
    <div class="content"><p class="empty">{t("hymnal.loadingOne")}</p></div>
  {:else}
    <div class="content hymn">
      {#if lang !== s.hymnLang}
        <!-- The reader's language is a PREFERENCE, not a promise: a hymn that
             exists in one language only shows that one. Saying so beats silently
             handing a German reader an English hymn. -->
        <p class="fellback">
          {t("hymnal.notInYourLanguage", { language: endonym(s.hymnLang), shown: endonym(lang) })}
        </p>
      {/if}
      <p class="credit">
        {text.author}{#if text.translator}, {t("hymnal.tr", { name: text.translator })}{/if}{#if text.year}, {text.year}{/if}
        · {hymn.tune} {hymn.meter}
      </p>
      {#each text.stanzas as st, i (i)}
        <div class="stanza">
          <span class="sn">{i + 1}</span>
          <div class="slines">
            {#each st.lines as line, li (li)}
              <p class="line" class:chorded={s.hymnChords && line.parts.some((p: any) => p.chord)}>
                {#each line.parts as part, pi (pi)}<span class="part"
                    >{#if s.hymnChords && part.chord}<span class="chord">{part.chord}</span>{/if}<span
                      class="lyric">{part.text}</span
                    ></span
                  >{/each}
              </p>
            {/each}
          </div>
        </div>
        {#if text.chorus}
          <div class="stanza refrain">
            <span class="sn" aria-hidden="true"></span>
            <div class="slines">
              <p class="rlabel">{t("hymnal.refrain")}</p>
              {#each text.chorus.lines as line, li (li)}
                <p class="line" class:chorded={s.hymnChords && line.parts.some((p: any) => p.chord)}>
                  {#each line.parts as part, pi (pi)}<span class="part"
                      >{#if s.hymnChords && part.chord}<span class="chord">{part.chord}</span>{/if}<span
                        class="lyric">{part.text}</span
                      ></span
                    >{/each}
                </p>
              {/each}
            </div>
          </div>
        {/if}
      {/each}
      <p class="pd">{t("hymnal.publicDomain")}</p>
    </div>
  {/if}
</section>

{#if s.hymnSinging && hymn && text}
  <!-- SING MODE: the same sunlight surface Present uses, for the same reason —
       a phone held up between two people in a room with the lights on. Fixed
       light, big type, and the app theme deliberately does not reach it. -->
  <div class="sing-host" use:modal={{ close: () => (s.hymnSinging = false) }} role="dialog" aria-modal="true"
       aria-label={t("hymnal.singing", { title: text.title })}>
    <div class="sbar">
      <button class="sclose" onclick={() => (s.hymnSinging = false)} aria-label={t("hymnal.stopSinging")}>✕</button>
      <span class="stitle">{text.title}</span>
      <span class="spacer"></span>
      <div class="transpose" role="group" aria-label={t("hymnal.scrollSpeed")}>
        <button onclick={() => (s.hymnScroll = Math.max(0, s.hymnScroll - 1))} aria-label={t("hymnal.scrollSlower")}>−</button>
        <span class="key">{s.hymnScroll === 0 ? t("hymnal.hold") : `${s.hymnScroll}`}</span>
        <button onclick={() => (s.hymnScroll = Math.min(9, s.hymnScroll + 1))} aria-label={t("hymnal.scrollFaster")}>+</button>
      </div>
    </div>
    <div class="sbody" bind:this={scroller}>
      {#each text.stanzas as st, i (i)}
        <div class="sstanza">
          {#each st.lines as line, li (li)}
            <p class="sline">
              {#each line.parts as part, pi (pi)}<span class="part"
                  >{#if s.hymnChords && part.chord}<span class="schord">{part.chord}</span>{/if}<span
                    >{part.text}</span
                  ></span
                >{/each}
            </p>
          {/each}
        </div>
        {#if text.chorus}
          <div class="sstanza srefrain">
            {#each text.chorus.lines as line, li (li)}
              <p class="sline">
                {#each line.parts as part, pi (pi)}<span class="part"
                    >{#if s.hymnChords && part.chord}<span class="schord">{part.chord}</span>{/if}<span
                      >{part.text}</span
                    ></span
                  >{/each}
              </p>
            {/each}
          </div>
        {/if}
      {/each}
    </div>
  </div>
{/if}

<style>
  .screen {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--paper, #fcf9f4);
  }
  .langs {
    display: flex;
    gap: 2px;
  }
  .chip {
    font-size: calc(13px * var(--uiScale, 1));
    padding: 6px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    color: var(--faded, #8a8276);
  }
  .chip.on {
    color: var(--gold, #9e7d38);
    border-color: var(--gold, #9e7d38);
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  .transpose {
    display: flex;
    align-items: center;
    gap: 2px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
  }
  .transpose button {
    padding: 5px 10px;
    font-size: calc(15px * var(--uiScale, 1));
    color: var(--gold, #9e7d38);
  }
  .key {
    min-width: 3ch;
    text-align: center;
    font-size: calc(13px * var(--uiScale, 1));
    color: var(--ink, #211f1a);
  }
  .sing {
    font-size: calc(13px * var(--uiScale, 1));
    padding: 6px 14px;
    border: 1px solid var(--gold, #9e7d38);
    border-radius: 6px;
    color: var(--gold, #9e7d38);
  }
  .find {
    padding: 10px 14px 0;
  }
  .find input {
    width: 100%;
    font-size: calc(15px * var(--uiScale, 1));
    padding: 8px 12px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    background: var(--popupPaper, #f2eee6);
    color: var(--ink, #211f1a);
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 12px 14px;
    align-content: start;
  }
  .empty {
    color: var(--faded, #8a8276);
    font-size: calc(15px * var(--uiScale, 1));
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 12px;
    width: 100%;
    text-align: left;
    padding: 10px 12px;
    border-bottom: 1px solid var(--rule, #d8cba8);
  }
  .row:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 8%, transparent);
  }
  .num {
    min-width: 3ch;
    text-align: right;
    color: var(--faded, #8a8276);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .rt {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .rtitle {
    font-size: calc(16px * var(--uiScale, 1));
    color: var(--ink, #211f1a);
  }
  .rsub {
    font-size: calc(13px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tune {
    font-size: calc(12px * var(--uiScale, 1));
    letter-spacing: 0.04em;
    color: var(--faded, #8a8276);
  }
  /* ── one hymn ─────────────────────────────────────────────────────────── */
  .hymn {
    font-family: "EB Garamond", Georgia, serif;
    max-width: 46rem;
  }
  /* A note, not a warning: nothing is wrong, this hymn simply exists in one
     language. Quieter than the credit line under it. */
  .fellback {
    font-size: calc(12.5px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    font-style: italic;
    margin: 0 0 6px;
  }
  .credit {
    font-size: calc(13px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    margin: 0 0 14px;
  }
  .stanza {
    display: flex;
    gap: 10px;
    margin-bottom: 16px;
  }
  .sn {
    min-width: 2ch;
    text-align: right;
    color: var(--faded, #8a8276);
    font-size: calc(14px * var(--uiScale, 1));
  }
  .slines {
    min-width: 0;
  }
  .refrain .slines {
    border-left: 2px solid var(--rule, #d8cba8);
    padding-left: 10px;
  }
  .rlabel {
    margin: 0 0 2px;
    font-size: calc(12px * var(--uiScale, 1));
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--faded, #8a8276);
  }
  .line {
    margin: 0;
    font-size: calc(17px * var(--uiScale, 1));
    line-height: 1.5;
    color: var(--ink, #211f1a);
  }
  /* A chorded line needs headroom for the chord sitting above it, and only a
     chorded one — otherwise every verse of a lyrics-only hymn is double-spaced
     for a chord that is not there. */
  .line.chorded {
    line-height: 2.5;
  }
  .part {
    position: relative;
    white-space: pre-wrap;
  }
  .chord {
    position: absolute;
    bottom: 1.05em;
    left: 0;
    font-family: ui-sans-serif, system-ui, sans-serif;
    font-size: calc(12.5px * var(--uiScale, 1));
    font-weight: 600;
    letter-spacing: 0.01em;
    color: var(--gold, #9e7d38);
    white-space: nowrap;
  }
  .lyric {
    white-space: pre-wrap;
  }
  .pd {
    font-size: calc(12px * var(--uiScale, 1));
    color: var(--faded, #8a8276);
    margin-top: 24px;
  }
  /* ── sing mode ────────────────────────────────────────────────────────── */
  .sing-host {
    position: fixed;
    inset: 0;
    bottom: max(var(--bottomNavH, 0px), var(--safeBottom));
    z-index: 60;
    background: #fcf9f4;
    color: #211f1a;
    display: flex;
    flex-direction: column;
    font-family: "EB Garamond", Georgia, serif;
    padding: var(--safeTop) var(--safeRight) 0 var(--safeLeft);
  }
  .sbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-bottom: 1px solid #d8cba8;
  }
  .sclose,
  .sbar .transpose button {
    color: #9e7d38;
    font-size: calc(20px * var(--uiScale, 1));
    padding: 6px 12px;
  }
  .sbar .transpose {
    border-color: #d8cba8;
  }
  .sbar .key {
    color: #211f1a;
  }
  .stitle {
    font-size: calc(17px * var(--uiScale, 1));
  }
  .sbody {
    flex: 1;
    overflow-y: auto;
    padding: 20px 22px 60vh;
    scrollbar-width: none;
  }
  .sstanza {
    margin-bottom: 1.4em;
  }
  .srefrain {
    padding-left: 12px;
    border-left: 2px solid #d8cba8;
  }
  .sline {
    margin: 0;
    /* Fluid, so the same hymn is legible on a phone held up across a table and
       on a laptop propped on a piano. */
    font-size: clamp(22px, 4.2vw, 44px);
    line-height: 2.1;
  }
  .schord {
    position: absolute;
    bottom: 1.1em;
    left: 0;
    font-family: ui-sans-serif, system-ui, sans-serif;
    font-size: 0.52em;
    font-weight: 600;
    color: #9e7d38;
    white-space: nowrap;
  }
</style>
