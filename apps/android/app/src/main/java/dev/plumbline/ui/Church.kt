// The home church a shared link carries — the shell half of `core::church`.
//
// The point: one QR hands over both the Bible and the people who
// sent it. Whoever shares sets their church in Settings; the link they share
// carries it; whoever opens that link has it saved locally and sees it in the
// welcome, so a card handed out at a service leads back to that service.
//
// Carried as READABLE query parameters rather than an encoded blob: someone
// deciding whether to open a link should be able to see what is in it, and a
// church that mistypes its own details can fix them by reading the URL.
//
// The clamps, the link and the checks live in the core
// (`crates/core/src/church.rs`) and are reached through one ABI call — one
// implementation shared with the web's `shell/church.ts`, not two that can
// drift.
//
// The core follows the web's encoding, because the thing that opens a shared
// link is always the web app: `Uri.appendQueryParameter` percent-encodes while
// `URLSearchParams` form-encodes, and a literal `+` that `Uri` leaves alone is
// read back by `URLSearchParams` as a SPACE — so "Faith + Hope Chapel" would
// otherwise reach the recipient as "Faith  Hope Chapel".
//
// Every call here is engine-independent and pure string work — no engine lock,
// nothing that touches the disk — so it is safe from a composition. Prefer
// [shareOf] once behind a `remember` to calling four one-field helpers, which
// would each cross the ABI.
//
// Shell delta: an Android reader SETS and SHARES a church; it never RECEIVES
// one. A plumblinebible.org link opens the PWA (there is no App Links intent
// filter, deliberately — the web shell is the thing a link should open), so the
// core's `from_query` / `starts_as_new_believer` / `shared_at_ref` have no
// Android caller.

package dev.plumbline.ui

import android.content.Context
import android.content.Intent
import android.net.Uri
import dev.plumbline.ChurchState
import dev.plumbline.PlumblineJson
import dev.plumbline.Share
import dev.plumbline.ShareRequest
import dev.plumbline.StudyEngine
import dev.plumbline.parseWire
import kotlinx.serialization.encodeToString

/** Reached only if the native library is not loadable, which means nothing else
 *  in the app works either. Kept so a share surface still renders something. */
private const val FALLBACK_URL = "https://plumblinebible.org/"

/** The hosted PWA — what every share hands over. The core owns the value
 *  (`church::PWA_URL`); this asks for it once. */
val PWA_URL: String by lazy { shareOf(null).base }

/**
 * Everything a share surface needs for [church], in one call to the core: the
 * link for the QR and the share sheet, the church as the core cleaned it, the
 * label for the Church button and the site (if any) to open.
 *
 * [startAsNewBeliever] marks the link as one handed to someone meeting the
 * Bible — the recipient's welcome opens on the new-believer path instead of
 * asking them to pick. ONLY the Present screen sets it: an ordinary share goes
 * to whoever, often someone from the same church, and must stay an ordinary
 * link. [at] opens the recipient straight at a verse, which is what
 * a shared PASSAGE means; the refKey ("Ps 23:1") is the frozen compact form.
 *
 * [base] is for the one caller that already holds a finished link and only wants
 * a verse added to it (see [linkAtVerse]); everything else lets the core supply
 * the hosted PWA.
 */
fun shareOf(
    church: ChurchState?,
    startAsNewBeliever: Boolean = false,
    at: String? = null,
    base: String? = null,
): Share {
    val req = ShareRequest(base = base, church = church, startAsNewBeliever = startAsNewBeliever, at = at)
    return runCatching {
        StudyEngine.ShareJson(PlumblineJson.encodeToString(req))?.let { parseWire<Share>(it) }
    }.getOrNull() ?: Share(url = base ?: FALLBACK_URL, base = base ?: FALLBACK_URL)
}

/** Whether a church has been set at all — a name is the minimum. */
fun hasChurch(c: ChurchState?): Boolean = shareOf(c).hasChurch

/** Normalize whatever came off a settings field (trimmed, capped). */
fun cleanChurch(c: ChurchState?): ChurchState = shareOf(c).church

/** A share link for [church] (the plain app link when unset). */
fun shareUrl(church: ChurchState?, startAsNewBeliever: Boolean = false, at: String? = null): String =
    shareOf(church, startAsNewBeliever, at).url

/** Add the opening verse to an ALREADY-built share link, so a caller holding
 *  only the finished link (Present) doesn't have to rebuild it from the church.
 *  Setting `at` twice sets it once, rather than appending a second value the
 *  recipient's `URLSearchParams.get` would ignore. */
fun linkAtVerse(link: String, refKey: String?): String {
    if (refKey.isNullOrBlank()) return link
    return shareOf(church = null, at = refKey, base = link).url
}

/** What the reader sees on the Church button when there is no site to open:
 *  who and when, which is all we were given. */
fun churchTitle(c: ChurchState?): String = shareOf(c).title

/** Open the church's website, or fall back to [onNoSite] with a description. */
fun visitChurch(context: Context, church: ChurchState?, onNoSite: (String) -> Unit) {
    val share = shareOf(church)
    val url = share.siteUrl ?: run {
        onNoSite(share.title)
        return
    }
    runCatching {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
    }.onFailure { onNoSite(share.title) }
}
