// What a reader is shown when the app names a verse.
//
// The refKey ("1Cor 13:4") is the frozen wire form: it is what goes to the
// engine, what sits inside stored study files, and what rides in a `?at=` share
// link. None of that changes here. This module changes only what gets PAINTED,
// because reader-facing copy was reading out the OSIS id — "Tag 1Cor 13:4" in
// the tag sheet, "Memorizing Ps 23:1" in a toast — while Android has named books
// in full since it shipped (Memorize.kt swaps the id for the TOC's name,
// ui/Notes.kt paints the wire's own `display`). Android is the UX gold standard,
// so the web follows it.
//
// The name comes off the wire, never from a table in here: `session.bookName`
// reads the TOC prefetched at boot, whose names ARE `canon::BOOKS[].name` — the
// same strings `VRef::display()` composes. A second copy of 66 book names in
// TypeScript is a second thing to get wrong.

import type { Session } from "../state/session.svelte";

/**
 * `"1Cor 13:4"` → `"1 Corinthians 13:4"`. A passage label passes through the
 * same call: `"Ps 23:1–6"` → `"Psalms 23:1–6"`.
 *
 * Split on the LAST space, which is the core's own rule (`VRef::parse_ref_key`
 * does `rsplit_once(' ')`). Splitting on the FIRST space would be right for the
 * ids the corpus ships — every one is a single word — and wrong the moment this
 * function is handed something it already translated, since a third of the canon
 * has a space in its display name ("1 John 3:16"). Everything after the book
 * travels untouched, which is what lets a verse range through unharmed.
 *
 * Anything that is not ref-shaped comes back unchanged, and the tail has to
 * start with a digit for that reason: a tag or thread NAME can also end in a
 * word ("Romans Road"), and mangling one into a book lookup would be worse than
 * printing it as it is. This is display sugar, never a validator.
 */
export function refDisplay(s: Session, ref: string): string {
  const sp = ref.lastIndexOf(" ");
  if (sp <= 0 || !/^\d/.test(ref.slice(sp + 1))) return ref;
  return `${s.bookName(ref.slice(0, sp))} ${ref.slice(sp + 1)}`;
}
