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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
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

    // Primary Bible pane state (defaults to John 3, like the desktop shells).
    var book by remember { mutableStateOf("John") }
    var chapter by remember { mutableStateOf(3) }
    // Second Bible pane state (for Bible∥Bible in fold mode).
    var secondBook by remember { mutableStateOf("John") }
    var secondChapter by remember { mutableStateOf(3) }

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
    var prompt by remember { mutableStateOf<AuthorPrompt?>(null) }   // text-input authoring dialog

    // Shared reader prefs (config): body size + horizontal margin + line spacing +
    // default copy shape. Persisted to the cross-shell config so they survive
    // restarts and (eventually) carry across shells.
    val loadedCfg = remember { runCatching { parseWire<ConfigState>(StudyConfig.LoadJson()) }.getOrNull() }
    var bodySize by remember { mutableStateOf((loadedCfg?.bodySize ?: 18.0).coerceIn(12.0, 40.0)) }
    var sideMargin by remember { mutableStateOf((loadedCfg?.sideMargin ?: 28.0).coerceIn(8.0, 96.0)) }
    var lineSpacing by remember { mutableStateOf((loadedCfg?.lineSpacing ?: 1.35).coerceIn(1.0, 2.2)) }
    var copyStyle by remember { mutableStateOf(loadedCfg?.copyStyle ?: "verseRef") }
    var showReading by remember { mutableStateOf(false) }
    var showCopyFormat by remember { mutableStateOf(false) }
    fun persistCfg() {
        val cfg = (loadedCfg ?: ConfigState()).copy(
            bodySize = bodySize, sideMargin = sideMargin, lineSpacing = lineSpacing, copyStyle = copyStyle,
        )
        runCatching { StudyConfig.SaveJson(PureJson.encodeToString(cfg)) }
    }
    val studyScale = (bodySize / 18.0).toFloat()

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

    // ── word tap → word study (bottom sheet on a phone, right pane on the fold) ─
    fun onWord(hit: Hit) {
        studyBlocks = runCatching {
            engine.WordStudyBlocksJson(hit.verse, hit.tokenIndex.toInt(), fullMode)
        }.getOrNull()
        revealStudy()
    }

    // Load a study library (threads / tags / weaves / suggested / guide / about)
    // into the study surface — StudyPane renders each block list identically.
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

    // systemBarsPadding keeps the app's chrome out from under the status bar
    // (top) + gesture nav (bottom) — Android 15 (targetSdk 35) draws edge-to-edge
    // by default, which otherwise put the top bar under the clock/wifi and ate
    // taps there. The window background (paper) fills the bars for a seamless look.
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
            onMemorize = { memView = it },
            onConstellation = { showConstellation = true },
            onChord = { showChord = true },
            onLibrary = ::openLibrary,
            fullStudy = fullMode,
            onToggleFull = { fullMode = !fullMode },
            onReading = { showReading = true },
            onCopyFormat = { showCopyFormat = true },
            bundledOn = bundledOn,
            onToggleBundled = onToggleBundled,
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
        actionVerse?.let { v ->
            VerseActionSheet(
                engine, palette, v,
                copyStyle = copyStyle,
                onHighlightsChanged = { highlightEpoch++ },
                onDismiss = { actionVerse = null },
            )
        }
        memView?.let { v -> MemorizeScreen(engine, v, toc, palette, onClose = { memView = null }) }
        if (showReading) {
            AlertDialog(
                onDismissRequest = { showReading = false; persistCfg() },
                title = { Text("Text & spacing") },
                text = {
                    Column {
                        Text("Size — reader & study", color = palette.faded, fontSize = 12.sp)
                        Text("Aa", fontSize = bodySize.sp, color = palette.ink)
                        Slider(
                            value = bodySize.toFloat(),
                            onValueChange = { bodySize = it.toDouble() },
                            valueRange = 12f..40f, steps = 27,
                        )
                        Text("Margin — space either side of the text", color = palette.faded, fontSize = 12.sp)
                        Slider(
                            value = sideMargin.toFloat(),
                            onValueChange = { sideMargin = it.toDouble() },
                            valueRange = 8f..96f,
                        )
                        Text("Line spacing", color = palette.faded, fontSize = 12.sp)
                        Slider(
                            value = lineSpacing.toFloat(),
                            onValueChange = { lineSpacing = it.toDouble() },
                            valueRange = 1.0f..2.2f,
                        )
                    }
                },
                confirmButton = {
                    TextButton(onClick = { showReading = false; persistCfg() }) { Text("Done") }
                },
            )
        }
        if (showCopyFormat) {
            CopyFormatDialog(
                current = copyStyle, palette = palette,
                onPick = { copyStyle = it; persistCfg() },
                onDismiss = { showCopyFormat = false },
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

/** Top chrome: on a phone, inline ‹ Book Ch › nav; then a search icon and an
 *  overflow (⋮) menu. Icons over text (Glendon's Android direction). */
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
    onMemorize: (MemorizeView) -> Unit,
    onConstellation: () -> Unit,
    onChord: () -> Unit,
    onLibrary: (Library) -> Unit,
    fullStudy: Boolean,
    onToggleFull: () -> Unit,
    onReading: () -> Unit,
    onCopyFormat: () -> Unit,
    bundledOn: Boolean,
    onToggleBundled: () -> Unit,
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

            Box {
                var menu by remember { mutableStateOf(false) }
                IconButton(onClick = { menu = true }) {
                    Icon(Icons.Filled.MoreVert, contentDescription = "Menu", tint = palette.ink)
                }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    if (mode == UiMode.FoldFullscreen) {
                        DropdownMenuItem(
                            text = { Text(if (secondStudy) "Right pane: Study  ✓" else "Right pane: second Bible") },
                            onClick = { onToggleSecondPane(); menu = false },
                        )
                        HorizontalDivider(color = palette.rule)
                    }
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
                    DropdownMenuItem(text = { Text("Text & spacing…") }, onClick = { onReading(); menu = false })
                    DropdownMenuItem(text = { Text("Copy format…") }, onClick = { onCopyFormat(); menu = false })
                    DropdownMenuItem(
                        text = { Text(if (bundledOn) "Bundled study set  ✓" else "Bundled study set") },
                        onClick = { onToggleBundled(); menu = false },
                    )
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
                StudyPane(blocks, palette, onLink = { uri -> onLink(uri); onClose() }, scale = scale)
            }
        }
    }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
}

/** Choose the default one-tap copy shape (persisted to config). */
@Composable
private fun CopyFormatDialog(
    current: String,
    palette: ReaderPalette,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val options = listOf(
        "verse" to "Verse text only",
        "verseRef" to "Verse with reference",
        "verseMarkdown" to "Markdown blockquote",
    )
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Copy format") },
        text = {
            Column {
                for ((token, label) in options) {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clickable { onPick(token); onDismiss() }
                            .padding(vertical = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(selected = current == token, onClick = { onPick(token); onDismiss() })
                        Text(label, color = palette.ink)
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Done") } },
    )
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
