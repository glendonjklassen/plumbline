# pure-study — future work

The approved feature roadmap from the product review of 2026-07-18: what the
app grows next, beyond the in-flight architecture work
([ARCHITECTURE-REVIEW-2026-07-16.md](ARCHITECTURE-REVIEW-2026-07-16.md)) and
the features already tracked in [TODO.md](TODO.md) (Luther tagging,
allusive-book weave coverage). The product stays free — what God has given us
in the KJV is far better than a Porsche, and it is free — with a paid sync
service as the only premium piece (the workman's wages, never a feature gate).

House rules apply to every item here: shell parity in one change set (or a
logged delta in [docs/FEATURE-MANIFEST.md](docs/FEATURE-MANIFEST.md)),
additive camelCase wire, frozen formats untouched. Platforms: Linux, Windows,
macOS, Android — ARM and x86; no iOS.

## Tier 0 — daily-driver gaps (small, additive, do first)

- [ ] **1. Copy & context menu.** There is no clipboard anywhere in either
      shell. Left-click is taken (weave pinning), so: right-click a verse →
      context menu — copy verse · copy with reference (`…text — John 3:16
      (KJV)`) · copy chapter · tag · add to thread · note. Copy affordances on
      panel cards too. Formats: plain, ref-suffixed, markdown.
- [ ] **2. Back/forward history.** Study is a chain of jumps (concordance →
      verse → xref → …) with no way back. Per-pane history stack, Alt+←/→,
      mouse buttons 4/5. Everything already funnels through the link router
      (`handle_link` / `StudyPanel.Link`), so the push points exist today and
      consolidate further when the router moves into the core (P1.4).
- [ ] **3. Personal margin notes.** The 1769 translators' notes are
      first-class; the user's aren't (notes exist only on
      threads/entries/tags/weaves — [tag.rs:66](crates/core/src/tag.rs#L66)).
      Per-verse notes in a `notes/` dir sibling to `tags/` (same atomic store,
      refKey-keyed, additive endpoints), a gutter mark beside the weave dots,
      a "your notes" panel section. Also the anchor of future sync value.
- [ ] **4. Highlighting — reuse tags, add color.** Additive `color` field on
      tags; members render as a soft wash behind the verse's line rects (the
      highlight-band mechanism already computes these). A fixed, muted palette
      of 5–6 tones tuned to the paper and the dark theme. The tags browser
      doubles as the highlight browser for free.
- [ ] **5. Dark + night themes, crafted like the light one.** `ForceLight`
      today ([main.rs:414](apps/desktop/src/main.rs#L414)). A candlelight-warm
      dark paper and a true-black night mode (OLED Android), designed with the
      same care as `#fcf9f4` — not an inverted afterthought. Define the palette
      tokens once (a small table/JSON every shell embeds at build) so shells ×
      themes can't drift. Follow-system + manual toggle, persisted in config.
- [ ] **6. Kill the first-study-click pause.**
      [GUIDE.md:23-24](docs/GUIDE.md#L23-L24) documents it honestly. When
      config says Full mode, warm the analytics indexes on a background thread
      at startup; consider extending the `*.idxcache` to cover more of it.
- [ ] **7. In-app guide, shortcuts overlay, About.** GUIDE.md and
      BIBLIOGRAPHY.md are excellent and invisible — no Help affordance in
      either header. `?`/F1 shortcut overlay; the guide rendered in-app; an
      About page stating edition, provenance, and the "yours forever, no
      account, no telemetry" covenant. Fix the stale "Not yet" list in
      [apps/windows/PureStudyWin/README.md](apps/windows/PureStudyWin/README.md).
- [ ] **8. Small unifications.** Cross-book header stepping (WinUI has it,
      GTK clamps — manifest §Multi-pane): unify on cross-book. Paint **all**
      search hits in the visible chapter, not just the goto verse. Modifier-
      click a `go:` link → open in the *other* pane.

## Tier 1 — reach

- [ ] **9. Android (Compose).** The single biggest lever; already planned.
      The pack (~48 MB `data/` + `bridge/`) fits a Play asset pack;
      Simple-first onboarding matters more on phones; Kotlin binding gaps are
      manifest-tracked. Typography survives intact (core layout +
      `TextMeasurer`).
- [ ] **10. Linux packaging.** Flathub (Linux discoverability, full stop),
      AUR, AppImage — plus **ARM64 Linux** builds: a Raspberry Pi is a $70
      offline study machine (missions, low-connectivity, church labs).
- [ ] **11. Windows distribution.** `release.yml` already builds
      self-contained arm64/x64/x86 apps. Missing: **code signing** (SmartScreen
      scares exactly the non-technical people this serves — Azure Trusted
      Signing or SignPath OSS), a **winget** manifest, a **Microsoft Store**
      listing. Native ARM64 Windows is already a differentiator — say so.
- [ ] **12. A one-page website + quiet update check.** Screenshots (the
      constellation sells itself), downloads, the ethos statement. In-app: a
      manual "check for updates" against GitHub releases — no auto-update, no
      phoning home.
- [ ] **13. macOS shell.** The portable crates already build on macOS and the
      data home already resolves `~/Library/Application Support/pure-study`
      ([GUIDE.md:127-131](docs/GUIDE.md#L127-L131)) — only the shell is
      missing. SwiftUI/AppKit over the same C ABI with a CoreText-backed
      measure callback, sequenced after the view-model consolidation so shell
      #4 is paint-and-route. Developer ID signing + notarization, universal
      binary, notarized DMG + Homebrew cask (Mac App Store later if ever — its
      sandbox containerizes the file-based data home). Add a macos CI runner
      for the portable crates and a macOS delta section to the manifest.
- [ ] **14. Print & PDF export.** The core lays out via a measure callback; a
      PDF measure/paint pass gives print output typeset exactly like the
      screen, with no shell involved. Chapter handouts, large-print passages,
      memorization flashcards (#15). No Bible app, free or paid, prints
      beautifully — this architecture can, cheaply.

## Tier 2 — study differentiators

- [ ] **15. Memorization — first-letter mode + spaced repetition.**
      *Flagged top priority of the differentiators (2026-07-18).* Source = any
      tag or thread ("memorize this thread"). First-letter prompts,
      progressive blank-out, typed recall, SM-2 scheduling; printable
      flashcards once #14 lands. Include a **coverage map**: the canon
      strip/dispersion visual language reused to paint what you've spent time
      with — cells shaded by verses memorized and review depth/recency, the
      OT/NT divide marked, so a glance shows where your memory work has
      reached and where it hasn't. The scheduler's per-verse review history
      provides the data by construction. The KJV is *the* memorization text
      (homeschool, AWANA, Bible bees) and that world has no quality free tool —
      possibly the largest untapped audience. Progress lives in the "yours"
      dirs (→ sync later).
- [ ] **16. Finish grammar search; add the power tier.** `tense:aorist` is an
      honest placeholder ([README.md:108](README.md#L108)); the morphology is
      shipped and parsed. Then, all in core so every shell gets it at once:
      scope filters (`in:Psalms`, `ot:`/`nt:`, ranges); case-exact and
      **divine-name search** (FLAG_DIVINE is in the tokens); **italics search**
      (`added:` — translator-supplied words; FLAG_ADDED is in the tokens — a
      uniquely KJV study discipline with no good tool anywhere);
      boolean/NEAR; search history; saved searches.
- [ ] **17. Interlinear-lite → original-language pack.** Phase 1 needs no new
      data: an under-word toggle showing lemma/xlit/parse (strongs.json +
      morphology.jsonl are already keyed to tokens). Phase 2: optional WLC/TR
      text pack (both PD; import pipeline exists in overlay/data-prep) for a
      true reverse-interlinear — PLAN.md already notes cosmic-text gives RTL
      Hebrew for free. Offline and beautiful where the web tools are neither.
- [ ] **18. Harmony mode.** A curated Gospel-harmony weave pack (Robertson's
      *Harmony*, PD, importable) plus "follow the weave": panes align
      pericope-by-pericope as you scroll (Shift-lockstep exists; harmony mode
      locks by link pairs instead of pixels). "Read all four Gospels as one" —
      a headline feature that is ~90% built already.
- [ ] **19. People & places.**
      [bridge/stepbible-tipnr.json](bridge/stepbible-tipnr.json) already ships
      TIPNR identities; upstream TIPNR carries unique person IDs +
      relationships. A People browser and a chip on name-words: "Herod
      Antipas, tetrarch of Galilee — distinct from Herod the Great." Six
      Marys, four Herods, thirty Zechariahs — nobody free does this inline.
      Genealogies from TIPNR relations later; places + offline maps
      (openbible.info geodata, CC-BY) as a pack after that.
- [ ] **20. Corpus-wide leitwort browser.** Port overlay `Burst.hs` (PLAN.md
      marks it "later"): the per-word LEITWORT tier answers "does *this* word
      cluster?"; Burst answers it for every word at once — a browsable index
      of the canon's repeated motifs. Discovery, not just display.
- [ ] **21. Quotation/allusion detection — raise its priority.** Already on
      [TODO.md](TODO.md) (weave coverage for allusive books). Ambient
      connectors are the crown jewel and 17 books have zero weave endpoints —
      Revelation, the most allusive book in the canon, is dark. Coverage here
      *is* product quality.
- [ ] **22. Reading plans — quiet ones.** M'Cheyne, Horner, canonical,
      chronological (all PD). A chip — "Day 37 · Ps 119 ▸" — no streaks, no
      badges, no guilt mechanics. Plans live in the "yours" dirs (→ sync).
- [ ] **23. Read-aloud (TTS) with word-level highlighting.** Platform TTS
      (SAPI/OneCore, Android TTS, macOS AVSpeechSynthesizer; optional Piper
      voices on Linux) driving the existing per-word display list —
      karaoke-style highlight, chapter autoplay. Zero licensing risk, and it
      doubles as the honest answer to accessibility: canvas-drawn text is
      invisible to screen readers today. PD human audio (LibriVox KJV) as an
      optional pack later.
- [ ] **24. Command palette (Ctrl+K).** The discoverability answer: the depth
      is hidden behind Ctrl+click and small header buttons, and every action
      already routes through the URI verb table (manifest §Link routing) +
      search. A fuzzy palette over verbs/refs/actions/recents. Cheap after the
      router consolidation lands.

## Tier 3 — ecosystem & content

- [ ] **25. Weave commons.** Weaves/threads are already portable JSON. Add
      export/import affordances + a `pure-study-commons` community repo where
      PR review mirrors the in-app `approved` ethic. Ship more curated content
      in-box: 29 approved weaves and one thread (`romans-road`) today; a dozen
      excellent threads (Messianic prophecies, the Tabernacle, prayers of the
      Bible) cost an afternoon each and make first-run Full mode feel
      inhabited.
- [ ] **26. Docs & showing the depth.** The guide as a small site with GIFs
      (constellation, connectors, renderings lens) — the features are
      unphotographable in prose; motion sells them.

## The premium sync service (the only paid piece)

Scope = exactly the "yours" list ([README.md:96](README.md#L96)): weaves,
threads, tags, patches — plus personal notes (#3), plans/memorization
progress, config, reading position.

- **E2EE by default** — study notes are pastoral and private; zero-knowledge
  server.
- **Per-file version history** ("restore my library to last Tuesday") — the
  atomic single-file JSON store makes this nearly free server-side.
- **Continuity** — pane state / last position across devices (sync sells best
  once Android exists; sequence accordingly).
- **Shared libraries** — family / class / congregation spaces: a pastor
  publishes a weave library; members subscribe read-only into their
  *Suggested* queue and approve into their own library — the existing
  approval flow *is* the sharing UX, already built.
- **Read-only web publish** of a weave/thread — the invitation surface, and
  the only "web app" this product ever needs.
- **The covenant, stated on the pricing page**: local files remain canonical
  and exportable forever; sync never gates a local feature. Convenience, not
  captivity.

## Suggested sequence

1. **Tier 0 (1–8)** now, alongside the architecture work — small, additive,
   and they compound into every future screenshot and review.
2. **Android (9)** + packaging/signing/website (10–12); the macOS shell (13)
   follows once the view-model consolidation makes a fourth shell
   paint-and-route cheap.
3. Differentiators: **memorization (15) first**, then allusion coverage (21) →
   power search (16) → harmony mode (18) → print/PDF (14, which also unlocks
   15's flashcards) → interlinear-lite (17) → command palette (24) → reading
   plans (22) → TTS/a11y (23) → people (19) → leitwort browser (20).
4. **Commons (25) + the sync service** after Android ships (continuity is the
   sync product's best demo).
