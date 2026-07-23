# pure-study ProGuard / R8 rules.
# Release builds currently ship with minification off, but these keep rules make
# it safe to flip isMinifyEnabled on later.

# ── JNA ─────────────────────────────────────────────────────────────────────
# JNA maps interfaces/structures to native memory via reflection; obfuscating
# or stripping method names, @FieldOrder fields, or the Structure subclasses
# breaks the ABI mapping at runtime.
-keep class com.sun.jna.** { *; }
-keepclassmembers class com.sun.jna.** { *; }
-dontwarn java.awt.**

# Our JNA binding lives in dev.purestudy.core (PureStudyNative interface + the
# PureLayoutConfig Structure with @Structure.FieldOrder). Keep it intact.
-keep class dev.purestudy.core.** { *; }
-keepclassmembers class dev.purestudy.core.** { *; }

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
-keep,includedescriptorclasses class dev.purestudy.**$$serializer { *; }
-keepclassmembers @kotlinx.serialization.Serializable class dev.purestudy.** {
    *** Companion;
    kotlinx.serialization.KSerializer serializer(...);
}
