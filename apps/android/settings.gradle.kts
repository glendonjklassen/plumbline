// Plumbline Android shell — Gradle settings.
// One of the product's two shells (this Compose app + the web PWA) over the same
// plumbline-core C ABI. The GTK and WinUI desktop shells were retired, so parity
// now means Compose ↔ web.

pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}

@Suppress("UnstableApiUsage")
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "plumbline-android"
include(":app")
