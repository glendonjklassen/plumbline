// The personal-notes browser (Glendon's feedback, 2026-07-24): every note the
// reader has written, browsable from Explore — verse + note text, canonical
// order (UserNotesJson). Tap a row to open the passage in the reader; Edit
// rewrites (or clears) the note in place.
//
// Author D (Compose UI).

package dev.purestudy.ui

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
import dev.purestudy.StudyEngine
import dev.purestudy.UserNote
import dev.purestudy.UserNotes
import dev.purestudy.parseWire
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
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back", tint = palette.ink)
                }
                Text("Notes", color = palette.ink)
            }
        }
        HorizontalDivider(color = palette.rule)

        val list = notes
        when {
            list == null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text("Loading…", color = palette.faded)
            }
            list.isEmpty() -> Box(Modifier.fillMaxSize().padding(28.dp), contentAlignment = Alignment.Center) {
                Text(
                    "No notes yet.\n\nLong-press a verse → “Note…” to write one.",
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
                        TextButton(onClick = { editing = n }) { Text("Edit", color = palette.gold) }
                    }
                    HorizontalDivider(color = palette.rule)
                }
            }
        }
    }

    editing?.let { n ->
        var text by remember(n) { mutableStateOf(n.text) }
        AlertDialog(
            onDismissRequest = { editing = null },
            title = { Text("Note on ${n.display}", color = palette.ink) },
            text = {
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    placeholder = { Text("Your note (leave empty to remove)") },
                    modifier = Modifier.fillMaxWidth(),
                    minLines = 3,
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    val ref = n.verse
                    editing = null
                    notes = null
                    scope.launch {
                        withContext(Dispatchers.Default) {
                            runCatching { synchronized(engine) { engine.UserNoteSet(ref, text, nowUtc()) } }
                        }
                        reload++
                    }
                }) { Text("Save") }
            },
            dismissButton = { TextButton(onClick = { editing = null }) { Text("Cancel") } },
            containerColor = palette.panelBg,
        )
    }
}
