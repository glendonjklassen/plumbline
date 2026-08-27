<script lang="ts">
  // SHARE, as a destination — the evangelism role's home. What was a header
  // QR dialog plus three church fields buried at the bottom of Settings is one
  // screen: the QR and link that hand the app over, and the church that a
  // shared link carries — set where its effect is visible, not in Settings.
  // The Android twin is ui/ShareScreen.kt.
  import { getSession } from "../state/session.svelte";
  import ScreenBar from "../lib/ScreenBar.svelte";
  import QrCode from "./QrCode.svelte";
  import { cleanChurch, churchTitle, hasChurch, visitChurch } from "./church";
  import { t } from "../lib/i18n.svelte";

  const s = getSession();

  // What we actually hand over: the app, plus this reader's church when they
  // have set one below. One QR, both things.
  const link = $derived(s.shareLink);
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
  .field {
    padding: 7px 10px;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 6px;
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
    font-size: calc(14.5px * var(--uiScale, 1));
  }
</style>
