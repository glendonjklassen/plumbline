plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

android {
    namespace = "dev.plumbline"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.plumbline"
        minSdk = 26
        targetSdk = 35
        // Version identity comes from the release workflow, derived from the git
        // tag (-PplumblineVersionName / -PplumblineVersionCode). Both spellings
        // must match .github/workflows/release.yml exactly — a mismatch makes
        // findProperty return null and silently ships the dev stamp below.
        // versionCode MUST increase per release or Android won't treat a new
        // APK as an in-place upgrade.
        // Local + CI-debug builds fall back to a dev stamp.
        versionCode = (project.findProperty("plumblineVersionCode") as String?)?.toInt() ?: 1
        versionName = (project.findProperty("plumblineVersionName") as String?) ?: "0.0.0-dev"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        // arm64-v8a is the only ABI: every device this installs on is a
        // 64-bit ARM phone, sideloaded from a GitHub Release. The filter is
        // what keeps JNA's @aar contributions for the other ABIs
        // (armeabi-v7a/x86/x86_64 libjnidispatch.so) out of the APK.
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    // Release signing reads the keystore + passwords from the environment (the
    // GitHub Actions secrets wired in .github/workflows/release.yml). Absent
    // locally, so a local `assembleRelease` just produces an unsigned APK and
    // debug builds (their own auto keystore) are unaffected. The stable release
    // key is what lets a tagged build install in place over the previous one.
    val releaseKeystore: String? = System.getenv("ANDROID_KEYSTORE_FILE")
    signingConfigs {
        if (releaseKeystore != null) {
            create("release") {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        debug {
            isMinifyEnabled = false
        }
        release {
            // R8 SHRINKS AND OPTIMIZES BUT DOES NOT RENAME (`-dontobfuscate` sits
            // in proguard-rules.pro). Shrinking is what pays: 22,773,588 bytes of
            // dex across two files became 3,748,844 in one, because a Compose app
            // links far more of Material3/AndroidX than it calls. Together with
            // the arm64-only native libs, the APK went 20,116,989 → 11,099,262
            // bytes (measured 2026-07-30, both builds unsigned).
            //
            // Renaming on top of that is worth 82,179 bytes — 0.7% — because most
            // of what is left is assets, and it is refused at that price for two
            // reasons particular to this product. The JNA binding and the wire
            // model are both reached BY NAME at runtime, so a keep rule that
            // misses one has no compile-time signal and surfaces as a dead feature
            // or a silently dropped JSON field on a reader's device. And the APK
            // is sideloaded from a GitHub Release: the only crash report this
            // project will ever get is a stack trace pasted into an issue, with no
            // Play Console to symbolicate it and mapping.txt discarded with the CI
            // runner that built it.
            isMinifyEnabled = true
            // Resources only. ASSETS ARE NOT TOUCHED by resource shrinking, so
            // the corpus, Strong's, the fonts and the stock study set all ship
            // whole — the shrinker's scope is res/ and resources.arsc, where what
            // it removes is unreferenced AndroidX drawables/strings. Safe mode is
            // the default (anything reachable via getIdentifier is kept).
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            if (releaseKeystore != null) signingConfig = signingConfigs.getByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
    }

    lint {
        // Lint DOES gate the build. A blanket `abortOnError = false` hides every
        // real finding, and it never even bought what it was added for: the four
        // checks below crash inside `lintAnalyze`, which fails the task no matter
        // what abortOnError says. All four are androidx-shipped detectors that
        // resolve calls through a Kotlin analysis API newer than the one AGP 8.7's
        // bundled lint carries, so they die with IncompatibleClassChangeError
        // ("Found class ...KaFunctionCall, but interface was expected") — one
        // tooling bug, not our code. adaptive:1.2.0 is what drags compose-runtime
        // from the BOM's 1.7.5 up to 1.9.0, whose lint jars are the mismatch, so
        // the real cure is a newer AGP (or android.experimental.lint.version),
        // not these lines. Disable exactly those four, by ISSUE id (lint's crash
        // message names the id, which is NOT the detector's class name), and keep
        // every other check fatal. Note the crash is JVM-state dependent — a warm
        // daemon can hide it — so re-check with `--no-daemon`, which is what CI is.
        abortOnError = true
        disable += "NullSafeMutableLiveData" // androidx.lifecycle
        disable += "FrequentlyChangingValue" // androidx.compose.runtime
        disable += "RememberInComposition" // androidx.compose.runtime
        disable += "AutoboxingStateCreation" // androidx.compose.runtime
    }

    // Compile the shared Kotlin/JNA binding (package dev.plumbline.core) straight
    // out of the FFI crate — it is the single source of truth for the ABI and is
    // NOT copied into this module.
    sourceSets {
        getByName("main") {
            java.srcDirs("../../../crates/ffi/bindings/kotlin")
            assets.srcDirs("src/main/assets")
        }
    }

    // Uncompressed data assets — OFF, and the measurement is why.
    //
    // `noCompress` would let the first-run extraction read kjv.jsonl and friends
    // straight out of the APK instead of inflating them on the way into the
    // writable home. Measured against the assets actually shipped in the
    // release APK (2026-07-30): the four files this
    // covers — kjv.jsonl, cross-references.tsv, akjv.akjvb, kjv-notes.jsonl —
    // are 29,079,604 bytes raw and 4,936,245 stored, so keeping them whole adds
    // 24,143,359. The phone's APK goes 11,099,262 → ~35,242,621 bytes: a 3.2×
    // download for a product whose distribution IS a file pulled by hand from a
    // GitHub Release, with no store to hand out deltas. Installed footprint
    // rises by the same 24 MB, because the extracted copy exists either way.
    //
    // Against that: one inflate, once, on first launch. Reading the same 32.2 MB
    // of data assets took 48 ms deflated and 12 ms stored on this workstation
    // (median of five, 256 KB buffer) — so a few hundred milliseconds on a
    // phone, once per install. Refused at 24 MB a release.
    //
    // The switch stays rather than being deleted because the trade inverts the
    // day the engine can open the pack IN PLACE instead of extracting it: then
    // the 24 MB replaces the extracted copy rather than joining it, and the
    // inflate disappears from every launch rather than the first.
    //
    //   apps/android/gradlew -PplumblineNoCompressData :app:assembleRelease
    androidResources {
        if (project.hasProperty("plumblineNoCompressData")) {
            noCompress += listOf("jsonl", "tsv", "akjvb")
        }
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

// Compose compiler diagnostics, opt-in.
//
// Writes the per-module stability metrics (CSV) and the per-composable report
// (which functions are restartable and skippable, which parameters the compiler
// judged unstable) under app/build/compose-metrics and app/build/compose-reports
// — the numbers you need before claiming a recomposition fix, rather than
// reading the code and hoping. Behind a property because it adds an output to
// every Kotlin compilation in the module, and this is a switch to throw for an
// investigation, not a tax on every build.
//
//   apps/android/gradlew -PplumblineComposeMetrics :app:assembleDebug
composeCompiler {
    if (project.hasProperty("plumblineComposeMetrics")) {
        metricsDestination.set(layout.buildDirectory.dir("compose-metrics"))
        reportsDestination.set(layout.buildDirectory.dir("compose-reports"))
    }
}

dependencies {
    // Compose — versions governed by the BOM.
    val composeBom = platform("androidx.compose:compose-bom:2024.10.01")
    implementation(composeBom)
    androidTestImplementation(composeBom)

    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    // Common Material icons (Search, Close, MoreVert, ArrowBack) for the top-bar
    // chrome — the phone shell leans on icons over text labels.
    implementation("androidx.compose.material:material-icons-core")

    // Adaptive / fold-aware panes (the three fold modes). These APIs churn —
    // member names are version-specific; pinned deliberately.
    implementation("androidx.compose.material3.adaptive:adaptive:1.2.0")
    implementation("androidx.compose.material3.adaptive:adaptive-layout:1.2.0")
    implementation("androidx.compose.material3.adaptive:adaptive-navigation:1.2.0")

    // WindowInfoTracker / FoldingFeature — source of truth for the hinge.
    implementation("androidx.window:window:1.5.0")

    // Installs the ART baseline profile (src/main/baseline-prof.txt, which AGP
    // compiles into assets/dexopt/baseline.prof) into the app's reference profile
    // on first launch, so ART AOT-compiles the startup path from the second
    // launch on. REQUIRED for it to do anything at all here: the alternative
    // installer is the Play Store's cloud profile, and this product ships a
    // sideloaded APK by decision. Without this line the profile ships and is
    // inert, which is what every release through v0.35.0 did: the build before
    // this change already put 8,175 bytes of merged Compose/AndroidX profile in
    // assets/dexopt/baseline.prof, with nothing on the device to read it.
    implementation("androidx.profileinstaller:profileinstaller:1.4.0")

    // Activity + lifecycle-aware Compose entry points.
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("androidx.core:core-ktx:1.13.1")

    // Wire JSON — kotlinx.serialization mirrors PureStudyWin/Wire.cs.
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")

    // JNA over the frozen C ABI. The @aar classifier is REQUIRED: it bundles
    // libjnidispatch.so per-ABI and (5.17+) fixes the 16 KB page-size crash.
    implementation("net.java.dev.jna:jna:5.17.0@aar")

    // QR encoding for the share sheet. The matrix used to be a build-time
    // constant for one fixed URL, which cannot carry a church (2026-07-27) —
    // shared links are now per-reader, so the code is generated at render time.
    // `core` only: the android-integration artifact drags in camera/scanning we
    // never use. Apache-2.0, no transitive dependencies.
    implementation("com.google.zxing:core:3.5.3")

    debugImplementation("androidx.compose.ui:ui-tooling")

    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}

// Copy the frozen data pack into app assets so the engine can be opened via
// OpenFromBytes (no writable home needed for reading). Runs before every build.
tasks.register<Copy>("syncData") {
    description = "Copy data/*.jsonl + strongs.json into app assets for OpenFromBytes."
    group = "plumbline"
    from(rootProject.file("../../data")) {
        // akjv.akjvb is the PACKED overlay, not the JSONL: core's load_akjv
        // prefers the packed sibling, it is 578 KB against 1.35 MB, and it
        // parses without going through serde_json. Same choice the web pack
        // makes. Without it the engine reports the overlay unavailable and the
        // Android toggle correctly hides itself — the feature was fully wired
        // in Kotlin but invisible on device, because the data never shipped.
        // luther1912.jsonl is the GERMAN corpus (data-prep/README.md). Bundled
        // rather than downloaded: it compresses to ~1.8 MB inside the APK, which
        // is small beside cross-references.tsv, and it means a German reader has
        // scripture the moment the app opens. The web fetches it on demand
        // instead, because nothing there is ever bundled.
        include(
            "kjv.jsonl",
            "luther1912.jsonl",
            "strongs.json",
            "kjv-notes.jsonl",
            "cross-references.tsv",
            "akjv.akjvb",
            "hymnal.json",
        )
    }
    into(layout.projectDirectory.dir("src/main/assets/data"))
}

// Bundle the cross-testament bridge sources so the fused bridge (and the
// dispersion "bridge row") has data on-device once the engine opens from a home.
tasks.register<Copy>("syncBridge") {
    description = "Copy bridge/*.json into app assets."
    group = "plumbline"
    from(rootProject.file("../../bridge")) { include("*.json") }
    into(layout.projectDirectory.dir("src/main/assets/bridge"))
}

tasks.named("preBuild") {
    dependsOn("syncData", "syncBridge")
}
