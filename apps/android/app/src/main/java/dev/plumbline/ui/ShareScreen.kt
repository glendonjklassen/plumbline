// SHARE, as a destination — the evangelism role's home (web twin
// shell/ShareScreen.svelte). What was a top-bar QR dialog plus three church
// fields buried at the bottom of Settings is one screen: the QR and link that
// hand the app over, and the church that a shared link carries — set where its
// effect is visible, not in Settings.
//
// Author D (Compose UI).

package dev.plumbline.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.activity.compose.BackHandler
import dev.plumbline.ChurchState

/**
 * The Share destination: "Scan for the app" (QR + the system share sheet) and
 * the reader's home church, edited beside the QR that carries it. [barActions]
 * is the ≡ utilities menu every destination's bar carries.
 */
@Composable
fun ShareScreen(
    palette: ReaderPalette,
    church: ChurchState?,
    onChurch: (ChurchState) -> Unit,
    onPresentGospel: () -> Unit,
    onClose: () -> Unit,
    barActions: @Composable RowScope.() -> Unit = {},
) {
    val context = LocalContext.current
    // One trip to the core per church: the link, the "with …" line and the
    // hasChurch test all come off the same answer (the ShareAppDialog stance).
    val share = remember(church) { shareOf(church) }
    BackHandler(onBack = onClose)
    Column(Modifier.fillMaxSize().background(palette.paper)) {
        ScreenBar(t("nav.share"), palette, onClose, actions = barActions)
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp),
        ) {
            // The QR card stays dark-on-white whatever the theme — scanners
            // want contrast, and the white field is the quiet zone.
            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(16.dp))
                    .background(Color.White)
                    .padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(t("share.title"), color = Color(0xFF101010), fontSize = 19.sp, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.height(4.dp))
                Text(
                    if (share.hasChurch) t("share.subChurch") else t("share.sub"),
                    color = Color(0xFF5A564E), fontSize = 13.sp, textAlign = TextAlign.Center,
                )
                Spacer(Modifier.height(16.dp))
                QrCode(text = share.url, size = 220.dp)
                Spacer(Modifier.height(10.dp))
                Text(
                    share.base.substringAfter("://").trimEnd('/'),
                    color = Color(0xFF5A564E), fontSize = 12.sp, textAlign = TextAlign.Center,
                )
                if (share.hasChurch) {
                    Spacer(Modifier.height(4.dp))
                    Text(
                        t("share.with", "church" to share.church.name),
                        color = Color(0xFF101010), fontSize = 13.sp, fontWeight = FontWeight.Medium,
                        textAlign = TextAlign.Center,
                    )
                }
                Spacer(Modifier.height(16.dp))
                Button(onClick = { shareAppLink(context, church) }) {
                    Text(t("share.action"))
                }
            }

            Spacer(Modifier.height(20.dp))

            // Share is the app AND the Gospel (maintainer direction,
            // 2026-08-11): the same Present that Preach raises, opened
            // straight onto the Romans Road — the first-run "sharing the
            // gospel" path, living where the sharing happens.
            Text(t("share.gospel"), color = palette.faded, fontSize = 12.sp)
            Text(t("share.gospelDesc"), color = palette.faded, fontSize = 12.sp)
            TextButton(onClick = onPresentGospel) {
                Text(t("share.gospelGo"), color = palette.gold)
            }

            Spacer(Modifier.height(20.dp))

            // Your church — what this reader's own shared links carry. Held
            // locally in edit state and pushed up on every change (the same
            // shape the Settings dialog used before the fields moved here).
            Text(t("settings.church"), color = palette.faded, fontSize = 12.sp)
            Text(t("settings.churchDesc"), color = palette.faded, fontSize = 12.sp)
            val cc = remember(church) { cleanChurch(church) }
            var cName by remember { mutableStateOf(cc.name) }
            var cInfo by remember { mutableStateOf(cc.info) }
            var cUrl by remember { mutableStateOf(cc.url) }
            fun pushChurch() = onChurch(cleanChurch(ChurchState(cName, cInfo, cUrl)))
            OutlinedTextField(
                value = cName,
                onValueChange = { cName = it; pushChurch() },
                label = { Text(t("settings.churchName")) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
            )
            OutlinedTextField(
                value = cInfo,
                onValueChange = { cInfo = it; pushChurch() },
                label = { Text(t("settings.churchInfo")) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
            )
            OutlinedTextField(
                value = cUrl,
                onValueChange = { cUrl = it; pushChurch() },
                label = { Text(t("settings.churchUrl")) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
            )
            if (hasChurch(church)) {
                // The recipient's path to the congregation a shared link named —
                // this was the ⋮ menu's Church entry before Share was a role.
                TextButton(onClick = { visitChurch(context, church) { /* no site: the label said who */ } }) {
                    Text(t("shell.church"), color = palette.gold)
                }
            }
        }
    }
}
