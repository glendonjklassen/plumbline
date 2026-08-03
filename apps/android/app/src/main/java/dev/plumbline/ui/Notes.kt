// The personal-notes browser (product feedback, 2026-07-24): every note the
// reader has written, browsable from Explore — verse + note text, canonical
// order (UserNotesJson). Tap a row to open the passage in the reader; Edit
// rewrites (or clears) the note in place.
//
// Author D (Compose UI).

package dev.plumbline.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.plumbline.StudyEngine
import dev.plumbline.UserNote
import dev.plumbline.UserNotes
import dev.plumbline.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * All personal notes. [onOpen] receives the note's verse refKey (the caller
 * navigates the reader there and closes); [onClose] dismisses.
 */
@Composable
fun NotesScreen(
    engine: StudyEngine,
    palette: ReaderPalette,
    onOpen: (refKey: String) -> Unit,
    onClose: () -> Unit,
) {
    var notes by remember { mutableStateOf<List<UserNote>?>(null) }
    var reload by remember { mutableStateOf(0) }
    var editing by remember { mutableStateOf<UserNote?>(null) }
    val scope = rememberCoroutineScope()
    BackHandler(onBack = onClose)

    LaunchedEffect(reload) {
        notes = withContext(Dispatchers.Default) {
            runCatching { synchronized(engine) { engine.UserNotesJson() } }.getOrNull()
                ?.let { runCatching { parseWire<UserNotes>(it).notes }.getOrNull() }
        } ?: emptyList()
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
                Text(t("notes.title"), color = palette.ink)
            }
        }
        HorizontalDivider(color = palette.rule)

        val list = notes
        when {
            list == null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text(t("common.loading"), color = palette.faded)
            }
            list.isEmpty() -> Box(Modifier.fillMaxSize().padding(28.dp), contentAlignment = Alignment.Center) {
                Text(
                    t("notes.empty"),
                    color = palette.ink, fontSize = 16.sp,
                )
            }
            else -> LazyColumn(Modifier.fillMaxSize()) {
                items(list) { n ->
                    Row(
                        Modifier.fillMaxWidth()
                            .clickable { onOpen(n.verse) }
                            .padding(start = 20.dp, end = 8.dp, top = 12.dp, bottom = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(n.display, color = palette.ink, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                            Text(
                                n.text, color = palette.faded, fontSize = 14.sp,
                                maxLines = 3, overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.padding(top = 3.dp),
                            )
                        }
                        TextButton(onClick = { editing = n }) { Text(t("notes.edit"), color = palette.gold) }
                    }
                    HorizontalDivider(color = palette.rule)
                }
            }
        }
    }

    editing?.let { n ->
        var text by remember(n) { mutableStateOf(n.text) }
        // A save that failed keeps this dialog — and the rewritten note in it —
        // and says why. Closing on the way in (as this did) meant a refused write
        // took the reader's edit with it and looked identical to a good one.
        var error by remember(n) { mutableStateOf<String?>(null) }
        AlertDialog(
            onDismissRequest = { editing = null },
            title = { Text(t("notes.on", "passage" to n.display), color = palette.ink) },
            text = {
                Column {
                    OutlinedTextField(
                        value = text,
                        onValueChange = { text = it },
                        placeholder = { Text(t("notes.field")) },
                        modifier = Modifier.fillMaxWidth(),
                        minLines = 3,
                    )
                    error?.let {
                        Text(
                            it, color = palette.disputed, fontSize = 13.sp,
                            modifier = Modifier.padding(top = 8.dp),
                        )
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    val ref = n.verse
                    val written = text
                    error = null
                    scope.launch {
                        val outcome = withContext(Dispatchers.Default) {
                            saveOutcome(
                                runCatching { synchronized(engine) { engine.UserNoteSet(ref, written, nowUtc()) } },
                            )
                        }
                        when (outcome) {
                            // Only a write the engine took closes the editor and
                            // re-reads the list.
                            is SaveOutcome.Saved -> { editing = null; notes = null; reload++ }
                            is SaveOutcome.Failed -> error = noteSaveFailedLine(outcome.message)
                        }
                    }
                }) { Text(t("common.save")) }
            },
            dismissButton = { TextButton(onClick = { editing = null }) { Text(t("common.cancel")) } },
            containerColor = palette.panelBg,
        )
    }
}
