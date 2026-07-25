// Thread presentation mode (Glendon's #1, 2026-07-24): hand-the-phone-out
// Gospel sharing. A thread (e.g. "Romans Road") becomes a clean, fullscreen,
// large-type presentation:
//
//   picker    pick which thread to present (big rows, no chrome clutter).
//   overview  the whole thread as one scrollable column of large verses — the
//             presenter can bounce anywhere the conversation goes; tapping a
//             passage focuses it.
//   focus     one passage, huge, centred; swipe/arrows step the trail; a
//             "context" toggle fades the surrounding verses in around it; the
//             page past the last entry is the end card.
//   end card  thread name + passage list + a plain-text Share (the take-home).
//
// Deliberately high-contrast ("sunlight" palette: near-black on white, large
// EB Garamond) so it reads outdoors at arm's length — this surface is for the
// person being shown, not the studier. Android-first; the desktop shells get a
// projection variant later (docs/FEATURE-MANIFEST.md shell deltas).
//
// Author D (Compose UI).

package dev.purestudy.ui

import android.content.Intent
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowLeft
import androidx.compose.material.icons.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.purestudy.StudyEngine
import dev.purestudy.Thread1
import dev.purestudy.Threads
import dev.purestudy.VerseData
import dev.purestudy.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

// ── the "sunlight" presentation palette: maximum contrast, no washes ─────────
private val SunPaper = Color(0xFFFFFFFF)
private val SunInk = Color(0xFF141414)
private val SunAccent = Color(0xFF6B5417)   // dark gold — AA on white at 15sp+
private val SunFaded = Color(0xFF5A564E)
private val SunRule = Color(0xFFE3DFD6)

/** One presentable passage: the ref, its display form, and the FULL verse text
 *  (fetched fresh — the thread's word-span snapshot is for study, not showing). */
private data class PresentEntry(val ref: String, val display: String, val body: String)

/** The refKey split ("John 3:16" → book "John", ch 3, v 16); see VerseActions. */
private fun refParts(ref: String): Triple<String, Int, Int>? {
    val sp = ref.lastIndexOf(' ')
    if (sp <= 0) return null
    val cv = ref.substring(sp + 1)
    val colon = cv.indexOf(':')
    if (colon <= 0) return null
    val ch = cv.substring(0, colon).toIntOrNull() ?: return null
    val v = cv.substring(colon + 1).toIntOrNull() ?: return null
    return Triple(ref.substring(0, sp), ch, v)
}

/** An online KJV link for the whole trail (BibleGateway takes a comma-separated
 *  passage list), so the take-home is readable without the app. Long trails link
 *  the first passage only to keep the URL sane. */
private fun onlineLink(entries: List<PresentEntry>): String? {
    if (entries.isEmpty()) return null
    val refs = if (entries.size <= 8) entries.map { it.display } else listOf(entries.first().display)
    val search = refs.joinToString(",") { it.replace(' ', '+') }
    return "https://www.biblegateway.com/passage/?search=$search&version=KJV"
}

/** The plain-text take-home for a presented thread (the end-card Share). */
private fun shareText(name: String, entries: List<PresentEntry>): String =
    buildString {
        appendLine(name)
        for (e in entries) {
            appendLine()
            appendLine("“${e.body}” — ${e.display}")
        }
        onlineLink(entries)?.let {
            appendLine()
            appendLine("Read online: $it")
        }
    }

/**
 * The presentation surface, layered fullscreen over the app. [onClose] tears the
 * whole mode down; back steps focus → overview → picker → closed.
 */
@Composable
fun PresentOverlay(
    engine: StudyEngine,
    palette: ReaderPalette,
    onClose: () -> Unit,
) {
    val context = LocalContext.current
    val serif = remember {
        runCatching {
            FontFamily(
                Font("fonts/EBGaramond-Regular.ttf", context.assets),
                Font("fonts/EBGaramond-Italic.ttf", context.assets, style = FontStyle.Italic),
                Font("fonts/EBGaramond-Bold.ttf", context.assets, weight = FontWeight.Bold),
            )
        }.getOrElse { FontFamily.Serif }
    }

    var threads by remember { mutableStateOf<List<Thread1>?>(null) }
    var thread by remember { mutableStateOf<Thread1?>(null) }
    var entries by remember { mutableStateOf<List<PresentEntry>>(emptyList()) }
    // null = overview; entries.size = the end card.
    var focus by remember { mutableStateOf<Int?>(null) }

    LaunchedEffect(Unit) {
        threads = withContext(Dispatchers.Default) {
            runCatching { synchronized(engine) { engine.ThreadsJson() } }.getOrNull()
                ?.let { runCatching { parseWire<Threads>(it).threads }.getOrNull() }
        } ?: emptyList()
    }

    // Resolve every entry to its full verse text when a thread is chosen.
    LaunchedEffect(thread) {
        val t = thread ?: run { entries = emptyList(); return@LaunchedEffect }
        entries = withContext(Dispatchers.Default) {
            t.entries.map { e ->
                val vd = runCatching { synchronized(engine) { engine.VerseJson(e.verse) } }.getOrNull()
                    ?.let { runCatching { parseWire<VerseData>(it) }.getOrNull() }
                PresentEntry(
                    ref = e.verse,
                    display = vd?.display ?: e.display,
                    body = vd?.body?.ifBlank { null } ?: e.text.joinToString(" "),
                )
            }
        }
    }

    BackHandler {
        when {
            focus != null -> focus = null
            thread != null -> { thread = null; focus = null }
            else -> onClose()
        }
    }

    Box(Modifier.fillMaxSize().background(SunPaper)) {
        val t = thread
        when {
            t == null -> PresentPicker(threads, palette, serif, onPick = { thread = it }, onClose = onClose)
            focus == null -> PresentOverview(
                t.name, entries, serif,
                onFocus = { focus = it },
                onBack = { thread = null },
                onShare = { sharePlain(context, shareText(t.name, entries)) },
            )
            else -> PresentFocus(
                engine, t.name, entries, focus!!, serif,
                onStep = { i -> focus = i.coerceIn(0, entries.size) },
                onOverview = { focus = null },
                onShare = { sharePlain(context, shareText(t.name, entries)) },
            )
        }
    }
}

private fun sharePlain(context: android.content.Context, text: String) {
    val send = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_TEXT, text)
    }
    runCatching { context.startActivity(Intent.createChooser(send, "Share")) }
}

/** Pick which thread to present. Big targets; no study chrome. */
@Composable
private fun PresentPicker(
    threads: List<Thread1>?,
    palette: ReaderPalette,
    serif: FontFamily,
    onPick: (Thread1) -> Unit,
    onClose: () -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onClose) {
                Icon(Icons.Filled.Close, contentDescription = "Close", tint = SunInk)
            }
            Text("Present", color = SunInk, fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
        }
        HorizontalDivider(color = SunRule)
        when {
            threads == null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text("Loading threads…", color = SunFaded)
            }
            threads.isEmpty() -> Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) {
                Text(
                    "No threads yet.\n\nA thread is a trail of passages — build one from a word study (“＋ thread”), then present it here.",
                    color = SunInk, fontSize = 17.sp, fontFamily = serif,
                )
            }
            else -> LazyColumn(Modifier.fillMaxSize()) {
                items(threads) { t ->
                    Column(
                        Modifier.fillMaxWidth().clickable { onPick(t) }
                            .padding(horizontal = 24.dp, vertical = 18.dp),
                    ) {
                        Text(t.name, color = SunInk, fontSize = 22.sp, fontFamily = serif, fontWeight = FontWeight.Bold)
                        val first = t.entries.firstOrNull()?.display
                        Text(
                            "${t.entries.size} passage${if (t.entries.size == 1) "" else "s"}" +
                                (first?.let { " · begins at $it" } ?: ""),
                            color = SunFaded, fontSize = 13.sp,
                            modifier = Modifier.padding(top = 3.dp),
                        )
                    }
                    HorizontalDivider(color = SunRule)
                }
            }
        }
    }
}

/** The whole thread, scrollable, in large presentable type. Tap a passage to
 *  focus it — the presenter jumps wherever the conversation goes. */
@Composable
private fun PresentOverview(
    name: String,
    entries: List<PresentEntry>,
    serif: FontFamily,
    onFocus: (Int) -> Unit,
    onBack: () -> Unit,
    onShare: () -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back", tint = SunInk)
            }
            Text(
                name, color = SunInk, fontSize = 18.sp, fontFamily = serif,
                fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f),
            )
            IconButton(onClick = onShare) {
                Icon(Icons.Filled.Share, contentDescription = "Share", tint = SunAccent)
            }
        }
        HorizontalDivider(color = SunRule)
        LazyColumn(Modifier.fillMaxSize()) {
            items(entries.size) { i ->
                val e = entries[i]
                Column(
                    Modifier.fillMaxWidth().clickable { onFocus(i) }
                        .padding(horizontal = 24.dp, vertical = 16.dp),
                ) {
                    Text(
                        e.display, color = SunAccent, fontSize = 14.sp,
                        fontWeight = FontWeight.SemiBold, letterSpacing = 1.sp,
                    )
                    Text(
                        e.body, color = SunInk, fontSize = 23.sp, lineHeight = 33.sp,
                        fontFamily = serif, modifier = Modifier.padding(top = 6.dp),
                    )
                }
                HorizontalDivider(color = SunRule, modifier = Modifier.padding(horizontal = 24.dp))
            }
            item { EndCard(name, entries, serif, onShare) }
        }
    }
}

/** One passage, huge. Swipe (or the edge arrows) steps the trail; "In context"
 *  fades the surrounding verses in around it; past the end is the end card. */
@Composable
private fun PresentFocus(
    engine: StudyEngine,
    name: String,
    entries: List<PresentEntry>,
    index: Int,
    serif: FontFamily,
    onStep: (Int) -> Unit,
    onOverview: () -> Unit,
    onShare: () -> Unit,
) {
    val density = LocalDensity.current
    var swipeDx by remember { mutableStateOf(0f) }
    val swipeState = rememberDraggableState { delta -> swipeDx += delta }
    val atEnd = index >= entries.size

    var showContext by remember(index) { mutableStateOf(false) }
    var context by remember(index) { mutableStateOf<List<Pair<PresentEntry, Boolean>>>(emptyList()) }

    // Fetch the surrounding verses (±2) the first time context is revealed.
    LaunchedEffect(showContext) {
        if (!showContext || atEnd || context.isNotEmpty()) return@LaunchedEffect
        val e = entries[index]
        val parts = refParts(e.ref) ?: return@LaunchedEffect
        context = withContext(Dispatchers.Default) {
            ((parts.third - 2)..(parts.third + 2)).mapNotNull { v ->
                if (v < 1) return@mapNotNull null
                if (v == parts.third) return@mapNotNull e to true
                val ref = "${parts.first} ${parts.second}:$v"
                runCatching { synchronized(engine) { engine.VerseJson(ref) } }.getOrNull()
                    ?.let { runCatching { parseWire<VerseData>(it) }.getOrNull() }
                    ?.let { PresentEntry(ref, it.display, it.body) to false }
            }
        }
    }

    Column(
        Modifier.fillMaxSize().draggable(
            state = swipeState,
            orientation = Orientation.Horizontal,
            onDragStopped = {
                val threshold = with(density) { 64.dp.toPx() }
                if (swipeDx <= -threshold) onStep(index + 1)
                else if (swipeDx >= threshold) onStep(index - 1)
                swipeDx = 0f
            },
        ),
    ) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onOverview) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "All passages", tint = SunInk)
            }
            Text(name, color = SunFaded, fontSize = 14.sp, modifier = Modifier.weight(1f))
            Text(
                if (atEnd) "end" else "${index + 1} / ${entries.size}",
                color = SunFaded, fontSize = 14.sp,
            )
        }

        Box(Modifier.fillMaxWidth().weight(1f)) {
            if (atEnd) {
                Column(
                    Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.Center,
                ) { EndCard(name, entries, serif, onShare) }
            } else {
                val e = entries[index]
                Column(
                    Modifier.fillMaxSize().verticalScroll(rememberScrollState())
                        .padding(horizontal = 26.dp, vertical = 12.dp),
                    verticalArrangement = Arrangement.Center,
                ) {
                    if (showContext) {
                        for ((v, isFocus) in context) {
                            if (isFocus) {
                                FocusVerse(e, serif)
                            } else {
                                Text(
                                    v.body, color = SunFaded, fontSize = 16.sp, lineHeight = 23.sp,
                                    fontFamily = serif, modifier = Modifier.padding(vertical = 6.dp),
                                )
                            }
                        }
                        if (context.isEmpty()) FocusVerse(e, serif)
                    } else {
                        FocusVerse(e, serif)
                    }
                }
            }
        }

        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = { onStep(index - 1) }, enabled = index > 0) {
                Icon(
                    Icons.Filled.KeyboardArrowLeft, contentDescription = "Previous",
                    tint = if (index > 0) SunInk else SunRule,
                )
            }
            Spacer(Modifier.weight(1f))
            if (!atEnd) {
                TextButton(onClick = { showContext = !showContext }) {
                    Text(if (showContext) "Hide context" else "In context", color = SunAccent, fontSize = 15.sp)
                }
            }
            Spacer(Modifier.weight(1f))
            IconButton(onClick = { onStep(index + 1) }, enabled = !atEnd) {
                Icon(
                    Icons.Filled.KeyboardArrowRight, contentDescription = "Next",
                    tint = if (!atEnd) SunInk else SunRule,
                )
            }
        }
    }
}

@Composable
private fun FocusVerse(e: PresentEntry, serif: FontFamily) {
    Column(Modifier.fillMaxWidth().padding(vertical = 10.dp)) {
        Text(
            e.body, color = SunInk, fontSize = 30.sp, lineHeight = 42.sp,
            fontFamily = serif,
        )
        Text(
            e.display, color = SunAccent, fontSize = 17.sp, fontFamily = serif,
            fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(top = 12.dp),
        )
    }
}

/** The closing card: the trail by name + references, and the plain-text
 *  take-home Share — so the person you showed can carry it away. */
@Composable
private fun EndCard(
    name: String,
    entries: List<PresentEntry>,
    serif: FontFamily,
    onShare: () -> Unit,
) {
    Column(
        Modifier.fillMaxWidth().padding(horizontal = 26.dp, vertical = 28.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            name, color = SunInk, fontSize = 28.sp, fontFamily = serif,
            fontWeight = FontWeight.Bold, textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(14.dp))
        for (e in entries) {
            Text(e.display, color = SunAccent, fontSize = 17.sp, fontFamily = serif, textAlign = TextAlign.Center)
        }
        Spacer(Modifier.height(22.dp))
        Button(
            onClick = onShare,
            colors = ButtonDefaults.buttonColors(containerColor = SunAccent, contentColor = SunPaper),
        ) {
            Icon(Icons.Filled.Share, contentDescription = null)
            Spacer(Modifier.padding(horizontal = 4.dp))
            Text("Share these passages", fontSize = 16.sp)
        }
    }
}
