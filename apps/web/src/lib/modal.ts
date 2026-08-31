// One dialog behaviour — focus in, Tab trapped, focus back, Escape — for every
// `aria-modal` surface in the shell. Applied as an action so the next dialog
// gets it by adding one word.

import type { Action } from "svelte/action";

export interface ModalOptions {
  /**
   * The dialog's own close path, called on Escape — never a close invented here,
   * since each dialog already knows what leaving it means (PromptDialog resolves
   * null, the confirmation resolves "no", FirstRun refuses while it is asking).
   *
   * Omitted where Escape must not close, e.g. SettingsDialog's restore-failed
   * alert: a stray key must not take the message away before it is read.
   */
  close?: () => void;
}

/** Focusable, as the browser means it. `summary` is here because the settings
 *  dialog's disclosures are its only control in places. */
const FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "summary",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

/**
 * The focusable controls actually on screen, in tab order.
 *
 * Filtered on having a box, not `offsetParent`: these dialogs are
 * `position: fixed`, whose `offsetParent` is null whether or not they are
 * visible. It also drops boxless controls that cannot be tabbed to anyway (the
 * `hidden` file input behind "Restore from backup…"), which would otherwise be
 * a dead stop in the cycle.
 */
function focusables(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => el.getClientRects().length > 0,
  );
}

/** Every open modal, oldest first. The last one is the one in front of the
 *  reader, and the only one Escape may close. */
interface Entry {
  node: HTMLElement;
  readonly close: (() => void) | undefined;
}
const OPEN: Entry[] = [];

/** Escape is the one key handled document-level, because it must work even when
 *  focus has left the dialog — which happens whenever the control the reader was
 *  on is removed while the dialog is still up. A node listener silently stops
 *  firing then, and for `askConfirm`, settled only by a close path, that leaves
 *  a promise that never settles.
 *
 *  Capture, so it runs before Shell's own Escape ladder, and it stops the event
 *  whether or not the top dialog closes on it: a press aimed at the surface in
 *  front of the reader must never peel something behind it. */
function onDocumentEscape(e: KeyboardEvent): void {
  if (e.key !== "Escape" || OPEN.length === 0) return;
  e.preventDefault();
  e.stopPropagation();
  OPEN[OPEN.length - 1].close?.();
}
if (typeof document !== "undefined") {
  document.addEventListener("keydown", onDocumentEscape, true);
}

/**
 * Focus in, Tab trapped, focus back on close, Escape handled by the stack above.
 *
 * Focus lands on the dialog itself by default (the ARIA authoring practice when
 * there is no obvious first control: the screen reader reads the label and
 * heading first, and no dialog can hand a keyboard reader a destructive button).
 * A dialog with an obvious first control marks it `data-modal-focus`. One with
 * no focusable control at all still works — the container takes `tabindex="-1"`
 * and Tab is swallowed rather than escaping to the page underneath.
 *
 * The observer is for content that arrives late: several of these fill from an
 * engine round trip, so `data-modal-focus` may not exist yet. Focus goes to the
 * container immediately either way and moves on to the marked control if it
 * appears — but only while focus is still on the container, so a reader who has
 * already tabbed somewhere does not have it taken back.
 */
export const modal: Action<HTMLElement, ModalOptions | undefined> = (node, options) => {
  let opts: ModalOptions = options ?? {};

  // Captured before anything is focused: the control the reader was on when the
  // dialog opened, and where they are owed a return. `body` is what the browser
  // reports when nothing has focus, so restoring to it would be a blur.
  const opener = document.activeElement;
  const returnTo = opener instanceof HTMLElement && opener !== document.body ? opener : null;

  // The container has to be able to hold focus itself.
  if (!node.hasAttribute("tabindex")) node.setAttribute("tabindex", "-1");

  const marked = (): HTMLElement | null => node.querySelector<HTMLElement>("[data-modal-focus]");

  const target = marked();
  if (target) target.focus();
  // `preventScroll`: the container is the whole dialog, and focusing a scrollable
  // box scrolls it to the top, losing a reader's position in a long settings list.
  else node.focus({ preventScroll: true });

  let watcher: MutationObserver | null = null;
  if (!target) {
    watcher = new MutationObserver(() => {
      // The reader has moved on; the moment for an opening focus has passed.
      if (document.activeElement !== node) return stopWatching();
      const late = marked();
      if (!late) return;
      late.focus();
      stopWatching();
    });
    watcher.observe(node, { childList: true, subtree: true });
  }
  function stopWatching(): void {
    watcher?.disconnect();
    watcher = null;
  }

  function onKeydown(e: KeyboardEvent): void {
    // Escape is not handled here; see the document-level handler above.
    if (e.key !== "Tab") return;

    const items = focusables(node);
    if (items.length === 0) {
      // Nothing to move to, and the page behind a modal is not somewhere Tab
      // may go.
      e.preventDefault();
      node.focus({ preventScroll: true });
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    const inside = active instanceof Node && node.contains(active);
    if (e.shiftKey) {
      // Backwards off the front, or backwards off the container, wraps to the end.
      if (!inside || active === first || active === node) {
        e.preventDefault();
        last.focus();
      }
    } else if (!inside || active === last) {
      e.preventDefault();
      first.focus();
    }
  }

  node.addEventListener("keydown", onKeydown);
  const entry: Entry = { node, get close() { return opts.close; } };
  OPEN.push(entry);

  return {
    update(next) {
      opts = next ?? {};
    },
    destroy() {
      stopWatching();
      node.removeEventListener("keydown", onKeydown);
      const at = OPEN.indexOf(entry);
      if (at >= 0) OPEN.splice(at, 1);
      // Give focus back, but never take it from somewhere it has since gone.
      // Only two states are worth restoring from: dropped to the body (the usual
      // case, the reader's control having just been removed) or still inside the
      // dying dialog.
      const active = document.activeElement;
      const stranded =
        active === null || active === document.body || (active instanceof Node && node.contains(active));
      if (stranded && returnTo?.isConnected) returnTo.focus();
    },
  };
};
