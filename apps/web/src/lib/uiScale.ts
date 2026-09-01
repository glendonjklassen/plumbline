// Publish `--uiScale` on `:root` — the one number the whole chrome multiplies by.
//
// It carries both the reader's text size (Settings ▸ Text size, 12–40px) and the
// browser's own default font size. Nothing in this app sets the root font-size,
// so `1rem` is exactly the browser preference; a probe one rem wide reports it as
// a number to multiply in. Probe plus observer rather than a read at boot,
// because changing the browser's font size fires no event a script can hear. It
// cannot loop — the probe's width does not depend on `--uiScale`.

import type { Action } from "svelte/action";

/** The CSS initial font size, which every `Npx` in this shell was chosen
 *  against. (The other divisor, 18px, is the `bodySize` the chrome was
 *  calibrated at and lives in the caller's `textScale`; changing either
 *  rescales every existing reader's chrome.) */
const CSS_DEFAULT_PX = 16;

/**
 * Keep `--uiScale` on the document element in step with the reader's text size.
 *
 * Applied to a probe element CSS has sized at `1rem` (see Shell.svelte). Reads
 * that box rather than `getComputedStyle`, because a box is what a
 * `ResizeObserver` can watch.
 *
 * @param textScale the app's own factor: `bodySize / 18` times the chrome face's
 *   optical scale (`FONT_SCALE`), composed by the caller so `--uiScale` stays
 *   one variable.
 */
export const uiScale: Action<HTMLElement, number> = (node, textScale) => {
  let reader = textScale ?? 1;

  function publish(): void {
    // `|| CSS_DEFAULT_PX`: a probe with no box (display:none, or not laid out
    // yet) must read as "no opinion", never as a scale of 0.
    const rootPx = node.getBoundingClientRect().width || CSS_DEFAULT_PX;
    const scale = (reader * rootPx) / CSS_DEFAULT_PX;
    // Three decimals: exact enough for a 12px label, coarse enough that the
    // attribute does not churn on sub-pixel noise.
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
