# Plumbline

> And the LORD said unto me, Amos, what seest thou? And I said, A plumbline.
> Then said the Lord, Behold, I will set a plumbline in the midst of my people
> Israel…
>
> — **Amos 7:8**

**Plumbline** is a Bible-study tool: a clean parallel-passage reader with an
optional "Full study" tier of Strong's, morphology, cross-references, and
corpus analytics. Everything runs on your device and works offline — no
account, no sign-in, nothing phoned home.

![Genesis 15 and Romans 4 side by side, joined by the "Abraham believed God"
weave's connector lines](assets/readme/reader-weaves.png)

## Install

### Web — the way most people should get it

**[plumblinebible.org](https://plumblinebible.org/)** —
open it in any browser and install it from the address bar (phone or
desktop). It works offline after the first visit, and your study data lives
in browser storage.

### Android (sideload)

For unlocked phones and people comfortable sideloading: download the APK
from the [Releases page](https://github.com/glendonjklassen/plumbline/releases)
(arm64-v8a + x86_64, signed; no Play Store, no Google services required —
note Google's newer install rules can make sideloading a chore on stock
devices).

## Getting started (60 seconds)

First launch asks who's opening the Book — **new in the faith** (a welcome
with next steps, landing in John), **sharing the gospel** (straight into the
Romans Road presentation), or **established believer** (pick which analysis
layers sit beside the text: the scholars' tier and the machine tier). All of
it is switchable any time in **Settings**; the text and your own notes, tags,
and threads are always on.

Then:

1. **Read.** Tap the passage button (`John 3 ▾`) for the book → chapter →
   verse navigator, swipe (or `←`/`→`) to step chapters. The reader reopens
   exactly where you left off — mid-chapter included.
2. **Tap a word** (double-click on desktop) — the study pane opens: your
   note first, then the dictionary entry and whichever analysis tiers you
   keep on. Every claim is marked with its provenance (✝ the text ·
   † scholarship · ≈ machine).
3. **Long-press a verse** (right-click on desktop) — copy, share, note,
   highlight, **tag**, add to a thread, or memorize it.
4. **Tag as you go, weave later.** Tag passages by topic ("Rapture") over
   weeks; open the tag and hit **⇔ make weave** to chain them through the
   canon. Point two panes at linked passages and the connectors draw
   themselves.
5. **≡ menu** holds the rest: Memorize (spaced repetition), Explore (all the
   study tools, described), History, Present (hand-the-phone-across mode),
   the guide, and Settings — including **backup to a zip** that restores on
   any device.

See **[docs/GUIDE.md](docs/GUIDE.md)** for the full tour — search syntax, the
study panel tier by tier, weaves, the constellation, threads and tags.

## Shortcuts

The reading pane holds focus (click it if a dropdown steals it):

| Key | Action |
|-----|--------|
| `Up` / `Down` | scroll a few lines |
| `PageUp` / `PageDown` / `Space` | scroll nearly a page |
| `Home` / `End` | chapter start / end |
| `Left` / `Right` (or `[` / `]`) | step chapters, rolling across book boundaries |
| **`Shift`** + wheel / `Up` / `Down` / `PageUp` / `PageDown` / `Space` | **lock every pane together** (parallel reading) |
| `Ctrl` + wheel, `Ctrl` `+` / `-` | zoom the body text · `Ctrl 0` resets |
| `Ctrl`+click a word (or double-click) | open its Strong's study panel |
| `Esc` | close the study panel / any popup (clicking outside a popup also closes it) |

## Weaves — parallel passages

A **weave** ties parallel passages together (a Gospel harmony, a prophecy and
its fulfillment, an OT verse and the NT that quotes it). Links are **ambient**:
point two panes at parallel passages and any weave connecting them draws its
connector lines across the gap — no mode to enter. A verse scrolled out of view
leaves its connector pinned at the pane edge as a hint.

- **Map** — book-to-book weave density as chord ribbons.
- **Constellation** — the whole weave library, one weave per labelled lane on
  the canon backbone; page with `‹ ›` (or `Left`/`Right`), **pin** a lane
  (click its `▪`) to hold it while paging others past it, click a node to jump
  there, an edge to open the weave.

![The constellation: weave lanes over the canon backbone](assets/readme/constellation.png)

> [!NOTE]
> The weaves shipped with the app began life as **AI-generated study aids**.
> Each records an `approved` flag, surfaced in the reader; approving one (from
> its compare card) is how a parallel graduates from a study prompt to
> something you've checked against the text yourself.

## Your data

Your notes, highlights, tags, threads, weaves, and memory work are yours, and
they stay on your device. **Settings → Back up (.zip)** exports all of it, and
**Restore from backup…** loads it on any device — the archive layout is shared
between the phone and the browser, so a phone backup restores in the browser
and vice versa. Every write is atomic, so a crash or a dead battery can't
leave a half-written note behind.

## Data provenance

The KJV text (public domain) comes via eBible.org's SWORD module; Strong's via
Open Scriptures (CC-BY-SA); morphology from OSHB (CC-BY 4.0) and Robinson's
public-domain Textus Receptus tagging; cross-references from the TSK via
openbible.info. Full credits and licenses: **[BIBLIOGRAPHY.md](BIBLIOGRAPHY.md)**.
Scripture renders in EB Garamond (OFL, bundled).
