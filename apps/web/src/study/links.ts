// The one link dispatcher (manifest P1.4): every panel URI parses in the core
// via plumbline_route_link_json; this switch owns only navigation, prompts, and the
// write choreography (author endpoint → engine reloads → shell re-fetches).
// Shift/Ctrl-click on a go: link opens the other pane (Tier-0 #8).

import { nowStamp } from "../engine/StudyEngine";
import type { Session } from "../state/session.svelte";

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
    case "occurrences":
      s.panel = { kind: "concordance", code: link.code };
      break;
    case "rendering":
      s.panel = { kind: "renderingConcordance", code: link.code, rendering: link.rendering };
      break;
    case "codeStudy":
      s.panel = { kind: "codeStudy", code: link.code, word: link.word ?? null };
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
    // No `conceptMap` case: the `conceptmap:` verb left the core's link
    // vocabulary on 2026-07-30 when the concept map was removed.
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
      // instead of failing (2026-07-28 feedback).
      s.threadPickFor = link.refKey;
      break;
    case "untag": {
      // The wire carries the tag ordinal; authoring wants the name.
      const tag = (await s.fetchQ("tags"))?.tags?.[link.tag];
      if (!tag) break;
      if (
        !(await s.askConfirm(
          `Remove ${link.refKey} from “${tag.name}”?`,
          "The verse leaves this tag. The tag and its other verses stay as they are.",
          "Remove",
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
          "Reject this suggested weave?",
          "It is deleted, not hidden — it will not come back for review.",
          "Reject",
        ))
      ) {
        break;
      }
      report(s, s.author("weaveReject", link.index));
      break;
    case "editThreadNotes": {
      const thread = (await s.fetchQ("threads"))?.threads?.[link.index];
      if (!thread) break;
      const notes = await s.askText(`Notes — ${thread.name}`, thread.notes ?? "", true);
      if (notes !== null) report(s, s.author("threadSetNotes", thread.name, notes));
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
      if (text !== null) report(s, s.author("userNoteSet", link.refKey, text, nowStamp()));
      break;
    }
    default:
      console.warn("unhandled panel verb", link);
  }
}

/** Authoring endpoints resolve null on success, else an error string. */
function report(s: Session, p: Promise<string | null>): void {
  void p.then((err) => {
    if (err) s.showToast(err);
  });
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

/** Loading a weave pulls its first link's two passages up (product 2026-07-25,
 *  both shells): active pane → endpoint a, the next pane → endpoint b — no
 *  hunting through the card to see the weave in the text. */
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
