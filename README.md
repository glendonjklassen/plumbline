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

![Isaiah 2 and Micah 4 open side by side, joined by weave connector lines,
with the study panel on "treasures" — Strong's H214, its morphology, every
KJV rendering, and where the concept concentrates across the
canon](assets/readme/reader-weaves.png)

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
with next steps, landing in John), **curious about the Bible** (for someone
still deciding what they believe), **sharing the gospel** (straight into the
Romans Road presentation), or **established believer** (set up study and
memorization, and pick which analysis layers sit beside the text). All of it
is switchable any time in **Settings**; the text and your own notes, tags,
and threads are always on. Whichever welcome you were given stays one tap
away afterwards, under **Welcome** in the top bar.

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

## Share the Bible — and your church — with one code

Put your church in **Settings → Your church** (name, when and where you meet,
your website) and it travels with everything you share. **Share** in the top
bar gives a QR and a link that carry both: whoever scans it gets the whole
Bible, offline and free, *and* your church's details saved on their device —
with a **Church** button in their top bar that opens your website. If they
pass the app on, your church goes with it.

Hand a card across at a service, print the code on a bulletin, or text the
link. Nothing is registered anywhere: the details ride in the link itself
(`?church=…`), so you can read exactly what you are sending before you send
it, and there is no account or server involved on either end.

Sharing from **Present** — the hand-the-phone-across mode — is the same code
with one difference: it opens for someone meeting the Bible rather than
setting up a study tool, offering just *new in the faith* or *curious about
the Bible*. Turn that off in Settings if you'd rather it behave like an
ordinary share.

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

## Translation selection

This application is built with the 1769 King James Version of the Bible, which
translates the Masoretic Hebrew texts and the Textus Receptus. This is the same
version that has been used for hundreds of years in English and has seen
millions of souls saved and lives transformed. It is also the only version in
English that satisfies the Bible's own promises about the preservation of
Scripture (Psalm 12:6–7; Psalm 119:89, 152, 160; Proverbs 30:5–6; Isaiah 40:8;
Matthew 5:18; Matthew 24:35; Luke 21:33; 1 Peter 1:23–25; Revelation 22:18–19).
It is easily the most beautiful translation in English. It is also **public
domain**.

However, even if I did not want to use this version, it's worth noting that I
could not use any of the modern versions that so many churches use today. Below
is a sampling of why that is the case:

| Version | Who holds it | The term that blocks an app like this |
|---|---|---|
| **NIV** | Biblica; North American commercial rights to HarperCollins Christian Publishing (News Corp) | 500 verses / 25% of the work / never a complete book. Beyond that, a negotiated commercial licence. Separately: *any* use "in connection with artificial intelligence, machine learning, large language models, chatbots, or similar technologies requires a valid license." [Terms](https://www.biblica.com/permissions/) |
| **ESV** | Crossway | 500 verses / half a book / 25%. And: "The ESV text may not be quoted in any publication made available to the public by a Creative Commons license." Their API is online-only, non-commercial, and licensed "to organizations, not to individuals or solo developers." [Terms](https://www.esv.org/about/terms/) · [Permissions](https://www.crossway.org/permissions/) |
| **NKJV** | Thomas Nelson → HarperCollins Christian → News Corp | 500 verses / 25%; the text is "not to be reproduced except as permitted in writing." [Permissions](https://www.harpercollinschristian.com/sales-and-rights/permissions/) |
| **NASB** | The Lockman Foundation | 1,000 verses / 50% / never a complete book; beyond that, a signed request form. [Terms](https://www.lockman.org/permission-to-quote-copyright-trademark-information/) |
| **Amplified** | The Lockman Foundation | Same terms as the NASB. [Terms](https://www.lockman.org/permission-to-quote-copyright-trademark-information/) |
| **NLT** | Tyndale House Publishers | 500 verses / 25% / never a complete book. [Permissions](https://www.tyndale.com/permissions) |
| **CSB** | Holman Bible Publishers / Lifeway | 1,000 verses / 50% / never a complete book. [Permissions](https://csbible.com/permissions/) |
| **The Message** | Eugene Peterson estate / NavPress, licensed through Tyndale | 500 verses / 25%. [Permissions](https://www.tyndale.com/permissions) |
| **NRSV** | National Council of Churches, via Friendship Press | Closed. Except for the Catholic Edition, the NRSV "is no longer available for new licenses or permission agreements." There is no price at which you may ship it. [Licensing](https://friendship-press.org/bible-licensing/) |
| **NRSVue** | National Council of Churches; rights brokered by Petradi International Rights Services | Licence required, routed through an outside rights-management firm. [Guidelines](https://www.friendshippress.org/pages/nrsvue-quick-faq) |
| **RSV** | National Council of Churches | Licence required, same channel. [Licensing](https://friendship-press.org/bible-licensing/) |
| **Good News (GNT)** | American Bible Society | 500 verses / 50% of a book / 25% of the work. [Rights](https://www.americanbible.org/rights-and-permissions/) |
| **CEV** | American Bible Society | Same terms. [Licensing](https://cev.bible/licensing/) |
| **NABRE** | Confraternity of Christian Doctrine (the US bishops) | Under 5,000 words / 40% of a book / 40% of the work. Royalty income is an explicit purpose of the copyright. [Permissions](https://www.usccb.org/offices/new-american-bible/permissions) |
| **NET** | Biblical Studies Press | The near miss: 500 verses free, but distributing the full text "in any form other than paper" requires written permission *and* compliance with their content-control guidelines. [Permissions](https://bible.org/permissions) |

It is not lost on me, nor should it be lost on you, that five publishers hold
the modern English Bibles anyone actually reads — HarperCollins Christian
Publishing (the NIV in North America, and the NKJV), Crossway (the ESV), the
Lockman Foundation (the NASB, the Amplified), Tyndale House (the NLT, The
Message), and Holman/Lifeway (the CSB). Two of those are the same company: the
best-selling English Bible in the world and the runner-up both answer to News
Corp. The rest of the table is held by church bodies rather than corporations —
the National Council of Churches, the American Bible Society, the US bishops —
and not one of them would permit me to make an application like this to bolster
my ability to study and share the Word.

A worthy exercise I leave with the reader is to click on any one of the license
agreements above and see how it reads when you replace the name of their
intellectual property with "The Word of God". It did not sit well with me when
I did.

## Final note

May God bless you as you read his Word, whether here or elsewhere, and don't
fail to share it with others.
