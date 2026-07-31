# Plumbline ProGuard / R8 rules. Release builds run R8 (isMinifyEnabled = true).
#
# SHRINK AND OPTIMIZE, DO NOT RENAME. Everything below keeps a name that
# something reads at runtime; -dontobfuscate makes the whole question moot
# instead of leaving it to the completeness of these rules, which is the right
# trade for an app whose two runtime-reflective surfaces are its native ABI and
# its on-disk/wire data model, and whose crash reports arrive as text in an issue
# (sideloaded APK, no Play Console, no retained mapping.txt). Keep the rules
# anyway: they are also what stops the shrinker DELETING these members, which
# -dontobfuscate does nothing about.
-dontobfuscate

# ── JNA ─────────────────────────────────────────────────────────────────────
# JNA maps interfaces/structures to native memory via reflection; obfuscating
# or stripping method names, @FieldOrder fields, or the Structure subclasses
# breaks the ABI mapping at runtime.
-keep class com.sun.jna.** { *; }
-keepclassmembers class com.sun.jna.** { *; }
-dontwarn java.awt.**

# Our JNA binding lives in dev.plumbline.core (PlumblineNative interface + the
# PlumblineLayoutConfig Structure with @Structure.FieldOrder). Keep it intact.
-keep class dev.plumbline.core.** { *; }
-keepclassmembers class dev.plumbline.core.** { *; }

# Keep anything implementing a JNA Library or Callback, and Structure subclasses.
-keep class * implements com.sun.jna.Library { *; }
-keep class * implements com.sun.jna.Callback { *; }
-keep class * extends com.sun.jna.Structure { *; }

# ── kotlinx.serialization ────────────────────────────────────────────────────
# Keep generated serializers and the companion .serializer() accessors for the
# @Serializable wire model (mirrors PureStudyWin/Wire.cs).
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.**

-keepclassmembers class **$$serializer { *; }
-keepclassmembers class * {
    *** Companion;
}
-keepclasseswithmembers class * {
    kotlinx.serialization.KSerializer serializer(...);
}
# Keep the @Serializable wire data classes themselves.
-keep,includedescriptorclasses class dev.plumbline.**$$serializer { *; }
-keepclassmembers @kotlinx.serialization.Serializable class dev.plumbline.** {
    *** Companion;
    kotlinx.serialization.KSerializer serializer(...);
}
