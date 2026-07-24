// Root build script. Plugin versions are declared once here (apply false) and
// applied per-module. Pins are deliberate — see apps/android/README.md.
plugins {
    id("com.android.application") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.0.21" apply false
    // Kotlin 2.0 moved the Compose compiler into its own Gradle plugin; its
    // version tracks the Kotlin version.
    id("org.jetbrains.kotlin.plugin.compose") version "2.0.21" apply false
    id("org.jetbrains.kotlin.plugin.serialization") version "2.0.21" apply false
}
