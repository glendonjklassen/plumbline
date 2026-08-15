// The top-level shell: icon chrome (search + an overflow menu) over a fold-aware
// arrangement of the reader. The Android mirror of apps/windows/PureStudyWin/
// MainWindow.cs, adapted to a touch phone (the v1 phone shell):
//
//   UiMode.FullscreenVertical  a phone: ONE fullscreen reading pane. The book
//                              nav lives inline in the top bar; study, search,
//                              and libraries surface as a dismissible bottom
//                              sheet / full-screen overlay on demand — never a
//                              permanent split with a toggle button.
//   UiMode.FoldFullscreen      device opened flat: a Row of two panes; the
//                              second is a second Bible or the study pane
//                              (toggled from the overflow menu), split at the hinge.
//
// The shell leans on icons over text for chrome (search, overflow, chapter
// arrows). All study logic lives across the ABI — this is orchestration only.
//
// Author D (Compose UI).

package dev.plumbline.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.clickable
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowLeft
import androidx.compose.material.icons.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.window.layout.FoldingFeature
import dev.plumbline.AkjvToken
import dev.plumbline.ChurchState
import dev.plumbline.Hit
import android.widget.Toast
import dev.plumbline.PaneRef1
import dev.plumbline.Tags
import dev.plumbline.Thread1
import dev.plumbline.Threads
import dev.plumbline.PanelLinkData
import dev.plumbline.SearchResult
import dev.plumbline.StudyEngine
import dev.plumbline.Toc
import dev.plumbline.TocBook
import dev.plumbline.MemoryDue
import dev.plumbline.ReadingBooks
import dev.plumbline.UserNote
import dev.plumbline.UserNotes
import dev.plumbline.WeaveLib
import dev.plumbline.WeaveLink1
import dev.plumbline.ConfigState
import dev.plumbline.PlumblineJson
import dev.plumbline.StudyConfig
import dev.plumbline.parseWire
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString

/** What the second (right) pane shows in fold mode. */
private enum class SecondPane { Study, Bible }

/** The bottom-nav ROLES (one-handed reach): Read · Study · Preach · Share ·
 *  Sing. `Explore` is the Study hub's internal name and `Memorize` a screen the
 *  hub opens (it lights the Study tab); Preach is its own hub (Present + the
 *  sermon's materials), and Present raised from it stays a fullscreen overlay
 *  that lights the same tab. */
private enum class Dest { Read, Explore, Memorize, Preach, Hymnal, Share }

/** A study "library" the Explore screen loads into the study surface as blocks. */
enum class Library { Threads, Tags, Weaves, Suggested, Guide, About }

/** A one-field text-input authoring dialog (add tag / add thread / edit note). */
private data class AuthorPrompt(val title: String, val initial: String, val onConfirm: (String) -> Unit)

/** One word tap's whole answer, gathered in a single background read: the study
 *  card, and what the plain-English overlay did to the tapped word. */
private class WordStudy(val blocks: String?, val akjv: AkjvToken?)

/** One weave tap's whole answer: where the reader lands, and the compare card. */
private class WeaveOpened(val opening: WeaveOpening?, val blocks: String?)

/**
 * The app root. Resolves a palette from the current theme and mounts [StudyScreen].
 * [fold] is the live FoldingFeature (null when the device is not opened flat).
 */
@Composable
fun PlumblineApp(
    engine: StudyEngine,
    fold: FoldingFeature?,
    bundledOn: Boolean = true,
    onToggleBundled: () -> Unit = {},
    onLanguage: (String) -> Unit = {},
) {
    // The reader's theme choice, persisted in the shared config
    // ("system" | "light" | "dark" | "night"); System follows the OS.
    // Read ONCE, together: three independent axes off one config parse — colour,
    // the scripture face and the chrome face (core::font). Nothing here derives
    // one from another; every combination is legal.
    val bootConfig = remember { runCatching { parseWire<ConfigState>(StudyConfig.LoadJson()) }.getOrNull() }
    var themeChoice by remember { mutableStateOf(bootConfig?.theme ?: "system") }
    var textFont by remember { mutableStateOf(fontFor(bootConfig?.textFont).token) }
    var chromeFont by remember { mutableStateOf(fontFor(bootConfig?.chromeFont).token) }
    val systemDark = isSystemInDarkTheme()
    val resolved = if (themeChoice == "system") (if (systemDark) "dark" else "light") else themeChoice
    val palette = remember(resolved) { ReaderPalette.forTheme(resolved) }
    val scheme = if (palette.dark) {
        darkColorScheme(background = palette.paper, surface = palette.paper, onSurface = palette.ink)
    } else {
        lightColorScheme(background = palette.paper, surface = palette.paper, onSurface = palette.ink)
    }
    // The CHROME face paints the type scale; the SCRIPTURE face is published to
    // the reader, Present, Memorize and the maps through LocalTextFont. Two
    // axes, neither one the other's default (see ui/Fonts.kt).
    MaterialTheme(colorScheme = scheme, typography = rememberSerifTypography(chromeFont)) {
      CompositionLocalProvider(LocalTextFont provides textFont) {
        // NAMED, not positional: a defaulted parameter (like `onLanguage`, which
        // carries a `{}` default) is silently dropped if a positional call omits
        // it — the compiler says nothing and the picker calls the no-op. Every
        // argument named means it cannot be forgotten.
        StudyScreen(
            engine = engine,
            fold = fold,
            palette = palette,
            themeChoice = themeChoice,
            onThemeChoice = { themeChoice = it },
            textFont = textFont,
            onTextFont = { textFont = it },
            chromeFont = chromeFont,
            onChromeFont = { chromeFont = it },
            bundledOn = bundledOn,
            onToggleBundled = onToggleBundled,
            onLanguage = onLanguage,
        )
      }
    }
}

@Composable
fun StudyScreen(
    engine: StudyEngine,
    fold: FoldingFeature?,
    palette: ReaderPalette,
    themeChoice: String = "system",
    onThemeChoice: (String) -> Unit = {},
    textFont: String = DEFAULT_FONT.token,
    onTextFont: (String) -> Unit = {},
    chromeFont: String = DEFAULT_FONT.token,
    onChromeFont: (String) -> Unit = {},
    bundledOn: Boolean = true,
    onToggleBundled: () -> Unit = {},
    onLanguage: (String) -> Unit = {},
) {
    val toc = remember {
        runCatching { parseWire<Toc>(engine.TocJson()).books }.getOrElse { emptyList() }
    }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    // Who owns the study surface right now — see [StudyTurns] and [engineCall] at
    // the foot of this file. Every engine call the reader's taps make goes through
    // them, so a second tap can never be repainted by the first.
    val turns = remember { StudyTurns() }

    // Persisted reader config (shared, cross-shell): last-viewed passage, reader
    // prefs, and reading history all restore from it.
    val loadedCfg = remember { runCatching { parseWire<ConfigState>(StudyConfig.LoadJson()) }.getOrNull() }
    val plainLast = loadedCfg?.openPanes
        ?.getOrNull(loadedCfg.activePane.coerceAtLeast(0))
        ?: loadedCfg?.openPanes?.firstOrNull()

    // WHICH SEATING is this? A reader's last chapter is not one thing: somebody
    // who studies on weekday mornings, sits in a Sunday service and goes to a
    // Wednesday meeting has three separate places they were, and one "last
    // chapter" serves whichever they did most recently — so arriving at church
    // reopened Saturday night's study (maintainer, 2026-08-13).
    //
    // Resolved ONCE per launch, synchronously, because the pane below is built
    // from it: this is a string comparison in the core, not a file read. The
    // date and hour are LOCAL — a slot computed in UTC would put a Sunday
    // evening service in Monday for half the world.
    val sessionSlot = remember {
        val now = java.util.Calendar.getInstance()
        val date = String.format(
            java.util.Locale.US,
            "%04d-%02d-%02d",
            now.get(java.util.Calendar.YEAR),
            now.get(java.util.Calendar.MONTH) + 1,
            now.get(java.util.Calendar.DAY_OF_MONTH),
        )
        runCatching { StudyEngine.SessionSlot(date, now.get(java.util.Calendar.HOUR_OF_DAY)) }
            .getOrNull() ?: "other"
    }
    // This seating's own position wins; a seating never used falls through to
    // the plain last position, which is what every reader has today.
    val lastPane = loadedCfg?.slots?.get(sessionSlot) ?: plainLast

    // Primary Bible pane — restore where we left off, else John 3 (desktop default).
    var book by remember { mutableStateOf(lastPane?.book ?: "John") }
    var chapter by remember { mutableStateOf(lastPane?.chapter?.takeIf { it > 0 } ?: 3) }
    // Second Bible pane state (for Bible∥Bible in fold mode).
    var secondBook by remember { mutableStateOf(lastPane?.book ?: "John") }
    var secondChapter by remember { mutableStateOf(lastPane?.chapter?.takeIf { it > 0 } ?: 3) }
    // The fold's second pane scrolls to its own target verse — a weave's `b`
    // endpoint — tracked apart from the primary pane's pendingVerse/navEpoch so
    // the two panes land on their own verses independently.
    var secondPendingVerse by remember { mutableStateOf<Int?>(null) }
    var secondNavEpoch by remember { mutableStateOf(0) }

    var studyBlocks by remember { mutableStateOf<String?>(null) }
    /** The About/Guide card is showing — it gets the build stamp footer. */
    var studyIsAbout by remember { mutableStateOf(false) }
    /** The overlay's answer for the tapped word (null when it re-rendered none). */
    var studyAkjv by remember { mutableStateOf<AkjvToken?>(null) }
    // A study read is in flight. A COLD read is slow and a warm one is instant:
    // the first definition builds the occurrence index, the first analytical
    // answer sweeps the whole corpus. A bare flash of nothing reads as a hang,
    // so the pane says why, and that it is one-time.
    var studyLoading by remember { mutableStateOf(false) }
    var searchHits by remember { mutableStateOf<Set<String>>(emptySet()) }
    var searchText by remember { mutableStateOf("") }

    var secondPane by remember { mutableStateOf(SecondPane.Study) }

    // The bottom-nav destination plus overlays / sheets layered over it.
    var dest by remember { mutableStateOf(Dest.Read) }
    var actionVerse by remember { mutableStateOf<String?>(null) }   // long-press verse sheet
    var memView by remember { mutableStateOf(MemorizeView.List) }   // memorize sub-view (dest Memorize)
    var drillRef by remember { mutableStateOf<String?>(null) }      // drill one chosen verse
    var showConstellation by remember { mutableStateOf(false) }
    var showChord by remember { mutableStateOf(false) }
    var noteEpoch by remember { mutableStateOf(0) }            // repaint the note marks after a note edit
    // Per-tier content gates: the text is always on; curated and machine analysis
    // are independently switchable. Persisted in the config.
    // Opt-in: absent means off, so a first-time reader gets the text and their own
    // notes, not a study apparatus they never asked for.
    var humanAnalysis by remember { mutableStateOf(loadedCfg?.humanAnalysis ?: false) }
    var machineAnalysis by remember { mutableStateOf(loadedCfg?.machineAnalysis ?: false) }
    // First run: the core derives firstRun from "no config file yet", so this
    // shows exactly once — the first persist clears it for good.
    var showFirstRun by remember { mutableStateOf(loadedCfg?.firstRun == true) }
    var clearPinEpoch by remember { mutableStateOf(0) }             // un-highlight the tapped word
    var presentThread by remember { mutableStateOf<Thread1?>(null) } // Present: chosen thread (picker keeps nav)
    var makeWeaveTag by remember { mutableStateOf<Int?>(null) }     // tag→weave sheet (tag ordinal)
    var firstVisibleVerse by remember { mutableStateOf(lastPane?.verse) } // scroll restore across sessions
    // The reading map: how deep into the chapter the reader has got, and a
    // counter that any scroll/tap bumps so the tracker can tell reading from a
    // chapter left open. Reset per chapter — each is its own pass.
    var reachedVerse by remember(book, chapter) { mutableStateOf(0) }
    var readerInput by remember { mutableStateOf(0) }
    var studySheet by remember { mutableStateOf(false) }            // phone: study as a bottom sheet
    var showSearch by remember { mutableStateOf(false) }            // full-screen search overlay
    var showPresent by remember { mutableStateOf(false) }           // thread presentation mode
    var hymnSing by remember { mutableStateOf<HymnSing?>(null) }    // hymnal sing mode (fullscreen)
    var showNotes by remember { mutableStateOf(false) }             // personal-notes browser
    var showHistory by remember { mutableStateOf(false) }           // reading-history sheet
    var showSettings by remember { mutableStateOf(false) }          // settings dialog
    // Re-reading the welcome this reader was given, from the ⋮ menu — changes no
    // settings, it just reads. Holds "new"/"curious"; null closed.
    var reopenIntro by remember { mutableStateOf<String?>(null) }
    var showWeaves by remember { mutableStateOf(false) }            // Weaves screen (All/Suggested filter)
    var bookNavPane by remember { mutableStateOf<Int?>(null) }      // passage navigator (0 primary, 1 second)
    var tagPickRef by remember { mutableStateOf<String?>(null) }    // tag-picker target verse
    var threadPickRef by remember { mutableStateOf<String?>(null) } // thread-picker target verse
    var confirmAction by remember { mutableStateOf<ConfirmRequest?>(null) } // pending destructive act
    var prompt by remember { mutableStateOf<AuthorPrompt?>(null) }   // text-input authoring dialog

    /**
     * Close every surface layered over the reader.
     *
     * ONE PLACE, and it lives here beside the declarations above rather than at
     * the call sites: a hand-kept list at the call sites silently omits every
     * surface added after it was written (open Notes from Explore, tap Memorize,
     * and Notes was still there covering the screen).
     *
     * Web twin: the `Session.TRANSIENT` table carries the identical reasoning —
     * a class of bug is a class on both shells.
     *
     * NOT `showFirstRun`: a reader who has never chosen a path must not be able
     * to tab past the question — it closes by being answered. NOT `confirmAction`
     * or `prompt` either: both are asked FROM another surface, so closing what is
     * under them would leave the question with nothing behind it.
     *
     * `CallbackWiringTest` checks that every surface declared above appears here.
     */
    fun dismissTransient() {
        actionVerse = null
        showConstellation = false
        showChord = false
        studySheet = false
        showSearch = false
        showPresent = false
        hymnSing = null
        showNotes = false
        showHistory = false
        showSettings = false
        reopenIntro = null
        showWeaves = false
        bookNavPane = null
        tagPickRef = null
        threadPickRef = null
        makeWeaveTag = null
        drillRef = null
        presentThread = null
    }

    // The navigator's verse target: ReaderPane scrolls it into view on layout.
    var pendingVerse by remember { mutableStateOf<Int?>(null) }
    var navEpoch by remember { mutableStateOf(0) }

    // Reader prefs + reading history (all persisted to the shared config).
    var bodySize by remember { mutableStateOf((loadedCfg?.bodySize ?: 18.0).coerceIn(12.0, 40.0)) }
    var sideMargin by remember { mutableStateOf((loadedCfg?.sideMargin ?: 28.0).coerceIn(8.0, 96.0)) }
    var lineSpacing by remember { mutableStateOf((loadedCfg?.lineSpacing ?: 1.35).coerceIn(1.0, 2.2)) }
    var copyStyle by remember { mutableStateOf(loadedCfg?.copyStyle ?: "verseRef") }
    // The two typography switches, both ON by default: `!= false` so an absent
    // key (a config written before they existed) reads as the shipped reader.
    var verseNumbers by remember { mutableStateOf(loadedCfg?.verseNumbers != false) }
    var addedItalics by remember { mutableStateOf(loadedCfg?.addedItalics != false) }
    var history by remember { mutableStateOf(loadedCfg?.history ?: emptyList()) }
    var slots by remember { mutableStateOf(loadedCfg?.slots ?: emptyMap()) }
    // The lifetime counter: seeded once by hand, earned thereafter. -1 is
    // "never said", deliberately not 0 — a reader who answers "none" has told
    // us something and must not be asked again.
    var bibleReads by remember { mutableIntStateOf(loadedCfg?.bibleReads ?: -1) }
    var bibleReadsCredited by remember { mutableStateOf(loadedCfg?.bibleReadsCredited ?: false) }
    var askReads by remember { mutableStateOf(false) }
    /** An open list-pick: its title, the options, and what to do with the choice.
     *  A picker rather than a text field — every caller is choosing among things
     *  that already exist, and retyping a name is how a typo makes a second tag. */
    var pick by remember { mutableStateOf<Triple<String, List<String>, (String) -> Unit>?>(null) }
    // The reader's home church — what their own shared links carry (web parity).
    // `intro` is which welcome they were given, so the Welcome button can show it
    // again without a reinstall.
    var church by remember { mutableStateOf(cleanChurch(loadedCfg?.church)) }
    var presentSharesAsNew by remember { mutableStateOf(loadedCfg?.presentSharesAsNew != false) }
    // The plain-English overlay (the AKJV delta). Off unless asked; only
    // offered once stage 2 has actually brought one into the engine.
    var akjvOverlay by remember { mutableStateOf(loadedCfg?.akjvOverlay == true) }
    var akjvAvailable by remember { mutableStateOf(false) }
    var introChoice by remember {
        mutableStateOf(loadedCfg?.intro?.takeIf { it == "new" || it == "curious" })
    }

    // Does this home carry an overlay at all? Asked once the engine is up; the
    // toggle stays hidden if not, rather than doing nothing when tapped.
    LaunchedEffect(Unit) {
        akjvAvailable = withContext(Dispatchers.Default) {
            runCatching { synchronized(engine) { engine.AkjvAvailable() } }.getOrDefault(false)
        }
    }

    val gates = (if (humanAnalysis) 1 else 0) or (if (machineAnalysis) 2 else 0)

    fun persistCfg() {
        val cfg = (loadedCfg ?: ConfigState()).copy(
            bodySize = bodySize, sideMargin = sideMargin, lineSpacing = lineSpacing, copyStyle = copyStyle,
            openPanes = listOf(PaneRef1(book, chapter, firstVisibleVerse)), activePane = 0, history = history,
            slots = slots, bibleReads = bibleReads, bibleReadsCredited = bibleReadsCredited,
            theme = themeChoice, textFont = textFont, chromeFont = chromeFont,
            humanAnalysis = humanAnalysis, machineAnalysis = machineAnalysis,
            church = church, presentSharesAsNew = presentSharesAsNew, intro = introChoice,
            akjvOverlay = akjvOverlay,
            verseNumbers = verseNumbers, addedItalics = addedItalics,
        )
        scope.launch { withContext(Dispatchers.Default) { runCatching { StudyConfig.SaveJson(PlumblineJson.encodeToString(cfg)) } } }
    }
    val studyScale = (bodySize / 18.0).toFloat()

    // Record the primary passage to history (most-recent-first, deduped) and
    // persist the session whenever it changes — this is what "start where I left
    // off" restores from, and what the History sheet lists.
    LaunchedEffect(book, chapter) {
        history = (listOf(PaneRef1(book, chapter)) +
            history.filterNot { it.book == book && it.chapter == chapter }).take(50)
        // …and against this seating, so the next one like it reopens here.
        slots = slots + (sessionSlot to PaneRef1(book, chapter))
        persistCfg()
    }

    // Reopen mid-chapter: scroll the saved first-visible verse into view once
    // (the same target mechanism the navigator uses).
    LaunchedEffect(Unit) {
        lastPane?.verse?.takeIf { it > 1 }?.let { v ->
            pendingVerse = v
            navEpoch++
        }
    }

    // Persist on backgrounding — chapter changes save eagerly, but the scroll
    // position (and anything mid-flight) must survive a plain app switch/close.
    val lifecycleOwner = androidx.lifecycle.compose.LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val obs = androidx.lifecycle.LifecycleEventObserver { _, event ->
            if (event == androidx.lifecycle.Lifecycle.Event.ON_PAUSE) persistCfg()
        }
        lifecycleOwner.lifecycle.addObserver(obs)
        onDispose { lifecycleOwner.lifecycle.removeObserver(obs) }
    }

    // Time spent in the chapter on screen, for the navigator's reading map. Only
    // the primary pane is tracked: a second pane is usually a parallel reference
    // being consulted, not a chapter being read through.
    ReadingTracker(
        engine = engine,
        book = book,
        chapter = chapter,
        reachedVerse = reachedVerse,
        interactionEpoch = readerInput,
        // Only on the Read tab, and only with the text actually in front of the
        // reader: Present and the full-screen map overlays cover it entirely, so
        // a thread presented for twenty minutes must not credit whatever chapter
        // was underneath. (Web twin: `target()` in Shell.svelte guards on the
        // same set plus concept study, which this shell does not have.)
        enabled = dest == Dest.Read && !showPresent && !showConstellation && !showChord,
    )

    val mode = rememberUiMode(fold)

    // ── navigation: roll a (book, chapter) across book bounds, per pane.
    //    Mirrors MainWindow.StepActive. ───────────────────────────────────────
    fun step(curBook: String, curChap: Int, dir: Int): Pair<String, Int> {
        val idx = toc.indexOfFirst { it.id == curBook }
        if (idx < 0) return curBook to curChap
        val ch = curChap + dir
        return when {
            ch < 1 -> if (idx > 0) toc[idx - 1].let { it.id to maxOf(1, it.chapters.toInt()) } else curBook to curChap
            ch > toc[idx].chapters.toInt() -> if (idx < toc.size - 1) toc[idx + 1].id to 1 else curBook to curChap
            else -> curBook to ch
        }
    }

    // Reveal the study surface: the right pane on the fold, a bottom sheet on a phone.
    fun revealStudy() {
        if (mode == UiMode.FoldFullscreen) secondPane = SecondPane.Study else studySheet = true
    }

    // Reveal the surface and put it in its loading state — what every read that
    // will paint a card does before it starts, so the tap answers on the frame it
    // happened even when the card behind it is a cold corpus sweep.
    fun beginStudy() {
        revealStudy()
        studyLoading = true
    }

    // Produce a study block-list OFF the main thread, then show it. The word
    // study / concordance / weave producers can be heavy (they build the lazy
    // analytics on first use) — running them on the UI thread is the tap→load
    // lag. Reveal immediately (previous content stays until the new arrives).
    fun loadStudy(producer: () -> String?) {
        beginStudy()
        scope.engineCall(engine, turns, producer) { blocks ->
            studyLoading = false
            if (blocks != null) studyBlocks = blocks
        }
    }

    // ── word tap → word study (bottom sheet on a phone, right pane on the fold) ─
    fun onWord(hit: Hit) {
        val overlayOn = akjvOverlay
        beginStudy()
        // The card and the overlay header come back from ONE read: the header
        // names the word THIS card is about, so the two must not be able to land
        // out of order. The lookup itself is a span hit, but it takes the engine
        // monitor, and on the main thread that waits out whatever else is holding
        // it — a cold word study sweeps the corpus.
        scope.engineCall(
            engine, turns,
            {
                WordStudy(
                    blocks = engine.WordStudyBlocks2Json(hit.verse, hit.tokenIndex.toInt(), gates),
                    // What the overlay did to this word, if anything — shown under
                    // the headword and above the Strong's, because the codes are
                    // keyed to the KJV word. Only while the overlay is on: with it
                    // off there is nothing to explain.
                    akjv = if (overlayOn) {
                        engine.AkjvTokenJson(hit.verse, hit.tokenIndex.toInt())
                            ?.let { runCatching { parseWire<AkjvToken>(it) }.getOrNull() }
                    } else {
                        null
                    },
                )
            },
        ) { w ->
            studyLoading = false
            studyAkjv = w?.akjv
            if (w?.blocks != null) studyBlocks = w.blocks
        }
    }

    // Navigate the reader to a refKey ("John 3:16"), scrolling the verse into view.
    fun goToRef(ref: String) {
        val sp = ref.lastIndexOf(' ')
        if (sp <= 0) return
        val cv = ref.substring(sp + 1).split(':')
        val ch = cv.getOrNull(0)?.toIntOrNull() ?: return
        book = ref.substring(0, sp)
        chapter = ch
        pendingVerse = cv.getOrNull(1)?.toIntOrNull()
        navEpoch++
        dest = Dest.Read
    }

    // Loading a weave pulls its first link's passages up behind the card
    // (both shells — the web opens both in split panes):
    // the reader shows endpoint `a`; the fold's second pane picks up `b` so
    // flipping back to the Bible lands on the other side.
    //
    // The library read and the compare card are ONE turn, because which passages
    // and which card are one answer: split across two turns the second would
    // cancel the first. [weaveOpening] is the whole decision, and it is pure.
    fun openWeave(index: Int) {
        studyIsAbout = false
        beginStudy()
        scope.engineCall(
            engine, turns,
            {
                WeaveOpened(
                    opening = engine.WeavesJson()
                        ?.let { j -> runCatching { parseWire<WeaveLib>(j) }.getOrNull() }
                        ?.weaves?.getOrNull(index)?.links
                        ?.let { weaveOpening(it) },
                    blocks = engine.CompareBlocksJson(index, true),
                )
            },
        ) { w ->
            studyLoading = false
            if (w == null) return@engineCall
            w.opening?.let { o ->
                goToRef(o.primary)
                o.second?.let { s ->
                    secondBook = s.book; secondChapter = s.chapter
                    secondPendingVerse = o.secondVerse; secondNavEpoch++
                }
            }
            if (w.blocks != null) studyBlocks = w.blocks
        }
    }

    // Load a study library (threads / tags / weaves / suggested / guide / about)
    // into the study surface — StudyPane renders each block list identically.
    fun openLibrary(which: Library) {
        studyIsAbout = which == Library.Guide || which == Library.About
        loadStudy {
            when (which) {
                Library.Threads -> engine.ThreadsBlocksJson()
                Library.Tags -> engine.TagsBlocksJson()
                Library.Weaves -> engine.WeavesBlocksJson()
                Library.Suggested -> engine.SuggestedBlocksJson()
                // Guide now includes the About content (combined card); the
                // About enum value is only reachable via the `about` link verb.
                Library.Guide -> StudyEngine.GuideBlocksJson()
                Library.About -> StudyEngine.AboutBlocksJson()
            }
        }
    }

    // Writing a note is a FILE write through the atomic store, so it never
    // belonged on the main thread — and it needs the engine monitor like every
    // other write (Notes.kt, VerseActions.kt). No turn: a study card the reader
    // asked for in the meantime must not swallow the repaint of their own note.
    //
    // The marks re-fetch when the write LANDS. Bumping the epoch first re-read the
    // notes straight away, and that read takes the same monitor — nothing ordered
    // it behind the write, so the mark could go missing until the next chapter.
    // The engine's error message is dropped, exactly as it was here before; the
    // note editor with retry lives in Notes.kt.
    fun saveNote(ref: String, text: String) {
        scope.engineCall(engine, null, { engine.UserNoteSet(ref, text, nowUtc()) }) { noteEpoch++ }
    }

    // What a routed link does. Pure UI state (navigate, open a map, open a picker)
    // lands here on the main thread; anything that asks the engine a question goes
    // through [loadStudy] / [engineCall], so no branch of this `when` can block the
    // frame it runs on.
    fun route(link: PanelLinkData) {
        // Every routed card clears the About footer; the `about` verb re-sets it.
        fun show(producer: () -> String?) {
            studyIsAbout = false
            loadStudy(producer)
        }
        when (link.verb) {
            "go" -> if (link.book != null && link.chapter != null) {
                book = link.book!!; chapter = link.chapter!!.toInt()
                pendingVerse = link.verse?.toInt(); navEpoch++
                dest = Dest.Read
            }
            // Named rather than `it` throughout: the producer is a second lambda
            // now, and it is read on another thread — what it captures should be
            // spelled out.
            "occurrences" -> link.code?.let { code -> show { engine.ConcordanceBlocksJson(code) } }
            "rendering" -> if (link.code != null && link.rendering != null) {
                val code = link.code!!
                val rendering = link.rendering!!
                show { engine.RenderingConcordanceBlocksJson(code, rendering) }
            }
            "codeStudy" -> link.code?.let { code ->
                val word = link.word
                show { engine.CodeStudyBlocks2Json(code, word, gates) }
            }
            "thread" -> link.index?.let { at -> show { engine.ThreadBlocksJson(at) } }
            "tag" -> link.index?.let { at -> show { engine.TagBlocksJson(at) } }
            "weave" -> link.index?.let { openWeave(it) }
            // Tag→weave: the accumulate-then-organize flow — pick the members
            // (default all), name it, chain it through the canon.
            "makeWeave" -> link.tag?.let { makeWeaveTag = it }
            "guide" -> show { StudyEngine.GuideBlocksJson() }
            "about" -> { show { StudyEngine.AboutBlocksJson() }; studyIsAbout = true }
            // Tagging offers the existing tags first; freetext is the secondary
            // path inside the picker.
            "addTag" -> link.refKey?.let { ref -> tagPickRef = ref }
            // Pick from the threads that exist, or name a new one — a bare text
            // field made you retype an existing name exactly, and a typo forked a
            // second thread instead of failing.
            "addThread" -> link.refKey?.let { ref -> threadPickRef = ref }
            // The note the reader already has comes back off the main thread too;
            // the dialog opens with it when it lands. "" is "no note yet", which is
            // also what an unreadable one falls back to.
            "editNote" -> link.refKey?.let { ref ->
                scope.engineCall(
                    engine, turns,
                    {
                        engine.UserNoteJson(ref)
                            ?.let { runCatching { parseWire<UserNote>(it).text }.getOrNull() } ?: ""
                    },
                ) { cur ->
                    prompt = AuthorPrompt(t("notes.on", "passage" to ref), cur ?: "") { text ->
                        // Saving an EMPTIED editor deletes the note (UserNoteSet's
                        // empty-clears contract), so it asks first — same wording
                        // as the notes browser's ✕ and the verse sheet's editor.
                        if (text.isBlank() && !(cur ?: "").isBlank()) {
                            confirmAction = ConfirmRequest(
                                title = t("notes.deleteAsk", "passage" to ref),
                                body = t("notes.deleteBody"),
                                verb = t("notes.deleteVerb"),
                            ) { saveNote(ref, text) }
                        } else {
                            saveNote(ref, text)
                        }
                    }
                }
            }
            // The write, then the list it changed. Turn-guarded unlike [saveNote],
            // because re-listing Suggested IS a claim on the study surface: if the
            // reader has tapped something else since, they keep what they tapped.
            "approve" -> link.index?.let { idx ->
                scope.engineCall(engine, turns, { engine.WeaveApprove(idx) }) { openLibrary(Library.Suggested) }
            }
            // Rejecting DELETES the suggestion — it does not come back for review
            // — so it asks first, like every other destructive action.
            "reject" -> link.index?.let { idx ->
                confirmAction = ConfirmRequest(
                    title = t("suggested.rejectAsk"),
                    body = t("suggested.rejectBody"),
                    verb = t("suggested.rejectVerb"),
                ) {
                    scope.engineCall(engine, turns, { engine.WeaveReject(idx) }) { openLibrary(Library.Suggested) }
                }
            }
            // Untag (remove one verse from a tag): the wire carries the tag
            // ordinal; authoring wants the name — so the lookup rides a
            // background read first, and the ask can name what it removes.
            // Success gets a toast: the card the ✕ sits on is a snapshot, so
            // nothing else would tell the reader the write landed.
            "untag" -> if (link.tag != null && link.refKey != null) {
                val at = link.tag!!
                val ref = link.refKey!!
                scope.engineCall(
                    engine, turns,
                    { engine.TagsJson()?.let { runCatching { parseWire<Tags>(it).tags }.getOrNull() }?.getOrNull(at) },
                ) { tag ->
                    if (tag != null) {
                        confirmAction = ConfirmRequest(
                            title = t("tag.removeAsk", "passage" to ref, "tag" to tag.name),
                            body = t("tag.removeBody"),
                            verb = t("tag.removeVerb"),
                        ) {
                            scope.engineCall(engine, null, { engine.TagRemove(tag.name, "verse", ref) }) { err ->
                                Toast.makeText(
                                    context,
                                    if (err.isNullOrBlank()) {
                                        t("tag.removed", "passage" to ref, "tag" to tag.name)
                                    } else {
                                        err
                                    },
                                    Toast.LENGTH_SHORT,
                                ).show()
                            }
                        }
                    }
                }
            }
            // The three whole-item deletes (web study/links.ts parity). Each
            // looks up the name first so the ask says what dies, confirms
            // through the shared dialog, then re-lists the library — ordinals
            // shift after every write, so the card just deleted must not stay
            // up pointing at whatever slid into its index.
            "deleteThread" -> link.index?.let { at ->
                scope.engineCall(
                    engine, turns,
                    { engine.ThreadsJson()?.let { runCatching { parseWire<Threads>(it).threads }.getOrNull() }?.getOrNull(at) },
                ) { thread ->
                    if (thread != null) {
                        confirmAction = ConfirmRequest(
                            title = t("thread.deleteAsk", "thread" to thread.name),
                            body = t("thread.deleteBody"),
                            verb = t("thread.deleteVerb"),
                        ) {
                            scope.engineCall(engine, turns, { engine.ThreadRemove(thread.name) }) {
                                openLibrary(Library.Threads)
                            }
                        }
                    }
                }
            }
            "deleteTag" -> link.index?.let { at ->
                scope.engineCall(
                    engine, turns,
                    { engine.TagsJson()?.let { runCatching { parseWire<Tags>(it).tags }.getOrNull() }?.getOrNull(at) },
                ) { tag ->
                    if (tag != null) {
                        confirmAction = ConfirmRequest(
                            title = t("tag.deleteAsk", "tag" to tag.name),
                            body = t("tag.deleteBody"),
                            verb = t("tag.deleteVerb"),
                        ) {
                            scope.engineCall(engine, turns, { engine.TagDelete(tag.name) }) {
                                openLibrary(Library.Tags)
                            }
                        }
                    }
                }
            }
            "deleteWeave" -> link.index?.let { at ->
                scope.engineCall(
                    engine, turns,
                    // Matched on the carried `index`, not the list position: the
                    // library JSON's field is the flat ordinal the verb carries.
                    { engine.WeavesJson()?.let { runCatching { parseWire<WeaveLib>(it).weaves }.getOrNull() }?.firstOrNull { it.index == at } },
                ) { weave ->
                    if (weave != null) {
                        confirmAction = ConfirmRequest(
                            title = t("weave.deleteAsk", "weave" to weave.name),
                            body = t("weave.deleteBody"),
                            verb = t("weave.deleteVerb"),
                        ) {
                            scope.engineCall(engine, turns, { engine.WeaveDelete(at) }) {
                                openLibrary(Library.Weaves)
                            }
                        }
                    }
                }
            }
            // editThreadNotes / editWeaveNotes / editEntryNote need an
            // index→name lookup — a documented follow-up (rarer authoring).
        }
    }

    // ── link routing: navigate, open a map, or load a study card into the surface.
    //    Routing the URI is itself an ABI call, so the whole tap — the routing and
    //    the card it asks for — happens off the main thread: a link can fan out to
    //    a row of blocking engine calls, each waiting on the monitor a cold study
    //    read can hold for seconds.
    fun onLink(uri: String) {
        scope.engineCall(
            engine, turns,
            { StudyEngine.RouteLinkJson(uri)?.let { runCatching { parseWire<PanelLinkData>(it) }.getOrNull() } },
        ) { link ->
            // This tap owns the study surface now, and so owns its spinner: a card
            // still in flight from an earlier tap was superseded by this turn and
            // will never clear it itself. A verb that loads a card sets it again on
            // this same frame.
            studyLoading = false
            if (link != null) route(link)
        }
    }

    // A Bible pane = an optional compact ‹ Book Ch › header (per-pane nav, used in
    // fold mode) + the reader. `isSecond` picks the primary or the second pane's
    // state; `showHeader` is false on the phone (the top bar carries the nav).
    val biblePane: @Composable (Modifier, Boolean, Boolean) -> Unit = { m, isSecond, showHeader ->
        val b = if (isSecond) secondBook else book
        val c = if (isSecond) secondChapter else chapter
        fun setPane(nb: String, nc: Int) {
            if (isSecond) { secondBook = nb; secondChapter = nc } else { book = nb; chapter = nc }
        }
        Column(m) {
            if (showHeader) {
                PaneHeader(
                    toc = toc, book = b, chapter = c, palette = palette,
                    onPrev = { val (nb, nc) = step(b, c, -1); setPane(nb, nc) },
                    onNext = { val (nb, nc) = step(b, c, +1); setPane(nb, nc) },
                    onOpenNav = { bookNavPane = if (isSecond) 1 else 0 },
                )
                HorizontalDivider(color = palette.rule)
            }
            ReaderPane(
                engine = engine, book = b, chapter = c, palette = palette,
                modifier = Modifier.weight(1f), searchHits = searchHits, fontSizeSp = bodySize.toFloat(),
                sideMargin = sideMargin.toFloat(), lineSpacing = lineSpacing.toFloat(),
                verseNumbers = verseNumbers, addedItalics = addedItalics,
                onWordTap = ::onWord,
                onVerseLongPress = { verse -> actionVerse = verse },
                onSwipeChapter = { dir -> val (nb, nc) = step(b, c, dir); setPane(nb, nc) },
                noteEpoch = noteEpoch,
                akjvOverlay = akjvOverlay,
                targetVerse = if (isSecond) secondPendingVerse else pendingVerse,
                targetEpoch = if (isSecond) secondNavEpoch else navEpoch,
                clearPinEpoch = clearPinEpoch,
                onFirstVisibleVerse = if (isSecond) ({ }) else ({ v ->
                    firstVisibleVerse = v
                    readerInput++
                }),
                onVerseReached = if (isSecond) ({ }) else ({ v -> reachedVerse = v; readerInput++ }),
            )
        }
    }

    val study: @Composable (Modifier) -> Unit = { m ->
        Box(m.background(palette.panelBg)) {
            StudyPane(
                studyBlocks, palette, onLink = ::onLink, scale = studyScale,
                loading = studyLoading,
                header = studyAkjv?.let { a -> { AkjvHeader(palette, studyScale, a.akjv, a.kjv) } },
                footer = if (studyIsAbout) {
                    { VersionFooter(palette, studyScale) }
                } else {
                    null
                },
            )
        }
    }

    // The window background is the chrome colour (paneNavBg) so the status-bar +
    // The ≡ utilities every destination's bar carries (web ScreenBar parity):
    // Settings, History, the guide and the welcome must not cost a trip back to
    // the Read tab — that was a three-tap detour from every other destination.
    val utilityMenu: @Composable RowScope.() -> Unit = {
        UtilityMenu(
            palette = palette,
            onWelcome = { reopenIntro = introChoice ?: "new" },
            onHistory = { showHistory = true },
            onGuide = { openLibrary(Library.Guide) },
            onSettings = { showSettings = true },
        )
    }

    // gesture-nav strips read as an extension of the top bar rather than a bare
    // white slice. systemBarsPadding then insets the actual content within the bars.
    Box(Modifier.fillMaxSize().background(palette.paneNavBg)) {
    Box(Modifier.fillMaxSize().systemBarsPadding()) {
    Column(Modifier.fillMaxSize().background(palette.paper)) {
        // The destination content, above the always-in-thumb-reach nav bar.
        Box(Modifier.fillMaxWidth().weight(1f)) {
            when (dest) {
                Dest.Read -> Column(Modifier.fillMaxSize()) {
                    TopBar(
                        palette = palette,
                        mode = mode,
                        toc = toc, book = book, chapter = chapter,
                        onPrev = { val (nb, nc) = step(book, chapter, -1); book = nb; chapter = nc },
                        onNext = { val (nb, nc) = step(book, chapter, +1); book = nb; chapter = nc },
                        onOpenNav = { bookNavPane = 0 },
                        onSearch = { showSearch = true },
                        secondStudy = secondPane == SecondPane.Study,
                        onToggleSecondPane = {
                            secondPane = if (secondPane == SecondPane.Study) SecondPane.Bible else SecondPane.Study
                            if (secondPane == SecondPane.Bible) clearPinEpoch++
                        },
                        onHistory = { showHistory = true },
                        onGuide = { openLibrary(Library.Guide) },
                        onSettings = { showSettings = true },
                        onWelcome = { reopenIntro = introChoice ?: "new" },
                        scale = studyScale,
                    )
                    HorizontalDivider(color = palette.rule)

                    when (mode) {
                        UiMode.FullscreenVertical -> biblePane(Modifier.fillMaxSize(), false, false)

                        UiMode.FoldFullscreen -> Row(Modifier.fillMaxSize()) {
                            biblePane(Modifier.fillMaxHeight().weight(1f), false, true)
                            HingeSpacerVertical(fold)
                            VerticalDivider(color = palette.rule)
                            if (secondPane == SecondPane.Study) study(Modifier.fillMaxHeight().weight(1f))
                            else biblePane(Modifier.fillMaxHeight().weight(1f), true, true)
                        }
                    }
                }

                Dest.Explore -> ExploreScreen(
                    engine = engine,
                    palette = palette,
                    refreshEpoch = noteEpoch,
                    bibleReads = bibleReads,
                    onAskReads = { askReads = true },
                    onRenameTag = { names ->
                        pick = Triple(t("tags.renameWhich"), names) { from ->
                            prompt = AuthorPrompt(t("tags.renameTo", "name" to from), from) { to ->
                                if (to.isNotBlank() && to != from) {
                                    scope.engineCall(engine, null, { engine.TagRename(from, to) }) { err ->
                                        noteEpoch++
                                        Toast.makeText(
                                            context,
                                            if (err.isNullOrBlank()) t("tags.renamed", "from" to from, "to" to to) else err,
                                            Toast.LENGTH_SHORT,
                                        ).show()
                                    }
                                }
                            }
                        }
                    },
                    onMergeTags = { names ->
                        pick = Triple(t("tags.mergeWhich"), names) { from ->
                            pick = Triple(t("tags.mergeInto", "name" to from), names.filter { it != from }) { into ->
                                // Destructive, so it asks — and the question names
                                // which one survives.
                                confirmAction = ConfirmRequest(
                                    title = t("tags.mergeAsk", "from" to from, "into" to into),
                                    body = t("tags.mergeBody", "from" to from, "into" to into),
                                    verb = t("tags.mergeVerb"),
                                ) {
                                    scope.engineCall(engine, null, { engine.TagMerge(from, into) }) { err ->
                                        noteEpoch++
                                        Toast.makeText(
                                            context,
                                            if (err.isNullOrBlank()) t("tags.merged", "from" to from, "into" to into) else err,
                                            Toast.LENGTH_SHORT,
                                        ).show()
                                    }
                                }
                            }
                        }
                    },
                    onCredit = { bibleReads = maxOf(bibleReads, 0) + 1; bibleReadsCredited = true; persistCfg() },
                    onUncredit = { bibleReadsCredited = false; persistCfg() },
                    credited = bibleReadsCredited,
                    onMemorize = { memView = MemorizeView.List; dest = Dest.Memorize },
                    onNotes = { showNotes = true },
                    onThreads = { openLibrary(Library.Threads) },
                    onTags = { openLibrary(Library.Tags) },
                    onWeaves = { showWeaves = true },
                    onConstellation = { showConstellation = true },
                    onChord = { showChord = true },
                    onClose = { dest = Dest.Read },
                    barActions = utilityMenu,
                )

                Dest.Memorize -> MemorizeScreen(
                    engine, memView, toc, palette,
                    onSelectView = { memView = it },
                    onDrill = { ref -> drillRef = ref },
                    barActions = utilityMenu,
                    onClose = {
                        // Memorize is a card inside the STUDY hub, so its
                        // hub's back goes up one layer — to Study, not Read.
                        if (memView == MemorizeView.List) dest = Dest.Explore
                        else memView = MemorizeView.List
                    },
                )

                Dest.Hymnal -> HymnalScreen(
                    engine, palette,
                    barActions = utilityMenu,
                    onClose = { dest = Dest.Read },
                    // The singing itself goes fullscreen over everything, the
                    // way Present's presentation does — see below the nav bar.
                    onSing = { hymnSing = it },
                )

                Dest.Preach -> PreachScreen(
                    palette = palette,
                    onPresent = { showPresent = true },
                    onWeaves = { showWeaves = true },
                    onTags = { openLibrary(Library.Tags) },
                    onNotes = { showNotes = true },
                    onClose = { dest = Dest.Read },
                    barActions = utilityMenu,
                )

                Dest.Share -> ShareScreen(
                    palette = palette,
                    church = church,
                    onChurch = { church = it },
                    onPresentGospel = {
                        // The first-run "sharing the gospel" landing, from where
                        // the sharing actually happens: straight into the trail;
                        // if the stock thread was somehow removed the picker
                        // shows instead.
                        scope.engineCall(
                            engine, null,
                            { engine.ThreadsJson()?.let { runCatching { parseWire<Threads>(it).threads }.getOrNull() } },
                        ) { threads ->
                            presentThread = threads?.firstOrNull { it.name == "Romans Road" }
                            showPresent = true
                        }
                    },
                    onClose = { dest = Dest.Read },
                    barActions = utilityMenu,
                )
            }

            // ── in-content overlays: these cover the destination but keep the
            //    bottom nav in reach (disappearing chrome is disorienting).
            //    Fullscreen-by-design surfaces (search, the live presentation)
            //    stay in the overlay layer below. ──────────
            if (showNotes) {
                NotesScreen(
                    engine, palette,
                    onOpen = { ref -> showNotes = false; goToRef(ref) },
                    onClose = { showNotes = false },
                )
            }
            if (showWeaves) {
                WeavesScreen(
                    engine, palette, studyScale,
                    onLink = { uri -> onLink(uri); showWeaves = false },
                    onClose = { showWeaves = false },
                )
            }
            // Drilling a single verse tapped in the hub — back returns to it.
            drillRef?.let { ref ->
                MemorizeReview(engine, palette, onClose = { drillRef = null }, only = ref)
            }
            if (showConstellation) MapOverlay(t("map.constellation"), palette, { showConstellation = false }) {
                Constellation(
                    engine, palette, Modifier.fillMaxSize(),
                    // Carry the tapped node's verse, not just its chapter, so the
                    // reader lands ON the verse. goToRef parses "Book c:v" (and a
                    // bare "Book c" when a node names only a chapter).
                    onNavigate = { b, ch, refKey -> goToRef(refKey ?: "$b $ch"); showConstellation = false },
                    onOpenWeave = {},
                )
            }
            if (showChord) MapOverlay(t("map.chordMap"), palette, { showChord = false }) {
                ChordMap(
                    engine, toc, palette, Modifier.fillMaxSize(),
                    onPickBook = { b -> book = b; chapter = 1; showChord = false; dest = Dest.Read },
                )
            }
            // Present: picking a thread keeps the nav; the presentation itself
            // (below, over everything) is deliberately fullscreen — the phone
            // gets handed across.
            if (showPresent && presentThread == null) {
                PresentOverlay(
                    engine, palette,
                    thread = null,
                    onThread = { presentThread = it },
                    onClose = { showPresent = false },
                    shareLink = shareUrl(church, presentSharesAsNew),
                )
            }
        }

        // The bottom nav bar: the five ROLES in thumb reach (Read · Study ·
        // Preach · Share · Sing). Preach launches the fullscreen Present
        // overlay; Memorize lives inside Study and lights that tab.
        val navColors = NavigationBarItemDefaults.colors(
            selectedIconColor = palette.gold,
            selectedTextColor = palette.gold,
            unselectedIconColor = palette.faded,
            unselectedTextColor = palette.faded,
            indicatorColor = palette.gold.copy(alpha = 0.14f),
        )
        NavigationBar(containerColor = palette.paneNavBg) {
            NavigationBarItem(
                selected = dest == Dest.Read && !showPresent,
                onClick = { dismissTransient(); dest = Dest.Read },
                icon = { Icon(NavIconRead, contentDescription = null) },
                label = { Text(t("nav.read")) },
                colors = navColors,
            )
            NavigationBarItem(
                // Memorize is a Study-hub card, so its screen lights this tab.
                selected = (dest == Dest.Explore || dest == Dest.Memorize) && !showPresent,
                onClick = { dismissTransient(); dest = Dest.Explore },
                icon = { Icon(NavIconStudy, contentDescription = null) },
                label = { Text(t("nav.study")) },
                colors = navColors,
            )
            NavigationBarItem(
                // A hub like Study, not a straight raise of Present: the role
                // holds the presentation AND its materials (weaves, tags,
                // notes). Present, raised from the hub, still lights this tab.
                selected = dest == Dest.Preach || showPresent,
                onClick = { dismissTransient(); dest = Dest.Preach },
                icon = { Icon(NavIconPresent, contentDescription = null) },
                label = { Text(t("nav.preach")) },
                colors = navColors,
            )
            NavigationBarItem(
                selected = dest == Dest.Share && !showPresent,
                onClick = { dismissTransient(); dest = Dest.Share },
                icon = { Icon(NavIconShare, contentDescription = null) },
                label = { Text(t("nav.share")) },
                colors = navColors,
            )
            NavigationBarItem(
                selected = dest == Dest.Hymnal && !showPresent,
                onClick = { dismissTransient(); dest = Dest.Hymnal },
                icon = { Icon(NavIconHymnal, contentDescription = null) },
                label = { Text(t("nav.sing")) },
                colors = navColors,
            )
        }
    }

        // ── overlays / sheets (parity features layered over the reader) ──────
        if (studySheet && mode == UiMode.FullscreenVertical) {
            // Swiping the study away also clears the tapped word's highlight.
            StudySheet(studyBlocks, palette, studyScale, ::onLink, studyLoading) {
                studySheet = false
                clearPinEpoch++
            }
        }
        bookNavPane?.let { paneIdx ->
            BookNavScreen(
                engine, toc, palette,
                currentBook = if (paneIdx == 1) secondBook else book,
                onGo = { b, c, v ->
                    if (paneIdx == 1) {
                        secondBook = b; secondChapter = c
                    } else {
                        book = b; chapter = c
                        pendingVerse = v; navEpoch++
                    }
                    bookNavPane = null
                },
                onClose = { bookNavPane = null },
            )
        }
        tagPickRef?.let { ref ->
            TagPickerSheet(engine, palette, ref, onDismiss = { tagPickRef = null })
        }
        threadPickRef?.let { ref ->
            ThreadPickerSheet(engine, palette, ref, onDismiss = { threadPickRef = null })
        }
        ConfirmDialog(confirmAction, palette) { confirmAction = null }
        if (showSearch) {
            SearchOverlay(
                engine, palette, studyScale, searchText,
                onQueryChange = { searchText = it },
                onHits = { searchHits = it },
                onNavigate = { b, c, v -> book = b; chapter = c; pendingVerse = v; navEpoch++ },
                onLink = ::onLink,
                onClose = { showSearch = false },
            )
        }
        if (showHistory) {
            HistorySheet(
                history, toc, palette,
                onOpen = { b, c -> book = b; chapter = c; showHistory = false },
                onDismiss = { showHistory = false },
            )
        }
        actionVerse?.let { v ->
            VerseActionSheet(
                engine, palette, v,
                copyStyle = copyStyle,
                onTag = { ref -> tagPickRef = ref },
                onDismiss = { actionVerse = null },
            )
        }
        makeWeaveTag?.let { ti ->
            TagWeaveSheet(engine, palette, ti, onDone = { makeWeaveTag = null })
        }
        if (showSettings) {
            SettingsDialog(
                palette = palette,
                humanAnalysis = humanAnalysis, onToggleHuman = { humanAnalysis = !humanAnalysis },
                machineAnalysis = machineAnalysis, onToggleMachine = { machineAnalysis = !machineAnalysis },
                themeChoice = themeChoice, onTheme = onThemeChoice,
                textFont = textFont, onTextFont = onTextFont,
                chromeFont = chromeFont, onChromeFont = onChromeFont,
                bodySize = bodySize, onBodySize = { bodySize = it },
                sideMargin = sideMargin, onSideMargin = { sideMargin = it },
                lineSpacing = lineSpacing, onLineSpacing = { lineSpacing = it },
                copyStyle = copyStyle, onCopyStyle = { copyStyle = it },
                verseNumbers = verseNumbers, onToggleVerseNumbers = { verseNumbers = !verseNumbers },
                addedItalics = addedItalics, onToggleAddedItalics = { addedItalics = !addedItalics },
                bundledOn = bundledOn, onToggleBundled = onToggleBundled,
                akjvAvailable = akjvAvailable,
                akjvOverlay = akjvOverlay,
                onToggleAkjv = { akjvOverlay = !akjvOverlay },
                presentSharesAsNew = presentSharesAsNew,
                onPresentSharesAsNew = { presentSharesAsNew = !presentSharesAsNew },
                language = loadedCfg?.language ?: "",
                onLanguage = { showSettings = false; onLanguage(it) },
                onDismiss = { showSettings = false; persistCfg() },
            )
        }
        prompt?.let { p ->
            var text by remember(p) { mutableStateOf(p.initial) }
            AlertDialog(
                onDismissRequest = { prompt = null },
                title = { Text(p.title) },
                text = { OutlinedTextField(value = text, onValueChange = { text = it }) },
                confirmButton = {
                    TextButton(onClick = {
                        // onConfirm writes off the main thread and bumps noteEpoch
                        // itself when the write lands — bumping it here re-read the
                        // notes before the write had taken the monitor.
                        p.onConfirm(text)
                        prompt = null
                    }) { Text(t("common.save")) }
                },
                dismissButton = { TextButton(onClick = { prompt = null }) { Text(t("common.cancel")) } },
            )
        }
        // Asked ONCE, and there is no edit path afterwards on purpose: a number
        // you can retype is a number that means nothing. The keyboard is the
        // NUMBER pad — the web twin sets inputmode="numeric" for the same reason.
        pick?.let { (title, options, onPicked) ->
            AlertDialog(
                onDismissRequest = { pick = null },
                title = { Text(title) },
                text = {
                    // Scrolls: a reader with sixty tags still has Cancel where
                    // they left it.
                    Column(Modifier.verticalScroll(rememberScrollState())) {
                        for (opt in options) {
                            TextButton(onClick = { pick = null; onPicked(opt) }) { Text(opt) }
                        }
                    }
                },
                confirmButton = {},
                dismissButton = { TextButton(onClick = { pick = null }) { Text(t("common.cancel")) } },
            )
        }
        if (askReads) {
            var entry by remember { mutableStateOf("") }
            AlertDialog(
                onDismissRequest = { askReads = false },
                title = { Text(t("explore.readsAsk")) },
                text = {
                    OutlinedTextField(
                        value = entry,
                        onValueChange = { v -> entry = v.filter { it.isDigit() }.take(4) },
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                        singleLine = true,
                    )
                },
                confirmButton = {
                    TextButton(onClick = {
                        entry.toIntOrNull()?.let { n ->
                            bibleReads = n
                            // Seeded as CREDITED, whatever the canon says: a
                            // reader who is already finished must not be
                            // immediately given a read they have just told us
                            // about. The band reconciles it on the next
                            // composition — if the canon is not complete it
                            // clears the flag, which is the honest state.
                            bibleReadsCredited = true
                            persistCfg()
                        }
                        askReads = false
                    }) { Text(t("common.save")) }
                },
                dismissButton = { TextButton(onClick = { askReads = false }) { Text(t("common.cancel")) } },
            )
        }
        // Presentation mode: once a thread is chosen
        // it goes fullscreen, over everything — the reader hands the phone
        // across, so no study chrome bleeds through. (The picker lives
        // in-content above, with the bottom nav.)
        if (showPresent && presentThread != null) {
            PresentOverlay(
                engine, palette,
                thread = presentThread,
                onThread = { presentThread = it },
                onClose = { presentThread = null; showPresent = false },
                // Present is the screen you show someone face to face, so its
                // link opens on the new-believer welcome by default (Settings
                // can turn that off). The ordinary Share never carries it.
                shareLink = shareUrl(church, presentSharesAsNew),
            )
        }

        // Singing a hymn is the same hand-the-phone-up situation as Present,
        // so it gets the same layer: fullscreen, over every piece of chrome.
        hymnSing?.let { s ->
            HymnalSingOverlay(s) { hymnSing = null }
        }

        // First run — over everything: who is opening the Book? (web twin
        // FirstRun.svelte; the three paths are described in FirstRun.kt.)
        if (showFirstRun) {
            FirstRunOverlay(
                engine,
                palette,
                onNewBeliever = { ref, which ->
                    humanAnalysis = false
                    machineAnalysis = false
                    showFirstRun = false
                    introChoice = which
                    book = "John"; chapter = 1
                    pendingVerse = null; navEpoch++
                    dest = Dest.Read
                    if (ref != null) {
                        if (mode == UiMode.FoldFullscreen) {
                            // Beside John: the referenced passage in the second pane
                            // (the same shape openWeave uses).
                            val sp = ref.refKey.lastIndexOf(' ')
                            secondBook = ref.refKey.substring(0, sp)
                            secondChapter =
                                ref.refKey.substring(sp + 1).substringBefore(':').toIntOrNull() ?: 1
                        } else {
                            // Phone: open the passage now — John 1 stays the saved
                            // start position and sits first in History.
                            goToRef(ref.refKey)
                        }
                    }
                    persistCfg()
                },
                onSharing = {
                    showFirstRun = false
                    persistCfg()
                    // No turn: first run happens once, and nothing else is
                    // competing for the presentation.
                    scope.engineCall(
                        engine, null,
                        { engine.ThreadsJson()?.let { runCatching { parseWire<Threads>(it).threads }.getOrNull() } },
                    ) { threads ->
                        // Straight into the trail; if the stock thread was somehow
                        // removed the picker shows instead.
                        presentThread = threads?.firstOrNull { it.name == "Romans Road" }
                        showPresent = true
                    }
                },
                onEstablished = { h, m ->
                    humanAnalysis = h
                    machineAnalysis = m
                    showFirstRun = false
                    persistCfg()
                },
                onChurch = { church = it },
            )
        }
        // Re-reading a welcome: the same page, no path chosen and no setting
        // moved — a reader should not have to reinstall to read it twice.
        reopenIntro?.let { which ->
            FirstRunOverlay(
                engine,
                palette,
                onNewBeliever = { ref, _ ->
                    reopenIntro = null
                    if (ref != null) goToRef(ref.refKey)
                },
                onSharing = { reopenIntro = null },
                onEstablished = { _, _ -> reopenIntro = null },
                reread = which,
                onCloseReread = { reopenIntro = null },
            )
        }
    }
    }
}

// ── off the main thread ──────────────────────────────────────────────────────
//
// Every engine call is a blocking native call behind the `synchronized(engine)`
// monitor the two reader panes and the study surface share, so making one on the
// main thread costs however long the LONGEST call already running takes — and the
// first word study, concordance or search sweeps the whole corpus building a lazy
// index.

/**
 * Which read owns the surface it paints into.
 *
 * A tap opens a turn; only the newest turn may paint. Without it two taps in a row
 * leave whichever read finished LAST on screen, and nothing orders them: they run
 * on `Dispatchers.Default` and contend for the engine monitor, so the first tap
 * can win the monitor second and paint its card over the second tap's.
 *
 * Main-thread only, and so needs no synchronization of its own: turns are opened
 * from Compose event handlers and read from continuations that resume on the same
 * dispatcher.
 */
internal class StudyTurns {
    private var issued = 0

    /** Take the newest turn — everything older may no longer paint. */
    fun open(): Int = ++issued

    /** May [turn]'s result be painted? Only if nothing newer has been asked for. */
    fun isCurrent(turn: Int): Boolean = turn == issued

    /** Nothing in flight may paint any more: the reader emptied the field the
     *  results were for, so landing them would repaint what they just cleared. */
    fun abandon() {
        issued++
    }
}

/**
 * The one way this screen reaches the engine: [call] runs on `Dispatchers.Default`
 * holding [lock], [paint] runs back on the caller's thread with the lock released.
 *
 * [lock] is the engine object — the monitor every shell call takes. It is typed
 * `Any` because that is all it is used as, which is also what lets the JVM unit
 * tests drive this function without a native engine (`EngineCallTest`).
 *
 * The monitor is never held across a suspension point: [call] cannot suspend, and
 * [paint] runs only after the `withContext` has returned. Held across one, a
 * `Default` worker would sit on the monitor waiting for a main-thread continuation
 * — with the main thread able to be waiting on the same monitor.
 *
 * [turns] makes the newest tap the only one that may paint (see [StudyTurns]).
 * Pass null for work nothing can supersede — a write whose result is a repaint of
 * the reader's own data rather than a claim on the study surface.
 *
 * [paint] runs even when [call] returned null or threw, so a caller can drop its
 * spinner on a read that produced nothing.
 */
internal fun <T> CoroutineScope.engineCall(
    lock: Any,
    turns: StudyTurns?,
    call: () -> T?,
    paint: (T?) -> Unit,
) {
    val turn = turns?.open() ?: 0
    launch {
        val v = withContext(Dispatchers.Default) {
            runCatching { synchronized(lock) { call() } }.getOrNull()
        }
        if (turns != null && !turns.isCurrent(turn)) return@launch
        paint(v)
    }
}

/** A book + chapter: where a pane is parked. */
internal data class ChapterRef(val book: String, val chapter: Int)

/** Where tapping a weave lands the reader. */
internal data class WeaveOpening(
    /** The refKey the reader's own pane opens — the link's `a` end. */
    val primary: String,
    /** The `b` end, for the fold's second pane, or null when it is the same
     *  chapter as [primary] (two panes on one chapter show nothing new). */
    val second: ChapterRef? = null,
    /** The verse of the `b` end, so the second pane scrolls to it and not just
     *  to the top of its chapter. Null when [second] is null or names a chapter. */
    val secondVerse: Int? = null,
)

/**
 * Which passages a weave tap opens: its first resolved link (else its first link
 * at all), `a` in the reader and `b` in the fold's second pane.
 *
 * A link whose `a` end is not a refKey opens NOTHING, rather than parking the
 * second pane on `b` while the reader's own pane stays put — which reads as the
 * app ignoring the tap and then moving the wrong pane.
 */
internal fun weaveOpening(links: List<WeaveLink1>): WeaveOpening? {
    val link = links.firstOrNull { it.resolved } ?: links.firstOrNull() ?: return null
    val a = chapterRefOf(link.a) ?: return null
    val b = chapterRefOf(link.b)
    val second = if (b != null && b != a) b else null
    return WeaveOpening(link.a, second, if (second != null) verseOf(link.b) else null)
}

/** A refKey's book and chapter ("Gen 1:7" → Gen 1), or null if it is not one. */
private fun chapterRefOf(refKey: String): ChapterRef? {
    val sp = refKey.lastIndexOf(' ')
    if (sp <= 0) return null
    val ch = refKey.substring(sp + 1).substringBefore(':').toIntOrNull() ?: return null
    return ChapterRef(refKey.substring(0, sp), ch)
}

/** A refKey's verse ("Gen 1:7" → 7), or null when it names only a chapter. */
private fun verseOf(refKey: String): Int? = refKey.substringAfterLast(':', "").toIntOrNull()

/** True when a search answer is a reference to open rather than hits to list.
 *  Both halves matter: the engine has to say `goto` AND say where — the reader's
 *  own pane is navigated on the strength of it. */
internal fun SearchResult.opensAPassage(): Boolean = kind == "goto" && book != null && chapter != null

/** A pane's own compact navigation: ‹ Book Ch › opening the passage navigator
 *  (fold mode, one per Bible pane, so the two panes navigate independently). */
@Composable
private fun PaneHeader(
    toc: List<TocBook>,
    book: String,
    chapter: Int,
    palette: ReaderPalette,
    onPrev: () -> Unit,
    onNext: () -> Unit,
    onOpenNav: () -> Unit,
) {
    val name = toc.firstOrNull { it.id == book }?.name ?: book
    Surface(color = palette.paneNavBg) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onPrev) {
                Icon(Icons.Filled.KeyboardArrowLeft, contentDescription = t("common.previousChapter"), tint = palette.ink)
            }
            TextButton(onClick = onOpenNav) { Text("$name $chapter", color = palette.ink) }
            IconButton(onClick = onNext) {
                Icon(Icons.Filled.KeyboardArrowRight, contentDescription = t("common.nextChapter"), tint = palette.ink)
            }
        }
    }
}

/** Top chrome: on a phone, inline ‹ Book Ch › nav (the title opens the passage
 *  navigator — OT/NT → book → chapter → verse taps); then share-the-app and
 *  search icons, the fold pane toggle (fold only), and a short overflow (⋮) menu. The study
 *  destinations live on the bottom nav bar, in thumb reach. */
@Composable
private fun TopBar(
    palette: ReaderPalette,
    mode: UiMode,
    toc: List<TocBook>,
    book: String,
    chapter: Int,
    onPrev: () -> Unit,
    onNext: () -> Unit,
    onOpenNav: () -> Unit,
    onSearch: () -> Unit,
    secondStudy: Boolean,
    onToggleSecondPane: () -> Unit,
    onHistory: () -> Unit,
    onGuide: () -> Unit,
    onSettings: () -> Unit,
    onWelcome: () -> Unit,
    /** The reader's text size as a factor of the shipped 18 — the same number
     *  the study panel and the search sheet take. The bar's own labels were
     *  fixed sp, so a reader who set 28 got a bar that ignored them, while the
     *  web's chrome followed (`--uiScale`). */
    scale: Float = 1f,
) {
    Surface(color = palette.paneNavBg) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Phone: the single pane's book nav lives here (no per-pane header).
            //
            // The group takes the row's spare width so the trailing icons can
            // never be pushed off the end — the passage ELLIPSIZES instead.
            // A Compose Row cannot wrap the way the web header does, and what
            // runs off the end here would be the ≡, i.e. the way to Settings.
            if (mode == UiMode.FullscreenVertical) {
                val name = toc.firstOrNull { it.id == book }?.name ?: book
                Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
                    IconButton(onClick = onPrev) {
                        Icon(Icons.Filled.KeyboardArrowLeft, contentDescription = t("common.previousChapter"), tint = palette.ink)
                    }
                    // 19sp, not 16: the bar's height is set by its 48dp touch
                    // targets, and the passage — the thing the bar is ABOUT,
                    // and its widest tap target — filled about a third of it
                    // and read as lost (maintainer, Pixel, 2026-08-13).
                    TextButton(onClick = onOpenNav, modifier = Modifier.weight(1f, fill = false)) {
                        Text(
                            "$name $chapter",
                            color = palette.ink,
                            fontSize = (19 * scale).sp,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    IconButton(onClick = onNext) {
                        Icon(Icons.Filled.KeyboardArrowRight, contentDescription = t("common.nextChapter"), tint = palette.ink)
                    }
                }
            } else {
                Spacer(Modifier.weight(1f))
            }

            // Share the app lives on the SHARE destination now (the bar role),
            // not as a header dialog.
            IconButton(onClick = onSearch) {
                Icon(Icons.Filled.Search, contentDescription = t("common.search"), tint = palette.ink)
            }

            // Fold only: flip the right pane between the study panel and a second
            // Bible — a direct icon (gold when study is showing), not a menu trip.
            if (mode == UiMode.FoldFullscreen) {
                IconButton(onClick = onToggleSecondPane) {
                    Icon(
                        Icons.AutoMirrored.Filled.List,
                        contentDescription = if (secondStudy) t("pane.rightStudy") else t("pane.rightBible"),
                        tint = if (secondStudy) palette.gold else palette.ink,
                    )
                }
            }

            UtilityMenu(
                palette = palette,
                onWelcome = onWelcome,
                onHistory = onHistory,
                onGuide = onGuide,
                onSettings = onSettings,
            )
        }
    }
}

/** The ≡ utilities (UTILITIES ONLY — the roles live in the nav bar): Welcome ·
 *  History · Guide & about · Settings. One composable so the Read top bar and
 *  every destination's ScreenBar raise the same menu. Welcome shows for EVERY
 *  reader — the church's own entry moved to the Share destination. */
@Composable
private fun UtilityMenu(
    palette: ReaderPalette,
    onWelcome: () -> Unit,
    onHistory: () -> Unit,
    onGuide: () -> Unit,
    onSettings: () -> Unit,
) {
    Box {
        var menu by remember { mutableStateOf(false) }
        IconButton(onClick = { menu = true }) {
            Icon(Icons.Filled.MoreVert, contentDescription = t("common.menu"), tint = palette.ink)
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text(t("shell.welcome")) }, onClick = { onWelcome(); menu = false })
            DropdownMenuItem(text = { Text(t("shell.history")) }, onClick = { onHistory(); menu = false })
            DropdownMenuItem(text = { Text(t("shell.guideAndAbout")) }, onClick = { onGuide(); menu = false })
            DropdownMenuItem(text = { Text(t("shell.settings")) }, onClick = { onSettings(); menu = false })
        }
    }
}

/** Study as a bottom sheet — the phone surface for a word tap / library / link
 *  result. Opens half-height; drag the handle up to fill (nearly) the whole
 *  screen. Swipe down or tap the scrim to dismiss.
 *  Links route through [onLink]. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun StudySheet(
    blocksJson: String?,
    palette: ReaderPalette,
    scale: Float,
    onLink: (String) -> Unit,
    loading: Boolean,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = palette.panelBg) {
        Box(Modifier.fillMaxWidth().fillMaxHeight(0.94f)) {
            StudyPane(blocksJson, palette, onLink = onLink, scale = scale, loading = loading)
        }
    }
}

/** What a search turned out to be, decided ONCE in the background read: a
 *  reference to open, or hits to band in the reader plus the result card. */
private class SearchAnswer(val goto: SearchResult?, val hits: Set<String>, val blocks: String?)

/** The full-screen search surface behind the top-bar 🔍: a query field over a
 *  live result list. A reference goes straight to the passage (and closes);
 *  a word/phrase bands the reader's hits and lists the results here. Tapping a
 *  result routes through [onLink] and closes. System-back closes. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SearchOverlay(
    engine: StudyEngine,
    palette: ReaderPalette,
    scale: Float,
    initialQuery: String,
    onQueryChange: (String) -> Unit,
    onHits: (Set<String>) -> Unit,
    onNavigate: (book: String, chapter: Int, verse: Int?) -> Unit,
    onLink: (String) -> Unit,
    onClose: () -> Unit,
) {
    var q by remember { mutableStateOf(initialQuery) }
    var blocks by remember { mutableStateOf<String?>(null) }
    var searching by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val scope = rememberCoroutineScope()
    val turns = remember { StudyTurns() }
    BackHandler(onBack = onClose)

    // Searching crosses the ABI twice — the query's answer, then its result card —
    // and the first search of a session builds the search index, which is a corpus
    // sweep. On the main thread that was the keyboard and the field freezing on
    // Enter. Both calls ride ONE background turn: they are one answer, so a `goto`
    // can never arrive after the hits it replaces, and only the newest Enter paints.
    fun run() {
        val query = q.trim()
        if (query.isEmpty()) {
            // Nothing already in flight may paint over a field the reader cleared.
            turns.abandon()
            searching = false
            blocks = null
            onHits(emptySet())
            return
        }
        searching = true
        scope.engineCall(
            engine, turns,
            {
                val r = engine.SearchJson(query)
                    ?.let { runCatching { parseWire<SearchResult>(it) }.getOrNull() }
                when {
                    r == null -> null
                    // A reference goes straight to the passage: no card to build.
                    r.opensAPassage() -> SearchAnswer(goto = r, hits = emptySet(), blocks = null)
                    else -> SearchAnswer(
                        goto = null,
                        hits = r.hits?.map { it.verse }?.toSet() ?: emptySet(),
                        blocks = engine.SearchBlocksJson(query),
                    )
                }
            },
        ) { a ->
            searching = false
            if (a == null) return@engineCall
            onHits(a.hits)
            if (a.goto != null) {
                onNavigate(a.goto.book!!, a.goto.chapter!!.toInt(), a.goto.verse)
                onQueryChange("")
                onClose()
            } else {
                blocks = a.blocks
            }
        }
    }

    Column(Modifier.fillMaxSize().background(palette.paper)) {
        Surface(color = palette.paneNavBg) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 2.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = onClose) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = t("common.closeSearch"), tint = palette.ink)
                }
                OutlinedTextField(
                    value = q,
                    onValueChange = { q = it; onQueryChange(it) },
                    placeholder = { Text(t("search.placeholder")) },
                    singleLine = true,
                    trailingIcon = {
                        if (q.isNotEmpty()) {
                            // Clearing abandons an in-flight search too, or its
                            // results paint over the empty field they came from.
                            IconButton(onClick = {
                                q = ""
                                turns.abandon()
                                searching = false
                                onQueryChange("")
                                onHits(emptySet())
                                blocks = null
                            }) {
                                Icon(Icons.Filled.Close, contentDescription = t("common.clear"), tint = palette.ink)
                            }
                        }
                    },
                    modifier = Modifier.weight(1f).focusRequester(focus),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                    keyboardActions = KeyboardActions(onSearch = { run() }),
                )
            }
        }
        HorizontalDivider(color = palette.rule)
        Box(Modifier.fillMaxSize()) {
            if (blocks == null && !searching) {
                Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
                    Text(
                        t("search.hint"),
                        color = palette.faded,
                    )
                }
            } else {
                // Selecting a result jumps and closes — clear the query so the
                // next 🔍 opens fresh. The pane carries the wait now that the
                // search is off the main thread (StudyPane says why past ~1s).
                StudyPane(
                    blocks, palette, scale = scale, loading = searching,
                    onLink = { uri -> onQueryChange(""); onLink(uri); onClose() },
                )
            }
        }
    }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
}

/** The STUDY hub (internally still Explore): what the study tools ARE,
 *  described, before you open one — so the features aren't cryptic menu words.
 *  Each card opens its tool. Memorize is a card here, not a bar destination:
 *  the bar carries the reader's ROLES and memorization is a study discipline. */
@Composable
private fun ExploreScreen(
    engine: StudyEngine,
    palette: ReaderPalette,
    /** Lifetime reads, -1 when the reader has never said. */
    bibleReads: Int,
    credited: Boolean,
    onAskReads: () -> Unit,
    onCredit: () -> Unit,
    onUncredit: () -> Unit,
    /** Rename / merge, asked and performed by the host — the dialogs live where
     *  the other authoring dialogs do. */
    onRenameTag: (List<String>) -> Unit,
    onMergeTags: (List<String>) -> Unit,
    /** Bumped by an authoring write. NOT a general study epoch — this shell has
     *  none — so it is the note epoch: the counts are otherwise refetched every
     *  time Study is opened, since this composable leaves the tree with it. */
    refreshEpoch: Int,
    onMemorize: () -> Unit,
    onNotes: () -> Unit,
    onThreads: () -> Unit,
    onTags: () -> Unit,
    onWeaves: () -> Unit,
    onConstellation: () -> Unit,
    onChord: () -> Unit,
    onClose: () -> Unit,
    barActions: @Composable RowScope.() -> Unit = {},
) {
    // The maps live under ONE card (web twin ExploreScreen.svelte; maintainer
    // UAT, 2026-08-12) — two sibling cards read as two more tools, when they
    // are two views of the same thing. That card is a DOOR: it opens a PAGE of
    // its own, with its own back arrow, rather than growing a branch in place.
    // The tree was the odd one out in a shell where every destination replaces
    // what came before (maintainer, 2026-08-13).
    var showViz by remember { mutableStateOf(false) }
    var showTags by remember { mutableStateOf(false) }

    // How much is in each tool. One fetch per open (and per authoring write),
    // off the main thread — the same three lists their own screens open with,
    // so nothing here is a new engine call.
    var notes by remember { mutableIntStateOf(0) }
    var threads by remember { mutableIntStateOf(0) }
    var tagNames by remember { mutableStateOf<List<String>>(emptyList()) }
    var weaves by remember { mutableIntStateOf(0) }
    LaunchedEffect(refreshEpoch) {
        withContext(Dispatchers.Default) {
            notes = runCatching {
                synchronized(engine) { engine.UserNotesJson() }?.let { parseWire<UserNotes>(it).notes.size }
            }.getOrNull() ?: 0
            threads = runCatching {
                synchronized(engine) { engine.ThreadsJson() }?.let { parseWire<Threads>(it).threads.size }
            }.getOrNull() ?: 0
            tagNames = runCatching {
                synchronized(engine) { engine.TagsJson() }?.let { parseWire<Tags>(it).tags.map { t -> t.name } }
            }.getOrNull() ?: emptyList()
            weaves = runCatching {
                synchronized(engine) { engine.WeavesJson() }?.let { parseWire<WeaveLib>(it).weaves.size }
            }.getOrNull() ?: 0
        }
    }

    MapOverlay(t("nav.study"), palette, onClose, actions = barActions) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            InProgressBand(
                engine, palette, refreshEpoch, onMemorize,
                bibleReads, credited, onAskReads, onCredit, onUncredit,
            )
            ExploreCard(t("explore.memorize"), t("explore.memorize.desc"), palette, onClick = onMemorize)
            ExploreCard(t("explore.notes"), t("explore.notes.desc"), palette, count = notes, onClick = onNotes)
            ExploreCard(t("explore.threads"), t("explore.threads.desc"), palette, count = threads, onClick = onThreads)
            // A DOOR, like Visualizations: there is more than one thing to do
            // with a tag library (browse, rename, merge) and a card that raised
            // the library directly had nowhere to put the rest.
            ExploreCard(t("explore.tags") + "  ›", t("explore.tags.desc"), palette, count = tagNames.size) { showTags = true }
            ExploreCard(t("explore.weaves"), t("explore.weaves.desc"), palette, count = weaves, onClick = onWeaves)
            ExploreCard(
                t("explore.viz") + "  ›",
                t("explore.viz.desc"),
                palette,
            ) { showViz = true }
        }
    }

    // One layer down, and its ‹ returns HERE rather than to the reader — the
    // same relationship Memorize has with this hub.
    if (showTags) {
        MapOverlay(t("explore.tags"), palette, onClose = { showTags = false }, actions = barActions) {
            Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                Text(
                    t("explore.tags.desc"),
                    color = palette.faded,
                    fontSize = 14.sp,
                    modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 14.dp, bottom = 10.dp),
                )
                ExploreCard(t("tags.browse"), t("tags.browse.desc"), palette) { showTags = false; onTags() }
                // An action with nothing to act on is DISABLED rather than
                // hidden: a menu whose items appear as you acquire data is a
                // menu you cannot learn. The reason it is off is the description.
                ExploreCard(
                    t("tags.rename"),
                    if (tagNames.size < 1) t("tags.rename.needs") else t("tags.rename.desc"),
                    palette,
                    enabled = tagNames.isNotEmpty(),
                ) { onRenameTag(tagNames) }
                ExploreCard(
                    t("tags.merge"),
                    if (tagNames.size < 2) t("tags.merge.needs") else t("tags.merge.desc"),
                    palette,
                    enabled = tagNames.size >= 2,
                ) { onMergeTags(tagNames) }
            }
        }
    }

    if (showViz) {
        MapOverlay(t("explore.viz"), palette, onClose = { showViz = false }, actions = barActions) {
            Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                Text(
                    t("explore.viz.desc"),
                    color = palette.faded,
                    fontSize = 14.sp,
                    modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 14.dp, bottom = 10.dp),
                )
                ExploreCard(t("explore.constellation"), t("explore.constellation.desc"), palette, onClick = onConstellation)
                // The same key the web card renders — the one label that had
                // drifted onto map.chordMap and risked translating twice.
                ExploreCard(t("explore.weaveMap"), t("explore.weaveMap.desc"), palette, onClick = onChord)
            }
        }
    }
}

/** The PREACH hub: the presentation and the materials it is built from
 *  (maintainer direction, 2026-08-11) — Present stays the headline card;
 *  weaves, tags and notes sit under it. The same tools appear inside Study:
 *  the bar carries ROLES, and one tool can serve two hats. */
@Composable
private fun PreachScreen(
    palette: ReaderPalette,
    onPresent: () -> Unit,
    onWeaves: () -> Unit,
    onTags: () -> Unit,
    onNotes: () -> Unit,
    onClose: () -> Unit,
    barActions: @Composable RowScope.() -> Unit = {},
) {
    MapOverlay(t("nav.preach"), palette, onClose, actions = barActions) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            ExploreCard(t("nav.present"), t("preach.present.desc"), palette, onClick = onPresent)
            ExploreCard(t("explore.weaves"), t("explore.weaves.desc"), palette, onClick = onWeaves)
            ExploreCard(t("explore.tags"), t("explore.tags.desc"), palette, onClick = onTags)
            ExploreCard(t("explore.notes"), t("explore.notes.desc"), palette, onClick = onNotes)
        }
    }
}

@Composable
private fun ExploreCard(
    title: String,
    desc: String,
    palette: ReaderPalette,
    /** Whether this action can act. A disabled card is dimmed and inert rather
     *  than absent — see the Tags page for why. */
    enabled: Boolean = true,
    /** How much is IN this tool. Null, or zero, draws nothing: an empty tool
     *  should read as quiet rather than as a score of nought. */
    count: Int? = null,
    onClick: () -> Unit,
) {
    Column(
        Modifier.fillMaxWidth()
            .clickable(enabled = enabled, onClick = onClick)
            .alpha(if (enabled) 1f else 0.55f)
            .padding(horizontal = 20.dp, vertical = 16.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(title, color = palette.ink, fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
            if (count != null && count > 0) {
                Box(
                    Modifier.padding(start = 8.dp)
                        .background(palette.gold.copy(alpha = 0.13f), RoundedCornerShape(999.dp))
                        .padding(horizontal = 8.dp, vertical = 1.dp),
                ) {
                    Text("$count", color = palette.gold, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                }
            }
        }
        Text(desc, color = palette.faded, fontSize = 14.sp, modifier = Modifier.padding(top = 3.dp))
    }
    HorizontalDivider(color = palette.rule)
}

/**
 * The IN PROGRESS band at the top of the Study hub — the web twin's
 * `.band` (ExploreScreen.svelte, where the reasoning lives).
 *
 * The hub was eight identical rectangles of FIXED text, so it looked the same
 * on the day you installed the app as after a year of study (maintainer,
 * 2026-08-13). This is the half that carries STATE.
 *
 * Reading plans are web-only for now (manifest §shell deltas), so this shell's
 * band is the memorize queue, the review queue and the reading map. Fetched
 * once per open, off the main thread, and re-fetched when [studyEpoch] moves —
 * an authoring write is the only thing that can change any of these.
 */
@Composable
private fun InProgressBand(
    engine: StudyEngine,
    palette: ReaderPalette,
    refreshEpoch: Int,
    onMemorize: () -> Unit,
    bibleReads: Int,
    credited: Boolean,
    onAskReads: () -> Unit,
    onCredit: () -> Unit,
    onUncredit: () -> Unit,
) {
    var due by remember { mutableIntStateOf(0) }
    var read by remember { mutableIntStateOf(0) }
    var chapters by remember { mutableIntStateOf(0) }
    // Whether the reads have landed, told apart from "landed and empty" — the
    // web twin's `showReal`. Without it the band drew empty for a beat and then
    // GREW, shoving the cards down the page as it went.
    var settled by remember { mutableStateOf(false) }
    LaunchedEffect(refreshEpoch) {
        withContext(Dispatchers.Default) {
            val now = nowUtc()
            due = runCatching {
                synchronized(engine) { engine.MemoryDueJson(now) }?.let { parseWire<MemoryDue>(it).refs.size }
            }.getOrNull() ?: 0
            val books = runCatching {
                synchronized(engine) { engine.ReadingBooksJson(now) }?.let { parseWire<ReadingBooks>(it).books }
            }.getOrNull().orEmpty()
            read = books.sumOf { it.read }
            chapters = books.sumOf { it.chapters }
        }
        settled = true
    }

    Column(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 14.dp, bottom = 6.dp)) {
        Text(
            t("explore.inProgress").uppercase(),
            color = palette.sectionGold,
            fontSize = 12.sp,
            fontWeight = FontWeight.SemiBold,
            letterSpacing = 1.sp,
        )
        if (!settled) {
            // A PLACEHOLDER OF THE SAME SHAPE, not a spinner: one row and the
            // coverage strip is what the band resolves to in the common cases,
            // so the cards below start where they will stay instead of being
            // shoved down when the reads land (web twin's `.ghost`).
            val ghost = palette.ink.copy(alpha = 0.04f)
            Box(
                Modifier.fillMaxWidth().padding(top = 10.dp).height(22.dp)
                    .background(ghost, RoundedCornerShape(6.dp)),
            )
            Box(
                Modifier.fillMaxWidth().padding(top = 12.dp).height(30.dp)
                    .background(ghost, RoundedCornerShape(6.dp)),
            )
        } else {
        if (due > 0) {
            Row(
                Modifier.fillMaxWidth().clickable(onClick = onMemorize).padding(top = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(t("explore.memorize"), color = palette.ink, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                Text(
                    Strings.plural("memorize.reviews.one", "memorize.reviews.other", due),
                    color = palette.gold,
                    fontSize = 14.sp,
                    modifier = Modifier.padding(start = 10.dp),
                )
            }
        }
        // How much of the canon has had a full pass, in the reading map's own
        // colours — so it belongs to whichever theme is on, for free. CHAPTERS
        // rather than a word-weighted percentage: "412 of 1,189" is a number a
        // reader can hold.
        if (chapters > 0) {
            Text(
                t("explore.chaptersRead", "read" to read, "total" to chapters),
                color = palette.faded,
                fontSize = 14.sp,
                modifier = Modifier.padding(top = 10.dp),
            )
            Box(
                Modifier.fillMaxWidth()
                    .padding(top = 7.dp, bottom = 4.dp)
                    .height(8.dp)
                    .background(palette.readUnread.copy(alpha = 0.12f), RoundedCornerShape(999.dp)),
            ) {
                Box(
                    Modifier.fillMaxWidth(fraction = (read.toFloat() / chapters).coerceIn(0f, 1f))
                        .height(8.dp)
                        .background(palette.readDone, RoundedCornerShape(999.dp)),
                )
            }
        }
        // THE LIFETIME COUNTER, beside the coverage bar: one says how far
        // through this pass you are, the other how many passes there have been.
        // Crediting happens exactly once per finished canon — `credited` marks
        // the CURRENT complete state as counted and is cleared if the map ever
        // drops below full, so the number moves on finishing rather than on
        // every visit to this screen.
        LaunchedEffect(read, chapters, credited, bibleReads) {
            if (bibleReads < 0 || chapters == 0) return@LaunchedEffect
            val complete = read >= chapters
            if (complete && !credited) onCredit() else if (!complete && credited) onUncredit()
        }
        if (bibleReads >= 0) {
            Row(
                Modifier.fillMaxWidth().padding(top = 12.dp),
                verticalAlignment = Alignment.Bottom,
            ) {
                Text("$bibleReads", color = palette.gold, fontSize = 22.sp, fontWeight = FontWeight.SemiBold)
                Text(
                    Strings.plural("explore.readsTimes.one", "explore.readsTimes.other", bibleReads),
                    color = palette.faded,
                    fontSize = 14.sp,
                    modifier = Modifier.padding(start = 8.dp, bottom = 2.dp),
                )
            }
        } else {
            Text(
                t("explore.readsSet"),
                color = palette.gold,
                fontSize = 14.sp,
                modifier = Modifier.fillMaxWidth().clickable(onClick = onAskReads).padding(top = 12.dp),
            )
        }
        if (due == 0 && chapters == 0) {
            Text(
                t("explore.nothingRunning"),
                color = palette.faded,
                fontSize = 14.sp,
                modifier = Modifier.padding(top = 10.dp),
            )
        }
        }
    }
    HorizontalDivider(color = palette.rule)
}

/** Weaves as one screen with an All / Suggested filter (was two menu items).
 *  Blocks fetch off the main thread; links route through [onLink] and close. */
@Composable
private fun WeavesScreen(
    engine: StudyEngine,
    palette: ReaderPalette,
    scale: Float,
    onLink: (String) -> Unit,
    onClose: () -> Unit,
) {
    var suggested by remember { mutableStateOf(false) }
    var blocks by remember { mutableStateOf<String?>(null) }
    BackHandler(onBack = onClose)
    LaunchedEffect(suggested) {
        blocks = withContext(Dispatchers.Default) {
            runCatching {
                synchronized(engine) { if (suggested) engine.SuggestedBlocksJson() else engine.WeavesBlocksJson() }
            }.getOrNull()
        }
    }
    Column(Modifier.fillMaxSize().background(palette.paper)) {
        Surface(color = palette.paneNavBg) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 2.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = onClose) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = t("bar.back"), tint = palette.ink)
                }
                Text(t("weaves.title"), color = palette.ink)
                Spacer(Modifier.weight(1f))
                TextButton(onClick = { suggested = false }) {
                    Text(t("weaves.all"), color = if (!suggested) palette.gold else palette.faded)
                }
                TextButton(onClick = { suggested = true }) {
                    Text(t("weaves.suggested"), color = if (suggested) palette.gold else palette.faded)
                }
            }
        }
        HorizontalDivider(color = palette.rule)
        Box(Modifier.fillMaxSize()) { StudyPane(blocks, palette, onLink = onLink, scale = scale) }
    }
}

/** Reading history — recently-read chapters, most-recent-first, tap to reopen. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun HistorySheet(
    history: List<PaneRef1>,
    toc: List<TocBook>,
    palette: ReaderPalette,
    onOpen: (book: String, chapter: Int) -> Unit,
    onDismiss: () -> Unit,
) {
    val nameOf = remember(toc) { toc.associate { it.id to it.name } }
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = palette.panelBg) {
        Column(Modifier.fillMaxWidth().heightIn(max = 520.dp).navigationBarsPadding()) {
            Text(
                t("history.title"),
                color = palette.faded, fontSize = 12.sp,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 10.dp),
            )
            if (history.isEmpty()) {
                Box(Modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) {
                    Text(t("history.empty"), color = palette.ink)
                }
            } else {
                // One line per RUN, not per chapter — see [historySpans].
                val spans = remember(history) { historySpans(history.map { it.book to it.chapter }) }
                LazyColumn(Modifier.fillMaxWidth()) {
                    items(spans) { sp ->
                        Row(
                            Modifier.fillMaxWidth()
                                .clickable { onOpen(sp.book, sp.open) }
                                .padding(horizontal = 20.dp, vertical = 12.dp),
                        ) {
                            Text(sp.label(nameOf[sp.book] ?: sp.book), color = palette.ink, fontSize = 16.sp)
                        }
                        HorizontalDivider(color = palette.rule)
                    }
                }
            }
        }
    }
}

/** One Settings dialog: the per-tier analysis gates, theme, text
 *  size/margin/spacing, copy format, and the bundled study set — folded
 *  together so the overflow menu stays short. */
/** One radio row — the shape the theme and copy-format lists already use, named
 *  so the language picker above them does not spell it a third time. */
@Composable
private fun SettingRadio(label: String, selected: Boolean, palette: ReaderPalette, onPick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onPick).padding(vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RadioButton(selected = selected, onClick = onPick)
        Text(label, color = palette.ink)
    }
}

@Composable
private fun SettingsDialog(
    palette: ReaderPalette,
    humanAnalysis: Boolean,
    onToggleHuman: () -> Unit,
    machineAnalysis: Boolean,
    onToggleMachine: () -> Unit,
    verseNumbers: Boolean,
    onToggleVerseNumbers: () -> Unit,
    addedItalics: Boolean,
    onToggleAddedItalics: () -> Unit,
    akjvAvailable: Boolean,
    akjvOverlay: Boolean,
    onToggleAkjv: () -> Unit,
    themeChoice: String,
    onTheme: (String) -> Unit,
    textFont: String,
    onTextFont: (String) -> Unit,
    chromeFont: String,
    onChromeFont: (String) -> Unit,
    bodySize: Double,
    onBodySize: (Double) -> Unit,
    sideMargin: Double,
    onSideMargin: (Double) -> Unit,
    lineSpacing: Double,
    onLineSpacing: (Double) -> Unit,
    copyStyle: String,
    onCopyStyle: (String) -> Unit,
    bundledOn: Boolean,
    onToggleBundled: () -> Unit,
    presentSharesAsNew: Boolean,
    onPresentSharesAsNew: () -> Unit,
    language: String,
    onLanguage: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    // The three reader-pref sliders are DRAFTED. Text size, margin and line
    // spacing are all layout inputs to the reading pane, so pushing them up per
    // tick re-laid the chapter per tick: a two-second drag was ~120 chapter
    // layouts, each one a native display list, all but the last orphaned the
    // instant the next value arrived. The thumb follows the draft, and so does the
    // specimen below (size and line multiple both), so the drag stays live under
    // the finger; the pane lays out once, when the finger lifts.
    //
    // Keyed on the committed value: the reader's own state stays the truth, and
    // the reset a commit causes lands on the value just pushed, so nothing jumps.
    val bodyDraft = remember(bodySize) { SliderDraft(bodySize.toFloat()) }
    val marginDraft = remember(sideMargin) { SliderDraft(sideMargin.toFloat()) }
    val spacingDraft = remember(lineSpacing) { SliderDraft(lineSpacing.toFloat()) }
    // Belt to onValueChangeFinished's braces: a thumb moved by a path that never
    // reports a release (an accessibility set-progress, a dialog dismissed
    // mid-drag) must not lose the value the reader chose. Idempotent — after a
    // normal lift there is nothing left to commit.
    fun commitDrafts() {
        bodyDraft.commit { onBodySize(it.toDouble()) }
        marginDraft.commit { onSideMargin(it.toDouble()) }
        spacingDraft.commit { onLineSpacing(it.toDouble()) }
    }
    AlertDialog(
        onDismissRequest = { commitDrafts(); onDismiss() },
        title = { Text(t("settings.title")) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                // FIRST, above everything: it decides what the rest of this
                // dialog is written in, and a reader who cannot read the labels
                // should not have to scroll past twenty of them to find it.
                Text(t("settings.language"), color = palette.faded, fontSize = 12.sp)
                Text(
                    t("settings.languageDesc"),
                    color = palette.faded, fontSize = 13.sp,
                    modifier = Modifier.padding(top = 2.dp, bottom = 4.dp),
                )
                // "" is "follow the device" — see ConfigState.language. The rest
                // are ENDONYMS, always: someone looking for German is looking for
                // "Deutsch", on a screen they may not be able to read.
                SettingRadio(t("settings.languageDevice"), language.isEmpty(), palette) { onLanguage("") }
                for (l in Strings.languages) {
                    SettingRadio(l.endonym, language == l.code, palette) { onLanguage(l.code) }
                }
                HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 10.dp))
                Text(t("settings.theme"), color = palette.faded, fontSize = 12.sp)
                // A dropdown, not a radio column: the theme list outgrew what a
                // column of radios can show without swamping the dialog. Keep the
                // tokens in step with core::theme::ThemeChoice.
                val themes = listOf(
                    "system" to t("settings.themeSystem"),
                    "light" to t("settings.themeLight"),
                    "dark" to t("settings.themeDark"),
                    "night" to t("settings.themeNight"),
                    "solarized-light" to t("settings.themeSolarizedLight"),
                    "solarized-dark" to t("settings.themeSolarizedDark"),
                    "gruvbox" to t("settings.themeGruvbox"),
                    "nord" to t("settings.themeNord"),
                    "one-dark" to t("settings.themeOneDark"),
                    "sepia" to t("settings.themeSepia"),
                    "catppuccin-mocha" to t("settings.themeCatppuccinMocha"),
                    "catppuccin-latte" to t("settings.themeCatppuccinLatte"),
                    "tokyo-night" to t("settings.themeTokyoNight"),
                    "rose-pine" to t("settings.themeRosePine"),
                    "synthwave" to t("settings.themeSynthwave"),
                    "scriptorium" to t("settings.themeScriptorium"),
                    "blueprint" to t("settings.themeBlueprint"),
                    "phosphor" to t("settings.themePhosphor"),
                    "high-contrast" to t("settings.themeHighContrast"),
                )
                var themeMenu by remember { mutableStateOf(false) }
                val currentTheme = themes.firstOrNull { it.first == themeChoice } ?: themes.first()
                Box {
                    Row(
                        Modifier.fillMaxWidth().clickable { themeMenu = true }.padding(vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(currentTheme.second, color = palette.ink, modifier = Modifier.weight(1f))
                        Icon(Icons.Filled.ArrowDropDown, contentDescription = null, tint = palette.faded)
                    }
                    DropdownMenu(expanded = themeMenu, onDismissRequest = { themeMenu = false }) {
                        for ((token, label) in themes) {
                            DropdownMenuItem(
                                text = { Text(label) },
                                onClick = { onTheme(token); themeMenu = false },
                            )
                        }
                    }
                }
                HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 8.dp))
                // The two TYPE axes, offered exactly like the theme above and
                // independent of it. Each row renders its options IN the face
                // they select — a font picker that names faces in one face is
                // asking the reader to imagine the answer.
                //
                // Labels come from BUNDLED_FONTS, not the i18n catalogue: a
                // typeface name is a proper noun. i18n-ignore: typeface names
                for ((label, axis) in listOf(
                    t("settings.textFont") to (textFont to onTextFont),
                    t("settings.chromeFont") to (chromeFont to onChromeFont),
                )) {
                    val (selected, onPick) = axis
                    Text(label, color = palette.faded, fontSize = 12.sp)
                    var menu by remember { mutableStateOf(false) }
                    val current = fontFor(selected)
                    Box {
                        Row(
                            Modifier.fillMaxWidth().clickable { menu = true }.padding(vertical = 6.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                current.displayName,
                                color = palette.ink,
                                fontFamily = rememberSerifFamily(current.token),
                                modifier = Modifier.weight(1f),
                            )
                            Icon(Icons.Filled.ArrowDropDown, contentDescription = null, tint = palette.faded)
                        }
                        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                            for (spec in BUNDLED_FONTS) {
                                DropdownMenuItem(
                                    text = { Text(spec.displayName, fontFamily = rememberSerifFamily(spec.token)) },
                                    onClick = { onPick(spec.token); menu = false },
                                )
                            }
                        }
                    }
                }
                HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 8.dp))
                Text(t("settings.textSize"), color = palette.faded, fontSize = 12.sp)
                // The live feedback a drag has instead of the pane behind the
                // scrim: the specimen takes the drafted SIZE and the drafted line
                // multiple, so two of the three sliders show what they do while
                // the finger is still down. Two lines because one cannot show
                // spacing. (Margin gets none: this column is not the reader's
                // column, so any preview of it here would be a made-up scale. It
                // lands when the finger lifts, like the pane's own layout.)
                Text(
                    // A TYPE SPECIMEN, not copy: it shows the reader what the size
                    // and spacing they are dragging look like, and the letters are
                    // the point. i18n-ignore: specimen
                    "Aa\nAa",
                    fontSize = bodyDraft.value.sp,
                    lineHeight = (bodyDraft.value * spacingDraft.value).sp,
                    color = palette.ink,
                )
                Slider(
                    value = bodyDraft.value,
                    onValueChange = { bodyDraft.drag(it) },
                    onValueChangeFinished = { bodyDraft.commit { v -> onBodySize(v.toDouble()) } },
                    valueRange = 14f..30f,
                    steps = 15,
                )
                Text(t("settings.margin"), color = palette.faded, fontSize = 12.sp)
                Slider(
                    value = marginDraft.value,
                    onValueChange = { marginDraft.drag(it) },
                    onValueChangeFinished = { marginDraft.commit { v -> onSideMargin(v.toDouble()) } },
                    valueRange = 16f..56f,
                )
                Text(t("settings.lineSpacing"), color = palette.faded, fontSize = 12.sp)
                Slider(
                    value = spacingDraft.value,
                    onValueChange = { spacingDraft.drag(it) },
                    onValueChangeFinished = { spacingDraft.commit { v -> onLineSpacing(v.toDouble()) } },
                    valueRange = 1.2f..2.0f,
                )
                HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 8.dp))
                // ADVANCED, folded shut (web parity: the <details> disclosure).
                // Analysis tiers, copy format, data — the rows most readers
                // never need, kept out of the everyday dialog's way.
                var advanced by remember { mutableStateOf(false) }
                Row(
                    Modifier.fillMaxWidth().clickable { advanced = !advanced }.padding(vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        t("settings.advanced"),
                        color = palette.ink, fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.weight(1f),
                    )
                    Icon(
                        if (advanced) Icons.Filled.KeyboardArrowUp else Icons.Filled.KeyboardArrowDown,
                        contentDescription = null, tint = palette.faded,
                    )
                }
                if (advanced) {
                Text(t("settings.advancedDesc"), color = palette.faded, fontSize = 12.sp)
                // The two typography switches. Numbers are a LAYOUT input (the
                // chapter is re-laid-out, since the number's width and gap
                // belong to the line); italics are paint-only, so that one
                // re-records the page without measuring anything again.
                SettingToggle(
                    t("settings.verseNumbers"),
                    t("settings.verseNumbersDesc"),
                    verseNumbers, palette, onToggleVerseNumbers,
                )
                SettingToggle(
                    t("settings.addedItalics"),
                    t("settings.addedItalicsDesc"),
                    addedItalics, palette, onToggleAddedItalics,
                )
                // The text is always on; each analysis tier switches off on its
                // own (the old all-or-nothing Full study switch is gone).
                SettingToggle(
                    t("settings.human"),
                    t("settings.humanDesc"),
                    humanAnalysis, palette, onToggleHuman,
                )
                SettingToggle(
                    t("settings.machine"),
                    t("settings.machineDesc"),
                    machineAnalysis, palette, onToggleMachine,
                )
                // A reading aid over the SAME text, not a version picker: the
                // words stay the KJV's everywhere it matters (memorize,
                // Present, copy, share), and every marked word tells you what
                // it replaced. Hidden when the home carries no overlay rather
                // than offering a switch that does nothing.
                if (akjvAvailable) {
                    SettingToggle(
                        t("settings.akjv"),
                        t("settings.akjvDesc"),
                        akjvOverlay, palette, onToggleAkjv,
                    )
                }
                HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 8.dp))
                Text(t("settings.copyFormat"), color = palette.faded, fontSize = 12.sp)
                val copyOpts = listOf(
                    "verse" to t("settings.copyVerse"),
                    "verseRef" to t("settings.copyVerseRef"),
                    "verseMarkdown" to t("settings.copyMarkdown"),
                )
                for ((token, label) in copyOpts) {
                    Row(
                        Modifier.fillMaxWidth().clickable { onCopyStyle(token) }.padding(vertical = 2.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(selected = copyStyle == token, onClick = { onCopyStyle(token) })
                        Text(label, color = palette.ink)
                    }
                }
                HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 8.dp))
                // The church's fields moved to the SHARE destination, beside
                // the QR their setting feeds.
                SettingToggle(
                    t("settings.presentAsNew"),
                    t("settings.presentAsNewDesc"),
                    presentSharesAsNew, palette, onPresentSharesAsNew,
                )
                HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 8.dp))
                // The welcome's one home is the ≡ utilities now (it shows for
                // every reader there, established believers included).
                SettingToggle(t("settings.bundled"), t("settings.bundledDesc"), bundledOn, palette, onToggleBundled)
                HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 8.dp))
                BackupRestoreRows(palette)
                } // advanced
            }
        },
        confirmButton = { TextButton(onClick = { commitDrafts(); onDismiss() }) { Text(t("settings.done")) } },
    )
}

/**
 * A slider's live value, held back from the caller until the drag ends.
 *
 * [value] is what the thumb (and any specimen next to it) shows, and it moves at
 * pointer rate — that is the whole point of a slider. [commit] is the only thing
 * that hands the value up, so a setting that costs real work downstream — the
 * three reader prefs each re-lay out the chapter, natively — pays that cost once
 * per drag instead of once per frame.
 *
 * [commit] is idempotent between drags: it pushes only a value that has moved
 * since the last push, so wiring it to both `onValueChangeFinished` and the
 * dialog's close is safe.
 */
internal class SliderDraft(committed: Float) {
    private val live = mutableFloatStateOf(committed)
    private var pushed = committed

    /** What the thumb shows. Compose state: reading it recomposes the slider and
     *  its specimen, and nothing else. */
    val value: Float get() = live.floatValue

    /** The finger moved. Cheap by construction — one state write, no work up. */
    fun drag(v: Float) {
        live.floatValue = v
    }

    /** Hand the value up if it has moved since the last time it was handed up. */
    fun commit(push: (Float) -> Unit) {
        val v = live.floatValue
        if (v == pushed) return
        pushed = v
        push(v)
    }
}

@Composable
private fun SettingToggle(
    title: String,
    desc: String,
    checked: Boolean,
    palette: ReaderPalette,
    onToggle: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onToggle).padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, color = palette.ink, fontSize = 15.sp)
            Text(desc, color = palette.faded, fontSize = 12.sp)
        }
        Switch(checked = checked, onCheckedChange = { onToggle() })
    }
}

/** A full-screen overlay frame for the map canvases: a back bar over the paper;
 *  system-back closes. The map composable paints into the content slot. */
@Composable
private fun MapOverlay(
    title: String,
    palette: ReaderPalette,
    onClose: () -> Unit,
    actions: @Composable RowScope.() -> Unit = {},
    content: @Composable () -> Unit,
) {
    BackHandler(onBack = onClose)
    Column(Modifier.fillMaxSize().background(palette.paper)) {
        ScreenBar(title, palette, onClose, actions = actions)
        Box(Modifier.fillMaxSize()) { content() }
    }
}

/** A spacer equal to the hinge's occluding width, keeping side-by-side content
 *  out from under a separating vertical hinge (docs/ANDROID-BOOTSTRAP.md). */
@Composable
private fun HingeSpacerVertical(fold: FoldingFeature?) {
    val w = hingeThickness(fold, vertical = true)
    if (w > 0.dp) Spacer(Modifier.fillMaxHeight().width(w))
}

@Composable
private fun hingeThickness(fold: FoldingFeature?, vertical: Boolean): Dp {
    if (fold == null || !fold.isSeparating) return 0.dp
    val density = LocalDensity.current
    val px = if (vertical) fold.bounds.width() else fold.bounds.height()
    return with(density) { px.toDp() }
}

/**
 * One run of reading history: adjacent entries in the same book with contiguous
 * chapters, collapsed. The web twin is `shell/historySpans.ts`, and
 * HistorySpansTest holds the two to the same rules.
 */
internal data class HistorySpan(
    val book: String,
    /** What a tap opens: the chapter of the run's MOST RECENT entry, which is
     *  where the reader actually was — not the lowest number in the span. */
    val open: Int,
    var lo: Int,
    var hi: Int,
) {
    /** "Genesis 1" for a single chapter, "Genesis 1–3" for a run (EN DASH, as
     *  everywhere else a range is written in this app). */
    fun label(name: String): String = if (lo == hi) "$name $lo" else "$name $lo–$hi"
}

/**
 * Collapse each run of adjacent same-book contiguous chapters into one span.
 *
 * ADJACENT IN THE LIST, not merely similar: `[Gen 3, John 1, Gen 2]` stays three
 * spans, because the reader went somewhere else in between and merging across
 * that would rewrite the order they did things in. Contiguity is checked against
 * either end of the run, so reading forwards (which lands in this
 * most-recent-first list as 3, 2, 1) and reading backwards both collapse.
 */
internal fun historySpans(history: List<Pair<String, Int>>): List<HistorySpan> {
    val out = mutableListOf<HistorySpan>()
    for ((book, chapter) in history) {
        val run = out.lastOrNull()
        if (run != null && run.book == book && (chapter == run.lo - 1 || chapter == run.hi + 1)) {
            run.lo = minOf(run.lo, chapter)
            run.hi = maxOf(run.hi, chapter)
            continue
        }
        out += HistorySpan(book, chapter, chapter, chapter)
    }
    return out
}
