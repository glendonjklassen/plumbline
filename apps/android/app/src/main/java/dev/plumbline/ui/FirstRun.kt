// First run: who is opening the Book? (product 2026-07-26; the web twin is
// FirstRun.svelte — keep the copy in sync). Three paths:
//
//   new in the faith   → a welcome with next steps; verse references are
//                        tappable and open BESIDE John (fold: second pane;
//                        phone: the passage opens with John 1 as the saved
//                        start), then John 1 with both analysis tiers off —
//                        just the text.
//   curious            → a way in for someone who is not sure what they
//                        believe; same landing as the welcome (2026-07-27).
//   sharing the gospel → the church step, then Present with the Romans Road.
//   established        → their church + the analysis-tier picker (text always on).
//
// The two paths likely to hand the app on (established, sharing) are asked for
// a home church: it travels in the links and QR codes they share, and nowhere
// else. Which welcome a reader was given is remembered (`intro` in the shared
// config) so the Welcome button can show it again without a reinstall.

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
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.runtime.LaunchedEffect
import dev.plumbline.ChurchState
import dev.plumbline.StudyEngine
import dev.plumbline.VerseData
import dev.plumbline.parseWire
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** One tappable verse reference in the welcome; [keys] are the verses QUOTED
 *  inline — the new believer reads scripture itself, not a row of links
 *  (product 2026-07-26).
 *
 *  The LABEL IS DERIVED, not stored: "Psalm 12:6–7" is a book name plus the
 *  catalogue's own reference template, and both localize (German writes
 *  "Psalm 12,6–7"). Fifteen stored labels would have been fifteen more English
 *  strings in a table nobody reads as copy. [end] is the last verse of a range. */
data class WelcomeRef(val refKey: String, val keys: List<String> = listOf(), val end: Int? = null)

/** How this reference reads, in the reader's language. The engine already
 *  localizes a refKey's display form; the range suffix is the catalogue's. */
fun welcomeLabel(engine: StudyEngine, r: WelcomeRef): String {
    val one = runCatching {
        synchronized(engine) { engine.VerseJson(r.refKey) }?.let { parseWire<VerseData>(it).display }
    }.getOrNull() ?: r.refKey
    val last = r.end ?: r.keys.lastOrNull()?.substringAfterLast(':')?.toIntOrNull()
    return if (last == null || "$last" == one.substringAfterLast(':').substringAfterLast(',')) {
        one
    } else {
        "$one–$last"
    }
}

private val LOVE = WelcomeRef("Rom 5:8", listOf("Rom 5:8"))
private val PURE = WelcomeRef("Ps 12:6", listOf("Ps 12:6", "Ps 12:7"))
private val CHURCH = WelcomeRef("Heb 10:24", listOf("Heb 10:24", "Heb 10:25"))
private val HEART = WelcomeRef("Ps 119:11", listOf("Ps 119:11"))
private val LOVED = WelcomeRef("John 3:16", listOf("John 3:16"))
private val KNOW = WelcomeRef("1John 5:13", listOf("1John 5:13"))
private val KEPT = WelcomeRef("John 10:28", listOf("John 10:28"), end = 29)
private val PERFECTED = WelcomeRef("Phil 1:6", listOf("Phil 1:6"))
private val FORGIVEN = WelcomeRef("1John 1:9", listOf("1John 1:9"))
private val WISDOM = WelcomeRef("2Tim 3:16", listOf("2Tim 3:16", "2Tim 3:17"))
// The curious path's verses (web twin: REF.treasure / unbelief / ask / seek / struggle).
private val TREASURE = WelcomeRef("Prov 2:4", listOf("Prov 2:4", "Prov 2:5"))
private val UNBELIEF = WelcomeRef("Mark 9:24", listOf("Mark 9:24"))
private val ASK = WelcomeRef("Matt 7:7", listOf("Matt 7:7"))
private val SEEK = WelcomeRef("Jer 29:13", listOf("Jer 29:13"))
private val STRUGGLE = WelcomeRef("Ps 34:18", listOf("Ps 34:18"))
private val ALL_QUOTED = listOf(
    LOVE, PURE, CHURCH, HEART, LOVED, KNOW, KEPT, PERFECTED, FORGIVEN, WISDOM,
    TREASURE, UNBELIEF, ASK, SEEK, STRUGGLE,
)

@Composable
fun FirstRunOverlay(
    engine: StudyEngine,
    palette: ReaderPalette,
    /** Chose a welcome path: the tapped reference (or null to just land in
     *  John), and which welcome it was — "new" or "curious", remembered so it
     *  can be re-read later. */
    onNewBeliever: (WelcomeRef?, intro: String) -> Unit,
    onSharing: () -> Unit,
    onEstablished: (human: Boolean, machine: Boolean) -> Unit,
    /** The church the reader gave, if any — saved to the shared config by the
     *  shell so their shared links carry it. */
    onChurch: (ChurchState) -> Unit = {},
    /** Re-reading a welcome ("new"/"curious") rather than first run: no path is
     *  chosen, no settings move, and the button just closes it. */
    reread: String? = null,
    onCloseReread: () -> Unit = {},
) {
    // The quoted verse bodies, fetched off-thread (keyed by refKey).
    var bodies by remember { mutableStateOf(mapOf<String, String>()) }
    // And every chip's LABEL, in the same pass, because it is the engine that
    // knows how a reference reads in this language ("Psalm 12,6–7" in German).
    // Fetched here rather than in the two paths so neither has to hold an engine.
    var labels by remember { mutableStateOf(mapOf<String, String>()) }
    LaunchedEffect(Unit) {
        bodies = withContext(Dispatchers.Default) {
            ALL_QUOTED.flatMap { it.keys }.distinct().associateWith { k ->
                runCatching { synchronized(engine) { engine.VerseJson(k) } }.getOrNull()
                    ?.let { runCatching { parseWire<VerseData>(it).body }.getOrNull() } ?: ""
            }
        }
        labels = withContext(Dispatchers.Default) {
            ALL_QUOTED.associate { it.refKey to welcomeLabel(engine, it) }
        }
    }
    // One shared family for the whole app (ui/Typography.kt) — it also drives the
    // Material typography, so chrome and body text are the same face.
    val serif = rememberSerifFamily()

    // 0 choose · 1 welcome · 2 tiers · 3 curious · 4 church-before-sharing
    var stage by remember {
        mutableStateOf(
            when (reread) {
                "curious" -> 3
                "new" -> 1
                else -> 0
            },
        )
    }
    // Unchecked to begin with: the tiers are opt-in, so this screen ASKS rather
    // than confirming something already decided (2026-07-28).
    var human by remember { mutableStateOf(false) }
    var machine by remember { mutableStateOf(false) }
    // Asked on the two paths that hand the app on. Optional; pushed up only
    // when a name was actually given.
    var cName by remember { mutableStateOf("") }
    var cInfo by remember { mutableStateOf("") }
    var cUrl by remember { mutableStateOf("") }
    fun saveChurchIfGiven() {
        val c = cleanChurch(ChurchState(cName, cInfo, cUrl))
        if (hasChurch(c)) onChurch(c)
    }

    // Back closes a re-read. Within first run it mirrors the web's click-away
    // rule exactly (FirstRun.svelte's `dismiss()`), because it had the identical
    // hole: `onEstablished()` from the chooser ended onboarding for good while
    // writing no `intro`, so the top bar's Welcome button — the only way back to
    // it — never appeared again.
    //
    // Welcome and curious are read-and-go, so Back there means "got it" and
    // takes the same exit the page's own Start button takes, recording which
    // welcome was read. Tiers and church are questions: Back steps to the
    // chooser and answers nothing.
    //
    // From the chooser itself the handler is DISABLED, so the system takes the
    // press and the app closes with nothing decided — first run is there again
    // next launch. That is the honest answer to "I don't want to choose yet",
    // and it is the one thing a Compose BackHandler cannot express by handling
    // the event.
    BackHandler(enabled = reread != null || stage != 0) {
        when {
            reread != null -> onCloseReread()
            stage == 1 -> onNewBeliever(null, "new")
            stage == 3 -> onNewBeliever(null, "curious")
            else -> stage = 0
        }
    }

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
                    0 -> Choose(
                        palette, serif,
                        onPath = { stage = it },
                        onSharing = { stage = 4 },
                    )
                    1 -> Welcome(
                        palette, serif, bodies, labels,
                        onRef = { if (reread != null) onNewBeliever(it, "new") else onNewBeliever(it, "new") },
                        onStart = { if (reread != null) onCloseReread() else onNewBeliever(null, "new") },
                        closeLabel = if (reread != null) t("common.close") else null,
                    )
                    3 -> Curious(
                        palette, serif, bodies, labels,
                        onRef = { onNewBeliever(it, "curious") },
                        onStart = { if (reread != null) onCloseReread() else onNewBeliever(null, "curious") },
                        closeLabel = if (reread != null) t("common.close") else null,
                    )
                    4 -> ChurchBeforeSharing(
                        palette,
                        cName, cInfo, cUrl,
                        onName = { cName = it }, onInfo = { cInfo = it }, onUrl = { cUrl = it },
                        onGo = { saveChurchIfGiven(); onSharing() },
                    )
                    else -> Tiers(
                        palette, human, machine,
                        onHuman = { human = it }, onMachine = { machine = it },
                        cName, cInfo, cUrl,
                        onName = { cName = it }, onInfo = { cInfo = it }, onUrl = { cUrl = it },
                        onStart = { saveChurchIfGiven(); onEstablished(human, machine) },
                    )
                }
            }
        }
    }
}

@Composable
private fun Choose(palette: ReaderPalette, serif: FontFamily, onPath: (Int) -> Unit, onSharing: () -> Unit) {
    Text("✦", color = palette.gold, fontSize = 25.sp)
    Spacer(Modifier.height(8.dp))
    Text(
        t("intro.title"), color = palette.ink, fontSize = 29.sp,
        fontFamily = serif, fontWeight = FontWeight.Bold,
    )
    Spacer(Modifier.height(6.dp))
    Text(
        t("intro.subShort"),
        color = palette.faded, fontSize = 16.5.sp,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Spacer(Modifier.height(22.dp))
    // Curious leads (2026-07-28): a stranger to the Bible is the likelier
    // first-time reader of the two, and the path that asks the least of someone
    // should be the one they see first. Web twin: FirstRun.svelte's choose stage.
    PathCard(palette, t("intro.pathCurious"), t("intro.pathCuriousDesc")) { onPath(3) }
    PathCard(palette, t("intro.pathNew"), t("intro.pathNewDesc")) { onPath(1) }
    PathCard(palette, t("intro.pathSharing"), t("intro.pathSharingDesc"), onSharing)
    PathCard(
        palette, t("intro.pathEstablished"), t("intro.pathEstablishedDesc"),
    ) { onPath(2) }
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
        Text(name, color = palette.ink, fontSize = 19.5.sp, fontWeight = FontWeight.SemiBold)
        Text(desc, color = palette.faded, fontSize = 15.5.sp, modifier = Modifier.padding(top = 3.dp))
    }
}

/** The new-believer welcome — the copy is shared with the web twin verbatim. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Welcome(
    palette: ReaderPalette,
    serif: FontFamily,
    bodies: Map<String, String>,
    labels: Map<String, String>,
    onRef: (WelcomeRef) -> Unit,
    onStart: () -> Unit,
    /** Non-null when re-reading: the button closes instead of starting. */
    closeLabel: String? = null,
) {
    @Composable
    fun Para(text: String) = Text(
        text, color = palette.ink, fontSize = 19.sp, lineHeight = 29.sp,
        fontFamily = serif, modifier = Modifier.padding(top = 12.dp).fillMaxWidth(),
    )

    // Scripture itself, inline: the quoted verses with their tappable refs.
    @Composable
    fun Quote(vararg refs: WelcomeRef) {
        val text = refs.flatMap { it.keys }.joinToString(" ") { bodies[it] ?: "" }.trim()
        Column(Modifier.fillMaxWidth().padding(top = 6.dp, start = 14.dp, end = 6.dp)) {
            if (text.isNotEmpty()) {
                Text(
                    t("quote.wrap", "text" to text), color = palette.ink, fontSize = 17.5.sp, lineHeight = 27.sp,
                    fontFamily = serif, fontStyle = FontStyle.Italic,
                )
            }
            FlowRow(
                Modifier.fillMaxWidth().padding(top = 2.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                for (r in refs) {
                    Text(
                        labels[r.refKey] ?: r.refKey,
                        color = palette.gold, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.clickable { onRef(r) }.padding(vertical = 3.dp),
                    )
                }
            }
        }
    }

    Text(
        t("intro.welcome.title"),
        color = palette.ink, fontSize = 25.sp, fontFamily = serif, fontWeight = FontWeight.Bold,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Para(t("intro.welcome.lead"))
    Para(
        t("intro.welcome.readLead") + " " + t("intro.welcome.read"),
    )
    Quote(PURE)
    Para(
        t("intro.welcome.churchLead") + " " + t("intro.welcome.churchMerged"),
    )
    Quote(CHURCH)
    Para(
        t("intro.welcome.memorizeLead") + " " + t("intro.welcome.memorize"),
    )
    Quote(HEART)
    Para(
        t("intro.welcome.loved"),
    )
    Quote(LOVE, LOVED)
    Para(t("intro.welcome.kept"))
    Quote(KEPT, KNOW)
    Para(
        t("intro.welcome.forgiven"),
    )
    Quote(PERFECTED, FORGIVEN)
    Para(
        t("intro.welcome.wisdom"),
    )
    Quote(WISDOM)
    Para(
        t("intro.welcome.blessing"),
    )
    Spacer(Modifier.height(10.dp))
    Text(
        t("intro.tapHint"),
        color = palette.faded, fontSize = 14.5.sp, fontStyle = FontStyle.Italic,
    )
    Spacer(Modifier.height(16.dp))
    Button(
        onClick = onStart,
        colors = ButtonDefaults.buttonColors(containerColor = palette.gold, contentColor = palette.paper),
    ) { Text(closeLabel ?: t("intro.open"), fontSize = 18.5.sp) }
}

/**
 * A way in for someone who is not sure what they believe (2026-07-27) — the
 * copy is shared with the web twin verbatim. Same landing as the welcome: the
 * book of John, both analysis tiers off.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Curious(
    palette: ReaderPalette,
    serif: FontFamily,
    bodies: Map<String, String>,
    labels: Map<String, String>,
    onRef: (WelcomeRef) -> Unit,
    onStart: () -> Unit,
    closeLabel: String? = null,
) {
    @Composable
    fun Para(text: String) = Text(
        text, color = palette.ink, fontSize = 19.sp, lineHeight = 29.sp,
        fontFamily = serif, modifier = Modifier.padding(top = 12.dp).fillMaxWidth(),
    )

    @Composable
    fun Quote(vararg refs: WelcomeRef) {
        val text = refs.flatMap { it.keys }.joinToString(" ") { bodies[it] ?: "" }.trim()
        Column(Modifier.fillMaxWidth().padding(top = 6.dp, start = 14.dp, end = 6.dp)) {
            if (text.isNotEmpty()) {
                Text(
                    t("quote.wrap", "text" to text), color = palette.ink, fontSize = 17.5.sp, lineHeight = 27.sp,
                    fontFamily = serif, fontStyle = FontStyle.Italic,
                )
            }
            FlowRow(
                Modifier.fillMaxWidth().padding(top = 2.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                for (r in refs) {
                    Text(
                        labels[r.refKey] ?: r.refKey,
                        color = palette.gold, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.clickable { onRef(r) }.padding(vertical = 3.dp),
                    )
                }
            }
        }
    }

    Text(
        t("intro.curious.title"),
        color = palette.ink, fontSize = 25.sp, fontFamily = serif, fontWeight = FontWeight.Bold,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Para(
        t("intro.curious.p1"),
    )
    Para(
        t("intro.curious.p2"),
    )
    Quote(LOVED)
    Para(
        t("intro.curious.p3"),
    )
    Quote(TREASURE)
    Para(
        t("intro.curious.p4"),
    )
    Quote(UNBELIEF)
    Para(
        t("intro.curious.p5"),
    )
    Quote(ASK, SEEK)
    Para(t("intro.curious.struggle"))
    Quote(STRUGGLE)
    Spacer(Modifier.height(10.dp))
    Text(
        t("intro.tapHint"),
        color = palette.faded, fontSize = 14.5.sp, fontStyle = FontStyle.Italic,
    )
    Spacer(Modifier.height(16.dp))
    Button(
        onClick = onStart,
        colors = ButtonDefaults.buttonColors(containerColor = palette.gold, contentColor = palette.paper),
    ) { Text(closeLabel ?: t("intro.open"), fontSize = 18.5.sp) }
}

/** The three optional church fields, with the reason they are being asked. */
@Composable
private fun ChurchFields(
    palette: ReaderPalette,
    name: String,
    info: String,
    url: String,
    onName: (String) -> Unit,
    onInfo: (String) -> Unit,
    onUrl: (String) -> Unit,
) {
    Text(
        t("intro.churchWhy"),
        color = palette.faded, fontSize = 14.5.sp,
    )
    OutlinedTextField(
        value = name, onValueChange = onName, label = { Text(t("settings.churchName")) },
        singleLine = true, modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
    )
    OutlinedTextField(
        value = info, onValueChange = onInfo,
        label = { Text(t("settings.churchInfo")) },
        singleLine = true, modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
    )
    OutlinedTextField(
        value = url, onValueChange = onUrl, label = { Text(t("settings.churchUrl")) },
        singleLine = true, modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
    )
}

/** Before walking someone down the Romans Road: this reader is the likeliest of
 *  all to hand the app over, so ask how the recipient finds their way back. */
@Composable
private fun ChurchBeforeSharing(
    palette: ReaderPalette,
    name: String,
    info: String,
    url: String,
    onName: (String) -> Unit,
    onInfo: (String) -> Unit,
    onUrl: (String) -> Unit,
    onGo: () -> Unit,
) {
    Text(
        t("intro.beforeShare"), color = palette.ink, fontSize = 27.sp, fontWeight = FontWeight.SemiBold,
    )
    Spacer(Modifier.height(6.dp))
    Text(
        t("intro.beforeShareSub"),
        color = palette.faded, fontSize = 16.sp,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Spacer(Modifier.height(14.dp))
    ChurchFields(palette, name, info, url, onName, onInfo, onUrl)
    Spacer(Modifier.height(16.dp))
    Button(
        onClick = onGo,
        colors = ButtonDefaults.buttonColors(containerColor = palette.gold, contentColor = palette.paper),
    ) { Text(t("intro.openPresent"), fontSize = 18.5.sp) }
    TextButton(onClick = onGo) { Text(t("intro.skip"), color = palette.faded) }
}

@Composable
private fun Tiers(
    palette: ReaderPalette,
    human: Boolean,
    machine: Boolean,
    onHuman: (Boolean) -> Unit,
    onMachine: (Boolean) -> Unit,
    cName: String,
    cInfo: String,
    cUrl: String,
    onName: (String) -> Unit,
    onInfo: (String) -> Unit,
    onUrl: (String) -> Unit,
    onStart: () -> Unit,
) {
    Text(
        t("intro.title"), color = palette.ink, fontSize = 27.sp, fontWeight = FontWeight.SemiBold,
    )
    Spacer(Modifier.height(10.dp))
    Text(t("intro.yourChurch"), color = palette.ink, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
    Spacer(Modifier.height(4.dp))
    ChurchFields(palette, cName, cInfo, cUrl, onName, onInfo, onUrl)
    HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 14.dp))
    Text(
        t("intro.tiersSub"),
        color = palette.faded, fontSize = 16.sp,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Spacer(Modifier.height(16.dp))
    TierCard(
        palette, human, onHuman, t("settings.human") + " †", t("intro.humanShort"),
    )
    TierCard(
        palette, machine, onMachine, t("settings.machine") + " ≈", t("intro.machineShort"),
    )
    Spacer(Modifier.height(8.dp))
    Text(
        t("intro.provenance"),
        color = palette.faded, fontSize = 14.sp,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Spacer(Modifier.height(14.dp))
    Button(
        onClick = onStart,
        colors = ButtonDefaults.buttonColors(containerColor = palette.gold, contentColor = palette.paper),
    ) { Text(t("intro.start"), fontSize = 18.5.sp) }
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
            Text(name, color = palette.ink, fontSize = 18.5.sp, fontWeight = FontWeight.SemiBold)
            Text(desc, color = palette.faded, fontSize = 15.sp, modifier = Modifier.padding(top = 3.dp))
        }
    }
}
