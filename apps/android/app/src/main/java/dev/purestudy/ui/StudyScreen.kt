// The top-level shell: icon chrome (search + an overflow menu) over a fold-aware
// arrangement of the reader. The Android mirror of apps/windows/PureStudyWin/
// MainWindow.cs, adapted to a touch phone (Glendon's v1 phone shell):
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

package dev.purestudy.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowLeft
import androidx.compose.material.icons.filled.KeyboardArrowRight
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.window.layout.FoldingFeature
import dev.purestudy.Hit
import dev.purestudy.PaneRef1
import dev.purestudy.PanelLinkData
import dev.purestudy.SearchResult
import dev.purestudy.StudyEngine
import dev.purestudy.Toc
import dev.purestudy.TocBook
import dev.purestudy.UserNote
import dev.purestudy.ConfigState
import dev.purestudy.PureJson
import dev.purestudy.StudyConfig
import dev.purestudy.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString

/** What the second (right) pane shows in fold mode. */
private enum class SecondPane { Study, Bible }

/** A study "library" the overflow menu loads into the study surface as blocks. */
enum class Library { Threads, Tags, Weaves, Suggested, Guide, About }

/** A one-field text-input authoring dialog (add tag / add thread / edit note). */
private data class AuthorPrompt(val title: String, val initial: String, val onConfirm: (String) -> Unit)

/**
 * The app root. Resolves a palette from the current theme and mounts [StudyScreen].
 * [fold] is the live FoldingFeature (null when the device is not opened flat).
 */
@Composable
fun PureStudyApp(
    engine: StudyEngine,
    fold: FoldingFeature?,
    bundledOn: Boolean = true,
    onToggleBundled: () -> Unit = {},
) {
    // Light is the v0 default; dark/night are a future toggle (item 6).
    val theme = "light"
    val palette = remember(theme) { ReaderPalette.forTheme(theme) }
    val scheme = if (palette.dark) {
        darkColorScheme(background = palette.paper, surface = palette.paper, onSurface = palette.ink)
    } else {
        lightColorScheme(background = palette.paper, surface = palette.paper, onSurface = palette.ink)
    }
    MaterialTheme(colorScheme = scheme) {
        StudyScreen(engine, fold, palette, bundledOn, onToggleBundled)
    }
}

@Composable
fun StudyScreen(
    engine: StudyEngine,
    fold: FoldingFeature?,
    palette: ReaderPalette,
    bundledOn: Boolean = true,
    onToggleBundled: () -> Unit = {},
) {
    val toc = remember {
        runCatching { parseWire<Toc>(engine.TocJson()).books }.getOrElse { emptyList() }
    }
    val scope = rememberCoroutineScope()

    // Persisted reader config (shared, cross-shell): last-viewed passage, reader
    // prefs, and reading history all restore from it.
    val loadedCfg = remember { runCatching { parseWire<ConfigState>(StudyConfig.LoadJson()) }.getOrNull() }
    val lastPane = loadedCfg?.openPanes
        ?.getOrNull(loadedCfg.activePane.coerceAtLeast(0))
        ?: loadedCfg?.openPanes?.firstOrNull()

    // Primary Bible pane — restore where we left off, else John 3 (desktop default).
    var book by remember { mutableStateOf(lastPane?.book ?: "John") }
    var chapter by remember { mutableStateOf(lastPane?.chapter?.takeIf { it > 0 } ?: 3) }
    // Second Bible pane state (for Bible∥Bible in fold mode).
    var secondBook by remember { mutableStateOf(lastPane?.book ?: "John") }
    var secondChapter by remember { mutableStateOf(lastPane?.chapter?.takeIf { it > 0 } ?: 3) }

    var studyBlocks by remember { mutableStateOf<String?>(null) }
    var searchHits by remember { mutableStateOf<Set<String>>(emptySet()) }
    var searchText by remember { mutableStateOf("") }

    var secondPane by remember { mutableStateOf(SecondPane.Study) }

    // Overlays / sheets layered over the reader (parity features).
    var actionVerse by remember { mutableStateOf<String?>(null) }   // long-press verse sheet
    var memView by remember { mutableStateOf<MemorizeView?>(null) } // memorize destinations
    var conceptCode by remember { mutableStateOf<String?>(null) }   // conceptmap:CODE
    var showConstellation by remember { mutableStateOf(false) }
    var showChord by remember { mutableStateOf(false) }
    var highlightEpoch by remember { mutableStateOf(0) }            // repaint after highlight edits
    var fullMode by remember { mutableStateOf(true) }               // Simple vs Full study depth
    var studySheet by remember { mutableStateOf(false) }            // phone: study as a bottom sheet
    var showSearch by remember { mutableStateOf(false) }            // full-screen search overlay
    var showExplore by remember { mutableStateOf(false) }           // "Explore" study-tools screen
    var showHistory by remember { mutableStateOf(false) }           // reading-history sheet
    var showSettings by remember { mutableStateOf(false) }          // settings dialog
    var showWeaves by remember { mutableStateOf(false) }            // Weaves screen (All/Suggested filter)
    var prompt by remember { mutableStateOf<AuthorPrompt?>(null) }   // text-input authoring dialog

    // Reader prefs + reading history (all persisted to the shared config).
    var bodySize by remember { mutableStateOf((loadedCfg?.bodySize ?: 18.0).coerceIn(12.0, 40.0)) }
    var sideMargin by remember { mutableStateOf((loadedCfg?.sideMargin ?: 28.0).coerceIn(8.0, 96.0)) }
    var lineSpacing by remember { mutableStateOf((loadedCfg?.lineSpacing ?: 1.35).coerceIn(1.0, 2.2)) }
    var copyStyle by remember { mutableStateOf(loadedCfg?.copyStyle ?: "verseRef") }
    var history by remember { mutableStateOf(loadedCfg?.history ?: emptyList()) }

    fun persistCfg() {
        val cfg = (loadedCfg ?: ConfigState()).copy(
            bodySize = bodySize, sideMargin = sideMargin, lineSpacing = lineSpacing, copyStyle = copyStyle,
            openPanes = listOf(PaneRef1(book, chapter)), activePane = 0, history = history,
        )
        scope.launch { withContext(Dispatchers.Default) { runCatching { StudyConfig.SaveJson(PureJson.encodeToString(cfg)) } } }
    }
    val studyScale = (bodySize / 18.0).toFloat()

    // Record the primary passage to history (most-recent-first, deduped) and
    // persist the session whenever it changes — this is what "start where I left
    // off" restores from, and what the History sheet lists.
    LaunchedEffect(book, chapter) {
        history = (listOf(PaneRef1(book, chapter)) +
            history.filterNot { it.book == book && it.chapter == chapter }).take(50)
        persistCfg()
    }

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

    // Produce a study block-list OFF the main thread, then show it. The word
    // study / concordance / weave producers can be heavy (they build the lazy
    // analytics on first use) — running them on the UI thread is the tap→load
    // lag. Reveal immediately (previous content stays until the new arrives).
    fun loadStudy(producer: () -> String?) {
        revealStudy()
        scope.launch {
            val b = withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { producer() } }.getOrNull()
            }
            if (b != null) studyBlocks = b
        }
    }

    // ── word tap → word study (bottom sheet on a phone, right pane on the fold) ─
    fun onWord(hit: Hit) {
        loadStudy { engine.WordStudyBlocksJson(hit.verse, hit.tokenIndex.toInt(), fullMode) }
    }

    // Load a study library (threads / tags / weaves / suggested / guide / about)
    // into the study surface — StudyPane renders each block list identically.
    fun openLibrary(which: Library) {
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

    // ── link routing: navigate, open a map, or load a study card into the surface.
    //    Authoring verbs (addTag/addThread/approve/reject/edit*) are pass 3. ────
    fun onLink(uri: String) {
        val j = runCatching { StudyEngine.RouteLinkJson(uri) }.getOrNull() ?: return
        val link = runCatching { parseWire<PanelLinkData>(j) }.getOrNull() ?: return
        fun show(blocks: String?) {
            if (blocks != null) { studyBlocks = blocks; revealStudy() }
        }
        when (link.verb) {
            "go" -> if (link.book != null && link.chapter != null) {
                book = link.book!!; chapter = link.chapter!!.toInt()
            }
            "conceptMap" -> link.code?.let { conceptCode = it }
            "occurrences" -> link.code?.let { show(engine.ConcordanceBlocksJson(it)) }
            "rendering" -> if (link.code != null && link.rendering != null) {
                show(engine.RenderingConcordanceBlocksJson(link.code!!, link.rendering!!))
            }
            "codeStudy" -> link.code?.let { show(engine.CodeStudyBlocksJson(it, link.word, fullMode)) }
            "thread" -> link.index?.let { show(engine.ThreadBlocksJson(it)) }
            "tag" -> link.index?.let { show(engine.TagBlocksJson(it)) }
            "weave" -> link.index?.let { show(engine.CompareBlocksJson(it, fullMode)) }
            "guide" -> show(StudyEngine.GuideBlocksJson())
            "about" -> show(StudyEngine.AboutBlocksJson())
            "addTag" -> link.refKey?.let { ref ->
                prompt = AuthorPrompt("New tag on $ref", "") { name ->
                    if (name.isNotBlank()) engine.TagAdd(name, "verse", ref, null, nowUtc())
                }
            }
            "addThread" -> link.refKey?.let { ref ->
                prompt = AuthorPrompt("New thread on $ref", "") { name ->
                    if (name.isNotBlank()) engine.ThreadAdd(name, ref, null, nowUtc())
                }
            }
            "editNote" -> link.refKey?.let { ref ->
                val cur = engine.UserNoteJson(ref)
                    ?.let { runCatching { parseWire<UserNote>(it).text }.getOrNull() } ?: ""
                prompt = AuthorPrompt("Note on $ref", cur) { text -> engine.UserNoteSet(ref, text, nowUtc()) }
            }
            "approve" -> link.index?.let { engine.WeaveApprove(it); openLibrary(Library.Suggested) }
            "reject" -> link.index?.let { engine.WeaveReject(it); openLibrary(Library.Suggested) }
            // editThreadNotes / editWeaveNotes / editEntryNote / untag need an
            // index→name lookup — a documented follow-up (rarer authoring).
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
                    onPick = { bk -> setPane(bk.id, 1) },
                )
                HorizontalDivider(color = palette.rule)
            }
            ReaderPane(
                engine = engine, book = b, chapter = c, palette = palette,
                modifier = Modifier.weight(1f), searchHits = searchHits, fontSizeSp = bodySize.toFloat(),
                sideMarginPx = sideMargin.toFloat(), lineSpacing = lineSpacing.toFloat(),
                onWordTap = ::onWord,
                onVerseLongPress = { verse -> actionVerse = verse },
                onSwipeChapter = { dir -> val (nb, nc) = step(b, c, dir); setPane(nb, nc) },
                highlightEpoch = highlightEpoch,
            )
        }
    }
    val study: @Composable (Modifier) -> Unit = { m ->
        Box(m.background(palette.panelBg)) {
            StudyPane(studyBlocks, palette, onLink = ::onLink, scale = studyScale)
        }
    }

    // The window background is the chrome colour (paneNavBg) so the status-bar +
    // gesture-nav strips read as an extension of the top bar rather than a bare
    // white slice. systemBarsPadding then insets the actual content within the bars.
    Box(Modifier.fillMaxSize().background(palette.paneNavBg)) {
    Box(Modifier.fillMaxSize().systemBarsPadding()) {
    Column(Modifier.fillMaxSize().background(palette.paper)) {
        TopBar(
            palette = palette,
            mode = mode,
            toc = toc, book = book, chapter = chapter,
            onPrev = { val (nb, nc) = step(book, chapter, -1); book = nb; chapter = nc },
            onNext = { val (nb, nc) = step(book, chapter, +1); book = nb; chapter = nc },
            onPick = { bk -> book = bk.id; chapter = 1 },
            onSearch = { showSearch = true },
            secondStudy = secondPane == SecondPane.Study,
            onToggleSecondPane = {
                secondPane = if (secondPane == SecondPane.Study) SecondPane.Bible else SecondPane.Study
            },
            onMemorize = { memView = MemorizeView.List },
            onExplore = { showExplore = true },
            onHistory = { showHistory = true },
            onGuide = { openLibrary(Library.Guide) },
            onSettings = { showSettings = true },
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

        // ── overlays / sheets (parity features layered over the reader) ──────
        if (studySheet && mode == UiMode.FullscreenVertical) {
            StudySheet(studyBlocks, palette, studyScale, ::onLink) { studySheet = false }
        }
        if (showSearch) {
            SearchOverlay(
                engine, palette, studyScale, searchText,
                onQueryChange = { searchText = it },
                onHits = { searchHits = it },
                onNavigate = { b, c -> book = b; chapter = c },
                onLink = ::onLink,
                onClose = { showSearch = false },
            )
        }
        if (showExplore) {
            ExploreScreen(
                palette = palette,
                onThreads = { showExplore = false; openLibrary(Library.Threads) },
                onTags = { showExplore = false; openLibrary(Library.Tags) },
                onWeaves = { showExplore = false; showWeaves = true },
                onConstellation = { showExplore = false; showConstellation = true },
                onChord = { showExplore = false; showChord = true },
                onClose = { showExplore = false },
            )
        }
        if (showWeaves) {
            WeavesScreen(
                engine, palette, studyScale,
                onLink = { uri -> onLink(uri); showWeaves = false },
                onClose = { showWeaves = false },
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
                onHighlightsChanged = { highlightEpoch++ },
                onDismiss = { actionVerse = null },
            )
        }
        memView?.let { v ->
            MemorizeScreen(
                engine, v, toc, palette,
                onSelectView = { memView = it },
                onOpen = { b, c -> book = b; chapter = c; memView = null },
                onClose = { memView = null },
            )
        }
        if (showSettings) {
            SettingsDialog(
                palette = palette,
                fullStudy = fullMode, onToggleFull = { fullMode = !fullMode },
                bodySize = bodySize, onBodySize = { bodySize = it },
                sideMargin = sideMargin, onSideMargin = { sideMargin = it },
                lineSpacing = lineSpacing, onLineSpacing = { lineSpacing = it },
                copyStyle = copyStyle, onCopyStyle = { copyStyle = it },
                bundledOn = bundledOn, onToggleBundled = onToggleBundled,
                onDismiss = { showSettings = false; persistCfg() },
            )
        }
        prompt?.let { p ->
            var text by remember(p) { mutableStateOf(p.initial) }
            AlertDialog(
                onDismissRequest = { prompt = null },
                title = { Text(p.title) },
                text = { OutlinedTextField(value = text, onValueChange = { text = it }) },
                confirmButton = { TextButton(onClick = { p.onConfirm(text); prompt = null }) { Text("Save") } },
                dismissButton = { TextButton(onClick = { prompt = null }) { Text("Cancel") } },
            )
        }
        conceptCode?.let { c ->
            MapOverlay("Concept map — $c", palette, { conceptCode = null }) {
                ConceptMap(engine, c, palette, Modifier.fillMaxSize())
            }
        }
        if (showConstellation) MapOverlay("Constellation", palette, { showConstellation = false }) {
            Constellation(
                engine, palette, Modifier.fillMaxSize(),
                onNavigate = { b, ch, _ -> book = b; chapter = ch; showConstellation = false },
                onOpenWeave = {},
            )
        }
        if (showChord) MapOverlay("Chord map", palette, { showChord = false }) {
            ChordMap(
                engine, toc, palette, Modifier.fillMaxSize(),
                onPickBook = { b -> book = b; chapter = 1; showChord = false },
            )
        }
    }
    }
}

/** A pane's own compact navigation: ‹ Book Ch › with a book picker (fold mode,
 *  one per Bible pane, so the two panes navigate independently). */
@Composable
private fun PaneHeader(
    toc: List<TocBook>,
    book: String,
    chapter: Int,
    palette: ReaderPalette,
    onPrev: () -> Unit,
    onNext: () -> Unit,
    onPick: (TocBook) -> Unit,
) {
    var menu by remember { mutableStateOf(false) }
    val name = toc.firstOrNull { it.id == book }?.name ?: book
    Surface(color = palette.paneNavBg) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onPrev) {
                Icon(Icons.Filled.KeyboardArrowLeft, contentDescription = "Previous chapter", tint = palette.ink)
            }
            Box {
                TextButton(onClick = { menu = true }) { Text("$name $chapter", color = palette.ink) }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    for (b in toc) {
                        DropdownMenuItem(text = { Text(b.name) }, onClick = { onPick(b); menu = false })
                    }
                }
            }
            IconButton(onClick = onNext) {
                Icon(Icons.Filled.KeyboardArrowRight, contentDescription = "Next chapter", tint = palette.ink)
            }
        }
    }
}

/** Top chrome: on a phone, inline ‹ Book Ch › nav; then a search icon, the fold
 *  pane toggle (fold only), and a short overflow (⋮) menu. Icons over text;
 *  study tools live behind the Explore screen, not a long scrolling menu. */
@Composable
private fun TopBar(
    palette: ReaderPalette,
    mode: UiMode,
    toc: List<TocBook>,
    book: String,
    chapter: Int,
    onPrev: () -> Unit,
    onNext: () -> Unit,
    onPick: (TocBook) -> Unit,
    onSearch: () -> Unit,
    secondStudy: Boolean,
    onToggleSecondPane: () -> Unit,
    onMemorize: () -> Unit,
    onExplore: () -> Unit,
    onHistory: () -> Unit,
    onGuide: () -> Unit,
    onSettings: () -> Unit,
) {
    Surface(color = palette.paneNavBg) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Phone: the single pane's book nav lives here (no per-pane header).
            if (mode == UiMode.FullscreenVertical) {
                var pick by remember { mutableStateOf(false) }
                val name = toc.firstOrNull { it.id == book }?.name ?: book
                IconButton(onClick = onPrev) {
                    Icon(Icons.Filled.KeyboardArrowLeft, contentDescription = "Previous chapter", tint = palette.ink)
                }
                Box {
                    TextButton(onClick = { pick = true }) { Text("$name $chapter", color = palette.ink, fontSize = 16.sp) }
                    DropdownMenu(expanded = pick, onDismissRequest = { pick = false }) {
                        for (b in toc) {
                            DropdownMenuItem(text = { Text(b.name) }, onClick = { onPick(b); pick = false })
                        }
                    }
                }
                IconButton(onClick = onNext) {
                    Icon(Icons.Filled.KeyboardArrowRight, contentDescription = "Next chapter", tint = palette.ink)
                }
            }

            Spacer(Modifier.weight(1f))

            IconButton(onClick = onSearch) {
                Icon(Icons.Filled.Search, contentDescription = "Search", tint = palette.ink)
            }

            // Fold only: flip the right pane between the study panel and a second
            // Bible — a direct icon (gold when study is showing), not a menu trip.
            if (mode == UiMode.FoldFullscreen) {
                IconButton(onClick = onToggleSecondPane) {
                    Icon(
                        Icons.AutoMirrored.Filled.List,
                        contentDescription = if (secondStudy) "Right pane: study (tap for a second Bible)" else "Right pane: second Bible (tap for study)",
                        tint = if (secondStudy) palette.gold else palette.ink,
                    )
                }
            }

            Box {
                var menu by remember { mutableStateOf(false) }
                IconButton(onClick = { menu = true }) {
                    Icon(Icons.Filled.MoreVert, contentDescription = "Menu", tint = palette.ink)
                }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    DropdownMenuItem(text = { Text("Memorize") }, onClick = { onMemorize(); menu = false })
                    DropdownMenuItem(text = { Text("Explore") }, onClick = { onExplore(); menu = false })
                    DropdownMenuItem(text = { Text("History") }, onClick = { onHistory(); menu = false })
                    DropdownMenuItem(text = { Text("Guide & About") }, onClick = { onGuide(); menu = false })
                    DropdownMenuItem(text = { Text("Settings") }, onClick = { onSettings(); menu = false })
                }
            }
        }
    }
}

/** Study as a bottom sheet — the phone surface for a word tap / library / link
 *  result. Swipe down or tap the scrim to dismiss. Links route through [onLink]. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun StudySheet(
    blocksJson: String?,
    palette: ReaderPalette,
    scale: Float,
    onLink: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = palette.panelBg) {
        Box(Modifier.fillMaxWidth().heightIn(max = 520.dp)) {
            StudyPane(blocksJson, palette, onLink = onLink, scale = scale)
        }
    }
}

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
    onNavigate: (book: String, chapter: Int) -> Unit,
    onLink: (String) -> Unit,
    onClose: () -> Unit,
) {
    var q by remember { mutableStateOf(initialQuery) }
    var blocks by remember { mutableStateOf<String?>(null) }
    val focus = remember { FocusRequester() }
    BackHandler(onBack = onClose)

    fun run() {
        val query = q.trim()
        if (query.isEmpty()) { blocks = null; onHits(emptySet()); return }
        val sj = runCatching { engine.SearchJson(query) }.getOrNull() ?: return
        val r = runCatching { parseWire<SearchResult>(sj) }.getOrNull() ?: return
        if (r.kind == "goto" && r.book != null && r.chapter != null) {
            onHits(emptySet())
            onNavigate(r.book!!, r.chapter!!.toInt())
            onQueryChange("")
            onClose()
        } else {
            onHits(r.hits?.map { it.verse }?.toSet() ?: emptySet())
            blocks = runCatching { engine.SearchBlocksJson(query) }.getOrNull()
        }
    }

    Column(Modifier.fillMaxSize().background(palette.paper)) {
        Surface(color = palette.paneNavBg) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 2.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = onClose) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Close search", tint = palette.ink)
                }
                OutlinedTextField(
                    value = q,
                    onValueChange = { q = it; onQueryChange(it) },
                    placeholder = { Text("Word, phrase, or reference") },
                    singleLine = true,
                    trailingIcon = {
                        if (q.isNotEmpty()) {
                            IconButton(onClick = { q = ""; onQueryChange(""); onHits(emptySet()); blocks = null }) {
                                Icon(Icons.Filled.Close, contentDescription = "Clear", tint = palette.ink)
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
            if (blocks == null) {
                Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
                    Text(
                        "Search the King James text — a word, a phrase, or a reference like “John 3:16”.",
                        color = palette.faded,
                    )
                }
            } else {
                // Selecting a result jumps and closes — clear the query so the
                // next 🔍 opens fresh.
                StudyPane(blocks, palette, onLink = { uri -> onQueryChange(""); onLink(uri); onClose() }, scale = scale)
            }
        }
    }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
}

/** The "Explore" screen: what the study tools ARE, described, before you open
 *  one — so the features aren't cryptic menu words. Each card opens its tool. */
@Composable
private fun ExploreScreen(
    palette: ReaderPalette,
    onThreads: () -> Unit,
    onTags: () -> Unit,
    onWeaves: () -> Unit,
    onConstellation: () -> Unit,
    onChord: () -> Unit,
    onClose: () -> Unit,
) {
    MapOverlay("Explore", palette, onClose) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            ExploreCard("Threads", "Ordered trails of passages you've linked — follow a theme across the canon.", palette, onThreads)
            ExploreCard("Tags", "Labelled sets of verses. Give a tag a colour and its verses get a highlight wash.", palette, onTags)
            ExploreCard("Weaves", "Parallel passages tied together — see how Scripture echoes itself.", palette, onWeaves)
            ExploreCard("Constellation", "The whole weave library as lanes across the canon — tap a node to jump there.", palette, onConstellation)
            ExploreCard("Chord map", "How strongly each pair of books is woven, drawn as arcs over the canon.", palette, onChord)
        }
    }
}

@Composable
private fun ExploreCard(title: String, desc: String, palette: ReaderPalette, onClick: () -> Unit) {
    Column(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 20.dp, vertical = 16.dp),
    ) {
        Text(title, color = palette.ink, fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
        Text(desc, color = palette.faded, fontSize = 14.sp, modifier = Modifier.padding(top = 3.dp))
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
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back", tint = palette.ink)
                }
                Text("Weaves", color = palette.ink)
                Spacer(Modifier.weight(1f))
                TextButton(onClick = { suggested = false }) {
                    Text("All", color = if (!suggested) palette.gold else palette.faded)
                }
                TextButton(onClick = { suggested = true }) {
                    Text("Suggested", color = if (suggested) palette.gold else palette.faded)
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
                "Recently read",
                color = palette.faded, fontSize = 12.sp,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 10.dp),
            )
            if (history.isEmpty()) {
                Box(Modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) {
                    Text("No history yet.", color = palette.ink)
                }
            } else {
                LazyColumn(Modifier.fillMaxWidth()) {
                    items(history) { p ->
                        Row(
                            Modifier.fillMaxWidth()
                                .clickable { onOpen(p.book, p.chapter) }
                                .padding(horizontal = 20.dp, vertical = 12.dp),
                        ) {
                            Text("${nameOf[p.book] ?: p.book} ${p.chapter}", color = palette.ink, fontSize = 16.sp)
                        }
                        HorizontalDivider(color = palette.rule)
                    }
                }
            }
        }
    }
}

/** One Settings dialog: study depth, text size/margin/spacing, copy format, and
 *  the bundled study set — folded together so the overflow menu stays short. */
@Composable
private fun SettingsDialog(
    palette: ReaderPalette,
    fullStudy: Boolean,
    onToggleFull: () -> Unit,
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
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Settings") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                SettingToggle("Full study", "Strong's, morphology, analytics, weave authoring.", fullStudy, palette, onToggleFull)
                HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 8.dp))
                Text("Text size — reader & study", color = palette.faded, fontSize = 12.sp)
                Text("Aa", fontSize = bodySize.sp, color = palette.ink)
                Slider(value = bodySize.toFloat(), onValueChange = { onBodySize(it.toDouble()) }, valueRange = 12f..40f, steps = 27)
                Text("Margin — space either side of the text", color = palette.faded, fontSize = 12.sp)
                Slider(value = sideMargin.toFloat(), onValueChange = { onSideMargin(it.toDouble()) }, valueRange = 8f..96f)
                Text("Line spacing", color = palette.faded, fontSize = 12.sp)
                Slider(value = lineSpacing.toFloat(), onValueChange = { onLineSpacing(it.toDouble()) }, valueRange = 1.0f..2.2f)
                HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 8.dp))
                Text("Copy format", color = palette.faded, fontSize = 12.sp)
                val copyOpts = listOf(
                    "verse" to "Verse text only",
                    "verseRef" to "Verse with reference",
                    "verseMarkdown" to "Markdown blockquote",
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
                SettingToggle("Bundled study set", "Ship-with-app threads, tags, and weaves.", bundledOn, palette, onToggleBundled)
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Done") } },
    )
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
    content: @Composable () -> Unit,
) {
    BackHandler(onBack = onClose)
    Column(Modifier.fillMaxSize().background(palette.paper)) {
        Surface(color = palette.paneNavBg) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 2.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = onClose) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back", tint = palette.ink)
                }
                Text(title, color = palette.ink)
            }
        }
        HorizontalDivider(color = palette.rule)
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
