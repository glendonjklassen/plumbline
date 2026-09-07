// The one link dispatcher (manifest P1.4): every panel URI parses in the core
// via plumbline_route_link_json; this switch owns only navigation, prompts, and the
// write choreography (author endpoint → engine reloads → shell re-fetches).
// Shift/Ctrl-click on a go: link opens the other pane (Tier-0 #8).

import { nowStamp } from "../engine/StudyEngine";
import type { Session } from "../state/session.svelte";
import { t } from "../lib/i18n.svelte";

/** The text language the OPEN study belongs to, so a link out of it stays in
 *  the same Bible. Undefined for every panel that is not language-bearing. */
function panelLang(s: Session): string | undefined {
  const p = s.panel;
  if (!p) return undefined;
  return "lang" in p ? p.lang : undefined;
}

export async function dispatchLink(s: Session, uri: string, ev?: MouseEvent): Promise<void> {
  const link = await s.rpc.static("routeLink", uri);
  if (!link) return;
  const otherPane = !!(ev && (ev.shiftKey || ev.ctrlKey || ev.metaKey)) && !s.narrow;

  switch (link.verb) {
    case "go": {
      let idx = s.activePane;
      if (otherPane) {
        if (s.panes.length < 2) s.addPane(idx);
        idx = (idx + 1) % s.panes.length;
      }
      s.navigate(idx, link.book, link.chapter, link.verse ?? null);
      break;
    }
    case "external":
      // The core only parses https ext: links (panel::parse_link), so this is
      // never a javascript: or data: URI. Same open shape as the church QR.
      window.open(link.url, "_blank", "noopener,noreferrer");
      break;
    // The language TRAVELS with the study. A German word study's "other
    // occurrences" link must list German verses: dropping the language here
    // would hand the reader English text under a German headword, which is the
    // seam this carries it across.
    case "occurrences":
      s.panel = { kind: "concordance", code: link.code, lang: panelLang(s) };
      break;
    case "rendering":
      s.panel = {
        kind: "renderingConcordance",
        code: link.code,
        rendering: link.rendering,
        lang: panelLang(s),
      };
      break;
    case "codeStudy":
      s.panel = { kind: "codeStudy", code: link.code, word: link.word ?? null, lang: panelLang(s) };
      break;
    case "wordUsage":
    case "codeUsage": {
      // wusage:/lusage: links are baked only inside the word's own usage card
      // (lens chips, scope chips, distribution books, paging), so the current
      // card's origin — the tapped verse whose head, note and extras it
      // carries — survives the re-open. A producer that ever bakes one of
      // these links to a DIFFERENT word must clear the origin here instead.
      const cur = s.panel?.kind === "wordUsage" ? s.panel : undefined;
      s.panel = {
        kind: "wordUsage",
        word: link.word,
        code: link.verb === "codeUsage" ? link.code : undefined,
        refKey: cur?.refKey,
        tokenIndex: cur?.tokenIndex,
        scope: link.scope,
        page: link.page,
        lang: panelLang(s),
      };
      break;
    }
    case "threadEditMode":
      s.panel = { kind: "thread", index: link.index, edit: link.edit };
      break;
    case "thread":
      s.panel = { kind: "thread", index: link.index };
      break;
    case "tag":
      s.panel = { kind: "tag", index: link.index };
      break;
    case "weave":
      s.panel = { kind: "compare", index: link.index };
      await openWeavePassages(s, link.index);
      break;
    // No `conceptMap` case: the `conceptmap:` verb is not in the core's link
    // vocabulary (the concept map was removed).
    case "guide":
      s.panel = { kind: "guide" };
      break;
    case "about":
      s.panel = { kind: "about" };
      break;

    case "addTag":
      // Same reasoning as addThread below: pick from what exists, freetext only
      // for something new. The context menu already opened the picker; this is
      // the study panel's route into the same thing.
      s.tagPickFor = link.refKey;
      break;
    case "addThread":
      // Pick from the threads that exist, or name a new one. A bare prompt made
      // you retype an existing name exactly, and a typo forked a second thread
      // instead of failing.
      s.threadPickFor = link.refKey;
      break;
    case "untag": {
      // The wire carries the tag ordinal; authoring wants the name.
      const tag = (await s.fetchQ("tags"))?.tags?.[link.tag];
      if (!tag) break;
      if (
        !(await s.askConfirm(
          t("tag.removeAsk", { passage: link.refKey, tag: tag.name }),
          t("tag.removeBody"),
          t("tag.removeVerb"),
        ))
      ) {
        break;
      }
      report(s, s.author("tagRemove", tag.name, "verse", link.refKey));
      break;
    }
    case "makeWeave":
      // Tag→weave: pick the members (default all), name it, chain it.
      s.tagWeaveFor = link.tag;
      break;
    case "approve":
      report(s, s.author("weaveApprove", link.index));
      break;
    case "reject":
      if (
        !(await s.askConfirm(
          t("suggested.rejectAsk"),
          t("suggested.rejectBody"),
          t("suggested.rejectVerb"),
        ))
      ) {
        break;
      }
      report(s, s.author("weaveReject", link.index));
      break;
    // The three whole-item deletes. Each looks up the name first (the wire
    // carries the library ordinal; the ask should say what dies), confirms
    // through the shared dialog, then returns to the item's list — ordinals
    // shift after every write, so the detail card just deleted must not stay
    // up pointing at whatever slid into its index.
    case "deleteThread": {
      const thread = (await s.fetchQ("threads"))?.threads?.[link.index];
      if (!thread) break;
      if (
        !(await s.askConfirm(
          t("thread.deleteAsk", { thread: thread.name }),
          t("thread.deleteBody"),
          t("thread.deleteVerb"),
        ))
      ) {
        break;
      }
      const err = await s.author("threadRemove", thread.name);
      s.showToast(err ?? t("thread.deleted", { thread: thread.name }));
      if (!err && s.panel?.kind === "thread") s.panel = { kind: "threads" };
      break;
    }
    case "deleteTag": {
      const tag = (await s.fetchQ("tags"))?.tags?.[link.index];
      if (!tag) break;
      if (
        !(await s.askConfirm(
          t("tag.deleteAsk", { tag: tag.name }),
          t("tag.deleteBody"),
          t("tag.deleteVerb"),
        ))
      ) {
        break;
      }
      const err = await s.author("tagDelete", tag.name);
      s.showToast(err ?? t("tag.deleted", { tag: tag.name }));
      if (!err && s.panel?.kind === "tag") s.panel = { kind: "tags" };
      break;
    }
    case "deleteWeave": {
      const weave = (await s.fetchQ("weaves"))?.weaves?.[link.index];
      if (!weave) break;
      if (
        !(await s.askConfirm(
          t("weave.deleteAsk", { weave: weave.name }),
          t("weave.deleteBody"),
          t("weave.deleteVerb"),
        ))
      ) {
        break;
      }
      const err = await s.author("weaveDelete", link.index);
      s.showToast(err ?? t("weave.deleted", { weave: weave.name }));
      if (!err && s.panel?.kind === "compare") s.panel = { kind: "weaves" };
      break;
    }
    case "editThreadNotes": {
      const thread = (await s.fetchQ("threads"))?.threads?.[link.index];
      if (!thread) break;
      const notes = await s.askText(`Notes — ${thread.name}`, thread.notes ?? "", true);
      if (notes !== null) report(s, s.author("threadSetNotes", thread.name, notes));
      break;
    }
    // The bookends. Same shape as the notes editor above, one verb each — and
    // the prompt opens on whatever is already there, so editing an existing
    // bookend does not mean retyping it.
    case "editThreadOpening":
    case "editThreadClosing": {
      const thread = (await s.fetchQ("threads"))?.threads?.[link.index];
      if (!thread) break;
      const isOpening = link.verb === "editThreadOpening";
      const label = t(isOpening ? "panel.openingHeader" : "panel.closingHeader");
      const current = (isOpening ? thread.opening : thread.closing) ?? "";
      const text = await s.askText(`${label} — ${thread.name}`, current, true);
      // `null` is a CANCELLED prompt and must not be written; `""` is the
      // reader deliberately emptying the box, which the engine reads as
      // "clear it". Collapsing the two would make Cancel destructive.
      if (text !== null) report(s, s.author(isOpening ? "threadSetOpening" : "threadSetClosing", thread.name, text));
      break;
    }
    case "editEntryNote": {
      const thread = (await s.fetchQ("threads"))?.threads?.[link.thread];
      if (!thread) break;
      const entry = thread.entries?.[link.entry];
      const note = await s.askText(`Entry note — ${thread.name}`, entry?.note ?? "", true);
      if (note !== null) report(s, s.author("threadEntrySetNote", thread.name, link.entry, note));
      break;
    }
    // REARRANGE. A thread's order is the argument it makes — the Romans Road is
    // a road — so moving an entry is an ordinary edit, no confirmation.
    case "moveEntry": {
      const thread = (await s.fetchQ("threads"))?.threads?.[link.thread];
      if (!thread) break;
      // Floored at 0: the destination crosses the ABI as a u32, so a stale or
      // crafted link with a negative sum would wrap to 4-billion — which the
      // engine then CLAMPS TO THE END, turning "move up from the top" into
      // "move to the bottom".
      report(s, s.author("threadEntryMove", thread.name, link.entry, Math.max(0, link.entry + link.delta)));
      break;
    }
    // REMOVE. Destructive and not undoable, so it asks first — the rule
    // `deletethread:` and `reject:` already follow. It names the passage,
    // because "remove entry 3" is not something a reader can check.
    case "removeEntry": {
      const thread = (await s.fetchQ("threads"))?.threads?.[link.thread];
      if (!thread) break;
      const entry = thread.entries?.[link.entry];
      const passage = entry?.display ?? entry?.verse ?? "";
      if (
        !(await s.askConfirm(
          t("threads.removeEntryAsk", { passage }),
          t("threads.removeEntryBody", { thread: thread.name }),
          t("threads.removeEntryVerb"),
        ))
      ) {
        break;
      }
      report(s, s.author("threadEntryRemove", thread.name, link.entry));
      break;
    }
    case "editWeaveNotes": {
      const weave = (await s.fetchQ("weaves"))?.weaves?.[link.index];
      if (!weave) break;
      const notes = await s.askText(`Notes — ${weave.name}`, weave.notes ?? "", true);
      if (notes !== null) report(s, s.author("weaveSetNotes", weave.name, notes));
      break;
    }
    case "editNote": {
      const existing = await s.fetchQ("userNote", link.refKey);
      const text = await s.askText(`Your note — ${link.refKey}`, existing?.text ?? "", true);
      if (text === null) break;
      // Saving an EMPTIED editor deletes the note (usernote.rs's contract), so
      // it asks exactly like the browser's ✕ — whether an action asks is a
      // property of the action, not of which button reached it. Same wording
      // on both paths (deleteNote below).
      if (text.trim() === "" && (existing?.text ?? "").trim() !== "") {
        if (
          !(await s.askConfirm(
            t("notes.deleteAsk", { passage: link.refKey }),
            t("notes.deleteBody"),
            t("notes.deleteVerb"),
          ))
        ) {
          break;
        }
      }
      report(s, s.author("userNoteSet", link.refKey, text, nowStamp()));
      break;
    }
    default:
      console.warn("unhandled panel verb", link);
  }
}

/** Delete one note outright — the notes browser's ✕, so deleting does not mean
 *  opening the note and emptying it. Confirms with the SAME wording as the
 *  emptied-editor path in `editNote` above, then writes empty text: the core's
 *  `set_note` removes the file (usernote.rs), the engine reloads study data,
 *  and the browser re-fetches — nothing else about the verse is touched. */
export async function deleteNote(s: Session, refKey: string, display?: string): Promise<void> {
  if (
    !(await s.askConfirm(
      t("notes.deleteAsk", { passage: display ?? refKey }),
      t("notes.deleteBody"),
      t("notes.deleteVerb"),
    ))
  ) {
    return;
  }
  report(s, s.author("userNoteSet", refKey, "", nowStamp()));
}

/** Authoring endpoints resolve null on success, else an error string. */
function report(s: Session, p: Promise<string | null>): void {
  void p.then((err) => {
    if (err) s.showToast(err);
  });
}

/** Drop-reorder of a thread's entries. The thread detail's entry rows carry a
 *  `drag` id, "{thread}:{entry}" (core `panel::thread_detail`); dropping row
 *  `from` on row `to` is the same write the row's ↑/↓ links make — the engine's
 *  `move_in_thread` removes the entry and re-inserts it at the target ordinal.
 *  A drop across two different threads' ids (impossible from one panel, cheap
 *  to refuse) is ignored rather than guessed at. */
export async function dragEntry(s: Session, from: string, to: string): Promise<void> {
  const [ft, fe] = from.split(":").map(Number);
  const [tt, te] = to.split(":").map(Number);
  if (![ft, fe, tt, te].every(Number.isInteger) || ft !== tt || fe === te) return;
  const thread = (await s.fetchQ("threads"))?.threads?.[ft];
  if (!thread) return;
  report(s, s.author("threadEntryMove", thread.name, fe, Math.max(0, te)));
}

/** refKey ("Gen 1:7", frozen wire form) → pane coordinates. */
function parseRefKey(ref: string | undefined): { book: string; chapter: number; verse: number } | null {
  if (!ref) return null;
  const sp = ref.lastIndexOf(" ");
  const colon = ref.indexOf(":", sp);
  if (sp <= 0 || colon < 0) return null;
  const chapter = Number(ref.slice(sp + 1, colon));
  const verse = Number(ref.slice(colon + 1));
  return chapter && verse ? { book: ref.slice(0, sp), chapter, verse } : null;
}

/** Loading a weave pulls its first link's two passages up (both shells): active
 *  pane → endpoint a, the next pane → endpoint b — no hunting through the card
 *  to see the weave in the text. */
async function openWeavePassages(s: Session, index: number): Promise<void> {
  const links = (await s.fetchQ("weaves"))?.weaves?.[index]?.links ?? [];
  const link = links.find((l: any) => l.resolved) ?? links[0];
  const a = parseRefKey(link?.a);
  const b = parseRefKey(link?.b);
  if (!a || !b) return;
  const first = s.activePane;
  s.navigate(first, a.book, a.chapter, a.verse);
  // One pane on a phone: endpoint b stays a tap away on the compare card.
  if (!s.narrow && (b.book !== a.book || b.chapter !== a.chapter)) {
    if (s.panes.length < 2) s.addPane(first);
    s.navigate((first + 1) % s.panes.length, b.book, b.chapter, b.verse);
  }
}
