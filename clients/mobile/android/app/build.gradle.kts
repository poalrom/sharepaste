import com.android.build.api.artifact.SingleArtifact
import org.gradle.process.ExecOperations
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import java.util.Properties
import javax.inject.Inject

plugins {
    alias(libs.plugins.android.application)
    // AGP 9's built-in Kotlin does not apply this for a conventional build
    // script — the auto-apply it carries is gated behind the declarative DSL —
    // so `buildFeatures { compose = true }` alone compiles nothing.
    alias(libs.plugins.compose.compiler)
}

// ---------------------------------------------------------------------------
// The native half of this application.
//
// Two tasks, wired into the build graph rather than left to a README. A stale
// `.so` sitting beside freshly generated Kotlin bindings compiles perfectly and
// then dies at the first call with a checksum mismatch, so "remember to run the
// script" is not an acceptable contract for anyone.
// ---------------------------------------------------------------------------

/** Where the Cargo workspace lives, relative to this Gradle project. */
val rustWorkspace: Directory = layout.projectDirectory.dir("../../..")

/**
 * The Rust sources the two tasks below are a function of.
 *
 * Declared precisely rather than as "the whole workspace" so Gradle can skip
 * both tasks when nothing under them has moved, and so a desktop-only edit does
 * not trigger a 40-second cross-compile.
 */
val rustInputs: FileCollection = files(
    rustWorkspace.dir("core/src"),
    rustWorkspace.dir("mobile/ffi/src"),
    rustWorkspace.file("core/Cargo.toml"),
    rustWorkspace.file("mobile/ffi/Cargo.toml"),
    rustWorkspace.file("mobile/ffi/uniffi.toml"),
    rustWorkspace.file("Cargo.toml"),
    rustWorkspace.file("Cargo.lock"),
)

/**
 * The ABIs the release carries: every real device, plus the emulator that is
 * the only thing anything here gets tested on.
 *
 * `armeabi-v7a` and `x86` are omitted on purpose, not by accident. Both build
 * clean with this toolchain — ticket 01 verified that — but neither can be run
 * anywhere available (the API 35 x86_64 system image ships no 32-bit loader),
 * and 64-bit has been mandatory for years. Re-adding one is a single entry
 * here and in `abiFilters` below.
 */
val shippedAbis = listOf("arm64-v8a", "x86_64")

/**
 * The API level the native library is compiled against. Tracks `minSdk`: the
 * NDK's clang wrapper carries the level in its own name.
 */
val nativeApiLevel = 29

/**
 * Cargo features for the FFI crate, decided by the build type rather than by a
 * flag somebody has to remember at release time.
 *
 * `testing` exposes the crypto known-answer vector and the in-memory facade,
 * which is what the instrumented tests drive. Ticket 08 left it on in every
 * variant so that the library the emulator proves is the library that ships,
 * and named `-Psharepaste.ffi.features=` as the way off. Ticket 13 takes the
 * other side of that trade: a shipped artifact carrying an in-memory-database
 * constructor and a fixed public key is surface with no user, and "pass the
 * flag on release day" is a contract with a human rather than with the build.
 *
 * Nothing is lost by variant. `androidTest` compiles and runs against the
 * **debug** variant, which still has every hook; what changes is only what can
 * be reached inside `app-release.apk`.
 *
 * The property still overrides, in both directions, for anyone reproducing the
 * other library locally.
 */
fun ffiFeaturesFor(buildType: String?): String =
    providers.gradleProperty("sharepaste.ffi.features").orNull
        ?: if (buildType == "release") "" else "testing"

val ndkVersionPinned = "27.2.12479018"

/** The SDK root, from the same places every other Android tool looks. */
val androidSdkDir: String = run {
    val fromLocalProperties = rootProject.layout.projectDirectory.file("local.properties").asFile
        .takeIf { it.exists() }
        ?.let { file ->
            val properties = Properties()
            file.inputStream().use { properties.load(it) }
            properties.getProperty("sdk.dir")
        }
    fromLocalProperties
        ?: providers.environmentVariable("ANDROID_HOME").orNull
        ?: providers.environmentVariable("ANDROID_SDK_ROOT").orNull
        ?: error("No Android SDK. Set sdk.dir in local.properties, or ANDROID_HOME.")
}

val androidNdkDir: String = "$androidSdkDir/ndk/$ndkVersionPinned"

/**
 * The `cargo` binary, resolved once here rather than left to `PATH` lookup
 * inside the Gradle daemon, whose environment is not the shell's.
 */
val cargoExecutable: String = run {
    val exe = if (System.getProperty("os.name").startsWith("Windows")) "cargo.exe" else "cargo"
    val candidates = listOfNotNull(
        providers.environmentVariable("CARGO_HOME").orNull?.let { "$it/bin/$exe" },
        "${System.getProperty("user.home")}/.cargo/bin/$exe",
    )
    candidates.firstOrNull { File(it).isFile } ?: exe
}

/** Builds `libsharepaste_ffi.so` for each shipped ABI. */
@CacheableTask
abstract class CargoNdkBuild : DefaultTask() {
    @get:Input abstract val abis: ListProperty<String>
    @get:Input abstract val features: Property<String>
    @get:Input abstract val apiLevel: Property<Int>
    @get:Input abstract val cargo: Property<String>
    @get:Input abstract val ndkDir: Property<String>

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val sources: ConfigurableFileCollection

    @get:Internal abstract val workspace: DirectoryProperty

    /**
     * `CARGO_TARGET_DIR` for this variant.
     *
     * `@Internal` because it is scratch space, not a declared output: what this
     * task produces is [jniLibs]. It is per variant so that the two feature
     * sets do not invalidate each other's fingerprints and recompile the whole
     * dependency graph on every alternation between debug and release.
     */
    @get:Internal abstract val targetDir: DirectoryProperty

    @get:OutputDirectory abstract val jniLibs: DirectoryProperty

    @get:Inject abstract val exec: ExecOperations

    @TaskAction
    fun build() {
        val args = mutableListOf("ndk")
        abis.get().forEach { args += listOf("-t", it) }
        args += listOf("-P", apiLevel.get().toString())
        args += listOf("-o", jniLibs.get().asFile.absolutePath)
        args += listOf("build", "-p", "sharepaste-ffi", "--release")
        if (features.get().isNotBlank()) args += listOf("--features", features.get())

        exec.exec {
            commandLine(listOf(cargo.get()) + args)
            workingDir = workspace.get().asFile
            // cargo-ndk derives every CC/AR/linker variable from this one path;
            // ticket 01's per-target recipe is what it reproduces.
            environment("ANDROID_NDK_HOME", ndkDir.get())
            environment("CARGO_TARGET_DIR", targetDir.get().asFile.absolutePath)
        }
    }
}

/** Generates the Kotlin bindings out of the compiled library's own metadata. */
@CacheableTask
abstract class UniffiBindgen : DefaultTask() {
    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val library: RegularFileProperty

    @get:Input abstract val cargo: Property<String>
    @get:Internal abstract val workspace: DirectoryProperty
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @get:Inject abstract val exec: ExecOperations

    @TaskAction
    fun generate() {
        val out = outputDir.get().asFile
        // Stale bindings for a symbol that no longer exists would still
        // compile; clearing first makes a removal a compile error.
        out.deleteRecursively()
        out.mkdirs()
        exec.exec {
            commandLine(
                cargo.get(), "run", "--quiet",
                "-p", "sharepaste-ffi", "--features", "cli", "--bin", "uniffi-bindgen",
                "--",
                "generate", "--language", "kotlin", "--no-format",
                "--out-dir", out.absolutePath,
                library.get().asFile.absolutePath,
            )
            workingDir = workspace.get().asFile
        }
    }
}

/**
 * The `versionCode` for a `major.minor.patch` version name.
 *
 * Derived, never maintained beside it. Android refuses an in-place update whose
 * `versionCode` did not increase — that refusal is the whole mechanism ADR 0008
 * leans on instead of an in-app updater — and a hand-kept integer next to a
 * hand-kept string is the one of the pair that gets forgotten, on the release
 * where forgetting it is fatal.
 *
 * `major * 10000 + minor * 100 + patch`: monotone for any component under 100,
 * and 0.2.0 reads as 200 in `dumpsys package` output, which is legible.
 */
fun versionCodeOf(name: String): Int {
    val parts = Regex("""^(\d+)\.(\d+)\.(\d+)$""").find(name)
        ?: error("versionName '$name' is not major.minor.patch, so no versionCode can be derived from it.")
    val (major, minor, patch) = parts.destructured
    return major.toInt() * 10000 + minor.toInt() * 100 + patch.toInt()
}

// ---------------------------------------------------------------------------
// Release signing.
//
// The keystore is the second irreplaceable secret in this project, and the
// backup drill for both of them is recorded together in ADR 0005's
// Consequences. Android pins the signing certificate: an APK signed by a
// different key cannot update an installed one, it is refused outright. That
// refusal is stronger than the minisign check compiled into the desktop binary
// and is precisely why the phone ships no update code (ADR 0008).
//
// So the keystore never enters this repository. The four values arrive as
// environment variables in CI — Actions secrets of the same names — or as
// Gradle properties in `~/.gradle/gradle.properties` locally.
//
// A contributor holding none of them is not blocked: `assembleRelease` then
// produces `app-release-unsigned.apk`, which is a name that cannot be mistaken
// for something publishable. What is refused is a *half* configured signer: a
// keystore with no alias would otherwise fail deep inside a packaging task with
// a message about nothing.
// ---------------------------------------------------------------------------

data class ReleaseSigning(
    val store: File,
    val storePassword: String,
    val keyAlias: String,
    val keyPassword: String,
)

/**
 * A signing value from a Gradle property, else from the environment.
 *
 * Blank counts as absent. An unset Actions secret expands to an empty string
 * rather than disappearing, and "" is a contributor without the material, not a
 * contributor with half of it.
 */
fun signingValue(property: String, environment: String): String? =
    (providers.gradleProperty(property).orNull ?: providers.environmentVariable(environment).orNull)
        ?.takeIf { it.isNotBlank() }

val releaseSigning: ReleaseSigning? =
    signingValue("sharepaste.keystore.file", "ANDROID_KEYSTORE_FILE")?.let { path ->
        val store = File(path)
        if (!store.isFile) {
            error("A release keystore was named ($path) but there is no file there.")
        }
        val missing = mutableListOf<String>()
        fun required(property: String, environment: String): String {
            val value = signingValue(property, environment)
            if (value.isNullOrEmpty()) missing += environment
            return value.orEmpty()
        }
        val signing = ReleaseSigning(
            store = store,
            storePassword = required("sharepaste.keystore.password", "ANDROID_KEYSTORE_PASSWORD"),
            keyAlias = required("sharepaste.key.alias", "ANDROID_KEY_ALIAS"),
            keyPassword = required("sharepaste.key.password", "ANDROID_KEY_PASSWORD"),
        )
        if (missing.isNotEmpty()) {
            error(
                "A release keystore was given but ${missing.joinToString(", ")} " +
                    (if (missing.size == 1) "is" else "are") + " missing. Half the signing " +
                    "material produces an APK that cannot update an installed copy.",
            )
        }
        signing
    }

// Only when a release artifact is actually being asked for. A debug build has no
// use for the keystore and a warning it cannot act on is a warning it learns to
// scroll past.
if (releaseSigning == null && gradle.startParameter.taskNames.any { it.contains("elease") }) {
    logger.warn(
        "No release keystore (ANDROID_KEYSTORE_FILE); any release APK from this build will be " +
            "unsigned and cannot install over, or be updated by, a signed one.",
    )
}

android {
    namespace = "com.sharepaste.android"
    compileSdk = 35
    // AGP 9.3's floor. It is independent of `compileSdk`, which stays at 35.
    buildToolsVersion = "36.0.0"
    ndkVersion = ndkVersionPinned

    defaultConfig {
        applicationId = "com.sharepaste.android"
        minSdk = 29
        targetSdk = 35
        // The one place the Android version lives, and the exact line
        // `.github/scripts/check-versions.mjs` reads: it refuses a release
        // where this disagrees with `clients/desktop/src-tauri/tauri.conf.json`,
        // which stays authoritative for every client.
        versionName = "0.3.0"
        versionCode = versionCodeOf(versionName!!)

        // The transport policy the shipped app hands the core, in one place.
        //
        // It is a BuildConfig field rather than a Kotlin constant so that what
        // ships is readable in the build file and assertable from a test in
        // both variants. Android's network security configuration does not
        // constrain Rust `reqwest` — ticket 08 proved that on the emulator by
        // running a full SSE session over `http://` with
        // `usesCleartextTraffic="false"` — so this value, travelling into
        // `Sharepaste.open`, is the only thing that actually refuses cleartext.
        buildConfigField("boolean", "REQUIRE_HTTPS", "true")
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        ndk {
            // See `shippedAbis`. Adding `armeabi-v7a` back is one entry here.
            abiFilters += shippedAbis
        }
    }

    signingConfigs {
        // Created only when the material is present. Its absence is a
        // contributor without the secret, not an error.
        releaseSigning?.let { material ->
            create("release") {
                storeFile = material.store
                storePassword = material.storePassword
                keyAlias = material.keyAlias
                keyPassword = material.keyPassword
                // v1 is the JAR-signature scheme Android 6 and below verified.
                // `minSdk` is 29, so it protects nobody and only widens what an
                // attacker may rewrite inside the zip without breaking a seal.
                enableV1Signing = false
                enableV2Signing = true
                enableV3Signing = true
            }
        }
    }

    buildTypes {
        debug {
            isMinifyEnabled = false
        }
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Null when no keystore was supplied, in which case AGP leaves the
            // artifact unsigned and calls it `app-release-unsigned.apk` — the
            // loudest available signal that it can update nobody's install.
            signingConfig = signingConfigs.findByName("release")
        }
    }

    // One universal APK, and the absence of ABI splits is the point.
    //
    // The channel is a releases page polled by Obtainium, not a store that can
    // serve a per-device variant. A split build would put several APKs on one
    // Release and turn "which one do I download?" into a question a person has
    // to answer correctly. `abiFilters` above already holds the payload to the
    // two ABIs that can run on anything available. Written out rather than left
    // to AGP's default, because a default is what changes silently.
    splits {
        abi {
            isEnable = false
        }
    }

    buildFeatures {
        buildConfig = true
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        jniLibs {
            // Uncompressed and page-aligned, so the loader maps the library
            // instead of extracting it. Required from API 23 up anyway.
            useLegacyPackaging = false
        }
    }
}

kotlin {
    compilerOptions {
        // Pinned rather than left to follow the JDK: the JDK here is 25 and AGP
        // compiles against 17.
        jvmTarget = JvmTarget.JVM_17
    }
}

// ---------------------------------------------------------------------------
// The native half of this application, wired per variant.
//
// Two variants, two libraries: `release` cross-compiles sharepaste-ffi without
// the `testing` feature and `debug` with it, so the hooks the instrumented
// tests drive cannot be reached inside a shipped artifact. They are registered
// here rather than once at the top of the file because this is the only place
// the build type is known.
//
// `addGeneratedSourceDirectory` registers each task as the declared *producer*
// of the directory the variant consumes, so Gradle wires the dependency itself
// and no compile or packaging task can run against a stale `.so` or against
// bindings generated for the other feature set. That is the requirement the
// Android contract states as "a real task dependency, not a manual step": a
// stale `.so` beside fresh bindings compiles perfectly and then dies at the
// first call with a checksum mismatch and no compile error anywhere.
// ---------------------------------------------------------------------------
androidComponents {
    onVariants { variant ->
        val suffix = variant.name.replaceFirstChar(Char::uppercase)
        val variantFeatures = ffiFeaturesFor(variant.buildType)

        val cargoBuild = tasks.register<CargoNdkBuild>("buildRustLibraries$suffix") {
            group = "build"
            description =
                "Cross-compiles sharepaste-ffi for ${variant.name} " +
                "(features: ${variantFeatures.ifBlank { "none" }})."
            abis.set(shippedAbis)
            features.set(variantFeatures)
            apiLevel.set(nativeApiLevel)
            cargo.set(cargoExecutable)
            ndkDir.set(androidNdkDir)
            sources.from(rustInputs)
            workspace.set(rustWorkspace)
            // A Cargo target directory per variant. Two feature sets sharing
            // one would invalidate each other's fingerprints and recompile the
            // whole dependency graph on every alternation between them.
            targetDir.set(rustWorkspace.dir("target/android/${variant.name}"))
        }
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoBuild, CargoNdkBuild::jniLibs)

        val bindgen = tasks.register<UniffiBindgen>("generateUniffiBindings$suffix") {
            group = "build"
            description = "Generates the Kotlin bindings from the ${variant.name} sharepaste-ffi library."
            // Read out of the x86_64 build purely because one of them has to be
            // picked; UniFFI's metadata is architecture-independent and every
            // ABI carries the same. Taking it from a *built* library rather
            // than from the sources is what makes a bindings/library mismatch
            // impossible — and with two feature sets in play that matters more,
            // not less: the release bindings must not name a function the
            // release library does not export.
            library.set(cargoBuild.flatMap { it.jniLibs.file("x86_64/libsharepaste_ffi.so") })
            cargo.set(cargoExecutable)
            workspace.set(rustWorkspace)
        }
        variant.sources.kotlin?.addGeneratedSourceDirectory(bindgen, UniffiBindgen::outputDir)
    }
}

dependencies {
    implementation(libs.androidx.security.crypto)
    implementation(libs.kotlinx.coroutines.android)
    // `@aar`, not the plain jar: the jar carries no Android native payload and
    // fails at the first FFI call with an UnsatisfiedLinkError.
    implementation(variantOf(libs.jna) { artifactType("aar") })

    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    debugImplementation(libs.androidx.compose.ui.tooling)

    // The scanner. CameraX drives the sensor; ZXing decodes the luminance plane
    // in this process, offline. See the catalogue for why it is not ML Kit.
    implementation(libs.androidx.camera.core)
    implementation(libs.androidx.camera.camera2)
    implementation(libs.androidx.camera.lifecycle)
    implementation(libs.androidx.camera.view)
    implementation(libs.zxing.core)

    // Reads the merged manifest off the build output, which is not a thing a
    // device can see. See `mergedManifestForUnitTests` below.
    testImplementation(libs.junit)

    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.rules)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.kotlinx.coroutines.test)
    androidTestImplementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
    // Supplies the empty activity `createComposeRule` hosts a composable in.
    debugImplementation(libs.androidx.compose.ui.test.manifest)
    // The QR fixture: a test synthesises a code bitmap, so it needs the writer
    // half of ZXing as well.
    androidTestImplementation(libs.zxing.core)
}

// ---------------------------------------------------------------------------
// The two leakage controls, proven from the build output.
//
// `allowBackup` and `dataExtractionRules` both default to *permissive*, so a
// merge that drops either one ships the exposure silently and nothing at
// runtime looks wrong. `MergedManifestTest` reads the manifest AGP actually
// produced and asserts both are denied; this hands it the path, because the
// merged manifest exists only under `build/` and a device never sees it.
// ---------------------------------------------------------------------------
androidComponents {
    onVariants { variant ->
        val mergedManifest = variant.artifacts.get(SingleArtifact.MERGED_MANIFEST)
        tasks.withType<Test>().configureEach {
            if (!name.contains(variant.name, ignoreCase = true)) return@configureEach
            inputs.file(mergedManifest).withPropertyName("mergedManifest")
            jvmArgumentProviders.add(
                CommandLineArgumentProvider {
                    listOf("-Dsharepaste.mergedManifest=${mergedManifest.get().asFile.absolutePath}")
                },
            )
        }
    }
}
