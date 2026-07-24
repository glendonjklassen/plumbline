plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

android {
    namespace = "dev.purestudy"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.purestudy"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.2"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        // Only the ABIs we cross-compile the core for: device (arm64-v8a) and
        // the AOSP emulator (x86_64). cargo-ndk drops the .so into
        // src/main/jniLibs/<abi>/libpure_ffi.so; JNA's @aar carries its own.
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
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
            // No obfuscation of the JNA/serialization surfaces — see proguard rules.
            isMinifyEnabled = false
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
        // AGP 8.7's bundled lint crashes analysing our Compose / Kotlin-2.0
        // sources: androidx.lifecycle's NonNullableMutableLiveDataDetector hits
        // an IncompatibleClassChangeError against the Kotlin 2.0 analysis API
        // (a lint/tooling bug, not our code). We don't gate releases on lint, so
        // skip the release-build vital check and never abort on a lint fault.
        checkReleaseBuilds = false
        abortOnError = false
    }

    // Compile the shared Kotlin/JNA binding (package dev.purestudy.core) straight
    // out of the FFI crate — it is the single source of truth for the ABI and is
    // NOT copied into this module.
    sourceSets {
        getByName("main") {
            java.srcDirs("../../../crates/ffi/bindings/kotlin")
            assets.srcDirs("src/main/assets")
        }
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
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

    // Adaptive / fold-aware panes (the three fold modes). These APIs churn —
    // member names are version-specific; pinned deliberately.
    implementation("androidx.compose.material3.adaptive:adaptive:1.2.0")
    implementation("androidx.compose.material3.adaptive:adaptive-layout:1.2.0")
    implementation("androidx.compose.material3.adaptive:adaptive-navigation:1.2.0")

    // WindowInfoTracker / FoldingFeature — source of truth for the hinge.
    implementation("androidx.window:window:1.5.0")

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
    group = "pure-study"
    from(rootProject.file("../../data")) {
        include("kjv.jsonl", "strongs.json", "kjv-notes.jsonl", "cross-references.tsv")
    }
    into(layout.projectDirectory.dir("src/main/assets/data"))
}

// Bundle the cross-testament bridge sources so the fused bridge (and the
// dispersion "bridge row") has data on-device once the engine opens from a home.
tasks.register<Copy>("syncBridge") {
    description = "Copy bridge/*.json into app assets."
    group = "pure-study"
    from(rootProject.file("../../bridge")) { include("*.json") }
    into(layout.projectDirectory.dir("src/main/assets/bridge"))
}

tasks.named("preBuild") {
    dependsOn("syncData", "syncBridge")
}
