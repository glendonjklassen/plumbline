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
// The curious path's verses (web twin: REF.treasure / unbelief / ask / seek / struggle).
private val TREASURE = WelcomeRef("Proverbs 2:4–5", "Prov 2:4", listOf("Prov 2:4", "Prov 2:5"))
private val UNBELIEF = WelcomeRef("Mark 9:24", "Mark 9:24", listOf("Mark 9:24"))
private val ASK = WelcomeRef("Matthew 7:7", "Matt 7:7", listOf("Matt 7:7"))
private val SEEK = WelcomeRef("Jeremiah 29:13", "Jer 29:13", listOf("Jer 29:13"))
private val STRUGGLE = WelcomeRef("Psalm 34:18", "Ps 34:18", listOf("Ps 34:18"))
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
    LaunchedEffect(Unit) {
        bodies = withContext(Dispatchers.Default) {
            ALL_QUOTED.flatMap { it.keys }.distinct().associateWith { k ->
                runCatching { synchronized(engine) { engine.VerseJson(k) } }.getOrNull()
                    ?.let { runCatching { parseWire<VerseData>(it).body }.getOrNull() } ?: ""
            }
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
                        palette, serif, bodies,
                        onRef = { if (reread != null) onNewBeliever(it, "new") else onNewBeliever(it, "new") },
                        onStart = { if (reread != null) onCloseReread() else onNewBeliever(null, "new") },
                        closeLabel = if (reread != null) "Close" else null,
                    )
                    3 -> Curious(
                        palette, serif, bodies,
                        onRef = { onNewBeliever(it, "curious") },
                        onStart = { if (reread != null) onCloseReread() else onNewBeliever(null, "curious") },
                        closeLabel = if (reread != null) "Close" else null,
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
        "Welcome to Plumbline", color = palette.ink, fontSize = 29.sp,
        fontFamily = serif, fontWeight = FontWeight.Bold,
    )
    Spacer(Modifier.height(6.dp))
    Text(
        "The Holy Bible, free and offline.\nWhere would you like to begin?",
        color = palette.faded, fontSize = 16.5.sp,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Spacer(Modifier.height(22.dp))
    // Curious leads (2026-07-28): a stranger to the Bible is the likelier
    // first-time reader of the two, and the path that asks the least of someone
    // should be the one they see first. Web twin: FirstRun.svelte's choose stage.
    PathCard(palette, "Curious about the Bible", "I'm not sure what I believe — where do I start?") { onPath(3) }
    PathCard(palette, "New believer", "Where to start if you have just put your faith in Jesus.") { onPath(1) }
    PathCard(palette, "Sharing the gospel", "Share the gospel and your church from your phone.", onSharing)
    PathCard(
        palette, "Established believer",
        "Set up your Bible for study and memorization and prepare to share the good news with others.",
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
                    "“$text”", color = palette.ink, fontSize = 17.5.sp, lineHeight = 27.sp,
                    fontFamily = serif, fontStyle = FontStyle.Italic,
                )
            }
            FlowRow(
                Modifier.fillMaxWidth().padding(top = 2.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                for (r in refs) {
                    Text(
                        r.label, color = palette.gold, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.clickable { onRef(r) }.padding(vertical = 3.dp),
                    )
                }
            }
        }
    }

    Text(
        "We're so glad you've put your faith in Jesus",
        color = palette.ink, fontSize = 25.sp, fontFamily = serif, fontWeight = FontWeight.Bold,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Para("There are some next steps you can take to grow in faith:")
    Para(
        "Start reading your Bible. The next page will open in the book of John, which is a " +
            "great place to start reading the inspired, inerrant word of God. You've been linked " +
            "the King James Version, which is the closest to the original texts and has been used " +
            "for hundreds of years by millions of believers. If you have trouble with the older " +
            "English, turn on the Plain-English overlay in Settings: it marks the words the " +
            "American King James Version puts differently, so a modern wording is a tap away " +
            "without ever leaving the King James text.",
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
        "One day you will be perfected, but not yet, and so while you are here, you are " +
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
        "Tap any verse reference to open it.",
        color = palette.faded, fontSize = 14.5.sp, fontStyle = FontStyle.Italic,
    )
    Spacer(Modifier.height(16.dp))
    Button(
        onClick = onStart,
        colors = ButtonDefaults.buttonColors(containerColor = palette.gold, contentColor = palette.paper),
    ) { Text(closeLabel ?: "Open the Bible", fontSize = 18.5.sp) }
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
                    "“$text”", color = palette.ink, fontSize = 17.5.sp, lineHeight = 27.sp,
                    fontFamily = serif, fontStyle = FontStyle.Italic,
                )
            }
            FlowRow(
                Modifier.fillMaxWidth().padding(top = 2.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                for (r in refs) {
                    Text(
                        r.label, color = palette.gold, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.clickable { onRef(r) }.padding(vertical = 3.dp),
                    )
                }
            }
        }
    }

    Text(
        "I'm glad you're curious about the Bible.",
        color = palette.ink, fontSize = 25.sp, fontFamily = serif, fontWeight = FontWeight.Bold,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Para(
        "For thousands of years this text has been the foundation of civilizations and of the " +
            "lives of individuals. People have been killed for reading it and for sharing it.",
    )
    Para(
        "It contains the history of our world from its creation to the incarnation of its Creator " +
            "here on earth with us. He came to save us because he loves us:",
    )
    Quote(LOVED)
    Para(
        "Whether you are just curious or returning to faith after a long time, there is treasure " +
            "here for you:",
    )
    Quote(TREASURE)
    Para(
        "If you are having trouble believing, you're not alone — someone said exactly that to " +
            "Jesus himself:",
    )
    Quote(UNBELIEF)
    Para(
        "I encourage you to read this book starting with the book of John, and to pray that if God " +
            "is real, he would reveal himself to you. I've known many people for whom that prayer " +
            "has been answered:",
    )
    Quote(ASK, SEEK)
    Para("If you are in a difficult place in your life, ask God to help you with your struggles:")
    Quote(STRUGGLE)
    Spacer(Modifier.height(10.dp))
    Text(
        "Tap any verse reference to open it.",
        color = palette.faded, fontSize = 14.5.sp, fontStyle = FontStyle.Italic,
    )
    Spacer(Modifier.height(16.dp))
    Button(
        onClick = onStart,
        colors = ButtonDefaults.buttonColors(containerColor = palette.gold, contentColor = palette.paper),
    ) { Text(closeLabel ?: "Open the Bible", fontSize = 18.5.sp) }
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
        "If you add your church, the links and QR codes you share contain your church " +
            "information, so whoever you hand the Bible to can also find your church. It stays " +
            "on your device and your data remains private.",
        color = palette.faded, fontSize = 14.5.sp,
    )
    OutlinedTextField(
        value = name, onValueChange = onName, label = { Text("Church name") },
        singleLine = true, modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
    )
    OutlinedTextField(
        value = info, onValueChange = onInfo,
        label = { Text("When and where you meet") },
        singleLine = true, modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
    )
    OutlinedTextField(
        value = url, onValueChange = onUrl, label = { Text("Website") },
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
        "Before you share it", color = palette.ink, fontSize = 27.sp, fontWeight = FontWeight.SemiBold,
    )
    Spacer(Modifier.height(6.dp))
    Text(
        "This app will enable you to easily share the gospel with someone. If they keep the " +
            "app afterwards, this is how they find your church.",
        color = palette.faded, fontSize = 16.sp,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Spacer(Modifier.height(14.dp))
    ChurchFields(palette, name, info, url, onName, onInfo, onUrl)
    Spacer(Modifier.height(16.dp))
    Button(
        onClick = onGo,
        colors = ButtonDefaults.buttonColors(containerColor = palette.gold, contentColor = palette.paper),
    ) { Text("Open the presentation screen", fontSize = 18.5.sp) }
    TextButton(onClick = onGo) { Text("Skip for now", color = palette.faded) }
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
        "Welcome to Plumbline", color = palette.ink, fontSize = 27.sp, fontWeight = FontWeight.SemiBold,
    )
    Spacer(Modifier.height(10.dp))
    Text("Your church", color = palette.ink, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
    Spacer(Modifier.height(4.dp))
    ChurchFields(palette, cName, cInfo, cUrl, onName, onInfo, onUrl)
    HorizontalDivider(color = palette.rule, modifier = Modifier.padding(vertical = 14.dp))
    Text(
        "Reading, search, memorization, tags, and notes are all available in this " +
            "application. Choose which additional analysis tools are installed with the Bible.",
        color = palette.faded, fontSize = 16.sp,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Spacer(Modifier.height(16.dp))
    TierCard(
        palette, human, onHuman, "Scholars' analysis †",
        "Curated scholarship: how the text renders each original word, word grammar, the same " +
            "root traced across the testaments, and the Treasury's cross-references.",
    )
    TierCard(
        palette, machine, onMachine, "Machine analysis ≈",
        "Statistical patterns to weigh for yourself: words that appear alongside, verses " +
            "like this one, and the concept maps.",
    )
    Spacer(Modifier.height(8.dp))
    Text(
        "Every piece of evidence is marked with where it comes from — ✝ the text · † scholarship · ≈ machine.",
        color = palette.faded, fontSize = 14.sp,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
    Spacer(Modifier.height(14.dp))
    Button(
        onClick = onStart,
        colors = ButtonDefaults.buttonColors(containerColor = palette.gold, contentColor = palette.paper),
    ) { Text("Start reading", fontSize = 18.5.sp) }
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
