// ONE dialog behaviour, for every `aria-modal` surface in the shell.
//
// There are fourteen of them, and before this each one answered the keyboard
// differently or not at all: none moved focus in, none held Tab, none gave focus
// back, and Escape was a single ladder on `svelte:window` (Shell.svelte) that
// returns early whenever the event came from a field — so Escape inside the
// "New thread…" box, the note editor or the church fields did nothing at all.
// That is the reported gap, and it is the one a keyboard reader meets first.
//
// A modal that does not take focus is a modal only to the mouse: a screen reader
// stays parked wherever it was, Tab walks the page BEHIND the dialog, and the
// reader never learns anything opened. So this is an action rather than a note in
// a review checklist — the next dialog gets it by adding one word.

import type { Action } from "svelte/action";

export interface ModalOptions {
  /**
   * The dialog's OWN close path, called on Escape.
   *
   * Never a close invented here. Each of these dialogs already knows what
   * leaving it means — PromptDialog resolves its promise with null, FirstRun
   * refuses to be dismissed while it is asking a question, the confirmation
   * resolves "no" — and a second answer to that question is how the two drift.
   *
   * Omitted where Escape must NOT close: SettingsDialog's restore-failed alert
   * has no backdrop dismiss for the same reason (a stray tap must not take the
   * message away before it is read, and a stray Escape is the same tap).
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
 * Filtered by whether the element has a box, not by `offsetParent`: every one of
 * these dialogs is `position: fixed`, and a fixed element's `offsetParent` is
 * null whether or not it is visible. It also drops the controls that are only
 * notionally there — the `hidden` file input behind "Restore from backup…" has
 * no box and cannot be tabbed to, so counting it would put a dead stop in the
 * cycle.
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

/** ESCAPE IS DOCUMENT-LEVEL, ON PURPOSE, and it is the one key that is.
 *
 *  Tab is handled on the dialog node, because Tab only means anything when focus
 *  is already inside it. Escape is not like that: it has to work even when focus
 *  has ended up somewhere else, which happens whenever the control the reader was
 *  on is removed while the dialog is still up. A node listener silently stops
 *  firing then — and for `askConfirm`, whose promise is only settled by a close
 *  path, that is not a dead key but a PROMISE THAT NEVER SETTLES: the caller
 *  waits forever and the action looks like a dead button
 *  (e2e/destructive.spec.ts pins exactly this).
 *
 *  Capture, so it runs before Shell's own Escape ladder, and it stops the event
 *  whether or not the top dialog closes on it: a press aimed at the surface in
 *  front of the reader must never reach past it and peel something behind. */
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
 * WHERE FOCUS LANDS is the one judgement call. Blindly focusing the first
 * focusable thing is wrong in both directions: on ConfirmDialog it would be
 * Cancel (harmless but arbitrary), and any dialog that grows a destructive
 * button at the top would hand a keyboard reader the trigger. So the default is
 * the DIALOG ITSELF — which is what the ARIA authoring practices say to do when
 * there is no obvious first control, and it makes a screen reader read the
 * dialog's label and heading before anything else. A dialog with an obvious
 * first control says so with `data-modal-focus`, and only PromptDialog's field
 * and the restore-failed acknowledgement do.
 *
 * A dialog with NO focusable control at all still works: the container takes
 * `tabindex="-1"` and holds focus, and Tab is swallowed rather than escaping to
 * the page underneath.
 *
 * CONTENT THAT ARRIVES LATE is why the observer is here. Several of these fill
 * from an engine round trip, so `data-modal-focus` may not exist when the action
 * runs. Focus goes to the container immediately either way (never nowhere), and
 * moves on to the marked control if it appears — but only while focus is still
 * on the container, because a reader who has already tabbed somewhere must not
 * have it taken back.
 */
export const modal: Action<HTMLElement, ModalOptions | undefined> = (node, options) => {
  let opts: ModalOptions = options ?? {};

  // Captured BEFORE anything is focused: this is the control the reader was on
  // when the dialog opened, and it is where they are owed a return. `body` is
  // not a control — it is what the browser reports when nothing has focus — and
  // focusing it back would be a blur dressed up as a restore.
  const opener = document.activeElement;
  const returnTo = opener instanceof HTMLElement && opener !== document.body ? opener : null;

  // The container has to be able to hold focus itself. Only added when the
  // dialog has not already said so (PromptDialog had it by hand).
  if (!node.hasAttribute("tabindex")) node.setAttribute("tabindex", "-1");

  const marked = (): HTMLElement | null => node.querySelector<HTMLElement>("[data-modal-focus]");

  const target = marked();
  if (target) target.focus();
  // `preventScroll`: the container is the whole dialog, and focusing a
  // scrollable box scrolls it to the top — which on a long settings dialog would
  // undo a reader's position for no reason.
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
    // Escape is NOT handled here; see the document-level handler and OPEN below.
    if (e.key !== "Tab") return;

    const items = focusables(node);
    if (items.length === 0) {
      // Nothing to move to, so Tab must not move at all — the page behind a
      // modal is not somewhere Tab may go.
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
      // At teardown the control the reader was on has usually just been removed,
      // so the browser has already dropped focus to the body — that, or focus
      // still inside the dying dialog, are the two cases worth restoring from.
      const active = document.activeElement;
      const stranded =
        active === null || active === document.body || (active instanceof Node && node.contains(active));
      if (stranded && returnTo?.isConnected) returnTo.focus();
    },
  };
};
