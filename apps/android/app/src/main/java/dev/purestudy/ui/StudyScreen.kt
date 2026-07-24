// The top-level shell: chrome (book/chapter nav, a mode/pane toggle, search) over
// the fold-aware arrangement of a Bible pane and a Study pane. The Android mirror
// of apps/windows/PureStudyWin/MainWindow.cs, reduced to a v0 reader:
//
//   UiMode.SplitVertical      Column: Bible over Study (stacked halves).
//   UiMode.FullscreenVertical single pane; a Bible↔Study toggle in the bar.
//   UiMode.FoldFullscreen     Row of two panes; the second switches Bible↔Study
//                             (so Bible∥Bible or Bible∥Study), split at the hinge.
//
// All study logic lives across the ABI — this is orchestration only.
//
// Author D (Compose UI).

package dev.purestudy.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.window.layout.FoldingFeature
import dev.purestudy.Hit
import dev.purestudy.PanelLinkData
import dev.purestudy.SearchResult
import dev.purestudy.StudyEngine
import dev.purestudy.Toc
import dev.purestudy.TocBook
import dev.purestudy.parseWire

/** What the second (right) pane shows in fold mode. */
private enum class SecondPane { Study, Bible }

/** The single-pane (mode 2) view choice. */
private enum class SingleView { Bible, Study }

/**
 * The app root. Resolves a palette from the current theme and mounts [StudyScreen].
 * [fold] is the live FoldingFeature (null when the device is not opened flat).
 */
@Composable
fun PureStudyApp(engine: StudyEngine, fold: FoldingFeature?) {
    // Light is the v0 default; dark/night are a future toggle (item 6).
    val theme = "light"
    val palette = remember(theme) { ReaderPalette.forTheme(theme) }
    val scheme = if (palette.dark) {
        darkColorScheme(background = palette.paper, surface = palette.paper, onSurface = palette.ink)
    } else {
        lightColorScheme(background = palette.paper, surface = palette.paper, onSurface = palette.ink)
    }
    MaterialTheme(colorScheme = scheme) {
        StudyScreen(engine, fold, palette)
    }
}

@Composable
fun StudyScreen(engine: StudyEngine, fold: FoldingFeature?, palette: ReaderPalette) {
    val toc = remember {
        runCatching { parseWire<Toc>(engine.TocJson()).books }.getOrElse { emptyList() }
    }

    // Primary Bible pane state (defaults to John 3, like the desktop shells).
    var book by remember { mutableStateOf("John") }
    var chapter by remember { mutableStateOf(3) }
    // Second Bible pane state (for Bible∥Bible in fold mode).
    var secondBook by remember { mutableStateOf("John") }
    var secondChapter by remember { mutableStateOf(3) }

    var studyBlocks by remember { mutableStateOf<String?>(null) }
    var searchHits by remember { mutableStateOf<Set<String>>(emptySet()) }
    var searchText by remember { mutableStateOf("") }

    var singlePane by remember { mutableStateOf(false) }       // mode 1 vs mode 2
    var singleView by remember { mutableStateOf(SingleView.Bible) }
    var secondPane by remember { mutableStateOf(SecondPane.Study) }

    // Overlays / sheets layered over the reader (parity features).
    var actionVerse by remember { mutableStateOf<String?>(null) }   // long-press verse sheet
    var memView by remember { mutableStateOf<MemorizeView?>(null) } // memorize destinations
    var conceptCode by remember { mutableStateOf<String?>(null) }   // conceptmap:CODE
    var showConstellation by remember { mutableStateOf(false) }
    var showChord by remember { mutableStateOf(false) }
    var highlightEpoch by remember { mutableStateOf(0) }            // repaint after highlight edits

    val base = rememberUiMode(fold)
    val mode = when {
        base == UiMode.FoldFullscreen -> UiMode.FoldFullscreen
        singlePane -> UiMode.FullscreenVertical
        else -> UiMode.SplitVertical
    }

    // ── navigation (mirrors MainWindow.StepActive: roll across book bounds) ──
    fun stepPrimary(dir: Int) {
        val idx = toc.indexOfFirst { it.id == book }
        if (idx < 0) return
        val ch = chapter + dir
        when {
            ch < 1 -> if (idx > 0) {
                val prev = toc[idx - 1]; book = prev.id; chapter = maxOf(1, prev.chapters.toInt())
            }
            ch > toc[idx].chapters.toInt() -> if (idx < toc.size - 1) {
                book = toc[idx + 1].id; chapter = 1
            }
            else -> chapter = ch
        }
    }

    fun openBook(b: TocBook) { book = b.id; chapter = 1 }

    // ── word tap → word study; ensure the study surface is visible ──────────
    fun onWord(hit: Hit) {
        studyBlocks = runCatching {
            engine.WordStudyBlocksJson(hit.verse, hit.tokenIndex.toInt(), false)
        }.getOrNull()
        when (mode) {
            UiMode.FullscreenVertical -> singleView = SingleView.Study
            UiMode.FoldFullscreen -> secondPane = SecondPane.Study
            else -> {}
        }
    }

    // ── search: a reference jumps; a query bands hits + shows result blocks ──
    fun runSearch(query: String) {
        val q = query.trim()
        if (q.isEmpty()) { searchHits = emptySet(); studyBlocks = null; return }
        val sj = runCatching { engine.SearchJson(q) }.getOrNull() ?: return
        val r = runCatching { parseWire<SearchResult>(sj) }.getOrNull() ?: return
        if (r.kind == "goto" && r.book != null && r.chapter != null) {
            searchHits = emptySet()
            book = r.book!!; chapter = r.chapter!!.toInt()
            studyBlocks = null
        } else {
            searchHits = r.hits?.map { it.verse }?.toSet() ?: emptySet()
            studyBlocks = runCatching { engine.SearchBlocksJson(q) }.getOrNull()
            when (mode) {
                UiMode.FullscreenVertical -> singleView = SingleView.Study
                UiMode.FoldFullscreen -> secondPane = SecondPane.Study
                else -> {}
            }
        }
    }

    // ── link routing (parity-lite): only "go" navigates in v0; TODO the rest ─
    fun onLink(uri: String) {
        val j = runCatching { StudyEngine.RouteLinkJson(uri) }.getOrNull() ?: return
        val link = runCatching { parseWire<PanelLinkData>(j) }.getOrNull() ?: return
        when {
            link.verb == "go" && link.book != null && link.chapter != null -> {
                book = link.book!!; chapter = link.chapter!!.toInt()
            }
            link.verb == "conceptMap" && link.code != null -> conceptCode = link.code
        }
        // TODO: occurrences / codeStudy / tag / thread authoring dialogs.
    }

    val readerFor: @Composable (Modifier, String, Int) -> Unit = { m, b, c ->
        ReaderPane(
            engine = engine, book = b, chapter = c, palette = palette,
            modifier = m, searchHits = searchHits,
            onWordTap = ::onWord,
            onVerseLongPress = { verse -> actionVerse = verse },
            highlightEpoch = highlightEpoch,
        )
    }
    val study: @Composable (Modifier) -> Unit = { m ->
        Box(m.background(palette.panelBg)) { StudyPane(studyBlocks, palette, onLink = ::onLink) }
    }

    Box(Modifier.fillMaxSize()) {
    Column(Modifier.fillMaxSize().background(palette.paper)) {
        TopBar(
            toc = toc, book = book, chapter = chapter, palette = palette,
            searchText = searchText, onSearchChange = { searchText = it },
            onSearchSubmit = { runSearch(searchText) },
            onPrev = { stepPrimary(-1) }, onNext = { stepPrimary(+1) },
            onPickBook = ::openBook,
            mode = mode,
            singlePane = singlePane, onToggleSplit = { singlePane = !singlePane },
            singleStudy = singleView == SingleView.Study,
            onToggleSingleView = {
                singleView = if (singleView == SingleView.Study) SingleView.Bible else SingleView.Study
            },
            secondStudy = secondPane == SecondPane.Study,
            onToggleSecondPane = {
                secondPane = if (secondPane == SecondPane.Study) SecondPane.Bible else SecondPane.Study
            },
            onMemorize = { memView = it },
            onConstellation = { showConstellation = true },
            onChord = { showChord = true },
        )
        HorizontalDivider(color = palette.rule)

        when (mode) {
            UiMode.SplitVertical -> Column(Modifier.fillMaxSize()) {
                readerFor(Modifier.fillMaxWidth().weight(1f), book, chapter)
                HingeSpacerHorizontal(fold)
                HorizontalDivider(color = palette.rule)
                study(Modifier.fillMaxWidth().weight(1f))
            }

            UiMode.FullscreenVertical -> Box(Modifier.fillMaxSize()) {
                if (singleView == SingleView.Study) study(Modifier.fillMaxSize())
                else readerFor(Modifier.fillMaxSize(), book, chapter)
            }

            UiMode.FoldFullscreen -> Row(Modifier.fillMaxSize()) {
                readerFor(Modifier.fillMaxHeight().weight(1f), book, chapter)
                HingeSpacerVertical(fold)
                VerticalDivider(color = palette.rule)
                if (secondPane == SecondPane.Study) study(Modifier.fillMaxHeight().weight(1f))
                else readerFor(Modifier.fillMaxHeight().weight(1f), secondBook, secondChapter)
            }
        }
    }

        // ── overlays / sheets (parity features layered over the reader) ──────
        actionVerse?.let { v ->
            VerseActionSheet(
                engine, palette, v,
                onHighlightsChanged = { highlightEpoch++ },
                onDismiss = { actionVerse = null },
            )
        }
        memView?.let { v -> MemorizeScreen(engine, v, toc, palette, onClose = { memView = null }) }
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

/** Minimal chrome: book picker + chapter nav, a mode/pane toggle, and search. */
@Composable
private fun TopBar(
    toc: List<TocBook>,
    book: String,
    chapter: Int,
    palette: ReaderPalette,
    searchText: String,
    onSearchChange: (String) -> Unit,
    onSearchSubmit: () -> Unit,
    onPrev: () -> Unit,
    onNext: () -> Unit,
    onPickBook: (TocBook) -> Unit,
    mode: UiMode,
    singlePane: Boolean,
    onToggleSplit: () -> Unit,
    singleStudy: Boolean,
    onToggleSingleView: () -> Unit,
    secondStudy: Boolean,
    onToggleSecondPane: () -> Unit,
    onMemorize: (MemorizeView) -> Unit,
    onConstellation: () -> Unit,
    onChord: () -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    val bookName = toc.firstOrNull { it.id == book }?.name ?: book

    Surface(color = palette.paneNavBg) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            TextButton(onClick = onPrev) { Text("‹", fontSize = 20.sp, color = palette.ink) }
            Box {
                TextButton(onClick = { menuOpen = true }) {
                    Text("$bookName $chapter", color = palette.ink)
                }
                DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                    for (b in toc) {
                        DropdownMenuItem(
                            text = { Text(b.name) },
                            onClick = { onPickBook(b); menuOpen = false },
                        )
                    }
                }
            }
            TextButton(onClick = onNext) { Text("›", fontSize = 20.sp, color = palette.ink) }

            OutlinedTextField(
                value = searchText,
                onValueChange = onSearchChange,
                placeholder = { Text("search — word, phrase, or reference") },
                singleLine = true,
                modifier = Modifier.weight(1f),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { onSearchSubmit() }),
            )

            // Mode / pane controls, adapted to the current layout.
            when (mode) {
                UiMode.FoldFullscreen -> TextButton(onClick = onToggleSecondPane) {
                    Text(if (secondStudy) "Study" else "Bible", color = palette.gold)
                }
                UiMode.FullscreenVertical -> {
                    TextButton(onClick = onToggleSingleView) {
                        Text(if (singleStudy) "Study" else "Bible", color = palette.gold)
                    }
                    TextButton(onClick = onToggleSplit) { Text("Split", color = palette.ink) }
                }
                UiMode.SplitVertical -> TextButton(onClick = onToggleSplit) {
                    Text(if (singlePane) "Split" else "Single", color = palette.ink)
                }
            }

            Box {
                var menu by remember { mutableStateOf(false) }
                TextButton(onClick = { menu = true }) { Text("⋮", fontSize = 20.sp, color = palette.ink) }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    Text(
                        "Memorize",
                        color = palette.faded,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                    )
                    DropdownMenuItem(text = { Text("Review due") }, onClick = { onMemorize(MemorizeView.ReviewDue); menu = false })
                    DropdownMenuItem(text = { Text("Coverage map") }, onClick = { onMemorize(MemorizeView.Coverage); menu = false })
                    DropdownMenuItem(text = { Text("Activity") }, onClick = { onMemorize(MemorizeView.Activity); menu = false })
                    HorizontalDivider(color = palette.rule)
                    DropdownMenuItem(text = { Text("Constellation") }, onClick = { onConstellation(); menu = false })
                    DropdownMenuItem(text = { Text("Chord map") }, onClick = { onChord(); menu = false })
                }
            }
        }
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
                Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TextButton(onClick = onClose) { Text("‹", fontSize = 20.sp, color = palette.ink) }
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

/** The stacked-layout equivalent for a separating horizontal (tabletop) hinge. */
@Composable
private fun HingeSpacerHorizontal(fold: FoldingFeature?) {
    val h = hingeThickness(fold, vertical = false)
    if (h > 0.dp) Spacer(Modifier.fillMaxWidth().height(h))
}

@Composable
private fun hingeThickness(fold: FoldingFeature?, vertical: Boolean): Dp {
    if (fold == null || !fold.isSeparating) return 0.dp
    val density = LocalDensity.current
    val px = if (vertical) fold.bounds.width() else fold.bounds.height()
    return with(density) { px.toDp() }
}
