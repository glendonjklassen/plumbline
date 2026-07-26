// "Scan for the app": a pre-generated QR code of the hosted PWA, so a phone
// held across a table (Present end card, or ⋮ → Share the app) carries the
// app itself — free, offline, no account. The matrix is a build-time constant
// (version 3, ECC M, 29×29) — no QR library, no network. Regenerate after a
// URL change with:
//   python3 -c "import qrcode; q=qrcode.QRCode(error_correction=qrcode.constants.ERROR_CORRECT_M, border=0); q.add_data('<url>'); q.make(fit=True); print('\n'.join(''.join('1' if c else '0' for c in r) for r in q.modules))"
// (pip install qrcode). Keep in sync with the web twin (QrCode.svelte).

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

const val PWA_URL = "https://plumblinebible.org/"

private val MODULES = arrayOf(
    "11111110101001101110101111111",
    "10000010001001110101001000001",
    "10111010010110100100101011101",
    "10111010110110000010001011101",
    "10111010110100000110001011101",
    "10000010100010001101101000001",
    "11111110101010101010101111111",
    "00000000110001011110100000000",
    "10001011110011000100111111001",
    "00000100010011101000001111111",
    "01111111000000001000011000001",
    "11000001100110100101010011011",
    "01110010010011111101110000010",
    "10101001011100000010001111111",
    "11100111010101110010100001101",
    "01000001110001011110011000011",
    "10000010011101000110110100010",
    "10101000111001101110001111011",
    "00101110111000001010010100101",
    "00001000011110100111001010011",
    "11010011100101111011111111001",
    "00000000100010000111100010001",
    "11111110111111110011101011101",
    "10000010010011011001100010000",
    "10111010101101000001111111001",
    "10111010011101101010110000010",
    "10111010000011101101010001111",
    "10000010000111100110100101011",
    "11111110100010011010101010010",
)

/** Paint the PWA QR code, [size] square. Always dark-on-white regardless of
 *  theme — scanners want contrast; the white field is the quiet zone. */
@Composable
fun PwaQrCode(size: Dp, modifier: Modifier = Modifier) {
    val n = MODULES.size
    val quiet = 2 // quiet-zone modules on each side
    Canvas(modifier.size(size)) {
        val px = this.size.width / (n + 2 * quiet)
        drawRect(Color.White, size = this.size)
        for (y in 0 until n) for (x in 0 until n) {
            if (MODULES[y][x] == '1') {
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

/** Fire the system share sheet with the PWA link. */
fun sharePwaUrl(context: Context) {
    val send = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_TEXT, "Plumbline — the 1769 KJV, free and offline: $PWA_URL")
    }
    context.startActivity(Intent.createChooser(send, "Share Plumbline"))
}

/** ⋮ → Share the app: the QR big enough to scan across a table, plus the
 *  system share sheet for sending the link directly. */
@Composable
fun ShareAppDialog(onDismiss: () -> Unit) {
    val context = LocalContext.current
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
                "Free, offline, no account.",
                color = Color(0xFF5A564E), fontSize = 13.sp, textAlign = TextAlign.Center,
            )
            Spacer(Modifier.height(16.dp))
            PwaQrCode(size = 220.dp)
            Spacer(Modifier.height(10.dp))
            Text(PWA_URL, color = Color(0xFF5A564E), fontSize = 12.sp, textAlign = TextAlign.Center)
            Spacer(Modifier.height(16.dp))
            Button(onClick = { sharePwaUrl(context) }) { Text("Share the link") }
            TextButton(onClick = onDismiss) { Text("Close") }
        }
    }
}
