#!/usr/bin/env node
// Refuses to let a release go out on disagreeing version numbers.
//
//   node .github/scripts/check-versions.mjs [repo-root]
//
// Prints the agreed version on stdout and appends `version=<v>` to
// $GITHUB_OUTPUT when running under Actions. Exits 1 on any disagreement, or
// when the changelog has no section for the version.
//
// This is not cosmetic. `core/http/client.rs` builds the Relay User-Agent from
// CARGO_PKG_VERSION, so a Cargo.toml left behind by a version change puts a lie
// on the wire for every request the device makes.

import { appendFileSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { extractSection } from "./changelog.mjs";

const DESKTOP = ["clients", "desktop"];

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

function collectVersions(root) {
  const read = (...parts) => readFileSync(join(root, ...parts), "utf8");
  return {
    "tauri.conf.json": JSON.parse(read(...DESKTOP, "src-tauri", "tauri.conf.json")).version,
    "package.json": JSON.parse(read(...DESKTOP, "package.json")).version,
    "Cargo.toml": cargoPackageVersion(read(...DESKTOP, "src-tauri", "Cargo.toml")),
  };
}

if (import.meta.filename === process.argv[1]) {
  const root = process.argv[2] ?? ".";
  const versions = collectVersions(root);

  // tauri.conf.json is the one the pipeline reads; the other two have to agree
  // with it, not merely with each other.
  const version = versions["tauri.conf.json"];
  const drifted = Object.entries(versions).filter(([, v]) => v !== version);
  if (!version || drifted.length > 0) {
    console.error("Version numbers disagree; refusing to publish.");
    for (const [file, v] of Object.entries(versions)) {
      console.error(`  ${file.padEnd(17)} ${v ?? "(unreadable)"}`);
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
