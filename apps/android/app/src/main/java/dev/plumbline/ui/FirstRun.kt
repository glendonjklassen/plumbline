// First run: who is opening the Book? (product 2026-07-26; the web twin is
// FirstRun.svelte — keep the copy in sync). Three paths:
//
//   new in the faith   → a welcome with next steps; verse references are
//                        tappable and open BESIDE John (fold: second pane;
//                        phone: the passage opens with John 1 as the saved
//                        start), then John 1 with both analysis tiers off —
//                        just the text.
//   sharing the gospel → straight into Present with the Romans Road.
//   established        → the analysis-tier picker (text is always on).

package dev.plumbline.ui

import androidx.activity.compose.BackHandler
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
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.runtime.LaunchedEffect
import dev.plumbline.StudyEngine
import dev.plumbline.VerseData
import dev.plumbline.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** One tappable verse reference in the welcome ("Psalm 12:6–7" → "Ps 12:6");
 *  [keys] are the verses QUOTED inline — the new believer reads scripture
 *  itself, not a row of links (product 2026-07-26). */
data class WelcomeRef(val label: String, val refKey: String, val keys: List<String> = listOf())

private val LOVE = WelcomeRef("Romans 5:8", "Rom 5:8", listOf("Rom 5:8"))
private val PURE = WelcomeRef("Psalm 12:6–7", "Ps 12:6", listOf("Ps 12:6", "Ps 12:7"))
private val CHURCH = WelcomeRef("Hebrews 10:24–25", "Heb 10:24", listOf("Heb 10:24", "Heb 10:25"))
private val HEART = WelcomeRef("Psalm 119:11", "Ps 119:11", listOf("Ps 119:11"))
private val LOVED = WelcomeRef("John 3:16", "John 3:16", listOf("John 3:16"))
private val KNOW = WelcomeRef("1 John 5:13", "1John 5:13", listOf("1John 5:13"))
private val KEPT = WelcomeRef("John 10:28–29", "John 10:28", listOf("John 10:28"))
private val PERFECTED = WelcomeRef("Philippians 1:6", "Phil 1:6", listOf("Phil 1:6"))
private val FORGIVEN = WelcomeRef("1 John 1:9", "1John 1:9", listOf("1John 1:9"))
private val WISDOM = WelcomeRef("2 Timothy 3:16–17", "2Tim 3:16", listOf("2Tim 3:16", "2Tim 3:17"))
private val ALL_QUOTED = listOf(LOVE, PURE, CHURCH, HEART, LOVED, KNOW, KEPT, PERFECTED, FORGIVEN, WISDOM)

@Composable
fun FirstRunOverlay(
    engine: StudyEngine,
    palette: ReaderPalette,
    onNewBeliever: (WelcomeRef?) -> Unit,
    onSharing: () -> Unit,
    onEstablished: (human: Boolean, machine: Boolean) -> Unit,
) {
    // The quoted verse bodies, fetched off-thread (keyed by refKey).
    var bodies by remember { mutableStateOf(mapOf<String, String>()) }
    LaunchedEffect(Unit) {
        bodies = withContext(Dispatchers.Default) {
            ALL_QUOTED.flatMap { it.keys }.distinct().associateWith { k ->
                runCatching { synchronized(engine) { engine.VerseJson(k) } }.getOrNull()
                    ?.let { runCatching { parseWire<VerseData>(it).body }.getOrNull() } ?: ""
            }
        }
    }
    val context = LocalContext.current
    val serif = remember {
        runCatching {
            FontFamily(
                Font("fonts/EBGaramond-Regular.ttf", context.assets),
                Font("fonts/EBGaramond-Italic.ttf", context.assets, style = FontStyle.Italic),
            )
        }.getOrElse { FontFamily.Serif }
    }

    var stage by remember { mutableStateOf(0) } // 0 choose · 1 welcome · 2 tiers
    var human by remember { mutableStateOf(true) }
    var machine by remember { mutableStateOf(true) }

    // Back steps to the chooser; from the chooser it keeps the defaults
    // (mirrors the web's click-away behaviour).
    BackHandler { if (stage != 0) stage = 0 else onEstablished(human, machine) }

    Box(Modifier.fillMaxSize().background(palette.paper)) {
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 26.dp, vertical = 34.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(Modifier.widthIn(max = 560.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                when (stage) {
                    0 -> Choose(palette, serif, onPath = { stage = it }, onSharing = onSharing)
                    1 -> Welcome(palette, serif, bodies, onRef = { onNewBeliever(it) }, onStart = { onNewBeliever(null) })
                    else -> Tiers(
                        palette, human, machine,
                        onHuman = { human = it }, onMachine = { machine = it },
                        onStart = { onEstablished(human, machine) },
                    )
                }
            }
        }
    }
}

@Composable
private fun Choose(palette: ReaderPalette, serif: FontFamily, onPath: (Int) -> Unit, onSharing: () -> Unit) {
    Text("✦", color = palette.gold, fontSize = 22.sp)
    Spacer(Modifier.height(8.dp))
    Text(
        "Welcome to Plumbline", color = palette.ink, fontSize = 26.sp,
        fontFamily = serif, fontWeight = FontWeight.Bold,
    )
    Spacer(Modifier.height(6.dp))
    Text(
        "The 1769 King James text, free and offline.\nWhere would you like to begin?",
        color = palette.faded, fontSize = 14.5.sp,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Spacer(Modifier.height(22.dp))
    PathCard(palette, "New in the faith", "I've just put my faith in Jesus — where do I start?") { onPath(1) }
    PathCard(palette, "Sharing the gospel", "Walk someone down the Romans Road, right now.", onSharing)
    PathCard(palette, "Established believer", "Set up which layers of analysis sit alongside the text.") { onPath(2) }
}

@Composable
private fun PathCard(palette: ReaderPalette, name: String, desc: String, onClick: () -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp)
            .border(1.dp, palette.rule, RoundedCornerShape(12.dp))
            .background(palette.panelBg, RoundedCornerShape(12.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 18.dp, vertical = 15.dp),
    ) {
        Text(name, color = palette.ink, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
        Text(desc, color = palette.faded, fontSize = 13.5.sp, modifier = Modifier.padding(top = 3.dp))
    }
}

/** The new-believer welcome — the copy is shared with the web twin verbatim. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Welcome(
    palette: ReaderPalette,
    serif: FontFamily,
    bodies: Map<String, String>,
    onRef: (WelcomeRef) -> Unit,
    onStart: () -> Unit,
) {
    @Composable
    fun Para(text: String) = Text(
        text, color = palette.ink, fontSize = 16.5.sp, lineHeight = 25.sp,
        fontFamily = serif, modifier = Modifier.padding(top = 12.dp).fillMaxWidth(),
    )

    // Scripture itself, inline: the quoted verses with their tappable refs.
    @Composable
    fun Quote(vararg refs: WelcomeRef) {
        val text = refs.flatMap { it.keys }.joinToString(" ") { bodies[it] ?: "" }.trim()
        Column(Modifier.fillMaxWidth().padding(top = 6.dp, start = 14.dp, end = 6.dp)) {
            if (text.isNotEmpty()) {
                Text(
                    "“$text”", color = palette.ink, fontSize = 15.5.sp, lineHeight = 23.sp,
                    fontFamily = serif, fontStyle = FontStyle.Italic,
                )
            }
            FlowRow(
                Modifier.fillMaxWidth().padding(top = 2.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                for (r in refs) {
                    Text(
                        r.label, color = palette.gold, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.clickable { onRef(r) }.padding(vertical = 3.dp),
                    )
                }
            }
        }
    }

    Text(
        "We're so glad you've put your faith in Jesus",
        color = palette.ink, fontSize = 22.sp, fontFamily = serif, fontWeight = FontWeight.Bold,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Para("There are some next steps you can take to grow in faith:")
    Para(
        "Start reading your Bible. The next page will open in the book of John, which is a " +
            "great place to start reading the inspired, inerrant word of God. You've been linked " +
            "the King James Version, which is the closest to the original texts and has been used " +
            "for hundreds of years by millions of believers. If you have trouble with the older " +
            "English, we recommend you read a newer translation like the ESV alongside (not " +
            "instead of) the King James to better understand.",
    )
    Quote(PURE)
    Para(
        "Find a church. Being part of a local church is a great way to grow in your faith and " +
            "connect with believers. If someone shared this app with you, consider reaching out " +
            "to them or attending a Sunday morning service at their church.",
    )
    Quote(CHURCH)
    Para(
        "Memorize. This app can also help you memorize scripture — hiding the word in your " +
            "heart is a wise and helpful thing to do.",
    )
    Quote(HEART)
    Para(
        "Know that Jesus loves you, and if you trust in him for your salvation, then you have " +
            "eternal life:",
    )
    Quote(LOVE, LOVED)
    Para("No one can take it away from you, and you can know that for certain:")
    Quote(KEPT, KNOW)
    Para(
        "One day you will be perfected, but not yet — and so while you are here, you are " +
            "imperfect but you are forgiven:",
    )
    Quote(PERFECTED, FORGIVEN)
    Para(
        "We highly recommend you read your Bible as it is rich with wisdom on how to navigate " +
            "this world and how to serve our Lord and Saviour Jesus Christ:",
    )
    Quote(WISDOM)
    Para(
        "May the peace and joy of Christ be with you, and may you share that peace and joy with " +
            "others. God bless you!",
    )
    Spacer(Modifier.height(10.dp))
    Text(
        "Tap any verse reference to open it beside the book of John.",
        color = palette.faded, fontSize = 12.5.sp, fontStyle = FontStyle.Italic,
    )
    Spacer(Modifier.height(16.dp))
    Button(
        onClick = onStart,
        colors = ButtonDefaults.buttonColors(containerColor = palette.gold, contentColor = palette.paper),
    ) { Text("Open the book of John", fontSize = 16.sp) }
}

@Composable
private fun Tiers(
    palette: ReaderPalette,
    human: Boolean,
    machine: Boolean,
    onHuman: (Boolean) -> Unit,
    onMachine: (Boolean) -> Unit,
    onStart: () -> Unit,
) {
    Text(
        "Welcome to Plumbline", color = palette.ink, fontSize = 24.sp, fontWeight = FontWeight.SemiBold,
    )
    Spacer(Modifier.height(6.dp))
    Text(
        "The 1769 King James text is always on — reading, search, and your own tags, notes, " +
            "and threads. Choose which layers of analysis sit alongside it:",
        color = palette.faded, fontSize = 14.sp,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Spacer(Modifier.height(16.dp))
    TierCard(
        palette, human, onHuman, "Scholars' analysis †",
        "Curated scholarship: how the 1769 renders each original word, word grammar, the same " +
            "root traced across the testaments, and the Treasury's cross-references.",
    )
    TierCard(
        palette, machine, onMachine, "Machine analysis ≈",
        "Statistical patterns to weigh for yourself: similar concepts, words that appear " +
            "alongside, verses like this one, and the concept maps.",
    )
    Spacer(Modifier.height(8.dp))
    Text(
        "Every piece of evidence is marked with where it comes from — ✝ the text · † scholarship · ≈ machine.",
        color = palette.faded, fontSize = 12.sp,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Spacer(Modifier.height(14.dp))
    Button(
        onClick = onStart,
        colors = ButtonDefaults.buttonColors(containerColor = palette.gold, contentColor = palette.paper),
    ) { Text("Start reading", fontSize = 16.sp) }
}

@Composable
private fun TierCard(
    palette: ReaderPalette,
    checked: Boolean,
    onChecked: (Boolean) -> Unit,
    name: String,
    desc: String,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp)
            .border(1.dp, palette.rule, RoundedCornerShape(12.dp))
            .background(palette.panelBg, RoundedCornerShape(12.dp))
            .clickable { onChecked(!checked) }
            .padding(end = 16.dp, top = 6.dp, bottom = 10.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Checkbox(
            checked = checked, onCheckedChange = onChecked,
            colors = CheckboxDefaults.colors(checkedColor = palette.gold),
        )
        Column {
            Text(name, color = palette.ink, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
            Text(desc, color = palette.faded, fontSize = 13.sp, modifier = Modifier.padding(top = 3.dp))
        }
    }
}
