# Desktop CI Build Artifacts Design

## Context

Sharepaste has a Tauri 2 desktop client in `clients/desktop`. The app already
has separate Node dependency roots for the Tauri shell and Vite UI, plus a
Rust toolchain file for the desktop workspace. There is currently no GitHub
Actions workflow for building desktop app artifacts on pushes to `main`.

## Goal

Build unsigned macOS and Windows desktop app artifacts automatically whenever
new commits are pushed to `main`, and make those artifacts available from the
GitHub Actions run.

## Non-Goals

This pass does not add code signing, notarization, GitHub Releases, updater
metadata, Linux builds, or release tagging. It also does not change application
runtime behavior.

## Approach

Add a GitHub Actions workflow with a matrix for `macos-latest` and
`windows-latest`. Each job checks out the repo, installs Node 25, installs
Rust stable, installs npm dependencies for `clients/desktop` and
`clients/desktop/ui`, then runs the Tauri build through
`tauri-apps/tauri-action`.

The workflow uploads Tauri workflow artifacts. macOS builds request `app` and
`dmg` bundles with the macOS private API config merged in. Windows builds
request the `nsis` bundle explicitly so they do not inherit the macOS-oriented
`app`/`dmg` target list from `tauri.conf.json`.

## Documentation

Update the desktop README prerequisites to require Node 25 specifically, so CI
and local development use the same major Node version.

## Error Handling

The workflow should fail if dependency installation, the UI build, Rust build,
Tauri bundling, or artifact upload fails. The matrix uses `fail-fast: false` so
a failure on one platform does not cancel the other platform build.

## Testing

Validate the workflow YAML locally for basic syntax by parsing it with an
available YAML parser. Run the desktop UI production build locally with Node 25
if available. If local Node is not version 25, report that constraint rather
than claiming full parity with CI.
