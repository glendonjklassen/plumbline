# Using Plumbline

The daily-driver manual: getting around, search, the study panel tier by tier,
weaves, and where your work is kept. Install and shortcuts live in the
[README](../README.md).

## Getting around

- **Panes.** The reader shows one to four panes; `＋` / `✕` in the pane strip
  add and remove them. Each pane has its own book dropdown, chapter spinner,
  and `‹ ›` steppers, and scrolls independently. Reading two passages side by
  side is the normal way to use it. The **active** pane (gold top border —
  click a pane to activate it) is where searches and jumps land.
- **Parallel reading.** Hold `Shift` while scrolling (wheel or keys) to lock
  every pane together.
- **Canon strip.** The band along the bottom is the whole Bible front to back,
  banded by section (Law · History · Wisdom · Prophets ∣ Gospels · Acts ·
  Letters · Revelation) with the OT/NT divide marked and a pin per pane —
  click anywhere on it to send the active pane there.
- **1769 notes.** The margin notes of the 1769 edition (literal Hebrew
  renderings, variants) appear in the study panel for verses that have them.
- **Simple ⇄ Full study.** The header button flips modes any time. Simple hides
  the entire study surface; Full shows it. The first double-click in Full
  study builds the analytics indexes — expect a short pause once per launch.

## Search

Type into the header search box:

| Query | What happens |
|-------|--------------|
| `love` | single word, answered in ranked tiers: exact matches, then morphological variants (*loveth*, *loved*), then other renderings of the same Strong's lemma, then near spellings — each tier labelled |
| `God so loved` | phrase match; when no verse has the exact phrase, verses containing every word are offered instead, plainly labelled |
| `John 3:16`, `1 Cor 13`, `psalms` | reference jump — OSIS id, display name, or any unambiguous prefix; with a verse it scrolls there and highlights |
| `H430`, `G26` | a bare Strong's code lists every verse tagged with it |

Results cap at 200 with an honest total. Click a result to open it in the
active pane. Form-predicate queries (`tense:aorist voice:passive`) are **not
yet implemented** — the box will say so rather than guess.

## The study panel

Hovering any word underlines it if it carries a Strong's tag and shows a quick
gloss. **`Ctrl`+click** (or double-click) opens the full panel. What you see,
top to bottom — and how much weight to give each tier:

**Dictionary tier (facts about the word):**

- **The Strong's entry** — lemma, transliteration, pronunciation, definition,
  and the KJV's own renderings, with an *N occurrences ▸* link that pages
  through every verse carrying the code.
- **Morphology** — the original-language parse of the exact word you clicked
  (tense, voice, mood, case…), when the sidecar annotates it.

**Analytics tier (statistics over the corpus — study prompts, not authorities):**

- **SAME ROOT ACROSS TESTAMENTS** — cross-language cognates: Strong's own
  etymology fused with external witnesses (Septuagint alignment, Abbott-Smith,
  proper-name links), each chip naming its sources. *“disputed by usage”*
  marks a link a self-trained text model disbelieves — the text's own usage
  pushing back on the tradition.
- **APPEARS ALONGSIDE** — the collocation community: concepts that share this
  one's verses.
- **MOST USED IN** — top books by occurrence count (`Genesis ×12`) and the
  OT/NT split.
- **LEITWORT** — flagged when the word's occurrences pack into one stretch far
  denser than chance (a repeated motif), with the span and how many of its
  uses cluster there.

Removed 2026-07-30, all three machine-generated from the concept embedding and
judged to be noise: SIMILAR CONCEPTS, the radial concept map, and "verses like
this". The embedding artifact no longer ships.

**Curated tier:**

- **cross-references** — weave links touching this verse (yours and shipped).
- **study cross-references (TSK)** — the Treasury of Scripture Knowledge's
  ~343,000 curated references, best-voted first. Clearly labelled; never
  blessed into weaves automatically.

Plus **＋ tag verse / ＋ add to thread** authoring links, your **tags** on the
verse, and the verse's **1769 margin notes**.

## Weaves

A weave is a set of verse-to-verse links tagged with a kind (`retelling`,
`type`, `prophecy`, `quotation`). Because it's a graph, a verse mapping to two
others is just two links, and combining weaves is a union.

- **Ambient connectors.** Two panes showing linked passages draw gold curves
  across the gutter between them, at each verse's height. A linked verse
  scrolled off-screen keeps a dot pinned at the pane's top/bottom edge — a
  hint you can scroll it into view.
- **Authoring.** Single-click a word to pin it (click another word in the same
  verse to widen the span); pin a word in a second pane, and **＋ link** in
  the header weaves the two spans. The link lands in a weave the verses
  already belong to, or a new one.
- **Compare card.** Click a weave's name anywhere to see its kind, notes, and
  every link with the linked words emphasized — with **✓ approve / ✕ reject /
  ✎ note** actions.
- **Suggested.** The review queue of proposed weaves. Approve to keep
  (moves into `weaves/`), reject to discard. Shipped suggestions are
  AI-generated study aids — nothing enters your approved library unreviewed.
- **Map.** Book-to-book weave density as chord ribbons; click a book to jump.
- **Constellation.** The library at a glance: one weave per labelled lane,
  largest first, nodes on the canon backbone, links as curves. `‹ ›` or
  `Left`/`Right` page the lanes; click the `▪` in the left gutter to **pin** a
  lane so it stays put while you page others past it (pin the crucifixion,
  page the Gospels past it). Click a node to jump the active pane there; click
  an edge to open that weave's compare card. Node size reflects how many links
  across the whole library touch that verse.

## Threads & tags

**A weave is something you find. A thread is something you make.** Weaves are
already in the Bible — the same event in three Gospels, a prophecy and its
fulfilment — and you write down what you noticed. A thread is passages you
gathered because *you* had a reason: a sermon, a lesson, an argument. The order
matters, because the order is your point.

- **Threads** are ordered trails of passages. *Add to thread…* from the verse
  menu opens a picker: tap a thread you already have, or name a new one (a
  freetext-only prompt made you retype an existing name exactly, and a typo
  forked a second thread). `✕` deletes a thread and everything on it. Entries
  carry their own notes, and the thread has a running notes document.
- **Tags** are flat labels on verses — same picker idiom, existing tags first.
  The slow way a topic accumulates: tag a verse each time you run into the idea
  again, and the tag becomes the list you wished you had kept.

> [!NOTE]
> Highlighting was removed in v0.33.0. Tags, notes and threads all say *why* a
> verse matters to you; a colour only says *that* it does, and three ways to mark
> a verse was two too many. Nothing you tagged or wrote was touched.

## The reading map

The book and chapter grids in **Go to…** tint themselves by how you have read:
gold for never, copper for partway, sage for read through. A book takes the
word-weighted blend of its chapters, so the chapters always add up to it.

On top of the hue, a chapter **glows** — and the glow says one of two things. On
something you have read, it means *you have been away a while*: nothing for the
first month, building to full at a year. On something you have never read, it is
lit from the moment you install, because that is treasure you have not opened yet.

Whichever it is, **being there recently silences it.** A chapter you read this
morning says nothing, whether or not you got to the end of it — the map's question
is where you have *not* been lately, and you were just there. The glow is an
invitation, never a scolding, and nobody sees it but you.

Reading is counted generously: a chapter fills as you move through it at a natural
pace (flipping past credits nothing, and neither does leaving it open), and 90%
counts as read through. Read on paper? Long-press a chapter's first verse and
choose **Mark chapter read…** to set the date yourself.

It all travels in the backup zip (`reading/`), and nobody but you ever sees it.

## Where everything lives

The data home resolves in this order: `$PLUMBLINE_HOME` / `$OVERLAY_HOME` →
the working directory tree (a checkout counts) → the executable's directory →
the per-user data dir (`~/.local/share/plumbline` on Linux,
`%APPDATA%\plumbline` on Windows, `~/Library/Application Support/plumbline`
on macOS). The window title's tooltip and the first line of `plumbline-hydrate
check` both print the resolved home.

Inside it, **yours** (back these up): `weaves/`, `threads/`, `tags/`, `notes/`,
`memory/`, `reading/`. Shipped/regenerable: `data/`, `bridge/`, and the `*.idxcache`
startup cache. Config (mode, text size, open panes) is separate, in the
platform config dir (`~/.config/plumbline/config.json` on Linux). Every write
is atomic; a corrupt or missing optional file degrades its feature, never the
reader.
