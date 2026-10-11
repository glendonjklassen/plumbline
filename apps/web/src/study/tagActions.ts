// The tag housekeeping flows, written once.
//
// Rename is offered from three places — the Tags screen's action row, the tag's
// own card in the study panel, and the picker sheet — because a reader looked
// for it in the study panel and found only the library screen (2026-10-11). Three
// buttons, one flow: the prompt, the author call and the toast live here, so the
// three surfaces cannot drift apart on what a rename asks or says.

import type { Session } from "../state/session.svelte";
import { t } from "../lib/i18n.svelte";
import { refDisplay } from "../reader/refname";

/** Prompt for a new name and rename `from` to it. A cancelled prompt, an empty
 *  answer or the same name again does nothing — the core refuses a name another
 *  tag already answers to, and that refusal arrives as the toast. Rename keeps
 *  the tag's IDENTITY: the core carries the id across, so the tag on the other
 *  side is the same tag and not a new one wearing the name.
 *
 *  Resolves to the new name once the rename is written, else null — a caller
 *  holding the tag by library ORDINAL needs to know, because the library sorts
 *  by name and the renamed tag may now sit at a different index. */
export async function renameTag(s: Session, from: string): Promise<string | null> {
  const to = await s.askText(t("tags.renameTo", { name: from }), from);
  if (to === null || to.trim() === "" || to.trim() === from) return null;
  const err = await s.author("tagRename", from, to.trim());
  s.showToast(err ?? t("tags.renamed", { from, to: to.trim() }));
  return err ? null : to.trim();
}

/** Take `refKey` out of the tag named `tag`, asking first. Removing a verse from
 *  a tag is one tap to undo, but it is still the reader's data going away, so it
 *  asks the way every destructive action in the app asks (`askConfirm`,
 *  e2e/destructive.spec.ts). Resolves true once the removal is written. The
 *  passage is named the way a reader says it ("1 Corinthians 13:4"), never as
 *  the wire key. */
export async function removeVerseFromTag(s: Session, tag: string, refKey: string): Promise<boolean> {
  const passage = refDisplay(s, refKey);
  const ok = await s.askConfirm(t("tag.removeAsk", { passage, tag }), t("tag.removeBody"), t("tag.removeVerb"));
  if (!ok) return false;
  const err = await s.author("tagRemove", tag, "verse", refKey);
  s.showToast(err ?? t("tag.removed", { passage, tag }));
  return !err;
}
