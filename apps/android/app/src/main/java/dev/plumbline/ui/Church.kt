// The home church a shared link carries — the Kotlin twin of the web shell's
// `shell/church.ts`. Keep the two in step: they build the SAME link, and a QR
// generated here must open the same app state as one generated there.
//
// The point (2026-07-27): one QR hands over both the Bible and the people who
// sent it. Whoever shares sets their church in Settings; the link they share
// carries it; whoever opens that link has it saved locally and sees it in the
// welcome — so a card handed out at a service leads back to that service.
//
// Carried as READABLE query parameters rather than an encoded blob: someone
// deciding whether to open a link should be able to see what is in it, and a
// church that mistypes its own details can fix them by reading the URL.
//
// Shell delta: an Android reader SETS and SHARES a church; it never RECEIVES
// one. A plumblinebible.org link opens the PWA (there is no App Links intent
// filter, deliberately — the web shell is the thing a link should open), so the
// incoming-church capture in App.svelte has no Android counterpart.

package dev.plumbline.ui

import android.content.Context
import android.content.Intent
import android.net.Uri
import dev.plumbline.ChurchState

/** Whether a church has been set at all — a name is the minimum. */
fun hasChurch(c: ChurchState?): Boolean = !c?.name?.trim().isNullOrEmpty()

/** Normalize whatever came off a settings field.
 *
 *  Capped: these end up in a URL, in a QR, and on a welcome screen, and the
 *  length that still scans is finite. Same limits as `cleanChurch` on the web. */
fun cleanChurch(c: ChurchState?): ChurchState = ChurchState(
    name = c?.name?.trim()?.take(80) ?: "",
    info = c?.info?.trim()?.take(120) ?: "",
    url = c?.url?.trim()?.take(200) ?: "",
)

/**
 * A share link for [base] carrying [church] (plain [base] when unset).
 *
 * [startAsNewBeliever] marks the link as one handed to someone meeting the
 * Bible — the recipient's welcome opens on the new-believer path instead of
 * asking them to pick. ONLY the Present screen sets it: an ordinary share goes
 * to whoever, often someone from the same church, and must stay an ordinary
 * link (2026-07-27).
 */
fun shareUrl(
    base: String = PWA_URL,
    church: ChurchState?,
    startAsNewBeliever: Boolean = false,
): String {
    val b = Uri.parse(base).buildUpon()
    val c = cleanChurch(church)
    if (hasChurch(c)) {
        b.appendQueryParameter("church", c.name)
        if (c.info.isNotEmpty()) b.appendQueryParameter("churchInfo", c.info)
        if (c.url.isNotEmpty()) b.appendQueryParameter("churchUrl", c.url)
    }
    if (startAsNewBeliever) b.appendQueryParameter("start", "new")
    return b.build().toString()
}

/** Only http(s) links are offered as links — a church URL is typed by hand, and
 *  nothing else should ever reach an Intent. Null when it isn't openable. */
fun safeChurchUrl(url: String?): String? {
    val u = runCatching { Uri.parse(url?.trim() ?: return null) }.getOrNull() ?: return null
    return if (u.scheme?.lowercase() in setOf("http", "https") && !u.host.isNullOrBlank()) {
        u.toString()
    } else {
        null
    }
}

/** What the reader sees on the Church button when there is no site to open:
 *  who and when, which is all we were given. */
fun churchTitle(c: ChurchState?): String {
    val cc = cleanChurch(c)
    return listOf(cc.name, cc.info).filter { it.isNotEmpty() }.joinToString(" — ")
        .ifEmpty { "Your church" }
}

/** Open the church's website, or fall back to [onNoSite] with a description. */
fun visitChurch(context: Context, church: ChurchState?, onNoSite: (String) -> Unit) {
    val url = safeChurchUrl(church?.url)
    if (url == null) {
        onNoSite(churchTitle(church))
        return
    }
    runCatching {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
    }.onFailure { onNoSite(churchTitle(church)) }
}
