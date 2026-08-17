// Publish `--uiScale` on `:root` — the one number the whole chrome multiplies by.
//
// The reader's text size (Settings ▸ Text size, 12–40px) must move the whole
// chrome — header, menus, every dialog and every sheet — not just the
// scripture: someone who turns the text up cannot read 13px chrome around a
// large page. The study panel multiplied by `--uiScale` already, so this lifts
// that one variable up to the root rather than standing a second mechanism up
// beside it.
//
// IT ALSO CARRIES THE BROWSER'S OWN TEXT SIZE, which is the other half of the
// same complaint. A reader who has set their browser's default font to 20px has
// asked every site to be bigger, and a chrome written entirely in `px` cannot
// hear it. `1rem` is exactly that preference — nothing in this app sets the root
// font-size, so the root's used value IS what the browser was told — and a probe
// one rem wide reports it as a number this can multiply in.
//
// A PROBE AND AN OBSERVER, not a one-off read at boot: changing the browser's
// font size reflows the page and fires no event any script can hear, so a value
// read once would be stale for the rest of an installed PWA's session. Observing
// a rem-sized box costs one hidden element and answers correctly whenever it
// changes. It cannot loop — the probe's own width does not depend on `--uiScale`.

import type { Action } from "svelte/action";

/** The size the chrome was drawn at, and the browser's own default. Both are
 *  divisors rather than magic numbers: 18px is the `bodySize` the chrome was
 *  calibrated at (the shipped default is 20 now, so a fresh install runs a
 *  little over 1 — changing this divisor instead would rescale every existing
 *  reader's chrome), and 16px is the CSS initial font size that every `Npx` in
 *  this shell was chosen against. */
const CSS_DEFAULT_PX = 16;

/**
 * Keep `--uiScale` on the document element in step with the reader's text size.
 *
 * Applied to a probe element that CSS has sized at `1rem` — see Shell.svelte.
 * The action reads that box rather than `getComputedStyle`, because the box is
 * also what a `ResizeObserver` can watch.
 *
 * @param textScale the app's own factor: `bodySize / 18`, times the chrome
 *   face's optical scale (`FONT_SCALE`) — composed by the caller so this stays
 *   one number and `--uiScale` stays one variable.
 */
export const uiScale: Action<HTMLElement, number> = (node, textScale) => {
  let reader = textScale ?? 1;

  function publish(): void {
    // `|| CSS_DEFAULT_PX`: a probe with no box (display:none, or a browser that
    // has not laid out yet) must read as "no opinion", never as a scale of 0.
    const rootPx = node.getBoundingClientRect().width || CSS_DEFAULT_PX;
    const scale = (reader * rootPx) / CSS_DEFAULT_PX;
    // Three decimals: enough that a 12px label lands on the same pixel it would
    // have, short enough that the attribute does not churn on sub-pixel noise.
    document.documentElement.style.setProperty("--uiScale", String(Math.round(scale * 1000) / 1000));
  }

  publish();
  const ro = new ResizeObserver(publish);
  ro.observe(node);

  return {
    update(next) {
      reader = next ?? 1;
      publish();
    },
    destroy() {
      ro.disconnect();
      document.documentElement.style.removeProperty("--uiScale");
    },
  };
};
