#!/usr/bin/env node
// Refuses to let a release go out on disagreeing version numbers.
//
//   node .github/scripts/check-versions.mjs [repo-root]
//
// Prints the agreed version on stdout and appends `version=<v>` to
// $GITHUB_OUTPUT when running under Actions. Exits 1 on any disagreement, or
// when the changelog has no section for the version.
//
// This is not cosmetic. `clients/core/src/http/client.rs` builds the Relay
// User-Agent from CARGO_PKG_VERSION, so a Cargo.toml left behind by a version
// change puts a lie on the wire for every request the device makes.

import { appendFileSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { extractSection } from "./changelog.mjs";

const DESKTOP = ["clients", "desktop"];
const CORE = ["clients", "core"];
const FFI = ["clients", "mobile", "ffi"];
const ANDROID = ["clients", "mobile", "android", "app", "build.gradle.kts"];

/**
 * The `version` of the `[package]` table.
 *
 * Hand-parsed rather than pulling in a TOML dependency: the file is generated
 * by nobody and edited by hand, and a workflow that needs `npm install` before
 * it can decide whether to publish is a workflow that fails for a new reason.
 */
function cargoPackageVersion(toml) {
  let inPackage = false;
  for (const line of toml.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed.startsWith("[")) {
      inPackage = trimmed === "[package]";
      continue;
    }
    if (!inPackage) continue;
    const match = /^version\s*=\s*"([^"]+)"/.exec(trimmed);
    if (match) return match[1];
  }
  return null;
}

/**
 * The Android `versionName`, read out of the Gradle build file.
 *
 * One line carries it — `versionName = "0.2.0"` inside `defaultConfig` — and
 * `versionCode` is derived from that string by the build file itself, so this is
 * the whole of the Android version. Two matches means somebody added a second
 * declaration (a flavour, a variant override) and the one this gate checks would
 * be a coin toss, so two is as fatal as none.
 */
function gradleVersionName(kts) {
  const matches = [...kts.matchAll(/^\s*versionName\s*=\s*"([^"]+)"/gm)];
  if (matches.length !== 1) return null;
  return matches[0][1];
}

/**
 * Every file that carries the version, keyed by its repo-relative path.
 *
 * The keys are the labels the failure report prints, and they are full paths so
 * that several same-named files — two `Cargo.toml`s and a Gradle script — can
 * sit here with no ambiguity about which is meant.
 *
 * **Deliberately not here**, so that the omissions read as decisions:
 *
 * - `clients/desktop/ui/package.json` (0.1.0, and has been since the 0.2.0
 *   release). Nothing reads it: `vite.config.ts` injects `__APP_VERSION__`
 *   from `tauri.conf.json`, the authority below, so the rail cannot disagree
 *   with the artifact it was built into. Adding this manifest would mean every
 *   release edits a fifth file whose number means nothing, and a gate that
 *   fails for cosmetic reasons is a gate people learn to re-run rather than
 *   read. Left drifted on purpose.
 *
 *   Until 0.7.0 that first sentence was false — vite read *this* file and the
 *   rail printed v0.1.0 on every build from 0.2.0 on. The fix pointed vite at
 *   the authority rather than adding a line here, because a number on screen
 *   should come from the manifest that ships, not from a second one a gate
 *   keeps in step.
 * - `clients/desktop/acl-tests/Cargo.toml` (0.1.0). A test-only crate with its
 *   own lockfile that is never built into anything shipped.
 * - `server/package.json`. The Relay is a different deliverable on its own
 *   cadence — operators build it from `docker compose`, it is never attached to
 *   a Release — so tying its version to a client release would be wrong, not
 *   merely noisy.
 *
 * What *is* here is every version that rides inside a shipped artifact or goes
 * out on the wire.
 */
function collectVersions(root) {
  const read = (...parts) => readFileSync(join(root, ...parts), "utf8");
  return {
    "clients/desktop/src-tauri/tauri.conf.json":
      JSON.parse(read(...DESKTOP, "src-tauri", "tauri.conf.json")).version,
    "clients/desktop/package.json": JSON.parse(read(...DESKTOP, "package.json")).version,
    "clients/desktop/src-tauri/Cargo.toml":
      cargoPackageVersion(read(...DESKTOP, "src-tauri", "Cargo.toml")),
    "clients/core/Cargo.toml": cargoPackageVersion(read(...CORE, "Cargo.toml")),
    // The crate compiled into `libsharepaste_ffi.so` and packaged in the APK.
    // Its own manifest already claims this script enforces the agreement; until
    // now that comment was a lie.
    "clients/mobile/ffi/Cargo.toml": cargoPackageVersion(read(...FFI, "Cargo.toml")),
    // The Android artifact's own version, and the one a phone shows in Settings
    // and Obtainium compares against a release tag.
    "clients/mobile/android/app/build.gradle.kts": gradleVersionName(read(...ANDROID)),
  };
}

if (import.meta.filename === process.argv[1]) {
  const root = process.argv[2] ?? ".";
  const versions = collectVersions(root);

  // tauri.conf.json is the one the pipeline reads; the others have to agree
  // with it, not merely with each other.
  const AUTHORITY = "clients/desktop/src-tauri/tauri.conf.json";
  const version = versions[AUTHORITY];
  const drifted = Object.entries(versions).filter(([, v]) => v !== version);
  if (!version || drifted.length > 0) {
    console.error("Version numbers disagree; refusing to publish.");
    // Widest key, not the authority's: the Gradle path is longer than it, and a
    // ragged column is how a reader misses which line is the odd one.
    const column = Math.max(...Object.keys(versions).map((file) => file.length));
    for (const [file, v] of Object.entries(versions)) {
      console.error(`  ${file.padEnd(column)} ${v ?? "(unreadable)"}`);
    }
    process.exit(1);
  }

  const changelogPath = join(root, ...DESKTOP, "CHANGELOG.md");
  if (extractSection(readFileSync(changelogPath, "utf8"), version) === null) {
    console.error(`clients/desktop/CHANGELOG.md has no '## ${version}' section.`);
    console.error("The section is the release body and the in-app prompt; both would ship empty.");
    process.exit(1);
  }

  console.log(version);
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(process.env.GITHUB_OUTPUT, `version=${version}\n`);
  }
}
