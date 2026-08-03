package dev.plumbline.ui

// Tag→weave conversion sheet (2026-07-25): a topic tag accumulates passages
// over time; this turns the tag — or a checked subset of its verse members —
// into a canon-ordered weave via plumbline_engine_weave_from_tag. Re-running after
// the tag grows just adds the new edges.

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.plumbline.StudyEngine
import dev.plumbline.Tags
import dev.plumbline.parseWire
import kotlinx.serialization.encodeToString

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TagWeaveSheet(
    engine: StudyEngine,
    palette: ReaderPalette,
    tagIndex: Int,
    onDone: () -> Unit,
) {
    val context = LocalContext.current
    val tag = remember(tagIndex) {
        runCatching { parseWire<Tags>(engine.TagsJson()!!).tags.getOrNull(tagIndex) }.getOrNull()
    }
    if (tag == null) {
        onDone()
        return
    }
    val members = remember(tag) { tag.members.filter { it.kind == "verse" && it.verse != null } }
    var checked by remember(tag) { mutableStateOf(members.map { it.verse!! }.toSet()) }
    var name by remember(tag) { mutableStateOf(tag.name) }

    ModalBottomSheet(
        onDismissRequest = onDone,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = palette.panelBg,
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .navigationBarsPadding()
                .padding(horizontal = 16.dp),
        ) {
            Text(t("weave.heading", "tag" to tag.name), color = palette.ink, fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
            Text(
                t("weave.hint"),
                color = palette.inkFaded,
                fontSize = 12.5.sp,
                modifier = Modifier.padding(top = 4.dp, bottom = 8.dp),
            )
            HorizontalDivider(color = palette.rule)
            for (m in members) {
                val ref = m.verse!!
                Row(
                    Modifier.fillMaxWidth().clickable {
                        checked = if (ref in checked) checked - ref else checked + ref
                    },
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Checkbox(
                        checked = ref in checked,
                        onCheckedChange = { checked = if (it) checked + ref else checked - ref },
                    )
                    Text(m.display ?: ref, color = palette.ink, fontSize = 15.sp)
                }
            }
            HorizontalDivider(color = palette.rule)
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                label = { Text(t("weave.name")) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
            )
            Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("${checked.size} of ${members.size} passages", color = palette.inkFaded, fontSize = 12.5.sp)
                Spacer(Modifier.weight(1f))
                TextButton(onClick = onDone) { Text(t("common.cancel")) }
                TextButton(
                    enabled = checked.size >= 2 && name.isNotBlank(),
                    onClick = {
                        val refsJson =
                            if (checked.size == members.size) null
                            else dev.plumbline.PlumblineJson.encodeToString(checked.toList())
                        val err = engine.WeaveFromTag(tag.name, refsJson, name.trim().takeIf { it != tag.name }, nowUtc())
                        Toast.makeText(
                            context,
                            err ?: t("weave.made", "name" to name.trim(), "n" to checked.size),
                            Toast.LENGTH_SHORT,
                        ).show()
                        onDone()
                    },
                ) { Text(t("weave.create")) }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}
