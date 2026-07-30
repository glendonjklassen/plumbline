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
                // Warm the indexes THIS reader's tiers use, off the UI thread, so
                // their first search / word study isn't a cold stall on tap. After
                // `LoadCoreData()` above, deliberately: that call reloads the study
                // data (the 1769 margin notes), and a search index built before it
                // keeps the notes it saw then — an empty set (see
                // `PlumblineEngine::load_core_data`).
                warmIndexes(e)
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

    /** Build the lazy indexes THIS reader's settings actually use, off the UI
     *  thread, so their first search / word study / map isn't a cold stall.
     *
     *  It used to be one `WarmIndexes()` call, which forces all eight —
     *  including the machine tier (concept, leitwort, SIF), which has been OFF
     *  by default since the tiers went opt-in (`core::config`
     *  `machine_analysis: false`). A reader who never asked for machine analysis
     *  paid its corpus-wide scans at every single cold start, for panels the
     *  gates then refuse to draw.
     *
     *  So the plan comes from the reader's own config ([warmPlan]) and is
     *  executed a step at a time ([warmTouch]). Deliberately NOT inside
     *  `synchronized(engine)`, exactly as the old single call was: the first
     *  chapter layout takes that monitor (on `Dispatchers.Default`, see
     *  `ReaderPane.publishOrClose`), so a warm that held it would put itself in
     *  front of first text — the opposite of the point.
     *
     *  The plan is read ONCE, at launch. Turning a tier on mid-session leaves
     *  its indexes to build on first use (the lazy path they have always had);
     *  the next launch warms them. */
    private fun warmIndexes(e: StudyEngine) {
        lifecycleScope.launch(Dispatchers.Default) {
            // The same read the rest of the shell does (StudyScreen's loadedCfg):
            // the static config endpoint, decoded through the wire models. An
            // unreadable/absent config means a reader who has asked for nothing
            // yet, which is the minimal warm.
            val cfg = runCatching { parseWire<ConfigState>(StudyConfig.LoadJson()) }.getOrNull()
            val plan = warmPlan(
                humanAnalysis = cfg?.humanAnalysis == true,
                machineAnalysis = cfg?.machineAnalysis == true,
            )
            if (plan.size == WarmIndex.entries.size) {
                // Both tiers on: one call, byte-for-byte the path this shell has
                // always taken — and the only route that builds the search index
                // without the read-guard nesting [warmTouch] documents.
                runCatching { e.WarmIndexes() }
                return@launch
            }
            for (ix in plan) runCatching { warmTouch(e, ix) }
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

// ---- The cold-start warm-up. The DECISION is a free function so it can be
// unit-tested without an Activity or an engine:
// src/test/java/dev/plumbline/WarmPlanTest.kt.

/** One lazily-built engine index.
 *
 *  These eight are exactly what `plumbline_engine_warm_indexes` forces, in the
 *  order it forces them (`crates/ffi/src/lib.rs`), and each is built once per
 *  session on first use. Which TIER owns which is `core::config`'s own division:
 *  **human** (curated scholarship) owns renderings, same-root and TSK; **machine**
 *  (learned/statistical) owns embeddings, concept, SIF and leitwort. */
internal enum class WarmIndex {
    /** Full-text search over the corpus + the reader's notes. Ungated: the
     *  search overlay is not analysis, and it is one tap from the reader. */
    Search,

    /** Strong's code → the verses carrying it. Ungated too, and needed by every
     *  word study: `panel::code_study` prints the occurrence count before it
     *  looks at a single gate, and the concordance behind that count is the
     *  same index. */
    Occurrences,

    /** The rendering lens (code → English words). `gates.human`. */
    Renderings,

    /** TSK study cross-references — an 8.5 MB TSV parsed from the home.
     *  `gates.human`. */
    StudyXrefs,

    /** The fused OT↔NT bridge. `gates.human` in the word study (SAME ROOT
     *  ACROSS TESTAMENTS), and also the partner band of the concept map. */
    Bridge,

    /** The concept model — a corpus-wide co-occurrence fold. `gates.machine`.
     *
     *  READ THE NOTE ON [warmPlan] BEFORE CHANGING THIS. The panel gate is only
     *  half the story: the embedded concept-map card asks for it with no gate at
     *  all, in BOTH shells. */
    Concept,

    /** The leitwort scan, discovered over the whole corpus. `gates.machine`.
     *  Reachable only through `concept_json`, which the panel gates. */
    Leitwort,

    /** SIF "verses like this". `gates.machine`, and inert on this shell until an
     *  embedding artifact exists to build it from (the APK bundles none, and
     *  nothing here calls `LoadRndData`), so warming it is free either way. */
    VerseSim,
}

/** Which indexes a cold start should build for a reader with these tiers.
 *
 *  Search and the occurrence index are for everybody — both are reachable
 *  without turning any analysis on. Everything else is warmed only for the tier
 *  that owns it, because an index the gates will not let a panel draw is an
 *  index nobody asked to be built. The bridge answers to either tier: the human
 *  word study lists same-root partners, the concept map bands them.
 *
 *  ONE PLACE STILL DISAGREES, and it is not this function's to fix. The embedded
 *  concept-map card is drawn for any tapped code with no tier check —
 *  `StudyScreen.kt`'s `studyEmbed` on Android, `study/StudyPanel.svelte`'s
 *  `{#if studyCode}` on the web — and `plumbline_engine_concept_map_json` reaches
 *  `concept_ready()` and `bridge_ready()`, so a reader with both tiers off can
 *  still make the engine build the concept model. While that is true, gating
 *  Concept here moves that build out of the background warm and into the tap that
 *  opens the card, behind `synchronized(engine)`. How long that costs on a phone
 *  is UNMEASURED HERE — the one number the tree has for this call is the web
 *  engine worker's ("THE 10,205 ms CALL", `plumbline_engine_concept_map_json` in
 *  `crates/ffi/src/lib.rs`), and a wasm worker is not this shell. It is the same
 *  corpus-wide fold either way. The fix belongs in the shells — either gate the
 *  card on the machine tier, or accept that the card is ungated and warm for it —
 *  and it is a product call, not a warm-plan one. Reported with this change; do
 *  not paper over it here.
 *
 *  Order matters — see [warmTouch] on why Search goes first. */
internal fun warmPlan(humanAnalysis: Boolean, machineAnalysis: Boolean): List<WarmIndex> = buildList {
    add(WarmIndex.Search)
    add(WarmIndex.Occurrences)
    if (humanAnalysis) {
        add(WarmIndex.Renderings)
        add(WarmIndex.StudyXrefs)
    }
    if (humanAnalysis || machineAnalysis) add(WarmIndex.Bridge)
    if (machineAnalysis) {
        add(WarmIndex.Concept)
        add(WarmIndex.Leitwort)
        add(WarmIndex.VerseSim)
    }
}

// Probe arguments, chosen only so a call reaches the build behind it: a word the
// KJV does not contain (so the search itself is a miss, not a ranked page), a
// Strong's code the bundled corpus certainly carries (H430, in 2,249 verses of
// `assets/data/kjv.jsonl`), and a real reference — an unparseable one returns
// before the index is touched. The answers are thrown away.
private const val PROBE_WORD = "zzzzq"
private const val PROBE_CODE = "H430"
private const val PROBE_REF = "Gen 1:1"

/** Force ONE index to build, by asking the engine a question that needs it.
 *
 *  The C ABI has no per-index warm: `plumbline_engine_warm_indexes` is all eight
 *  or nothing. (A sliced `warm_step` exists, but only on the wasm-only surface —
 *  promoting it is TODO §H, and this item is not allowed to add ABI surface.) So
 *  a shell that wants a subset has to reach each build through a reader-facing
 *  read. Every call below is a pure read that changes nothing the reader sees.
 *
 *  Search is FIRST, and that is not cosmetic. `plumbline_engine_search_json`
 *  holds a study READ guard across the index build, and the build's own
 *  `attach_notes` takes a second read on the same thread — a nesting that cannot
 *  proceed while a writer is queued, because Rust's futex `RwLock` makes a reader
 *  wait behind waiting writers. Every writer in the ABI is an authoring call
 *  (thread / tag / weave / personal note) plus `load_core_data`, which
 *  `onCreate` has already finished; the reading tracker never takes that lock at
 *  all (`crates/ffi/src/reading_map.rs`). So the only writer that could queue
 *  here needs a deliberate long-press on text that is only now appearing, and
 *  putting the build first makes that window as small as it can be. It is not
 *  zero: the same nesting is on the reader's first search TODAY, and warming it
 *  here is what stops that happening mid-session when writers do exist. The real
 *  cure is core-side — a warm that does not hold the guard. */
private fun warmTouch(e: StudyEngine, ix: WarmIndex) {
    when (ix) {
        WarmIndex.Search -> e.SearchJson(PROBE_WORD)
        WarmIndex.Occurrences -> e.StrongsOccurrencesJson(PROBE_CODE)
        WarmIndex.Renderings -> e.RenderingsJson(PROBE_CODE)
        WarmIndex.StudyXrefs -> e.StudyXrefsJson(PROBE_REF)
        WarmIndex.Bridge -> e.BridgePartnersJson(PROBE_CODE)
        // One call builds the concept model and then reads the leitwort scan
        // through it — there is no other route to the leitwort map — so the
        // second of these two steps finds both built and costs one lookup.
        WarmIndex.Concept, WarmIndex.Leitwort -> e.ConceptJson(PROBE_CODE)
        WarmIndex.VerseSim -> e.SimilarVersesJson(PROBE_REF, 1)
    }
}
