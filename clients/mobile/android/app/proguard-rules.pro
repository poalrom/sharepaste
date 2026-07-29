# JNA reflects over these at load time, so R8 must not rename or drop them.
# Without this the release build fails at the first FFI call, not at compile.
-keep class com.sun.jna.** { *; }
-keep interface com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { public *; }

# The generated bindings hand JNA callback objects to Rust; their methods are
# invoked from native code and are therefore unreachable to R8's analysis.
-keep class com.sharepaste.core.** { *; }
