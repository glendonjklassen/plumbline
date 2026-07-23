// The Android entry point. Loads the native core, opens the study engine from
// bundled asset bytes (no writable home needed for reading — personal study data
// would go to the app's private files dir later), tracks the fold posture, and
// hands off to the Compose UI. Mirrors the WinUI App/MainWindow startup: resolve
// data, open the engine off the UI thread, then build the reader.
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

        // Open the engine from bundled assets, off the UI thread (~22 MB).
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val kjv = assets.open("data/kjv.jsonl").use { it.readBytes() }
                    val strongs = assets.open("data/strongs.json").use { it.readBytes() }
                    StudyEngine.OpenFromBytes(kjv, strongs)
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
