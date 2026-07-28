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

        // Open from a WRITABLE home so authored study data — notes, highlights,
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
                    StudyEngine.Open(home.absolutePath)
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

    /** Seed the committed stock study set (threads / weaves / tags) into the home. */
    private fun seedStock(home: File) {
        for (kind in listOf("weaves", "threads", "tags")) {
            if ((assets.list("stock/$kind")?.size ?: 0) > 0) copyAsset("stock/$kind", File(home, kind))
        }
    }

    /** Remove just the stock items (by their bundled filenames); anything the
     *  reader authored themselves is left untouched. */
    private fun clearStock(home: File) {
        for (kind in listOf("weaves", "threads", "tags")) {
            for (n in assets.list("stock/$kind") ?: emptyArray()) File(File(home, kind), n).delete()
        }
    }

    /** Toggle the bundled study set on/off. Reconciles files immediately; the
     *  open engine reloads the study set on the next launch (hence the note). */
    private fun toggleBundled() {
        val home = filesDir
        bundledOn = !bundledOn
        lifecycleScope.launch {
            withContext(Dispatchers.IO) {
                val flag = File(home, ".no-bundle")
                if (bundledOn) { flag.delete(); seedStock(home) } else { clearStock(home); flag.createNewFile() }
            }
            Toast.makeText(
                this@MainActivity,
                (if (bundledOn) "Bundled study set on" else "Bundled study set off") + " — restart to apply",
                Toast.LENGTH_LONG,
            ).show()
        }
    }

    /** Recursively copy an asset path (file or directory) into [dest]. */
    private fun copyAsset(path: String, dest: File) {
        val children = assets.list(path) ?: emptyArray()
        if (children.isEmpty()) {
            dest.parentFile?.mkdirs()
            assets.open(path).use { input -> dest.outputStream().use { input.copyTo(it) } }
        } else {
            dest.mkdirs()
            for (c in children) copyAsset("$path/$c", File(dest, c))
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
    MaterialTheme {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            CircularProgressIndicator()
        }
    }
}

@Composable
private fun ErrorScreen(message: String) {
    MaterialTheme {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text("Startup failed: $message")
        }
    }
}
