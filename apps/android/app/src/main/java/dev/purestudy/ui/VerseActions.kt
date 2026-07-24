// The Tier-0 verse-action sheet (Tier 0 #1 + #4): a Material3 ModalBottomSheet the
// reader opens on a long-press over a verse. It mirrors the GTK context menu
// (apps/desktop/src/main.rs show_context_menu) and the WinUI flyout
// (apps/windows/PureStudyWin/MainWindow.cs ShowContextMenu), reduced to a touch
// sheet: copy shapes (+ an Android share), a personal note, a highlight tone with
// Glendon's verse-then-trim mechanic, and "Memorize this verse".
//
// All study logic stays across the ABI — this composable only orchestrates
// StudyEngine calls and paints their affordances. Every mutating call runs off the
// main thread under `synchronized(engine)` (two reader panes may touch the engine
// at once, exactly as ReaderPane serialises its layout/hit-test calls).
//
// The highlight mechanic (verse-then-trim), documented once here:
//   1. Pick a tone → the WHOLE verse is washed: HighlightAdd(tag, hex, ref, 0,
//      ref, lastTok). This lays down one word-precise run spanning every token, so
//      the reader paints it per-word (ReaderPane's `runs` pass).
//   2. Trim → tap a word chip in the sheet. The boundary nearest the tapped token
//      moves to it (tap left/right of the range extends that end; tap inside pulls
//      the nearer end in). Each trim re-issues the range via HighlightClearVerse +
//      HighlightAdd, so ranges never accumulate.
//   3. Remove → HighlightClearVerse drops every run covering the verse.
// The tag name is the capitalised tone ("amber" → "Amber"), matching GTK/WinUI so
// a highlight laid down on any shell reads identically on the others.
//
// Author D (Compose UI).

package dev.purestudy.ui

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.purestudy.ChapterHighlights
import dev.purestudy.HighlightTone
import dev.purestudy.HighlightTones
import dev.purestudy.StudyEngine
import dev.purestudy.UserNote
import dev.purestudy.VerseData
import dev.purestudy.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.Instant

/** The refKey split into its book id / chapter / verse (`"1John 3:16"` →
 *  book "1John", ch 3, v 16). Book ids carry no space (OSIS: `canon.rs`), so the
 *  last space cleanly divides the id from `chapter:verse`. */
private data class RefParts(val book: String, val chapter: Int, val verse: Int)

private fun parseRef(ref: String): RefParts? {
    val sp = ref.lastIndexOf(' ')
    if (sp <= 0) return null
    val book = ref.substring(0, sp)
    val cv = ref.substring(sp + 1)
    val colon = cv.indexOf(':')
    if (colon <= 0) return null
    val chapter = cv.substring(0, colon).toIntOrNull() ?: return null
    val verse = cv.substring(colon + 1).toIntOrNull() ?: return null
    return RefParts(book, chapter, verse)
}

/** "amber" → "Amber": the tag name a tone highlights under (matches GTK
 *  `highlight_verse` and WinUI `HighlightVerse`). */
private fun toneTag(tone: String): String =
    if (tone.isEmpty()) tone else tone.substring(0, 1).uppercase() + tone.substring(1)

/**
 * Move the [start, end] boundary nearest [tapped] to it — the verse-then-trim rule.
 * Tapping left of the range extends the start; right of it extends the end; inside
 * it pulls the nearer end inward. Endpoints are inclusive token indices.
 */
internal fun trimRange(start: Int, end: Int, tapped: Int): Pair<Int, Int> = when {
    tapped < start -> tapped to end
    tapped > end -> start to tapped
    tapped - start <= end - tapped -> tapped to end
    else -> start to tapped
}

/**
 * The verse-action sheet. Opened by the reader's long-press (see the wiring spec
 * in the task) with the verse [verseRef] (a refKey, e.g. `"John 3:16"`) and,
 * optionally, its [tokenCount]; a non-positive [tokenCount] is resolved from the
 * engine. [onHighlightsChanged] must make the reader re-fetch `ChapterHighlightsJson`
 * and repaint (highlights changed). [onDismiss] tears the sheet down.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun VerseActionSheet(
    engine: StudyEngine,
    palette: ReaderPalette,
    verseRef: String,
    tokenCount: Int = -1,
    onHighlightsChanged: () -> Unit = {},
    onDismiss: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val clipboard = LocalClipboardManager.current
    val context = LocalContext.current

    // Loaded on open: the verse display + its tokens (for the trim chips), the
    // fixed tone swatches, and any highlight already on this verse (to re-open in
    // trim state). Keyed on verseRef so re-targeting resets cleanly.
    var display by remember(verseRef) { mutableStateOf(verseRef) }
    var tokens by remember(verseRef) { mutableStateOf<List<String>>(emptyList()) }
    var tones by remember { mutableStateOf<List<HighlightTone>>(emptyList()) }
    var appliedTone by remember(verseRef) { mutableStateOf<HighlightTone?>(null) }
    var start by remember(verseRef) { mutableStateOf(0) }
    var end by remember(verseRef) { mutableStateOf(0) }

    var showNote by remember(verseRef) { mutableStateOf(false) }
    var noteText by remember(verseRef) { mutableStateOf("") }
    var noteLoaded by remember(verseRef) { mutableStateOf(false) }

    // The highest valid token index — from the fetched tokens, else the hint param.
    val lastTok = if (tokens.isNotEmpty()) tokens.lastIndex else tokenCount - 1

    LaunchedEffect(verseRef) {
        tones = withContext(Dispatchers.Default) {
            runCatching { parseWire<HighlightTones>(StudyEngine.HighlightTonesJson()).tones }
                .getOrElse { emptyList() }
        }
        val vd = withContext(Dispatchers.Default) {
            runCatching { synchronized(engine) { engine.VerseJson(verseRef) } }.getOrNull()
                ?.let { runCatching { parseWire<VerseData>(it) }.getOrNull() }
        }
        if (vd != null) {
            display = vd.display.ifBlank { verseRef }
            // Keep index i == token index i (frozen kjv1769-tok2); label with the
            // surface word, falling back to punctuation so every slot is tappable.
            tokens = vd.tokens.map { it.word.ifBlank { (it.pre + it.post).ifBlank { "·" } } }
        }
        // Prefill from any run already washing this verse, so re-opening lands in
        // trim mode on the live range.
        parseRef(verseRef)?.let { parts ->
            val ch = withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { engine.ChapterHighlightsJson(parts.book, parts.chapter) } }
                    .getOrNull()?.let { runCatching { parseWire<ChapterHighlights>(it) }.getOrNull() }
            }
            ch?.runs?.firstOrNull { it.verse == verseRef }?.let { run ->
                start = run.lo
                end = run.hi
                appliedTone = tones.firstOrNull { it.hex.equals(run.color, ignoreCase = true) }
                    ?: HighlightTone(name = "custom", hex = run.color)
            }
        }
    }

    // Lazy-load the note text the first time the dialog is opened.
    LaunchedEffect(showNote) {
        if (showNote && !noteLoaded) {
            noteText = withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { engine.UserNoteJson(verseRef) } }.getOrNull()
                    ?.let { runCatching { parseWire<UserNote>(it).text }.getOrNull() }
            } ?: ""
            noteLoaded = true
        }
    }

    fun hide() {
        scope.launch { sheetState.hide() }.invokeOnCompletion {
            if (!sheetState.isVisible) onDismiss()
        }
    }

    // Copy (or share) one CopyKind shape. Clipboard/share touch the UI thread; the
    // engine call is off it.
    fun copy(kind: String, share: Boolean = false) {
        scope.launch {
            val text = withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { engine.CopyText(verseRef, kind) } }.getOrNull()
            }
            if (text != null) {
                if (share) {
                    val send = Intent(Intent.ACTION_SEND).apply {
                        type = "text/plain"
                        putExtra(Intent.EXTRA_TEXT, text)
                    }
                    runCatching { context.startActivity(Intent.createChooser(send, "Share verse")) }
                } else {
                    clipboard.setText(AnnotatedString(text))
                }
            }
            hide()
        }
    }

    fun memorize() {
        scope.launch {
            withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { engine.MemoryAdd(verseRef, Instant.now().toString()) } }
            }
            hide()
        }
    }

    fun saveNote(text: String) {
        scope.launch {
            withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { engine.UserNoteSet(verseRef, text, Instant.now().toString()) } }
            }
            showNote = false
            hide()
        }
    }

    // Re-paint the current [start,end] range under [tone]: clear this verse's runs,
    // then re-add the trimmed range. Fires the reader re-paint hook.
    fun paintRange(tone: HighlightTone, lo: Int, hi: Int) {
        if (lastTok < 0) return
        scope.launch {
            withContext(Dispatchers.Default) {
                synchronized(engine) {
                    engine.HighlightClearVerse(verseRef)
                    engine.HighlightAdd(
                        toneTag(tone.name), tone.hex,
                        verseRef, lo, verseRef, hi, Instant.now().toString(),
                    )
                }
            }
            onHighlightsChanged()
        }
    }

    fun applyTone(tone: HighlightTone) {
        if (lastTok < 0) return
        appliedTone = tone
        start = 0
        end = lastTok
        paintRange(tone, 0, lastTok)
    }

    fun trimTo(tapped: Int) {
        val tone = appliedTone ?: return
        val (lo, hi) = trimRange(start, end, tapped)
        start = lo
        end = hi
        paintRange(tone, lo, hi)
    }

    fun removeHighlight() {
        scope.launch {
            withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { engine.HighlightClearVerse(verseRef) } }
            }
            appliedTone = null
            onHighlightsChanged()
        }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = palette.panelBg,
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .navigationBarsPadding()
                .padding(horizontal = 16.dp),
        ) {
            Text(
                display,
                color = palette.ink,
                fontSize = 18.sp,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(vertical = 8.dp),
            )
            HorizontalDivider(color = palette.rule)

            // ── copy shapes + share (Tier 0 #1) ─────────────────────────────
            ActionRow("Copy verse", palette.ink) { copy("verse") }
            ActionRow("Copy with reference", palette.ink) { copy("verseRef") }
            ActionRow("Copy (markdown)", palette.ink) { copy("verseMarkdown") }
            ActionRow("Copy chapter", palette.ink) { copy("chapter") }
            ActionRow("Share…", palette.ink) { copy("verseRef", share = true) }
            HorizontalDivider(color = palette.rule)

            // ── note + memorize ─────────────────────────────────────────────
            ActionRow("Note…", palette.ink) { showNote = true }
            ActionRow("Memorize this verse", palette.ink) { memorize() }
            HorizontalDivider(color = palette.rule)

            // ── highlight: tones, then verse-then-trim (Tier 0 #4) ───────────
            Text(
                "Highlight",
                color = palette.inkFaded,
                fontSize = 13.sp,
                modifier = Modifier.padding(top = 12.dp, bottom = 6.dp),
            )
            FlowRow(
                Modifier.fillMaxWidth().padding(bottom = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                for (tone in tones) {
                    val selected = appliedTone?.name == tone.name
                    Box(
                        Modifier
                            .size(36.dp)
                            .clip(CircleShape)
                            .background(ReaderPalette.hex(tone.hex), CircleShape)
                            .border(
                                width = if (selected) 3.dp else 1.dp,
                                color = if (selected) palette.gold else palette.rule,
                                shape = CircleShape,
                            )
                            .clickable(enabled = lastTok >= 0) { applyTone(tone) },
                    )
                }
            }

            if (appliedTone != null && tokens.isNotEmpty()) {
                val tone = appliedTone!!
                Text(
                    "Tap a word to trim the highlight to it.",
                    color = palette.inkFaded,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(top = 8.dp, bottom = 6.dp),
                )
                FlowRow(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    val wash = palette.wash(ReaderPalette.hex(tone.hex))
                    tokens.forEachIndexed { i, word ->
                        val inRange = i in start..end
                        Box(
                            Modifier
                                .clip(RoundedCornerShape(4.dp))
                                .background(if (inRange) wash else Color.Transparent, RoundedCornerShape(4.dp))
                                .border(1.dp, palette.rule, RoundedCornerShape(4.dp))
                                .clickable { trimTo(i) }
                                .padding(horizontal = 8.dp, vertical = 6.dp),
                        ) {
                            Text(word, color = palette.ink, fontSize = 15.sp)
                        }
                    }
                }
            }

            ActionRow("Remove highlight", palette.ink) { removeHighlight() }
            Spacer(Modifier.height(12.dp))
        }
    }

    if (showNote && noteLoaded) {
        NoteDialog(
            initial = noteText,
            palette = palette,
            onSave = { saveNote(it) },
            onCancel = { showNote = false },
        )
    }
}

/** A full-width tappable action row (a touch-friendly menu item). */
@Composable
private fun ActionRow(label: String, color: Color, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(vertical = 12.dp, horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = color, fontSize = 16.sp)
    }
}

/** The personal-note editor. Empty text clears the note (UserNoteSet contract). */
@Composable
private fun NoteDialog(
    initial: String,
    palette: ReaderPalette,
    onSave: (String) -> Unit,
    onCancel: () -> Unit,
) {
    var text by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onCancel,
        confirmButton = { TextButton(onClick = { onSave(text) }) { Text("Save") } },
        dismissButton = { TextButton(onClick = onCancel) { Text("Cancel") } },
        title = { Text("Note", color = palette.ink) },
        text = {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                placeholder = { Text("Your note (leave empty to clear)") },
                modifier = Modifier.fillMaxWidth(),
                minLines = 3,
            )
        },
        containerColor = palette.panelBg,
    )
}
