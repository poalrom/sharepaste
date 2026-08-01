// swift-tools-version: 6.0

// The iOS shell.
//
// There is no `.xcodeproj` here and there never will be. `xtool` builds this
// package directly with SwiftPM; its `XcodePacker` is wrapped in `#if
// os(macOS)` and only ever writes a throwaway under `xtool/.xtool-tmp`. A
// committed Xcode project would be a build artefact neither build path reads.
//
// Four targets, and the split is not decoration:
//
//  * `sharepaste_ffiFFI` is the C module the UniFFI bindings import. It is a
//    `systemLibrary` because it has no sources of its own — a header and a
//    module map — and because a `systemLibrary` target's name must equal the
//    module name in that map, which the generator derives from the crate.
//  * `SharepasteCore` is the generated Swift, and it is where the archive is
//    linked. Both directories are produced elsewhere and gitignored: run
//    `make ios-vendor` before the first build.
//  * `SharepasteKit` is the chokepoint. Every FFI call in this application
//    goes through `SharepasteFacade`, which is an actor, which is what keeps
//    the boundary's one rule — nothing on the main thread — true by
//    construction rather than by review.
//  * `Sharepaste` is the app: two screens, `Fui.swift`, and the two App
//    Intents. The intents are inside it rather than beside it because ADR 0007
//    wants them in the main binary, and a target of their own would have to
//    depend on the app that depends on them.

import PackageDescription

let package = Package(
    name: "Sharepaste",
    // iOS 16, carried from the mobile-client ledger. App Intents impose the
    // same floor, so nothing here is version-gated above it.
    platforms: [.iOS(.v16)],
    products: [
        // An xtool project contains exactly one library product, and that
        // product is the app.
        .library(name: "Sharepaste", targets: ["Sharepaste"]),
    ],
    targets: [
        .systemLibrary(name: "sharepaste_ffiFFI", path: "Sources/sharepaste_ffiFFI"),
        .target(
            name: "SharepasteCore",
            dependencies: ["sharepaste_ffiFFI"],
            path: "Sources/SharepasteCore",
            linkerSettings: [
                // The archive, whole. An app signed by a free Personal Team may
                // embed no dynamic library of its own, so there is no framework
                // to fall back on — and no xcframework either, because the
                // *slice* is decided by whoever populated `Vendor/` rather than
                // by anything encoded in the artefact. That is what lets CI link
                // a simulator build and the desk link a device build from this
                // one unconditional manifest.
                //
                // Absolute, via `Context.packageDirectory`, because the link
                // does not run here: `xtool dev build` synthesises a builder
                // package under `xtool/.xtool-tmp` that depends on this one by
                // path, and a relative `-L Vendor` resolves against *that*
                // directory. The failure is quiet and confusing — the archive is
                // simply never searched, and every FFI symbol comes back
                // undefined from `ld64.lld` with no word about a missing library.
                .unsafeFlags(["-L\(Context.packageDirectory)/Vendor"]),
                .linkedLibrary("sharepaste_ffi"),
            ]
        ),
        .target(name: "SharepasteKit", dependencies: ["SharepasteCore"]),
        .target(name: "Sharepaste", dependencies: ["SharepasteKit"]),
    ]
)
