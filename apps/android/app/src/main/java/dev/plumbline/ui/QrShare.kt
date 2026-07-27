// "Scan for the app": a QR code of the hosted PWA, so a phone held across a
// table (Present end card, or the Share button) carries the app itself — free,
// offline, no account.
//
// Generated at RENDER time (2026-07-27), because the link is per-reader now: it
// carries whatever church they set in Settings, so there is no one fixed URL to
// bake in. That replaced a build-time constant 29×29 matrix. Keep in sync with
// the web twin (QrCode.svelte), which does the same with qrcode-generator.
//
// Encoded as UTF-8 bytes explicitly — zxing defaults to ISO-8859-1 for byte
// mode, and a church named "Iglesia Bíblica" would come back as mojibake.

package dev.plumbline.ui

import android.content.Context
import android.content.Intent
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import dev.plumbline.ChurchState

const val PWA_URL = "https://plumblinebible.org/"

/** The QR modules for [text] as rows of booleans (true = dark), or null if it
 *  could not be encoded (absurdly long input — never for our links). */
internal fun qrModules(text: String): Array<BooleanArray>? = runCatching {
    val hints = mapOf(
        EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M,
        EncodeHintType.CHARACTER_SET to "UTF-8",
        // The writer pads to this size; 0 keeps the natural module count so we
        // scale it ourselves and stay crisp at any dp.
        EncodeHintType.MARGIN to 0,
    )
    val m = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, 0, 0, hints)
    Array(m.height) { y -> BooleanArray(m.width) { x -> m.get(x, y) } }
}.getOrNull()

/** Paint a QR code for [text], [size] square. Always dark-on-white regardless
 *  of theme — scanners want contrast; the white field is the quiet zone. */
@Composable
fun QrCode(text: String, size: Dp, modifier: Modifier = Modifier) {
    val modules = remember(text) { qrModules(text) }
    val n = modules?.size ?: 0
    Canvas(modifier.size(size)) {
        drawRect(Color.White, size = this.size)
        if (modules == null || n == 0) return@Canvas
        val quiet = 2 // quiet-zone modules on each side
        val px = this.size.width / (n + 2 * quiet)
        for (y in 0 until n) for (x in 0 until modules[y].size) {
            if (modules[y][x]) {
                drawRect(
                    Color(0xFF101010),
                    topLeft = Offset((x + quiet) * px, (y + quiet) * px),
                    // A hair of overlap keeps rounding from leaving hairline seams.
                    size = Size(px + 0.5f, px + 0.5f),
                )
            }
        }
    }
}

/** Fire the system share sheet with the link this reader hands over — the app
 *  plus their church, when they have set one. */
fun shareAppLink(context: Context, church: ChurchState?, startAsNewBeliever: Boolean = false) {
    val url = shareUrl(PWA_URL, church, startAsNewBeliever)
    val from = if (hasChurch(church)) " from ${cleanChurch(church).name}" else ""
    val send = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_TEXT, "Plumbline — the Holy Bible, free and offline$from: $url")
    }
    context.startActivity(Intent.createChooser(send, "Share Plumbline"))
}

/**
 * Share the app: the QR big enough to scan across a table, plus the system
 * share sheet for sending the link directly. [church] rides in both.
 *
 * [onWelcome] re-opens the welcome this reader was given, when they had one —
 * they should not have to reinstall to read it twice.
 */
@Composable
fun ShareAppDialog(
    church: ChurchState?,
    onDismiss: () -> Unit,
    startAsNewBeliever: Boolean = false,
    onWelcome: (() -> Unit)? = null,
) {
    val context = LocalContext.current
    val link = shareUrl(PWA_URL, church, startAsNewBeliever)
    Dialog(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .clip(RoundedCornerShape(16.dp))
                .background(Color.White)
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text("Share Plumbline", color = Color(0xFF101010), fontSize = 19.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(4.dp))
            Text(
                if (hasChurch(church)) {
                    "Free, offline, no account — and your church's details travel with it."
                } else {
                    "Free, offline, no account."
                },
                color = Color(0xFF5A564E), fontSize = 13.sp, textAlign = TextAlign.Center,
            )
            Spacer(Modifier.height(16.dp))
            QrCode(text = link, size = 220.dp)
            Spacer(Modifier.height(10.dp))
            Text("plumblinebible.org", color = Color(0xFF5A564E), fontSize = 12.sp, textAlign = TextAlign.Center)
            if (hasChurch(church)) {
                Spacer(Modifier.height(4.dp))
                Text(
                    "with ${cleanChurch(church).name}",
                    color = Color(0xFF101010), fontSize = 13.sp, fontWeight = FontWeight.Medium,
                    textAlign = TextAlign.Center,
                )
            }
            Spacer(Modifier.height(16.dp))
            Button(onClick = { shareAppLink(context, church, startAsNewBeliever) }) {
                Text("Share the link")
            }
            if (onWelcome != null) {
                TextButton(onClick = onWelcome) { Text("Welcome") }
            }
            TextButton(onClick = onDismiss) { Text("Close") }
        }
    }
}
