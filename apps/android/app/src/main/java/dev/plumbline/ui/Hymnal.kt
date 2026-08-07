// The hymnal: the fifth destination (web twin hymnal/HymnalScreen.svelte).
// Two views in one screen — the index and one hymn — plus a fullscreen sing
// overlay the StudyScreen hosts over everything, the way Present's
// presentation is hosted (a phone held up between people gets no chrome).
//
// The engine does the work. It hands over stanzas already split into
// (chord?, text) parts and already transposed, so nothing here parses a
// bracket or knows that G+3 is Bb. What lives here is what a shell knows and
// the core cannot: which language this reader wants, whether the chords are
// showing, and how fast the page should move while they sing.
//
// Author D (Compose UI).

package dev.plumbline.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
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
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import dev.plumbline.Hymn1
import dev.plumbline.HymnLine
import dev.plumbline.HymnStanza
import dev.plumbline.HymnText1
import dev.plumbline.HymnalEntry
import dev.plumbline.HymnalIndex
import dev.plumbline.StudyEngine
import dev.plumbline.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** What the sing overlay needs, snapshotted when "Sing" is tapped: the hymn as
 *  transposed at that moment, the language showing, and whether chords are on.
 *  The overlay is hosted by StudyScreen over everything (the Present pattern). */
class HymnSing(val hymn: Hymn1, val lang: String, val chords: Boolean)

/** A language code as that language calls itself, from the central list — a
 *  singer looking for the German text is looking for "Deutsch". Falls back to the
 *  bare code upper-cased for a language the hymn files carry but the app does not
 *  ship an interface in, which they are allowed to do. */
private fun endonymOf(code: String): String =
    Strings.languages.firstOrNull { it.code == code }?.endonym ?: code.uppercase()

/** If a search token names a language the hymnal knows — its code ("de"),
 *  English name ("German") or endonym ("Deutsch") — the code it names, else
 *  null. Empty before the catalogue lands, which recognises no language tokens
 *  yet. */
private fun langTokenOf(tok: String): String? =
    Strings.languages.firstOrNull {
        tok == it.code.lowercase() || tok == it.endonym.lowercase() || tok == it.name.lowercase()
    }?.code

/** The language to show, given what this hymn actually has. The reader's
 *  preference is a preference, not a promise: a German-only hymn shows German
 *  to an English reader rather than showing nothing. */
private fun pick(langs: List<String>, want: String): String =
    if (want in langs) want else langs.firstOrNull() ?: "en"

@Composable
fun HymnalScreen(
    engine: StudyEngine,
    palette: ReaderPalette,
    onClose: () -> Unit,
    onSing: (HymnSing) -> Unit,
) {
    var index by remember { mutableStateOf<List<HymnalEntry>?>(null) }
    var filter by remember { mutableStateOf("") }
    // The hymn being read, and how far its chords are transposed. The offset
    // lives with the open hymn and resets on open: a singer who dropped one
    // hymn a tone has said nothing about the next (web parity).
    var openId by remember { mutableStateOf<String?>(null) }
    var semis by remember { mutableIntStateOf(0) }
    var hymn by remember { mutableStateOf<Hymn1?>(null) }
    // FOLLOWS THE APP'S LANGUAGE. It was a hard-coded "en", so a German reader
    // opened a German interface onto English hymn texts and had to say "Deutsch"
    // again on every hymn (UAT, 2026-08-03) — the web was fixed for this and
    // Android was missed, which is the drift the whole catalogue exists to stop.
    //
    // Still its own state: the chips do a different job from the language
    // setting, and a bilingual singer picking the German text of one hymn has not
    // asked for a German interface.
    var wantLang by remember { mutableStateOf(Strings.lang) }
    var chords by remember { mutableStateOf(false) }

    BackHandler {
        if (openId != null) { openId = null; semis = 0 } else onClose()
    }

    LaunchedEffect(Unit) {
        index = withContext(Dispatchers.Default) {
            runCatching { synchronized(engine) { engine.HymnalJson() } }.getOrNull()
                ?.let { runCatching { parseWire<HymnalIndex>(it).hymns }.getOrNull() }
        } ?: emptyList()
    }

    LaunchedEffect(openId, semis) {
        val id = openId
        if (id == null) {
            hymn = null
            return@LaunchedEffect
        }
        hymn = withContext(Dispatchers.Default) {
            runCatching { synchronized(engine) { engine.HymnJson(id, semis) } }.getOrNull()
                ?.let { runCatching { parseWire<Hymn1>(it) }.getOrNull() }
        }
    }

    val open = hymn
    val langs = open?.texts?.keys?.toList() ?: emptyList()
    val lang = pick(langs, wantLang)
    val text = open?.texts?.get(lang)

    Column(Modifier.fillMaxSize().background(palette.paper)) {
        // ── the bar: back, title, and (on a hymn) the singer's controls ──────
        ScreenBar(
            title = if (openId != null && text != null) text.title else t("hymnal.title"),
            palette = palette,
            onBack = { if (openId != null) { openId = null; semis = 0 } else onClose() },
            backLabel = if (openId != null) t("hymnal.backToList") else t("bar.backToReading"),
        ) {
            if (openId != null && open != null) {
                // One hymn, two texts: the same tune sung in either language.
                // This is the toggle the German release grows out of.
                if (langs.size > 1) {
                    langs.forEach { l ->
                        TextButton(onClick = { wantLang = l }) {
                            Text(
                                l.uppercase(),
                                color = if (l == lang) palette.gold else palette.faded,
                                fontWeight = if (l == lang) FontWeight.SemiBold else FontWeight.Normal,
                            )
                        }
                    }
                }
                TextButton(onClick = { chords = !chords }) {
                    Text(t("hymnal.chords"), color = if (chords) palette.gold else palette.faded)
                }
                if (chords) {
                    TextButton(onClick = { semis-- }) { Text("−", color = palette.gold, fontSize = 18.sp) }
                    Text(open.transposedKey, color = palette.ink, fontWeight = FontWeight.SemiBold)
                    TextButton(onClick = { semis++ }) { Text("+", color = palette.gold, fontSize = 18.sp) }
                }
                TextButton(onClick = { text?.let { onSing(HymnSing(open, lang, chords)) } }) {
                    Text(t("hymnal.sing"), color = palette.gold, fontWeight = FontWeight.SemiBold)
                }
            }
        }

        if (openId == null) {
            HymnIndex(index, filter, { filter = it }, wantLang, palette) { openId = it; semis = 0 }
        } else if (text == null) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text(t("hymnal.loadingOne"), color = palette.faded)
            }
        } else {
            HymnBody(open, text, chords, palette, shown = lang, wanted = wantLang)
        }
    }
}

// ── the index ────────────────────────────────────────────────────────────────

@Composable
private fun HymnIndex(
    index: List<HymnalEntry>?,
    filter: String,
    onFilter: (String) -> Unit,
    wantLang: String,
    palette: ReaderPalette,
    onOpen: (String) -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        OutlinedTextField(
            value = filter,
            onValueChange = onFilter,
            placeholder = { Text(t("hymnal.find"), color = palette.faded) },
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
            singleLine = true,
        )
        val q = filter.trim().lowercase()
        // Number, title or first line, in any of the hymn's languages — a singer
        // looking for "Amazing grace" should not have to know it is number 14. A
        // token that NAMES a language ("de", "German", "Deutsch") narrows the
        // book to hymns carrying it, on top of the rest of the query.
        val shown = (index ?: emptyList()).filter { h ->
            if (q.isEmpty()) return@filter true
            val langCodes = mutableListOf<String>()
            val textTokens = mutableListOf<String>()
            for (tok in q.split(Regex("\\s+"))) {
                val code = langTokenOf(tok)
                if (code != null) langCodes.add(code) else textTokens.add(tok)
            }
            if (!langCodes.all { it in h.titles.keys }) return@filter false
            val textQ = textTokens.joinToString(" ")
            textQ.isEmpty() || h.number.toString() == textQ ||
                (h.titles.values + h.firstLines.values).any { it.lowercase().contains(textQ) }
        }
        when {
            index == null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text(t("common.loading"), color = palette.faded)
            }
            index.isEmpty() -> Box(Modifier.fillMaxSize().padding(28.dp), contentAlignment = Alignment.Center) {
                Text(t("hymnal.loading"), color = palette.ink, fontSize = 16.sp)
            }
            shown.isEmpty() -> Box(Modifier.fillMaxSize().padding(28.dp), contentAlignment = Alignment.Center) {
                Text(t("hymnal.noMatch", "query" to filter), color = palette.ink, fontSize = 16.sp)
            }
            else -> LazyColumn(Modifier.fillMaxSize()) {
                items(shown, key = { it.id }) { h ->
                    val l = pick(h.titles.keys.toList(), wantLang)
                    Row(
                        Modifier.fillMaxWidth()
                            .clickable { onOpen(h.id) }
                            .padding(start = 16.dp, end = 12.dp, top = 12.dp, bottom = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            "${h.number}", color = palette.gold, fontSize = 15.sp,
                            fontWeight = FontWeight.SemiBold, modifier = Modifier.width(34.dp),
                        )
                        Column(Modifier.weight(1f)) {
                            Text(
                                h.titles[l] ?: "", color = palette.ink, fontSize = 16.sp,
                                fontWeight = FontWeight.SemiBold,
                            )
                            val first = h.firstLines[l] ?: ""
                            if (first.isNotEmpty()) {
                                Text(
                                    first, color = palette.faded, fontSize = 13.sp,
                                    maxLines = 1, overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.padding(top = 2.dp),
                                )
                            }
                        }
                        Text(h.tune, color = palette.faded, fontSize = 11.sp)
                    }
                    HorizontalDivider(color = palette.rule)
                }
            }
        }
    }
}

// ── one hymn ─────────────────────────────────────────────────────────────────

@Composable
private fun HymnBody(
    hymn: Hymn1,
    text: HymnText1,
    chords: Boolean,
    palette: ReaderPalette,
    /** The language actually being shown, and the one the reader asked for.
     *  Equal on almost every hymn; when they differ this hymn does not exist in
     *  the reader's language and the note below says so. */
    shown: String = "",
    wanted: String = "",
) {
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp, vertical = 12.dp),
    ) {
        if (shown.isNotEmpty() && wanted.isNotEmpty() && shown != wanted) {
            // A note, not a warning: nothing is wrong, this hymn simply exists in
            // one language. Silently handing a German reader an English hymn is
            // what looked broken (UAT, 2026-08-03).
            Text(
                t("hymnal.notInYourLanguage", "language" to endonymOf(wanted), "shown" to endonymOf(shown)),
                color = palette.faded, fontSize = 12.5.sp, fontStyle = FontStyle.Italic,
                modifier = Modifier.padding(bottom = 6.dp),
            )
        }
        val credit = buildString {
            append(text.author)
            text.translator?.let { append(", tr. $it") }
            text.year?.let { append(", $it") }
            append(" · ${hymn.tune} ${hymn.meter}")
        }
        Text(credit, color = palette.faded, fontSize = 13.sp, modifier = Modifier.padding(bottom = 12.dp))

        text.stanzas.forEachIndexed { i, st ->
            Row(Modifier.padding(bottom = 14.dp)) {
                Text(
                    "${i + 1}", color = palette.gold, fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold, modifier = Modifier.width(24.dp).padding(top = 3.dp),
                )
                StanzaLines(st, chords, chordSize = 13, lyricSize = 17, chordColor = palette.gold, ink = palette.ink)
            }
            text.chorus?.let { chorus ->
                Row(Modifier.padding(start = 24.dp, bottom = 14.dp)) {
                    Column {
                        Text(
                            t("hymnal.refrain"), color = palette.faded, fontSize = 12.sp,
                            fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(bottom = 2.dp),
                        )
                        StanzaLines(chorus, chords, 13, 17, palette.gold, palette.ink)
                    }
                }
            }
        }
        Text(t("hymnal.publicDomain"), color = palette.faded, fontSize = 12.sp, modifier = Modifier.padding(top = 8.dp))
    }
}

/** One chord-carrying cell: the syllable run with its chord above. A cell in a
 *  chorded line without a chord of its own still gets the empty slot, so every
 *  lyric baseline in the line agrees. */
private class Cell(val chord: String?, val text: String)

/** Split a line's (chord?, text) parts into word-boundary cells so the flow
 *  can wrap long chordless runs without ever breaking inside a word. A chord
 *  that strikes mid-word stays exactly where the engine put it: the word
 *  becomes adjacent cells rendered without spacing (`A` + `[G]mazing`). */
private fun wordCells(line: HymnLine): List<List<Cell>> {
    val words = mutableListOf<MutableList<Cell>>()
    var word = mutableListOf<Cell>()
    for (p in line.parts) {
        var chord = p.chord
        var run = StringBuilder()
        for (ch in p.text) {
            run.append(ch)
            if (ch == ' ') {
                if (run.isNotEmpty() || chord != null) word.add(Cell(chord, run.toString()))
                words.add(word)
                word = mutableListOf()
                chord = null
                run = StringBuilder()
            }
        }
        if (run.isNotEmpty() || chord != null) word.add(Cell(chord, run.toString()))
    }
    if (word.isNotEmpty()) words.add(word)
    return words.filter { it.isNotEmpty() }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun StanzaLines(
    stanza: HymnStanza,
    chords: Boolean,
    chordSize: Int,
    lyricSize: Int,
    chordColor: androidx.compose.ui.graphics.Color,
    ink: androidx.compose.ui.graphics.Color,
) {
    Column {
        stanza.lines.forEach { line ->
            val chorded = chords && line.parts.any { it.chord != null }
            if (!chorded) {
                Text(
                    line.parts.joinToString("") { it.text },
                    color = ink, fontSize = lyricSize.sp,
                    modifier = Modifier.padding(vertical = 1.dp),
                )
            } else {
                FlowRow(Modifier.padding(vertical = 1.dp)) {
                    wordCells(line).forEach { word ->
                        Row {
                            word.forEach { cell ->
                                Column {
                                    Text(
                                        cell.chord ?: "",
                                        color = chordColor, fontSize = chordSize.sp,
                                        fontWeight = FontWeight.SemiBold,
                                    )
                                    Text(cell.text, color = ink, fontSize = lyricSize.sp)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── sing mode ────────────────────────────────────────────────────────────────
//
// The same sunlight surface Present uses, for the same reason: a phone held up
// between two people in a room with the lights on. Fixed light, big type, and
// the app theme deliberately does not reach it.

private val SingPaper = androidx.compose.ui.graphics.Color(0xFFFCF9F4)
private val SingInk = androidx.compose.ui.graphics.Color(0xFF211F1A)
private val SingFaded = androidx.compose.ui.graphics.Color(0xFF8A8276)
private val SingGold = androidx.compose.ui.graphics.Color(0xFF6B5417) // dark gold — AA at chord sizes
private val SingRule = androidx.compose.ui.graphics.Color(0xFFD8CBA8)

/** Pixels-per-second at speed 1..9 (dp, so a phone and a tablet creep at the
 *  same apparent rate). Slow end first: 12 is about a line every four seconds
 *  at sing-mode type sizes, which is a hymn taken gently. */
private val SPEED_DP = intArrayOf(0, 12, 18, 26, 36, 48, 62, 80, 104, 135)

@Composable
fun HymnalSingOverlay(sing: HymnSing, onClose: () -> Unit) {
    val text = sing.hymn.texts[sing.lang] ?: return
    var speed by remember { mutableIntStateOf(0) }
    val scroll = rememberScrollState()
    val density = LocalDensity.current.density
    BackHandler(onBack = onClose)

    // A CONTINUOUS CREEP, not a jump per line: singing is continuous, and a
    // page that steps makes everyone find their place again. Scaled by each
    // frame's own elapsed time so 60Hz and 120Hz screens creep at the same
    // rate, with fractional pixels carried between frames — at the slowest
    // speed a 120Hz frame is a tenth of a pixel, and flooring that every frame
    // would hold the page still forever.
    LaunchedEffect(speed) {
        if (speed <= 0) return@LaunchedEffect
        var last = 0L
        var carry = 0f
        while (true) {
            val now = withFrameNanos { it }
            if (last != 0L) {
                val dt = (now - last).coerceAtMost(250_000_000L) / 1_000_000_000f
                carry += SPEED_DP[speed] * density * dt
                val whole = carry.toInt()
                if (whole > 0) {
                    carry -= whole
                    scroll.scrollBy(whole.toFloat())
                }
            }
            last = now
        }
    }

    Column(Modifier.fillMaxSize().zIndex(30f).background(SingPaper).systemBarsPadding()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onClose) {
                Icon(Icons.Filled.Close, contentDescription = t("hymnal.stopSinging"), tint = SingInk)
            }
            Text(
                text.title, color = SingInk, fontSize = 17.sp, fontWeight = FontWeight.SemiBold,
                maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
            )
            TextButton(onClick = { speed = (speed - 1).coerceAtLeast(0) }) {
                Text("−", color = SingGold, fontSize = 20.sp)
            }
            Text(
                if (speed == 0) "hold" else "$speed",
                color = SingInk, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
            )
            TextButton(onClick = { speed = (speed + 1).coerceAtMost(9) }) {
                Text("+", color = SingGold, fontSize = 20.sp)
            }
        }
        HorizontalDivider(color = SingRule)

        Column(
            Modifier.fillMaxSize().verticalScroll(scroll)
                .padding(horizontal = 24.dp, vertical = 20.dp),
        ) {
            text.stanzas.forEach { st ->
                Column(Modifier.padding(bottom = 26.dp)) {
                    StanzaLines(st, sing.chords, chordSize = 16, lyricSize = 26, chordColor = SingGold, ink = SingInk)
                }
                text.chorus?.let { chorus ->
                    Column(Modifier.padding(start = 18.dp, bottom = 26.dp)) {
                        Text(
                            t("hymnal.refrain"), color = SingFaded, fontSize = 14.sp,
                            fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(bottom = 3.dp),
                        )
                        StanzaLines(chorus, sing.chords, 16, 26, SingGold, SingInk)
                    }
                }
            }
            Spacer(Modifier.padding(bottom = 60.dp))
        }
    }
}
