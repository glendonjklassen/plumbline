// The Android entry point. Loads the native core, extracts the bundled reference
// data into the app's private files dir (a WRITABLE home, so authored study data
// persists), opens the engine from there off the UI thread, tracks the fold
// posture, and hands off to the Compose UI. Mirrors the WinUI App/MainWindow
// startup: resolve data, open the engine, then build the reader.
//
// Author D (Compose UI).

package dev.purestudy

import android.os.Bundle
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
import dev.purestudy.ui.PureStudyApp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

class MainActivity : ComponentActivity() {

    private var engine by mutableStateOf<StudyEngine?>(null)
    private var loadError by mutableStateOf<String?>(null)
    private var fold by mutableStateOf<FoldingFeature?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Make sure the cdylib is present before the first JNA call. JNA's
        // Native.load would also resolve it, but loading here surfaces a missing
        // .so as a clear crash rather than a lazy failure deep in the binding.
        runCatching { System.loadLibrary("pure_ffi") }

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
                    val marker = File(home, ".data-v1")
                    if (!marker.exists()) {
                        copyAsset("data", File(home, "data"))
                        if ((assets.list("bridge")?.size ?: 0) > 0) {
                            copyAsset("bridge", File(home, "bridge"))
                        }
                        marker.createNewFile()
                    }
                    StudyEngine.Open(home.absolutePath)
                }
            }
            result.onSuccess { engine = it }
                .onFailure { loadError = it.message ?: "could not open corpus" }
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
                e != null -> PureStudyApp(e, fold)
                loadError != null -> ErrorScreen(loadError!!)
                else -> LoadingScreen()
            }
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
