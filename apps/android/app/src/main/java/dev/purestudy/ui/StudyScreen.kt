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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
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
import dev.purestudy.UserNote
import dev.purestudy.ConfigState
import dev.purestudy.PureJson
import dev.purestudy.StudyConfig
import dev.purestudy.parseWire
import kotlinx.serialization.encodeToString

/** What the second (right) pane shows in fold mode. */
private enum class SecondPane { Study, Bible }

/** The single-pane (mode 2) view choice. */
private enum class SingleView { Bible, Study }

/** A study "library" the ≡ menu loads into the study pane as blocks. */
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
    var fullMode by remember { mutableStateOf(true) }               // Simple vs Full study depth
    var wordSheet by remember { mutableStateOf(false) }             // word study as a sheet (narrow)
    var prompt by remember { mutableStateOf<AuthorPrompt?>(null) }   // text-input authoring dialog

    // Shared body text size (config.bodySize) — scales the reader + study pane,
    // persisted to the cross-shell config so it survives restarts.
    val loadedCfg = remember { runCatching { parseWire<ConfigState>(StudyConfig.LoadJson()) }.getOrNull() }
    var bodySize by remember { mutableStateOf((loadedCfg?.bodySize ?: 18.0).coerceIn(12.0, 40.0)) }
    var showTextSize by remember { mutableStateOf(false) }
    fun persistBodySize() {
        val cfg = (loadedCfg ?: ConfigState()).copy(bodySize = bodySize)
        runCatching { StudyConfig.SaveJson(PureJson.encodeToString(cfg)) }
    }

    val base = rememberUiMode(fold)
    val mode = when {
        base == UiMode.FoldFullscreen -> UiMode.FoldFullscreen
        singlePane -> UiMode.FullscreenVertical
        else -> UiMode.SplitVertical
    }

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

    // Reveal the study surface for the current mode (deeper study / search / links).
    fun revealStudy() {
        when (mode) {
            UiMode.FullscreenVertical -> singleView = SingleView.Study
            UiMode.FoldFullscreen -> secondPane = SecondPane.Study
            else -> {}
        }
    }

    // ── word tap → word study. On a narrow single screen it's a bottom sheet (a
    //    quick look-up that keeps the reading place); elsewhere the study pane. ─
    fun onWord(hit: Hit) {
        studyBlocks = runCatching {
            engine.WordStudyBlocksJson(hit.verse, hit.tokenIndex.toInt(), fullMode)
        }.getOrNull()
        when (mode) {
            UiMode.FullscreenVertical -> wordSheet = true
            UiMode.FoldFullscreen -> secondPane = SecondPane.Study
            else -> {}
        }
    }

    // Load a study library (threads / tags / weaves / suggested / guide / about)
    // into the study pane — StudyPane renders each block list identically.
    fun openLibrary(which: Library) {
        val b = when (which) {
            Library.Threads -> engine.ThreadsBlocksJson()
            Library.Tags -> engine.TagsBlocksJson()
            Library.Weaves -> engine.WeavesBlocksJson()
            Library.Suggested -> engine.SuggestedBlocksJson()
            Library.Guide -> StudyEngine.GuideBlocksJson()
            Library.About -> StudyEngine.AboutBlocksJson()
        }
        if (b != null) { studyBlocks = b; revealStudy() }
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
            revealStudy()
        }
    }

    // ── link routing: navigate, open a map, or load a study card into the pane.
    //    Authoring verbs (addTag/addThread/approve/reject/edit*) are pass 3. ────
    fun onLink(uri: String) {
        val j = runCatching { StudyEngine.RouteLinkJson(uri) }.getOrNull() ?: return
        val link = runCatching { parseWire<PanelLinkData>(j) }.getOrNull() ?: return
        fun show(blocks: String?) {
            if (blocks != null) { studyBlocks = blocks; wordSheet = false; revealStudy() }
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

    // A Bible pane = its own compact ‹ Book Ch › header (per-pane navigation) +
    // the reader. `isSecond` picks the primary or the second pane's state.
    val biblePane: @Composable (Modifier, Boolean) -> Unit = { m, isSecond ->
        val b = if (isSecond) secondBook else book
        val c = if (isSecond) secondChapter else chapter
        Column(m) {
            PaneHeader(
                toc = toc, book = b, chapter = c, palette = palette,
                onPrev = {
                    val (nb, nc) = step(b, c, -1)
                    if (isSecond) { secondBook = nb; secondChapter = nc } else { book = nb; chapter = nc }
                },
                onNext = {
                    val (nb, nc) = step(b, c, +1)
                    if (isSecond) { secondBook = nb; secondChapter = nc } else { book = nb; chapter = nc }
                },
                onPick = { bk ->
                    if (isSecond) { secondBook = bk.id; secondChapter = 1 } else { book = bk.id; chapter = 1 }
                },
            )
            HorizontalDivider(color = palette.rule)
            ReaderPane(
                engine = engine, book = b, chapter = c, palette = palette,
                modifier = Modifier.weight(1f), searchHits = searchHits, fontSizeSp = bodySize.toFloat(),
                onWordTap = ::onWord,
                onVerseLongPress = { verse -> actionVerse = verse },
                highlightEpoch = highlightEpoch,
            )
        }
    }
    val study: @Composable (Modifier) -> Unit = { m ->
        Box(m.background(palette.panelBg)) {
            StudyPane(studyBlocks, palette, onLink = ::onLink, scale = (bodySize / 18.0).toFloat())
        }
    }

    // systemBarsPadding keeps the app's chrome out from under the status bar
    // (top) + gesture nav (bottom) — Android 15 (targetSdk 35) draws edge-to-edge
    // by default, which otherwise put the top bar under the clock/wifi and ate
    // taps there. The window background (paper) fills the bars for a seamless look.
    Box(Modifier.fillMaxSize().systemBarsPadding()) {
    Column(Modifier.fillMaxSize().background(palette.paper)) {
        TopBar(
            palette = palette,
            searchText = searchText, onSearchChange = { searchText = it },
            onSearchSubmit = { runSearch(searchText) },
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
            onLibrary = ::openLibrary,
            fullStudy = fullMode,
            onToggleFull = { fullMode = !fullMode },
            onTextSize = { showTextSize = true },
            bundledOn = bundledOn,
            onToggleBundled = onToggleBundled,
        )
        HorizontalDivider(color = palette.rule)

        when (mode) {
            UiMode.SplitVertical -> Column(Modifier.fillMaxSize()) {
                biblePane(Modifier.fillMaxWidth().weight(1f), false)
                HingeSpacerHorizontal(fold)
                HorizontalDivider(color = palette.rule)
                study(Modifier.fillMaxWidth().weight(1f))
            }

            UiMode.FullscreenVertical -> Box(Modifier.fillMaxSize()) {
                if (singleView == SingleView.Study) study(Modifier.fillMaxSize())
                else biblePane(Modifier.fillMaxSize(), false)
            }

            UiMode.FoldFullscreen -> Row(Modifier.fillMaxSize()) {
                biblePane(Modifier.fillMaxHeight().weight(1f), false)
                HingeSpacerVertical(fold)
                VerticalDivider(color = palette.rule)
                if (secondPane == SecondPane.Study) study(Modifier.fillMaxHeight().weight(1f))
                else biblePane(Modifier.fillMaxHeight().weight(1f), true)
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
        if (wordSheet) WordStudySheet(studyBlocks, palette, (bodySize / 18.0).toFloat(), ::onLink) { wordSheet = false }
        if (showTextSize) {
            AlertDialog(
                onDismissRequest = { showTextSize = false; persistBodySize() },
                title = { Text("Text size") },
                text = {
                    Column {
                        Text("Aa — reader & study", fontSize = bodySize.sp, color = palette.ink)
                        Slider(
                            value = bodySize.toFloat(),
                            onValueChange = { bodySize = it.toDouble() },
                            valueRange = 12f..40f,
                            steps = 27,
                        )
                    }
                },
                confirmButton = {
                    TextButton(onClick = { showTextSize = false; persistBodySize() }) { Text("Done") }
                },
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

/** A pane's own compact navigation: ‹ Book Ch › with a book picker. One per
 *  Bible pane, so the left pane and the second (fold) pane navigate
 *  independently — no shared or duplicated toolbar. */
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
            Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TextButton(onClick = onPrev) { Text("‹", fontSize = 18.sp, color = palette.ink) }
            Box {
                TextButton(onClick = { menu = true }) { Text("$name $chapter", color = palette.ink) }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    for (b in toc) {
                        DropdownMenuItem(text = { Text(b.name) }, onClick = { onPick(b); menu = false })
                    }
                }
            }
            TextButton(onClick = onNext) { Text("›", fontSize = 18.sp, color = palette.ink) }
        }
    }
}

/** Top chrome: search, the mode/pane toggle, and the ≡ menu. Book/chapter
 *  navigation lives per-pane in [PaneHeader] now. */
@Composable
private fun TopBar(
    palette: ReaderPalette,
    searchText: String,
    onSearchChange: (String) -> Unit,
    onSearchSubmit: () -> Unit,
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
    onLibrary: (Library) -> Unit,
    fullStudy: Boolean,
    onToggleFull: () -> Unit,
    onTextSize: () -> Unit,
    bundledOn: Boolean,
    onToggleBundled: () -> Unit,
) {
    Surface(color = palette.paneNavBg) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
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
                    HorizontalDivider(color = palette.rule)
                    Text(
                        "Study",
                        color = palette.faded,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                    )
                    DropdownMenuItem(text = { Text("Threads") }, onClick = { onLibrary(Library.Threads); menu = false })
                    DropdownMenuItem(text = { Text("Tags") }, onClick = { onLibrary(Library.Tags); menu = false })
                    DropdownMenuItem(text = { Text("Weave library") }, onClick = { onLibrary(Library.Weaves); menu = false })
                    DropdownMenuItem(text = { Text("Suggested weaves") }, onClick = { onLibrary(Library.Suggested); menu = false })
                    DropdownMenuItem(text = { Text("Guide") }, onClick = { onLibrary(Library.Guide); menu = false })
                    DropdownMenuItem(text = { Text("About") }, onClick = { onLibrary(Library.About); menu = false })
                    HorizontalDivider(color = palette.rule)
                    DropdownMenuItem(
                        text = { Text(if (fullStudy) "Full study  ✓" else "Full study") },
                        onClick = { onToggleFull(); menu = false },
                    )
                    DropdownMenuItem(text = { Text("Text size…") }, onClick = { onTextSize(); menu = false })
                    DropdownMenuItem(
                        text = { Text(if (bundledOn) "Bundled study set  ✓" else "Bundled study set") },
                        onClick = { onToggleBundled(); menu = false },
                    )
                }
            }
        }
    }
}

/** Word study as a bottom sheet — the narrow-screen quick look-up (the reader
 *  stays behind it). Links inside route through [onLink] like the pane does. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun WordStudySheet(
    blocksJson: String?,
    palette: ReaderPalette,
    scale: Float,
    onLink: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = palette.panelBg) {
        Box(Modifier.fillMaxWidth().heightIn(max = 480.dp)) {
            StudyPane(blocksJson, palette, onLink = onLink, scale = scale)
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
