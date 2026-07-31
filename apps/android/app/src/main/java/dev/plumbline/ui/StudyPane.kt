// The study sidebar: it renders the core's typed block list (the
// plumbline_engine_*_blocks_json endpoints) as Compose text/cards — the Android
// mirror of apps/windows/PureStudyWin/StudyPanel.RenderBlocks. One Rust producer
// builds the blocks (word study, concordance, search results, guide/about); this
// pane only walks them and paints. Link runs route back through [onLink] (the
// URI vocabulary parsed by plumbline_route_link_json). v0 renders text + links; full
// interactive routing (tag/thread authoring, dialogs) is a TODO.
//
// LAZY, not eager (2026-07-30). This was a `Column(verticalScroll)`, which
// composes, measures and lays out EVERY block before the first frame — and the
// block list is not always short: the Weaves screen renders the whole weave
// library through this same pane, hundreds of blocks deep, each one an
// AnnotatedString of styled runs. A LazyColumn builds only what the viewport
// shows. The traps that would have silently undone it are all avoided at the
// call sites: no StudyPane is nested inside a parent `verticalScroll`, nothing
// asks it for an intrinsic height, and the four call sites all give it bounded
// height (StudyScreen.kt lines ~663 / ~1291 / ~1423 / ~1509).
//
// Author D (Compose UI).

package dev.plumbline.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.text.ClickableText
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.plumbline.PanelBlock
import dev.plumbline.PanelData
import dev.plumbline.PanelRun
import dev.plumbline.parseWire
import kotlinx.coroutines.delay

private const val LINK_TAG = "link"

/** How long a study read may take before it explains itself. */
private const val SLOW_READ_MS = 600L

/** The gap between blocks — the pane's vertical rhythm, and the reason the
 *  slot wrappers below exist. */
private val BLOCK_GAP = 8.dp

/** The kinds this pane knows how to paint. Anything else is dropped at parse
 *  rather than reaching the list: the eager Column emitted nothing for an
 *  unknown kind, but a lazy item that emits nothing still takes an arrangement
 *  slot and would open a gap over empty space. */
private val PAINTED_KINDS = setOf("rule", "section", "para")

/** The one-time-cost note shown under a slow study read. Deliberately still —
 *  an explanation should sit and be read, not pulse. */
@Composable
private fun FirstRunSlowNote(palette: ReaderPalette, scale: Float) {
    Text(
        "The first one takes a few seconds while the analysis is built for this text. " +
            "Every look after this is instant.",
        color = palette.faded,
        fontSize = (12.5 * scale).sp,
        lineHeight = (18 * scale).sp,
        modifier = Modifier.padding(top = 6.dp),
    )
}

/** A caller's slot (header / footer / embed) inside one lazy item.
 *
 *  The slots emit SEVERAL siblings — AkjvHeader is two Texts and a rule,
 *  VersionFooter a rule and two Texts — and a lazy item stacks its root nodes
 *  flush, with the list's arrangement applying only between items. This wrapper
 *  is what keeps those siblings on the same [BLOCK_GAP] rhythm they had inside
 *  the eager Column. */
@Composable
private fun SlotItem(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(BLOCK_GAP)) {
        content()
    }
}

/**
 * Renders a panel block-list JSON payload. Pass `null` for the idle placeholder.
 *
 * @param blocksJson a plumbline_engine_*_blocks_json payload (or null).
 * @param onLink invoked with a run's URI when a link is tapped.
 * @param embed an optional composable (the concept map + canon heatmap cards)
 *   slotted into the block flow just before the first titled section — after
 *   the headline paras, before the study tiers — so it reads first-class.
 */
@Composable
fun StudyPane(
    blocksJson: String?,
    palette: ReaderPalette,
    modifier: Modifier = Modifier,
    scale: Float = 1f,
    onLink: (String) -> Unit = {},
    embed: (@Composable () -> Unit)? = null,
    /** A study read is in flight — see [loading] handling below. */
    loading: Boolean = false,
    /** Rendered after the blocks — About uses it for the build stamp. */
    footer: (@Composable () -> Unit)? = null,
    /** Rendered BEFORE the blocks: the overlay's answer for the tapped word. */
    header: (@Composable () -> Unit)? = null,
) {
    val blocks = remember(blocksJson) {
        blocksJson?.let {
            runCatching { parseWire<PanelData>(it).blocks }.getOrNull()
        }?.filter { it.kind in PAINTED_KINDS }
    }
    // Once a read outlasts a frame or two, say why it is slow and promise the
    // rest are fast. Timed rather than flagged: whatever index is cold, the wait
    // itself is the honest signal. Web twin: StudyPanel.svelte's `slowRead`.
    var slowRead by remember { mutableStateOf(false) }
    LaunchedEffect(loading, blocksJson) {
        slowRead = false
        if (!loading) return@LaunchedEffect
        delay(SLOW_READ_MS)
        slowRead = true
    }

    LazyColumn(
        modifier = modifier.fillMaxSize(),
        // contentPadding, not Modifier.padding: the inset has to scroll with the
        // blocks, or the list clips against a padded viewport instead of running
        // under it.
        contentPadding = PaddingValues(horizontal = 18.dp, vertical = 14.dp),
        verticalArrangement = Arrangement.spacedBy(BLOCK_GAP),
    ) {
        if (blocks == null) {
            item(key = "placeholder") {
                if (loading) {
                    SlotItem {
                        Text(
                            "— loading —",
                            color = palette.faded,
                            fontStyle = FontStyle.Italic,
                            fontSize = (14 * scale).sp,
                        )
                        if (slowRead) FirstRunSlowNote(palette, scale)
                    }
                } else {
                    Text(
                        "Tap a word for study.",
                        color = palette.faded,
                        fontStyle = FontStyle.Italic,
                        fontSize = (14 * scale).sp,
                    )
                }
            }
            return@LazyColumn
        }
        // Refreshing over existing content: the note still explains a long wait.
        if (loading && slowRead) item(key = "slow") { FirstRunSlowNote(palette, scale) }
        if (header != null) item(key = "header") { SlotItem(header) }
        // Where the embed goes: before the first titled section, or after
        // everything if the payload has no section at all.
        val embedAt = if (embed == null) {
            blocks.size
        } else {
            blocks.indexOfFirst { it.kind == "section" }.let { if (it < 0) blocks.size else it }
        }
        blockItems(blocks, 0, embedAt, palette, scale, onLink)
        if (embed != null) item(key = "embed") { SlotItem(embed) }
        blockItems(blocks, embedAt, blocks.size, palette, scale, onLink)
        if (footer != null) item(key = "footer") { SlotItem(footer) }
    }
}

/** `blocks[from until until]` as one lazy item each.
 *
 *  Keys are index-derived rather than content-derived on purpose: a study
 *  payload can repeat a rule or a title verbatim, and a duplicate key is a
 *  runtime throw inside LazyColumn, not a layout wobble. The index is unique by
 *  construction and stable for as long as the parsed list is — which is exactly
 *  as long as `blocksJson` is unchanged. `contentType` is the kind, so scrolling
 *  a long weave list reuses a para's composition for the next para instead of
 *  building one from scratch. */
private fun LazyListScope.blockItems(
    blocks: List<PanelBlock>,
    from: Int,
    until: Int,
    palette: ReaderPalette,
    scale: Float,
    onLink: (String) -> Unit,
) {
    if (until <= from) return
    items(
        count = until - from,
        key = { "b${from + it}" },
        contentType = { blocks[from + it].kind },
    ) { offset ->
        val b = blocks[from + offset]
        when (b.kind) {
            "rule" -> HorizontalDivider(color = palette.rule)
            "section" -> SectionBlock(b, palette, scale)
            "para" -> ParaBlock(b, palette, scale, onLink)
        }
    }
}

/** The overlay's answer for the tapped word: what the AKJV says, and the KJV
 *  words it replaced. Above the Strong's, because the codes are keyed to the
 *  KJV word — the original has to be read before the lexicon detail. Web twin:
 *  StudyPanel.svelte's `.akjv`. */
@Composable
fun AkjvHeader(palette: ReaderPalette, scale: Float, akjv: String, kjv: String) {
    Text(
        akjv,
        color = palette.ink,
        fontSize = (15 * scale).sp,
        fontWeight = FontWeight.SemiBold,
    )
    Text("KJV: $kjv", color = palette.faded, fontSize = (13 * scale).sp)
    HorizontalDivider(color = palette.goldFaint, modifier = Modifier.padding(top = 6.dp, bottom = 2.dp))
}

/** The build stamp under About. Which build is this? Neither the maintainer nor
 *  a reader could answer that from a screenshot (feedback 2026-07-27), and
 *  "have you relaunched yet?" is a terrible way to debug. Web twin:
 *  StudyPanel.svelte's `.version`. */
@Composable
fun VersionFooter(palette: ReaderPalette, scale: Float) {
    val context = LocalContext.current
    val name = remember {
        runCatching {
            context.packageManager.getPackageInfo(context.packageName, 0).versionName
        }.getOrNull() ?: "dev"
    }
    HorizontalDivider(color = palette.rule, modifier = Modifier.padding(top = 10.dp))
    Text(
        "Plumbline $name",
        color = palette.ink,
        fontSize = (13 * scale).sp,
        fontWeight = FontWeight.SemiBold,
        modifier = Modifier.padding(top = 8.dp),
    )
    Text(
        "Android · sideloaded builds do not auto-update",
        color = palette.faded,
        fontSize = (11.5f * scale).sp,
    )
}

/** A spaced, muted-gold section header + an optional tier-mark glyph. */
@Composable
private fun SectionBlock(b: PanelBlock, palette: ReaderPalette, scale: Float) {
    val text = buildAnnotatedString {
        withStyle(
            SpanStyle(
                color = palette.sectionGold,
                fontWeight = FontWeight.Bold,
                fontSize = (11 * scale).sp,
                letterSpacing = 1.2.sp,
            ),
        ) { append(b.title ?: "") }
        b.markGlyph?.let { glyph ->
            withStyle(SpanStyle(color = palette.role(b.markColor), fontSize = (10 * scale).sp)) {
                append("  $glyph")
            }
        }
    }
    Text(text, modifier = Modifier.padding(top = 8.dp))
}

/** A flowing paragraph of styled runs; link runs route through [onLink]. */
@Composable
private fun ParaBlock(b: PanelBlock, palette: ReaderPalette, scale: Float, onLink: (String) -> Unit) {
    val runs = b.runs ?: emptyList()
    val annotated = buildAnnotatedString {
        for (run in runs) {
            val style = SpanStyle(
                color = if (run.uri != null) palette.gold else palette.role(run.color),
                fontSize = (run.size * scale).sp,
                fontWeight = if (run.bold) FontWeight.Bold else FontWeight.Normal,
                fontStyle = if (run.italic) FontStyle.Italic else FontStyle.Normal,
            )
            if (run.uri != null) {
                pushStringAnnotation(LINK_TAG, run.uri!!)
                withStyle(style) { append(run.text) }
                pop()
            } else {
                withStyle(style) { append(run.text) }
            }
        }
    }
    ClickableText(
        text = annotated,
        modifier = Modifier.padding(
            start = if (b.indent) 12.dp else 0.dp,
            top = if (b.topGap) 6.dp else 0.dp,
        ),
        onClick = { offset ->
            annotated.getStringAnnotations(LINK_TAG, offset, offset)
                .firstOrNull()?.let { onLink(it.item) }
        },
    )
}
