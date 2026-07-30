// The Android entry point. Loads the native core, extracts the bundled reference
// data into the app's private files dir (a WRITABLE home, so authored study data
// persists), opens the engine from there off the UI thread, tracks the fold
// posture, and hands off to the Compose UI. Mirrors the WinUI App/MainWindow
// startup: resolve data, open the engine, then build the reader.
//
// Author D (Compose UI).

package dev.plumbline

import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.window.layout.FoldingFeature
import androidx.window.layout.WindowInfoTracker
import dev.plumbline.ui.PlumblineApp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.io.InputStream
import java.util.concurrent.atomic.AtomicLong

class MainActivity : ComponentActivity() {

    private var engine by mutableStateOf<StudyEngine?>(null)
    private var loadError by mutableStateOf<String?>(null)
    private var fold by mutableStateOf<FoldingFeature?>(null)
    private var bundledOn by mutableStateOf(true)   // ship-with-stock study set

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // config.json resolves through XDG_CONFIG_HOME / $HOME on Linux-family
        // platforms (core::config::config_dir). Android sets neither to a
        // writable path, so the core's config save silently failed and every
        // launch loaded defaults — the reader always reopened John 3 and no
        // preference (text size, theme, last passage, history) persisted. Point
        // it at our private filesDir BEFORE any plumbline_config_* call. Os.setenv
        // writes the libc environ that the Rust core's std::env reads.
        runCatching { android.system.Os.setenv("XDG_CONFIG_HOME", filesDir.absolutePath, true) }

        // Make sure the cdylib is present before the first JNA call. JNA's
        // Native.load would also resolve it, but loading here surfaces a missing
        // .so as a clear crash rather than a lazy failure deep in the binding.
        runCatching { System.loadLibrary("plumbline_ffi") }
        bundledOn = !File(filesDir, ".no-bundle").exists()

        // Open from a WRITABLE home so authored study data — notes, tags,
        // tags, threads, weaves, memorization — persists (a bytes-opened engine
        // has no home and rejects every write). Extract the bundled read-only
        // reference data into the app's private filesDir once, then open FROM
        // there; the user's authored subdirs live alongside it and survive
        // restarts + app updates. Bump the marker name when the bundled data changes.
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val home = filesDir
                    // v2: the bundled set gained akjv.akjvb (the plain-English
                    // overlay). An install that already holds .data-v1 would
                    // never re-extract, so the overlay would reach new installs
                    // only — the marker is what carries a data change to a
                    // device that already has the app.
                    val corpus = File(home, ".data-v2")
                    if (!corpus.exists()) {
                        copyAsset("data", File(home, "data"))
                        if ((assets.list("bridge")?.size ?: 0) > 0) {
                            copyAsset("bridge", File(home, "bridge"))
                        }
                        corpus.createNewFile()
                    }
                    // Seed the stock study set (threads / weaves / tags) once, unless
                    // disabled — its own marker, independent of the corpus extraction.
                    val stock = File(home, ".stock-seeded")
                    if (bundledOn && !stock.exists()) {
                        seedStock(home)
                        stock.createNewFile()
                    }
                    val opened = StudyEngine.Open(home.absolutePath)
                    // STAGE 2, such as it is. `plumbline_engine_open` loads the
                    // corpus and Strong's, but the plain-English overlay arrives
                    // only through `load_core_data` — which the web calls in its
                    // background stage and this shell never called at all.
                    //
                    // So every piece of the Android overlay was built and wired —
                    // the engine binding, the dotted mark in ReaderPane, the
                    // AkjvHeader on a tap, the Settings toggle — and none of it
                    // could ever appear, because the toggle hides itself unless
                    // `AkjvAvailable()` is true and nothing had loaded an overlay
                    // for it to find (2026-07-28). A whole feature held shut by a
                    // missing call.
                    //
                    // Here, before the engine is handed to the UI, rather than as
                    // a background stage: Android has every file on local disk at
                    // open, so there is nothing to stage and nothing to race. That
                    // also makes `AkjvAvailable()` deterministic by the time
                    // StudyScreen asks it, instead of a question whose answer
                    // depends on which finished first.
                    opened.LoadCoreData()
                    opened
                }
            }
            result.onSuccess { e ->
                engine = e
                // Warm the analytics indexes off the UI thread so the first word
                // study / search / map isn't a cold multi-second stall on tap.
                lifecycleScope.launch(Dispatchers.Default) { runCatching { e.WarmIndexes() } }
            }.onFailure { loadError = it.message ?: "could not open corpus" }
        }

        // Track the fold posture lifecycle-aware; expose the FoldingFeature (if any)
        // to Compose so the layout mode can react to flat/half-open/hinge changes.
        lifecycleScope.launch {
            lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
                WindowInfoTracker.getOrCreate(this@MainActivity)
                    .windowLayoutInfo(this@MainActivity)
                    .collect { info ->
                        fold = info.displayFeatures.filterIsInstance<FoldingFeature>().firstOrNull()
                    }
            }
        }

        setContent {
            val e = engine
            when {
                e != null -> PlumblineApp(e, fold, bundledOn, ::toggleBundled)
                loadError != null -> ErrorScreen(loadError!!)
                else -> LoadingScreen()
            }
        }
    }

    /** Seed the committed stock study set (threads / weaves / tags) into the home.
     *  A file that is already there is the reader's and is left alone — see
     *  [shouldSeed]. */
    private fun seedStock(home: File) {
        for (kind in listOf("weaves", "threads", "tags")) {
            if ((assets.list("stock/$kind")?.size ?: 0) > 0) {
                copyAsset("stock/$kind", File(home, kind), keepExisting = true)
            }
        }
    }

    /** Remove the stock items the reader never made their own, and return how
     *  many were KEPT because they differ from the bundled bytes.
     *
     *  Candidates are the bundled filenames only — anything the reader authored
     *  under another name was never in scope. Of those, a file is deleted only
     *  when [isPristineCopy] proves it is byte-for-byte the shipped asset. An
     *  asset we cannot open is a file we cannot judge, so it stays. */
    private fun clearStock(home: File): Int {
        var kept = 0
        for (kind in listOf("weaves", "threads", "tags")) {
            for (n in assets.list("stock/$kind") ?: emptyArray()) {
                val dest = File(File(home, kind), n)
                // Not seeded, already deleted, or a directory (`weaves/suggested`
                // is an asset entry too): nothing here to remove.
                if (!dest.isFile) continue
                val pristine = runCatching { assets.open("stock/$kind/$n") }.getOrNull()
                if (pristine == null) {
                    kept++
                    continue
                }
                if (pristine.use { isPristineCopy(dest, it) }) dest.delete() else kept++
            }
        }
        return kept
    }

    /** Toggle the bundled study set on/off. Reconciles files immediately; the
     *  open engine reloads the study set on the next launch (hence the note).
     *  Turning it OFF says how many of the reader's own edits it kept, because a
     *  toggle that silently leaves files behind is as confusing as one that
     *  silently deletes them. */
    private fun toggleBundled() {
        val home = filesDir
        bundledOn = !bundledOn
        lifecycleScope.launch {
            val kept = withContext(Dispatchers.IO) {
                val flag = File(home, ".no-bundle")
                if (bundledOn) {
                    flag.delete()
                    seedStock(home)
                    0
                } else {
                    val k = clearStock(home)
                    flag.createNewFile()
                    k
                }
            }
            val what = when {
                bundledOn -> "Bundled study set on"
                kept > 0 -> "Bundled study set off — kept $kept you had edited"
                else -> "Bundled study set off"
            }
            Toast.makeText(this@MainActivity, "$what — restart to apply", Toast.LENGTH_LONG).show()
        }
    }

    /** Recursively copy an asset path (file or directory) into [dest]. Every file
     *  lands through [writeThroughTemp], so an interrupted copy can never leave a
     *  truncated one. With [keepExisting] a file already at the destination is
     *  left exactly as the reader left it ([shouldSeed]). */
    private fun copyAsset(path: String, dest: File, keepExisting: Boolean = false) {
        val children = assets.list(path) ?: emptyArray()
        if (children.isEmpty()) {
            if (keepExisting && !shouldSeed(dest)) return
            dest.parentFile?.mkdirs()
            assets.open(path).use { input -> writeThroughTemp(dest, input) }
        } else {
            dest.mkdirs()
            for (c in children) copyAsset("$path/$c", File(dest, c), keepExisting)
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        // The engine handle outlives config changes via mutableStateOf on the
        // Activity; free it only on a real teardown.
        if (isFinishing) engine?.close()
    }
}

@Composable
private fun LoadingScreen() {
    MaterialTheme(typography = dev.plumbline.ui.rememberSerifTypography()) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            CircularProgressIndicator()
        }
    }
}

@Composable
private fun ErrorScreen(message: String) {
    MaterialTheme(typography = dev.plumbline.ui.rememberSerifTypography()) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text("Startup failed: $message")
        }
    }
}

// ---- Seeding rules. Kept as free functions so they can be unit-tested without
// an Activity or an AssetManager: src/test/java/dev/plumbline/StockSeedTest.kt.

/** Does this seed pass write [dest]? Only when nothing is there.
 *
 *  Existence IS the per-file seeded-once marker, which is the web shell's rule
 *  too (`engine/home.ts` buildHome lays the reader's saved copies over the
 *  freshly-seeded stock, and skips the stock paths outright once the seeded flag
 *  is set). Android had no such rule: every launch re-copied the bundled bytes
 *  over the destination, so a stock thread the reader had renamed or re-noted was
 *  silently reverted. What is on disk is theirs. */
internal fun shouldSeed(dest: File): Boolean = !dest.exists()

/** Is [dest] byte-for-byte the bundled asset [pristine] delivers — i.e. may the
 *  OFF toggle delete it?
 *
 *  THE OTHER HALF OF [shouldSeed]. Their copy wins on the way in; this is the
 *  same invariant on the way out. Turning the bundled set off used to delete
 *  every stock FILENAME, so a stock thread or weave the reader had renamed,
 *  re-noted or added verses to went with the pristine ones — their own work,
 *  destroyed by a toggle that reads as "hide the examples".
 *
 *  RAW BYTES, exactly — not a parsed or normalised form, and not a hash:
 *
 *   * `copyAsset` seeds through [writeThroughTemp] with the asset's own bytes, so
 *     an untouched stock file is byte-identical by construction.
 *   * Merely OPENING one changes nothing. The core writes a thread/tag/weave
 *     only from an authoring call, never on a read, so "the reader looked at it"
 *     stays pristine — rightly, since none of their work is in it.
 *   * Any authoring write re-serializes through the core, whose output differs
 *     from the shipped bytes in whitespace and key order before the edit itself
 *     is counted, so an edited file lands on the KEEP side twice over.
 *   * A normalised comparison is the dangerous direction: it would call a
 *     re-saved file pristine even when the reader's change sits in a field the
 *     normaliser drops. Byte equality can only err toward keeping.
 *
 *  Anything unreadable answers false: a file we cannot compare is a file we
 *  cannot prove is untouched, and the safe verdict is to leave it alone. */
internal fun isPristineCopy(dest: File, pristine: InputStream): Boolean {
    if (!dest.isFile) return false
    return try {
        FileInputStream(dest).use { have -> sameStream(have, pristine) }
    } catch (_: IOException) {
        false
    }
}

/** Do two streams deliver exactly the same bytes AND end at the same place?
 *
 *  The "end together" half is load-bearing. A comparison that stops when either
 *  stream runs out calls a TRUNCATED copy pristine, and calls one the reader
 *  appended to pristine as well — both of which are edits, and both of which
 *  would then be deleted. */
private fun sameStream(a: InputStream, b: InputStream): Boolean {
    val bufA = ByteArray(8192)
    val bufB = ByteArray(8192)
    while (true) {
        val nA = fill(a, bufA)
        val nB = fill(b, bufB)
        if (nA != nB) return false      // one ended first: different lengths
        if (nA == 0) return true        // both ended together, every byte matched
        for (i in 0 until nA) if (bufA[i] != bufB[i]) return false
    }
}

/** Read until [buf] is full or the stream ends, returning how many bytes came.
 *  A short `read` is legal and an asset stream gives them, so comparing what two
 *  single reads happened to return would compare misaligned windows. */
private fun fill(s: InputStream, buf: ByteArray): Int {
    var got = 0
    while (got < buf.size) {
        val n = s.read(buf, got, buf.size - got)
        if (n < 0) break
        got += n
    }
    return got
}

/** The unique-per-copy part of a temp name. The core's `store::write_atomic_bytes`
 *  uses the pid; there is one process here, so a counter gives the same
 *  collision-freedom. */
private val tempSeq = AtomicLong()

/** Copy [input] into [dest] through a hidden temp sibling, then rename — so an
 *  interrupted copy leaves either the old file or the whole new one, never a
 *  half-written one. Same shape as the core's `store::write_atomic_bytes`: a
 *  sibling temp (rename is only atomic within one filesystem), flushed to disk
 *  before the rename, and cleaned up best-effort if anything throws. The name is
 *  dotted and `.tmp`-suffixed so a stranded one is ignorable. */
/** Does this file name belong to a half-finished atomic write?
 *
 *  The third statement of one rule. `store::is_temp_name` in the core is the
 *  first, `collectFiles` in the web home the second; all three must accept
 *  exactly `.<stem>.<digits>.tmp`, because every minter — the core's pid, the
 *  wasm counter, and [writeThroughTemp]'s `tempSeq` below — mints that shape.
 *
 *  All three legs are load-bearing, and each rescues something real: `.config`
 *  is a legitimate dotted directory, `config.json.bad` is a deliberate rescue
 *  file that must keep riding along in backups, and a reader may name a thread
 *  "notes.tmp". A stranded temp is unopenable by any loader, but one that
 *  reaches a backup zip is restored onto the next device forever. */
internal fun isTempName(name: String): Boolean {
    if (!name.startsWith(".") || !name.endsWith(".tmp")) return false
    // The stem keeps its own dots, so `.Gen.1.7.json.4242.tmp` is recognised as
    // readily as `.out.9.tmp` — split off the discriminator only.
    val rest = name.substring(1, name.length - 4)
    val cut = rest.lastIndexOf('.')
    if (cut <= 0 || cut == rest.length - 1) return false
    return rest.substring(cut + 1).all { it in '0'..'9' }
}

internal fun writeThroughTemp(dest: File, input: InputStream) {
    val tmp = File(dest.absoluteFile.parentFile, ".${dest.name}.${tempSeq.incrementAndGet()}.tmp")
    try {
        FileOutputStream(tmp).use { out ->
            input.copyTo(out)
            out.flush()
            out.fd.sync()
        }
        // POSIX rename replaces the destination in one step.
        if (!tmp.renameTo(dest)) throw IOException("could not move ${tmp.name} onto $dest")
    } catch (t: Throwable) {
        tmp.delete()
        throw t
    }
}
