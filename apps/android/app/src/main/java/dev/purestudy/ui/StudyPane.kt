// The study sidebar: it renders the core's typed block list (the
// pure_engine_*_blocks_json endpoints) as Compose text/cards — the Android
// mirror of apps/windows/PureStudyWin/StudyPanel.RenderBlocks. One Rust producer
// builds the blocks (word study, concordance, search results, guide/about); this
// pane only walks them and paints. Link runs route back through [onLink] (the
// URI vocabulary parsed by pure_route_link_json). v0 renders text + links; full
// interactive routing (tag/thread authoring, dialogs) is a TODO.
//
// Author D (Compose UI).

package dev.purestudy.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.ClickableText
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.purestudy.PanelBlock
import dev.purestudy.PanelData
import dev.purestudy.PanelRun
import dev.purestudy.parseWire

private const val LINK_TAG = "link"

/**
 * Renders a panel block-list JSON payload. Pass `null` for the idle placeholder.
 *
 * @param blocksJson a pure_engine_*_blocks_json payload (or null).
 * @param onLink invoked with a run's URI when a link is tapped.
 */
@Composable
fun StudyPane(
    blocksJson: String?,
    palette: ReaderPalette,
    modifier: Modifier = Modifier,
    scale: Float = 1f,
    onLink: (String) -> Unit = {},
) {
    val blocks = remember(blocksJson) {
        blocksJson?.let {
            runCatching { parseWire<PanelData>(it).blocks }.getOrNull()
        }
    }

    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(PaddingValues(horizontal = 18.dp, vertical = 14.dp)),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (blocks == null) {
            Text(
                "Tap a word for study.",
                color = palette.faded,
                fontStyle = FontStyle.Italic,
                fontSize = (14 * scale).sp,
            )
            return@Column
        }
        for (b in blocks) {
            when (b.kind) {
                "rule" -> HorizontalDivider(color = palette.rule)
                "section" -> SectionBlock(b, palette, scale)
                "para" -> ParaBlock(b, palette, scale, onLink)
            }
        }
    }
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
