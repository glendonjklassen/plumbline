# Plumbline

> And the LORD said unto me, Amos, what seest thou? And I said, A plumbline.
> Then said the Lord, Behold, I will set a plumbline in the midst of my people
> Israel…
>
> — **Amos 7:8**

**Plumbline** is a Bible-study tool: a clean parallel-passage reader with two
optional layers of analysis beside the text — **Scholars'** (Strong's, word
grammar, the same root traced across the testaments, the Treasury's
cross-references) and **Machine** (statistical patterns).
Everything runs on your device and works offline. No account and no sign-in,
and your data remains local and private.

Available in **English and German**, on both the web and Android — interface and
scripture. It follows your device's language, and you can pick one under
Settings. English reads the King James Version; German reads the Luther 1912,
both public domain. The German interface is machine translated for now, so
expect it to read a little stiff in places. Word study (Strong's, grammar) is
English only, because those are tied to the King James text.

![Isaiah 2 and Micah 4 open side by side, joined by weave connector lines,
with the study panel on "treasures" — Strong's H214, its morphology, every
KJV rendering, and where the concept concentrates across the
canon](assets/readme/reader-weaves.png)

## Features

### Reading

- Read passages side-by-side in a browser or on a tablet
- See parallel passages annotated across side-by-side chapters
- Saves progress from previous reading and across time
- Book → chapter → verse tap-grid navigator with an indicator for what you have read recently
- Theme, text size, alignment, and font customization
- Optional AKJV Plain-English overlay for easier reading
- German language support
- Canon strip "minimap" under the reader to show where you're at in the Bible

### Study

- Add notes to words/verses
- Tag verses with concepts as you study to accumulate verses for sermons or studies
- Strong's dictionary, with the ability to see other renderings of the word across scriptures
- Concordance per code, and filtered to a single rendering.
- Same root across the testaments — cross-testament bridge partners to see where words show up across the entire Bible
- The 1769 translators' own margin notes.
- Advanced search including partial matches and original language word matches

### Your own work

- Personal notes on any verse, marked in the gutter.
- Tags, with a picker that offers what you already have.
- Threads: a passage list built over time, with a note per entry and a snapshot
  of the verse text.
- Weaves: your own verse↔verse link graph, with notes.
- Convert a tag into a weave as you study
- Visualizations of your weaves: a chord diagram across the canon, and a
  constellation with pinnable lanes.
- A notes browser over everything you have written.

### Practice

- Spaced repetition memorization (SM-2) for single verses or whole passages, including three drills: first letters, progressive blank-out, typed recall scored word
  by word.
- Reading coverage map across the canon, and an activity heatmap with a history log.
- Reading map: coverage gated both by how far you reached and how long you
  spent
- Chapters you have not been in lately glow, ramping from 30 days to a year.
- Mark a chapter or a whole book read by hand, on a date you pick.
- Hymnal: public-domain hymns with chords the engine transposes, per-language
  texts, and a sing mode that scrolls continuously.

### Sharing with others

- Present mode for sharing the Gospel or sermons: a thread as a fullscreen, high-contrast, large-type
  presentation, with tap-to-focus and an "in context" fade.
- Share is a QR that puts the passage on the other person's phone.
- Your church information (time, location, website) in every link you share, and lands as a button in their top bar.

### Application Features

- Free. No account, no sign-in, no Play Store, no telemetry, no paid tier. Works offline.
- All user data can be backed up and transferred via zip files.

## Install

### Web

**[plumblinebible.org](https://plumblinebible.org/)** —
open it in any browser and install it from the address bar (phone or
desktop). It works offline after the first visit, and your study data lives
in browser storage.

### Android

No Play Store, no Google account: you download one file and open it. Four
steps, all on the phone.

1. **Download it.** Open the
   [Releases page](https://github.com/glendonjklassen/plumbline/releases) on the
   phone and, under **Assets** on the newest release, tap
   `plumbline-<version>-android.apk` (about 20 MB). Chrome will warn that a file
   of this type "can harm your device" — it says that about every APK anyone has
   ever downloaded. Keep it.
2. **Open the file.** Tap it in the download notification, or find it later
   under **Files → Downloads** and tap it there.
3. **Let Android install it.** The first time, Android stops you: your phone
   "is not allowed to install unknown apps from this source." Tap **Settings**
   in that message, turn on **Allow from this source**, then press Back — you
   land back on the installer. Tap **Install**.
4. **Get past Play Protect, if it shows up.** Some phones offer to scan the app
   first, or say **Unsafe app blocked**, because Google has never seen this file
   before. Choose **More details → Install anyway** (or **Don't send**). Then
   open **Plumbline** from your app drawer: the whole Bible is inside the file
   you just installed, so it opens with the network off.

Needs Android 8.0 or newer and a 64-bit phone (`arm64-v8a`). It declares **no
INTERNET permission at all**, so it cannot phone home even by accident. One study layer rides in the
web version only, because the APK doesn't carry its data file: the word-grammar
gloss. Everything else is in the APK — the
text, Strong's, the margin notes, the Treasury's cross-references, the
cross-testament bridge, the plain-English overlay, and all of your own work.

**What sideloading means.** You are the one deciding to trust this file, which
is why Android keeps asking. Two things follow. There is no auto-update — when
a new version ships you come back to Releases and repeat the four steps; it
installs over the old one and your notes, tags and threads stay where they are,
because every release is signed with the same key. And take the APK only from
that Releases page: a file called `plumbline-….apk` from anywhere else is not
something I built. If any of this stalls on your phone, the web version above
needs none of it.

**Checking what you downloaded** (optional). GitHub records a SHA-256 for every
file attached to a release. Ask it for the APK's, hash your own copy, and
compare:

```sh
curl -s https://api.github.com/repos/glendonjklassen/plumbline/releases/latest \
  | grep -o 'sha256:[0-9a-f]\{64\}'
sha256sum plumbline-v*-android.apk
```

(`shasum -a 256` on macOS, `Get-FileHash` in Windows PowerShell.) The two hex
strings have to match character for character.

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
3. **Long-press a verse** (right-click on desktop) — copy, share, write a
   note, **tag** it, add it to a thread, or memorize it.
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

## Threads and weaves

**A weave is something you find. A thread is something you make.**

Weaves are already in the Bible. When Matthew quotes Isaiah, when the same
event is told in three Gospels, when a Psalm turns up on the lips of Christ at
the cross — those connections were put there. You didn't invent them, you
noticed them. A weave is how you write down what you noticed, and once it's
written down the app draws the connection for you every time you're reading
either end of it. Nobody owns a weave: two people studying carefully will find
the same ones.

Threads are yours. A thread is a set of passages you gathered because *you* had
a reason — the sermon you're preaching Sunday, the case you want to walk a
friend through, a question you're chasing across the canon. The order matters,
because the order is your argument. The Romans Road is a thread: 3:23, then
6:23, then 5:8. It's a thread precisely *because* Paul didn't write it that way
— someone assembled it, on purpose, to explain the gospel to someone else.

So: **if the connection is in the text, weave it. If the connection is your
point, thread it.** Weaves are ambient and unordered, and you'll stumble back
into them for years. Threads are walked start to finish, and they carry your
notes as you go.

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
> The weaves shipped with the app began life as **AI-generated study aids**. The
> 28 that ship in the library arrive with every link marked `approved`, and the
> flag is surfaced in the reader; anything sitting in **Suggested** (under
> Explore) is still a proposal. Approving one from its compare card is what moves
> it into your library — a parallel graduating from a study prompt to something
> you have checked against the text yourself.

## Your data

Your notes, tags, threads, weaves, reading history and memory work are yours, and
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

## License

The code is MIT — **[LICENSE](LICENSE)**. The text and the lexical data keep
their own terms (public domain, CC-BY 4.0, CC-BY-SA); the carve-out at the end
of LICENSE names them directory by directory, and
[BIBLIOGRAPHY.md](BIBLIOGRAPHY.md) has it file by file.

## Translation selection

This application is built with the 1769 King James Version of the Bible, which
translates the Masoretic Hebrew texts and the Textus Receptus. This is the same
version that has been used for hundreds of years in English and has seen
millions of souls saved and lives transformed. I argue that it is also the only
version in English that satisfies the Bible's own promises about the
preservation of Scripture (Psalm 12:6–7; Psalm 119:89, 152, 160; Proverbs
30:5–6; Isaiah 40:8; Matthew 5:18; Matthew 24:35; Luke 21:33; 1 Peter 1:23–25;
Revelation 22:18–19).

If I did not want to use this version, I could not use any of the modern
versions that so many churches use today. Below is a sampling of why that is
the case:

| Version | Who holds it | The term that blocks an app like this |
|---|---|---|
| **NIV** | Biblica; North American commercial rights to HarperCollins Christian Publishing (News Corp) | 500 verses / 25% of the work / never a complete book. Beyond that, a negotiated commercial license. Separately: *any* use "in connection with artificial intelligence, machine learning, large language models, chatbots, or similar technologies requires a valid license." [Terms](https://www.biblica.com/permissions/) |
| **ESV** | Crossway | 500 verses / half a book / 25%. And: "The ESV text may not be quoted in any publication made available to the public by a Creative Commons license." Their API is online-only, non-commercial, and licensed "to organizations, not to individuals or solo developers." [Terms](https://www.esv.org/about/terms/) · [Permissions](https://www.crossway.org/permissions/) |
| **NKJV** | Thomas Nelson → HarperCollins Christian → News Corp | 500 verses / 25%; the text is "not to be reproduced except as permitted in writing." [Permissions](https://www.harpercollinschristian.com/sales-and-rights/permissions/) |
| **NASB** | The Lockman Foundation | 1,000 verses / 50% / never a complete book; beyond that, a signed request form. [Terms](https://www.lockman.org/permission-to-quote-copyright-trademark-information/) |
| **Amplified** | The Lockman Foundation | Same terms as the NASB. [Terms](https://www.lockman.org/permission-to-quote-copyright-trademark-information/) |
| **NLT** | Tyndale House Publishers | 500 verses / 25% / never a complete book. [Permissions](https://www.tyndale.com/permissions) |
| **CSB** | Holman Bible Publishers / Lifeway | 1,000 verses / 50% / never a complete book. [Permissions](https://csbible.com/permissions/) |
| **The Message** | Eugene Peterson estate / NavPress, licensed through Tyndale | 500 verses / 25%. [Permissions](https://www.tyndale.com/permissions) |
| **NRSV** | National Council of Churches, via Friendship Press | Closed. Except for the Catholic Edition, the NRSV "is no longer available for new licenses or permission agreements." There is no price at which you may ship it. [Licensing](https://friendship-press.org/bible-licensing/) |
| **NRSVue** | National Council of Churches; rights brokered by Petradi International Rights Services | License required, routed through an outside rights-management firm. [Guidelines](https://www.friendshippress.org/pages/nrsvue-quick-faq) |
| **RSV** | National Council of Churches | License required, same channel. [Licensing](https://friendship-press.org/bible-licensing/) |
| **Good News (GNT)** | American Bible Society | 500 verses / 50% of a book / 25% of the work. [Rights](https://www.americanbible.org/rights-and-permissions/) |
| **CEV** | American Bible Society | Same terms. [Licensing](https://cev.bible/licensing/) |
| **NABRE** | Confraternity of Christian Doctrine (the US bishops) | Under 5,000 words / 40% of a book / 40% of the work. Royalty income is an explicit purpose of the copyright. [Permissions](https://www.usccb.org/offices/new-american-bible/permissions) |
| **NET** | Biblical Studies Press | The near miss: 500 verses free, but distributing the full text "in any form other than paper" requires written permission *and* compliance with their content-control guidelines. [Permissions](https://bible.org/permissions) |

A worthy exercise I leave with the reader is to click on any one of the license
agreements above and see how it reads when you replace the name of their
intellectual property with "The Word of God". It did not sit well with me when
I did.

For these reasons, I have chosen to use the King James Version of the Bible.
I have included Strong's Concordance and other study tools to help where a word
might be unfamiliar, and if it remains a challenge for you, there are other
Bible applications that will provide some of the versions listed above. I do,
however, implore you to consider your translation selection carefully, as many
parts of Scripture have been omitted or changed in modern versions, namely:

- Matthew 17:21
- Matthew 18:11
- Matthew 23:14
- Mark 7:16
- Mark 9:44
- Mark 9:46
- Mark 11:26
- Mark 15:28
- Luke 17:36
- Luke 23:17
- John 5:4
- Acts 8:37
- Acts 15:34
- Acts 24:7
- Acts 28:29
- Romans 16:24

And the following passages are bracketed, footnoted, or marked as later
additions:

- Matthew 6:13b
- Mark 16:9–20
- Luke 22:43–44
- Luke 23:34a
- John 7:53–8:11
- 1 John 5:7–8

## Final note

May God bless you as you read his Word, whether here or elsewhere, and don't
fail to share it with others.
