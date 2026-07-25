// The one link dispatcher (manifest P1.4): every panel URI parses in the core
// via plumbline_route_link_json; this switch owns only navigation, prompts, and the
// write choreography (author endpoint → engine reloads → shell re-fetches).
// Shift/Ctrl-click on a go: link opens the other pane (Tier-0 #8).

import { nowStamp, routeLink } from "../engine/StudyEngine";
import type { Session } from "../state/session.svelte";

export async function dispatchLink(s: Session, uri: string, ev?: MouseEvent): Promise<void> {
  const link = routeLink(s.wasm, uri);
  if (!link) return;
  const otherPane = !!(ev && (ev.shiftKey || ev.ctrlKey || ev.metaKey));

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
      break;
    case "conceptMap":
      s.mapPopup = { kind: "conceptMap", code: link.code };
      break;
    case "guide":
      s.panel = { kind: "guide" };
      break;
    case "about":
      s.panel = { kind: "about" };
      break;

    case "addTag": {
      const name = await s.askText("Tag this verse — tag name");
      if (name?.trim()) report(s, s.engine.tagAdd(name.trim(), "verse", link.refKey, null, nowStamp()));
      break;
    }
    case "addThread": {
      const name = await s.askText("Add to thread — thread name");
      if (name?.trim()) report(s, s.engine.threadAdd(name.trim(), link.refKey, null, nowStamp()));
      break;
    }
    case "untag": {
      // The wire carries the tag ordinal; authoring wants the name.
      const tag = s.engine.tags()?.tags?.[link.tag];
      if (tag) report(s, s.engine.tagRemove(tag.name, "verse", link.refKey));
      break;
    }
    case "makeWeave":
      // Tag→weave: pick the members (default all), name it, chain it.
      s.tagWeaveFor = link.tag;
      break;
    case "approve":
      report(s, s.engine.weaveApprove(link.index));
      break;
    case "reject":
      report(s, s.engine.weaveReject(link.index));
      break;
    case "editThreadNotes": {
      const thread = s.engine.threads()?.threads?.[link.index];
      if (!thread) break;
      const notes = await s.askText(`Notes — ${thread.name}`, thread.notes ?? "", true);
      if (notes !== null) report(s, s.engine.threadSetNotes(thread.name, notes));
      break;
    }
    case "editEntryNote": {
      const thread = s.engine.threads()?.threads?.[link.thread];
      if (!thread) break;
      const entry = thread.entries?.[link.entry];
      const note = await s.askText(`Entry note — ${thread.name}`, entry?.note ?? "", true);
      if (note !== null) report(s, s.engine.threadEntrySetNote(thread.name, link.entry, note));
      break;
    }
    case "editWeaveNotes": {
      const weave = s.engine.weaves()?.weaves?.[link.index];
      if (!weave) break;
      const notes = await s.askText(`Notes — ${weave.name}`, weave.notes ?? "", true);
      if (notes !== null) report(s, s.engine.weaveSetNotes(weave.name, notes));
      break;
    }
    case "editNote": {
      const existing = s.engine.userNote(link.refKey);
      const text = await s.askText(`Your note — ${link.refKey}`, existing?.text ?? "", true);
      if (text !== null) report(s, s.engine.userNoteSet(link.refKey, text, nowStamp()));
      break;
    }
    default:
      console.warn("unhandled panel verb", link);
  }
}

/** Authoring endpoints return null on success, else an error string. */
function report(s: Session, err: string | null): void {
  if (err) s.showToast(err);
}
