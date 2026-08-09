// The verse-action sheet: a Material3 ModalBottomSheet the reader opens on a
// long-press over a verse. Copy (in the reader's chosen shape) · Copy chapter ·
// Share · Tag… · Note… · Add to thread… · Memorize · Mark chapter read….
//
// All study logic stays across the ABI — this composable only orchestrates
// StudyEngine calls and paints their affordances. Every mutating call runs off the
// main thread under `synchronized(engine)` (two reader panes may touch the engine
// at once, exactly as ReaderPane serialises its layout/hit-test calls).
//
// No highlighting: tags, notes and threads are the ways to mark and tie together
// scripture. Three ways to mark a verse was two too many, and the swatches were
// the loudest thing in a sheet opened to copy a verse.
//
// Author D (Compose UI).

package dev.plumbline.ui

import android.content.Intent
import android.widget.Toast
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
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDefaults
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.SelectableDates
import androidx.compose.material3.rememberDatePickerState
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
import dev.plumbline.StudyEngine
import dev.plumbline.Tag1
import dev.plumbline.Tags
import dev.plumbline.Thread1
import dev.plumbline.Threads
import dev.plumbline.UserNote
import dev.plumbline.VerseData
import dev.plumbline.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset

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

/**
 * What a write the engine answered means for the surface that asked for it.
 *
 * Every authoring endpoint's contract is "null = success, else an error message"
 * (StudyEngine), and a `runCatching` around one can come back holding a thrown
 * exception instead (a dead native library). Dropping either and closing anyway
 * makes a save that failed — disk full, a refused write, a bad ref — look exactly
 * like one that worked, and the reader's words leave with the sheet. So the
 * decision is one value: close, or stay open and say why.
 */
sealed interface SaveOutcome {
    /** The engine wrote it: the surface may close (or advance). */
    data object Saved : SaveOutcome

    /** It did not land. Keep the surface open, with the reader's text still in the
     *  field, and put [message] where they are looking. */
    data class Failed(val message: String) : SaveOutcome
}

/** Read one engine write — `runCatching { engine.Something(…) }` — as a
 *  [SaveOutcome]. A blank message counts as success (the ABI answers null, but an
 *  empty string means the same thing); an exception with no message of its own
 *  still gets human copy, because a reason-less "not saved" reads as a glitch. */
fun saveOutcome(attempt: Result<String?>): SaveOutcome {
    val thrown = attempt.exceptionOrNull()
    if (thrown != null) {
        return SaveOutcome.Failed(thrown.message?.takeIf { it.isNotBlank() } ?: "the engine stopped answering")
    }
    val err = attempt.getOrNull()
    return if (err.isNullOrBlank()) SaveOutcome.Saved else SaveOutcome.Failed(err)
}

/** The note editor's failure line: the engine's own words, then what the dialog is
 *  doing about it — it is still open, so the note can be retried or lifted out. */
fun noteSaveFailedLine(message: String): String =
    t("notes.notSaved", "why" to message)

/**
 * The verse-action sheet. Opened by the reader's long-press with the verse
 * [verseRef] (a refKey, e.g. `"John 3:16"`) and, optionally, its [tokenCount]; a
 * non-positive [tokenCount] is resolved from the engine. [onDismiss] tears the
 * sheet down.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun VerseActionSheet(
    engine: StudyEngine,
    palette: ReaderPalette,
    verseRef: String,
    tokenCount: Int = -1,
    // The reader's chosen copy shape (config): a single "Copy" honours it instead
    // of listing every variant. One of "verse" / "verseRef" / "verseMarkdown".
    copyStyle: String = "verseRef",
    /** Open the tag picker for this verse — tags are how a topic accumulates. */
    onTag: (String) -> Unit = {},
    onDismiss: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val clipboard = LocalClipboardManager.current
    val context = LocalContext.current

    // Loaded on open: the verse display + its tokens. Keyed on verseRef so
    // re-targeting resets cleanly.
    var display by remember(verseRef) { mutableStateOf(verseRef) }
    var tokens by remember(verseRef) { mutableStateOf<List<String>>(emptyList()) }

    var showNote by remember(verseRef) { mutableStateOf(false) }
    var showPassage by remember(verseRef) { mutableStateOf(false) }
    var noteText by remember(verseRef) { mutableStateOf("") }
    var noteLoaded by remember(verseRef) { mutableStateOf(false) }
    // The note dialog's last failure, shown inside it. It stays until the next
    // attempt, because until then it is still true: the note is not saved.
    var noteError by remember(verseRef) { mutableStateOf<String?>(null) }
    // The pending ask for the one destructive act in this sheet: saving an
    // emptied note editor, which deletes the note.
    var confirmDelete by remember(verseRef) { mutableStateOf<ConfirmRequest?>(null) }

    // The highest valid token index — from the fetched tokens, else the hint param.
    val lastTok = if (tokens.isNotEmpty()) tokens.lastIndex else tokenCount - 1

    LaunchedEffect(verseRef) {
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
                    runCatching { context.startActivity(Intent.createChooser(send, t("menu.shareLink"))) }
                } else {
                    clipboard.setText(AnnotatedString(text))
                }
            }
            hide()
        }
    }

    fun memorize() {
        scope.launch {
            val outcome = withContext(Dispatchers.Default) {
                saveOutcome(
                    runCatching { synchronized(engine) { engine.MemoryAdd(verseRef, Instant.now().toString()) } },
                )
            }
            Toast.makeText(
                context,
                when (outcome) {
                    is SaveOutcome.Saved -> t("memorize.added", "passage" to display)
                    is SaveOutcome.Failed -> t("memorize.notAdded", "why" to outcome.message)
                },
                Toast.LENGTH_SHORT,
            ).show()
            hide()
        }
    }

    /** Memorize a whole section as ONE card: the long-pressed verse starts it,
     *  the reader taps the end verse in [PassageEndPicker]. */
    fun memorizePassage(endVerse: Int) {
        val parts = parseRef(verseRef) ?: return
        val throughRef = "${parts.book} ${parts.chapter}:$endVerse"
        scope.launch {
            val outcome = withContext(Dispatchers.Default) {
                saveOutcome(
                    runCatching {
                        synchronized(engine) {
                            engine.MemoryAddPassage(verseRef, throughRef, Instant.now().toString())
                        }
                    },
                )
            }
            Toast.makeText(
                context,
                when (outcome) {
                    is SaveOutcome.Saved -> t("memorize.memorizingRange", "passage" to display, "end" to endVerse)
                    is SaveOutcome.Failed -> t("memorize.notAdded", "why" to outcome.message)
                },
                Toast.LENGTH_SHORT,
            ).show()
            hide()
        }
    }

    /** Write the note. Only a write the engine took closes anything — a failed save
     *  keeps the dialog, and the reader's words in it, so they can retry or copy
     *  them out. Their text is the one thing in this sheet that cannot be fetched
     *  again. */
    fun saveNote(text: String) {
        noteError = null
        scope.launch {
            val outcome = withContext(Dispatchers.Default) {
                saveOutcome(
                    runCatching {
                        synchronized(engine) { engine.UserNoteSet(verseRef, text, Instant.now().toString()) }
                    },
                )
            }
            when (outcome) {
                is SaveOutcome.Saved -> {
                    showNote = false
                    hide()
                }
                // Hold their text as the dialog's `initial` too, so it comes back
                // with what they wrote if it is ever recomposed from scratch.
                is SaveOutcome.Failed -> {
                    noteText = text
                    noteError = noteSaveFailedLine(outcome.message)
                }
            }
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

            // ── copy + share (Tier 0 #1) — one "Copy" in the reader's chosen
            //    shape; the format lives in Options ▸ Copy format. ─────────────
            ActionRow("Copy", palette.ink) { copy(copyStyle) }
            ActionRow(t("menu.copyChapter"), palette.ink) { copy("chapter") }
            ActionRow(t("menu.shareLink"), palette.ink) { copy(copyStyle, share = true) }
            HorizontalDivider(color = palette.rule)

            // ── tag + note + memorize — tagging first: it's how topics
            //    accumulate for later weaving ───────────
            ActionRow(t("menu.tag"), palette.ink) { onDismiss(); onTag(verseRef) }
            ActionRow(t("menu.note"), palette.ink) { showNote = true }
            ActionRow(t("menu.memorizeVerse"), palette.ink) { memorize() }
            ActionRow(t("menu.memorizePassage"), palette.ink) { showPassage = true }
            // Marking a chapter read lives in the passage navigator (long-press a
            // chapter tile), where reading standing already lives and a whole book
            // can be logged at once — see ui/BookNav.kt.
            HorizontalDivider(color = palette.rule)

            Spacer(Modifier.height(12.dp))
        }
    }

    if (showNote && noteLoaded) {
        NoteDialog(
            initial = noteText,
            palette = palette,
            error = noteError,
            onSave = { written ->
                // Saving an EMPTIED editor deletes the note (UserNoteSet's
                // empty-clears contract), so it asks first — same wording as
                // the notes browser's ✕. A save that never had a note to
                // delete stays a plain save.
                if (written.isBlank() && noteText.isNotBlank()) {
                    confirmDelete = ConfirmRequest(
                        title = t("notes.deleteAsk", "passage" to display),
                        body = t("notes.deleteBody"),
                        verb = t("notes.deleteVerb"),
                    ) { saveNote(written) }
                } else {
                    saveNote(written)
                }
            },
            onCancel = { showNote = false },
        )
    }

    if (showPassage) {
        PassageEndPicker(
            engine = engine,
            palette = palette,
            startRef = verseRef,
            startDisplay = display,
            onPick = { showPassage = false; memorizePassage(it) },
            onCancel = { showPassage = false },
        )
    }

    ConfirmDialog(confirmDelete, palette) { confirmDelete = null }
}

/**
 * Set the date a chapter was last read — the by-hand entry for reading done in a
 * paper Bible.
 *
 * Material's DatePicker, with the future closed off (you cannot have read
 * something tomorrow) and shortcuts for the answers people actually give:
 * "today", "yesterday", "last week". Clearing is offered here too, because this
 * dialog is the only place a wrong date can be undone.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun MarkReadDialog(
    palette: ReaderPalette,
    label: String,
    onPick: (String) -> Unit,
    onClear: () -> Unit,
    onCancel: () -> Unit,
) {
    val todayUtc = LocalDate.now(ZoneOffset.UTC)
    val state = rememberDatePickerState(
        initialSelectedDateMillis = todayUtc.toEpochDay() * 86_400_000L,
        selectableDates = object : SelectableDates {
            override fun isSelectableDate(utcTimeMillis: Long): Boolean =
                utcTimeMillis <= todayUtc.toEpochDay() * 86_400_000L
        },
    )

    fun pickDaysAgo(n: Long) = onPick(todayUtc.minusDays(n).toString())

    DatePickerDialog(
        onDismissRequest = onCancel,
        colors = DatePickerDefaults.colors(containerColor = palette.panelBg),
        confirmButton = {
            TextButton(onClick = {
                val ms = state.selectedDateMillis
                if (ms != null) onPick(LocalDate.ofEpochDay(ms / 86_400_000L).toString()) else onCancel()
            }) { Text(t("markRead.set"), color = palette.gold) }
        },
        dismissButton = {
            Row {
                TextButton(onClick = onClear) { Text(t("markRead.clearShort"), color = palette.faded) }
                TextButton(onClick = onCancel) { Text(t("common.cancel"), color = palette.faded) }
            }
        },
    ) {
        Text(
            t("markRead.question", "chapter" to label),
            color = palette.ink, fontSize = 16.sp, fontWeight = FontWeight.SemiBold,
            modifier = Modifier.padding(start = 24.dp, end = 24.dp, top = 16.dp),
        )
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            TextButton(onClick = { pickDaysAgo(0) }) { Text(t("markRead.today"), color = palette.gold, fontSize = 13.sp) }
            TextButton(onClick = { pickDaysAgo(1) }) { Text(t("markRead.yesterday"), color = palette.gold, fontSize = 13.sp) }
            TextButton(onClick = { pickDaysAgo(7) }) { Text(t("markRead.lastWeek"), color = palette.gold, fontSize = 13.sp) }
        }
        DatePicker(state = state, colors = DatePickerDefaults.colors(containerColor = palette.panelBg))
    }
}

/**
 * Pick the end of a passage to memorize as one chunk (§Memorization).
 *
 * The convention (both shells): the verse you long-pressed is the
 * START, and you tap the LAST verse from a grid of that chapter's remaining
 * verse numbers — the same tap-grid idiom as the passage navigator's chapter
 * grid. No new gesture, identical under touch and mouse, and the grid only ever
 * offers verses that exist, which makes the same-chapter limit self-evident.
 */
@Composable
private fun PassageEndPicker(
    engine: StudyEngine,
    palette: ReaderPalette,
    startRef: String,
    startDisplay: String,
    onPick: (Int) -> Unit,
    onCancel: () -> Unit,
) {
    val parts = parseRef(startRef)
    var picked by remember(startRef) { mutableStateOf<Int?>(null) }
    // One round trip when the picker opens (not per tap).
    var lastVerse by remember(startRef) { mutableStateOf(0) }
    LaunchedEffect(startRef) {
        if (parts == null) return@LaunchedEffect
        lastVerse = withContext(Dispatchers.Default) {
            runCatching {
                synchronized(engine) { engine.ChapterVerseCount(parts.book, parts.chapter) }
            }.getOrDefault(0)
        }
    }
    // The text of the chunk as it will be drilled, so the reader sees what they
    // are taking on before committing to it.
    var preview by remember(startRef) { mutableStateOf("") }
    LaunchedEffect(picked) {
        val end = picked
        if (parts == null || end == null) {
            preview = ""
            return@LaunchedEffect
        }
        preview = withContext(Dispatchers.Default) {
            (parts.verse..end).mapNotNull { v ->
                runCatching {
                    synchronized(engine) { engine.VerseJson("${parts.book} ${parts.chapter}:$v") }
                        ?.let { parseWire<VerseData>(it).body }
                }.getOrNull()
            }.joinToString(" ")
        }
    }

    val ends = if (parts != null && lastVerse > parts.verse) (parts.verse + 1)..lastVerse else IntRange.EMPTY
    AlertDialog(
        onDismissRequest = onCancel,
        title = {
            Text(
                t("memorize.passageTo", "passage" to startDisplay + (picked?.let { "–$it" } ?: "")),
                color = palette.ink,
            )
        },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                if (ends.isEmpty()) {
                    Text(
                        t("memorize.lastVerse", "passage" to startDisplay),
                        color = palette.inkFaded, fontSize = 13.sp,
                    )
                } else {
                    Text(
                        t("memorize.passageNote"),
                        color = palette.inkFaded, fontSize = 13.sp,
                    )
                    FlowRow(
                        Modifier.fillMaxWidth().padding(top = 10.dp),
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        for (v in ends) {
                            val on = picked == v
                            Text(
                                v.toString(),
                                color = if (on) palette.paper else palette.ink,
                                fontSize = 15.sp,
                                fontWeight = if (on) FontWeight.Bold else FontWeight.Normal,
                                modifier = Modifier
                                    .padding(vertical = 3.dp)
                                    .background(
                                        if (on) palette.gold else palette.panelBg,
                                        RoundedCornerShape(6.dp),
                                    )
                                    .clickable { picked = v }
                                    .padding(horizontal = 13.dp, vertical = 8.dp),
                            )
                        }
                    }
                    if (preview.isNotEmpty()) {
                        HorizontalDivider(
                            color = palette.rule,
                            modifier = Modifier.padding(vertical = 10.dp),
                        )
                        Text(preview, color = palette.ink, fontSize = 14.sp, lineHeight = 21.sp)
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { picked?.let(onPick) }, enabled = picked != null) {
                Text(t("nav.memorize"), color = if (picked != null) palette.gold else palette.inkFaded)
            }
        },
        dismissButton = { TextButton(onClick = onCancel) { Text(t("common.cancel"), color = palette.ink) } },
        containerColor = palette.panelBg,
    )
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

/**
 * The tag picker: tagging a verse offers the EXISTING tags first, alphabetically,
 * and "New tag…" is the secondary, freetext path. New tags are created colourless
 * (colour stays an explicit, optional choice; core never assigns one).
 * Opened by the study panel's `addtag:REF` link.
 *
 * Deleting lives here too, mirroring [ThreadPickerSheet]: a tag started by typo
 * needs a way out, and it asks first like every other destructive action.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TagPickerSheet(
    engine: StudyEngine,
    palette: ReaderPalette,
    verseRef: String,
    onDismiss: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var tags by remember { mutableStateOf<List<Tag1>?>(null) }
    var newMode by remember { mutableStateOf(false) }
    var newName by remember { mutableStateOf("") }
    var confirmDelete by remember { mutableStateOf<ConfirmRequest?>(null) }
    var reloadEpoch by remember { mutableStateOf(0) }

    LaunchedEffect(reloadEpoch) {
        tags = withContext(Dispatchers.Default) {
            runCatching { synchronized(engine) { engine.TagsJson() } }.getOrNull()
                ?.let { runCatching { parseWire<Tags>(it).tags }.getOrNull() }
        } ?: emptyList()
    }

    fun apply(name: String) {
        val tag = name.trim()
        if (tag.isEmpty()) return
        scope.launch {
            val err = withContext(Dispatchers.Default) {
                runCatching {
                    synchronized(engine) { engine.TagAdd(tag, "verse", verseRef, null, Instant.now().toString()) }
                }.getOrNull()
            }
            Toast.makeText(
                context,
                if (err.isNullOrBlank()) t("tag.tagged", "passage" to verseRef, "tag" to tag) else err,
                Toast.LENGTH_SHORT,
            ).show()
            onDismiss()
        }
    }

    fun delete(name: String) {
        scope.launch {
            val err = withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { engine.TagDelete(name) } }.getOrNull()
            }
            if (err.isNullOrBlank()) {
                reloadEpoch++
                Toast.makeText(context, t("tag.deleted", "tag" to name), Toast.LENGTH_SHORT).show()
            } else {
                Toast.makeText(context, err, Toast.LENGTH_SHORT).show()
            }
        }
    }

    /** Ask before deleting, through the shared confirmation (ui/Confirm.kt) —
     *  same reasoning as [ThreadPickerSheet]'s askDelete. */
    fun askDelete(name: String) {
        confirmDelete = ConfirmRequest(
            title = t("tag.deleteAsk", "tag" to name),
            body = t("tag.deleteBody"),
            verb = t("tag.deleteVerb"),
        ) { delete(name) }
    }

    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = palette.panelBg) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState())
                .navigationBarsPadding().padding(horizontal = 16.dp),
        ) {
            Text(
                t("tag.heading", "passage" to verseRef),
                color = palette.ink, fontSize = 18.sp, fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(vertical = 8.dp),
            )
            HorizontalDivider(color = palette.rule)

            val list = tags
            if (list == null) {
                Text("…", color = palette.faded, modifier = Modifier.padding(vertical = 12.dp))
            } else {
                // Every tag is a topic, so plain alphabetical is the whole ordering.
                val ordered = list.sortedBy { it.name.lowercase() }
                for (t in ordered) {
                    Row(
                        Modifier.fillMaxWidth().clickable { apply(t.name) }
                            .padding(vertical = 12.dp, horizontal = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(t.name, color = palette.ink, fontSize = 16.sp, modifier = Modifier.weight(1f))
                        Text(
                            Strings.plural("memorize.verses.one", "memorize.verses.other", t.members.size),
                            color = palette.faded, fontSize = 12.sp,
                        )
                        Text(
                            t("common.delete"),
                            color = palette.disputed, fontSize = 12.sp,
                            modifier = Modifier
                                .clickable { askDelete(t.name) }
                                .padding(start = 14.dp, top = 4.dp, bottom = 4.dp),
                        )
                    }
                }
                if (list.isEmpty()) {
                    Text(
                        t("tag.emptyNameFirst"),
                        color = palette.faded, fontSize = 14.sp,
                        modifier = Modifier.padding(vertical = 12.dp),
                    )
                }
            }

            HorizontalDivider(color = palette.rule)
            if (!newMode) {
                ActionRow(t("tag.new"), palette.ink) { newMode = true }
            } else {
                Row(
                    Modifier.fillMaxWidth().padding(vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    OutlinedTextField(
                        value = newName,
                        onValueChange = { newName = it },
                        placeholder = { Text(t("tag.name")) },
                        singleLine = true,
                        modifier = Modifier.weight(1f),
                    )
                    TextButton(onClick = { apply(newName) }) { Text(t("tag.add"), color = palette.gold) }
                }
            }
            Spacer(Modifier.height(12.dp))
        }
    }

    ConfirmDialog(confirmDelete, palette) { confirmDelete = null }
}

/** The personal-note editor. Empty text clears the note (UserNoteSet contract).
 *  [error] is a save that did not land, shown under the field — the dialog stays
 *  open on failure, so the reason belongs next to the words it is about. */
@Composable
private fun NoteDialog(
    initial: String,
    palette: ReaderPalette,
    error: String? = null,
    onSave: (String) -> Unit,
    onCancel: () -> Unit,
) {
    var text by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onCancel,
        confirmButton = { TextButton(onClick = { onSave(text) }) { Text(t("common.save")) } },
        dismissButton = { TextButton(onClick = onCancel) { Text(t("common.cancel")) } },
        title = { Text(t("notes.on"), color = palette.ink) },
        text = {
            Column {
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    placeholder = { Text(t("notes.fieldClear")) },
                    modifier = Modifier.fillMaxWidth(),
                    minLines = 3,
                )
                if (error != null) {
                    Text(
                        error,
                        color = palette.disputed, fontSize = 13.sp,
                        modifier = Modifier.padding(top = 8.dp),
                    )
                }
            }
        },
        containerColor = palette.panelBg,
    )
}

/**
 * Pick which thread a verse joins — an existing one, or a new one by name.
 *
 * Not a freetext-only prompt: that makes the common case — adding a fifth passage
 * to the thread you have been building all week — require you to retype its name
 * exactly, and a typo silently forks a second thread instead of failing. So this
 * mirrors [TagPickerSheet] exactly: what exists is a list you tap, and freetext
 * is only for something genuinely new.
 *
 * Deleting lives here too, for the same reason it does for a date set by mistake:
 * a thread you started by typo had no way out before.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ThreadPickerSheet(
    engine: StudyEngine,
    palette: ReaderPalette,
    verseRef: String,
    onDismiss: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var threads by remember { mutableStateOf<List<Thread1>?>(null) }
    var newMode by remember { mutableStateOf(false) }
    var newName by remember { mutableStateOf("") }
    var confirmDelete by remember { mutableStateOf<ConfirmRequest?>(null) }
    var reloadEpoch by remember { mutableStateOf(0) }

    LaunchedEffect(reloadEpoch) {
        threads = withContext(Dispatchers.Default) {
            runCatching { synchronized(engine) { engine.ThreadsJson() } }.getOrNull()
                ?.let { runCatching { parseWire<Threads>(it).threads }.getOrNull() }
        } ?: emptyList()
    }

    fun apply(name: String) {
        val t = name.trim()
        if (t.isEmpty()) return
        scope.launch {
            val err = withContext(Dispatchers.Default) {
                runCatching {
                    synchronized(engine) { engine.ThreadAdd(t, verseRef, null, Instant.now().toString()) }
                }.getOrNull()
            }
            Toast.makeText(
                context,
                if (err.isNullOrBlank()) t("thread.addedTo", "thread" to t) else err,
                Toast.LENGTH_SHORT,
            ).show()
            onDismiss()
        }
    }

    fun delete(name: String) {
        scope.launch {
            val err = withContext(Dispatchers.Default) {
                runCatching { synchronized(engine) { engine.ThreadRemove(name) } }.getOrNull()
            }
            if (err.isNullOrBlank()) {
                reloadEpoch++
                Toast.makeText(context, t("thread.deleted", "thread" to name), Toast.LENGTH_SHORT).show()
            } else {
                Toast.makeText(context, err, Toast.LENGTH_SHORT).show()
            }
        }
    }

    /** Ask before deleting, through the shared confirmation (ui/Confirm.kt) rather
     *  than an AlertDialog written out here — every destructive action in the app
     *  asks the same way now. */
    fun askDelete(name: String) {
        confirmDelete = ConfirmRequest(
            title = t("thread.deleteAsk", "thread" to name),
            body = t("thread.deleteBody"),
            verb = t("thread.deleteVerb"),
        ) { delete(name) }
    }

    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = palette.panelBg) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState())
                .navigationBarsPadding().padding(horizontal = 16.dp),
        ) {
            Text(
                t("thread.heading", "passage" to verseRef),
                color = palette.ink, fontSize = 18.sp, fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(vertical = 8.dp),
            )
            HorizontalDivider(color = palette.rule)

            val list = threads
            if (list == null) {
                Text("…", color = palette.faded, modifier = Modifier.padding(vertical = 12.dp))
            } else {
                for (t in list.sortedBy { it.name.lowercase() }) {
                    Row(
                        Modifier.fillMaxWidth().clickable { apply(t.name) }
                            .padding(vertical = 12.dp, horizontal = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(t.name, color = palette.ink, fontSize = 16.sp, modifier = Modifier.weight(1f))
                        Text(
                            Strings.plural("present.passages.one", "present.passages.other", t.entries.size),
                            color = palette.faded, fontSize = 12.sp,
                        )
                        Text(
                            t("common.delete"),
                            color = palette.disputed, fontSize = 12.sp,
                            modifier = Modifier
                                .clickable { askDelete(t.name) }
                                .padding(start = 14.dp, top = 4.dp, bottom = 4.dp),
                        )
                    }
                }
                if (list.isEmpty()) {
                    Text(
                        t("thread.empty"),
                        color = palette.faded, fontSize = 14.sp,
                        modifier = Modifier.padding(vertical = 12.dp),
                    )
                }
            }

            HorizontalDivider(color = palette.rule)
            if (!newMode) {
                ActionRow(t("thread.new"), palette.ink) { newMode = true }
            } else {
                Row(
                    Modifier.fillMaxWidth().padding(vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    OutlinedTextField(
                        value = newName,
                        onValueChange = { newName = it },
                        placeholder = { Text(t("thread.name")) },
                        singleLine = true,
                        modifier = Modifier.weight(1f),
                    )
                    TextButton(onClick = { apply(newName) }) { Text(t("tag.add"), color = palette.gold) }
                }
            }
            Spacer(Modifier.height(12.dp))
        }
    }

    ConfirmDialog(confirmDelete, palette) { confirmDelete = null }
}
