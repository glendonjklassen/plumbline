// "Scan for the app": a QR code of the hosted PWA, so a phone held across a
// table (Present end card, or the Share button) carries the app itself — free,
// offline, no account.
//
// Generated at RENDER time, because the link is per-reader: it carries whatever
// church they set in Settings, so there is no one fixed URL to bake in. Keep in
// sync with the web twin (QrCode.svelte), which does the same with
// qrcode-generator.
//
// Encoded as UTF-8 bytes explicitly — zxing defaults to ISO-8859-1 for byte
// mode, and a church named "Iglesia Bíblica" would come back as mojibake.

package dev.plumbline.ui

import android.content.Context
import android.content.Intent
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import dev.plumbline.ChurchState

// PWA_URL lives in Church.kt, and comes from the core's `church::PWA_URL`.

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
    val share = shareOf(church, startAsNewBeliever)
    val from = if (share.hasChurch) " from ${share.church.name}" else ""
    val send = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_TEXT, "Plumbline — the Holy Bible, free and offline$from: ${share.url}")
    }
    context.startActivity(Intent.createChooser(send, t("share.title")))
}
