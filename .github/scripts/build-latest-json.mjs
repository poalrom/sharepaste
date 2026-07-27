#!/usr/bin/env node
// Assembles the updater manifest that every device polls.
//
//   node .github/scripts/build-latest-json.mjs \
//     --version 0.2.0 --repo owner/name \
//     --notes-file notes.md --artifacts dist/ --out latest.json
//
// One writer, once, from the artifacts of both platforms.
//
// Deliberately not tauri-action's `includeUpdaterJson`. That merges by listing
// the release's assets, parsing the manifest it finds and re-uploading it
// (`src/upload-version-json.ts`) — an unlocked read-modify-write. Two matrix
// jobs finishing seconds apart both read the pre-write copy and the loser's
// platform silently vanishes from the manifest. Here both platforms are already
// on disk before a single byte is written.

import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/**
 * The updater's own target keys — `{os}-{arch}` as `tauri_plugin_updater::target()`
 * builds them — mapped to the bundle each one installs from.
 *
 * Only these two: macOS is Apple Silicon only and Windows is x64 NSIS. A third
 * key here without a matching build would 404 for whoever matched it.
 */
const PLATFORMS = {
  "darwin-aarch64": ".app.tar.gz",
  "windows-x86_64": ".nsis.zip",
};

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 2) {
    if (!argv[i].startsWith("--")) throw new Error(`expected a --flag, got ${argv[i]}`);
    args[argv[i].slice(2)] = argv[i + 1];
  }
  return args;
}

/** Every file under `dir`, recursively, as `{ name, path }`. */
function walk(dir) {
  return readdirSync(dir, { withFileTypes: true, recursive: true })
    .filter((e) => e.isFile())
    .map((e) => ({ name: e.name, path: join(e.parentPath ?? e.path, e.name) }));
}

/**
 * Exactly one file whose name ends with `suffix`. Both zero and two are fatal:
 * a missing bundle means a manifest pointing at nothing, and an ambiguous one
 * means the choice would be made by directory order.
 */
function soleFile(files, suffix, label) {
  const matches = files.filter((f) => f.name.endsWith(suffix));
  if (matches.length !== 1) {
    const found = matches.map((f) => f.name).join(", ") || "none";
    throw new Error(`expected exactly one ${suffix} for ${label}, found ${matches.length}: ${found}`);
  }
  return matches[0];
}

function buildManifest({ version, repo, notes, files, pubDate }) {
  const platforms = {};
  for (const [target, suffix] of Object.entries(PLATFORMS)) {
    const bundle = soleFile(files, suffix, target);
    const signature = soleFile(files, `${suffix}.sig`, target);
    platforms[target] = {
      signature: readFileSync(signature.path, "utf8").trim(),
      // Tag-pinned, never `/latest/`: a manifest that named a moving URL would
      // hand an install whatever the next release happens to be.
      url: `https://github.com/${repo}/releases/download/v${version}/${bundle.name}`,
    };
  }
  return { version, notes, pub_date: pubDate, platforms };
}

if (import.meta.filename === process.argv[1]) {
  const args = parseArgs(process.argv.slice(2));
  for (const required of ["version", "repo", "notes-file", "artifacts", "out"]) {
    if (!args[required]) {
      console.error(`missing --${required}`);
      process.exit(2);
    }
  }
  let manifest;
  try {
    manifest = buildManifest({
      version: args.version,
      repo: args.repo,
      notes: readFileSync(args["notes-file"], "utf8").trim(),
      files: walk(args.artifacts),
      pubDate: new Date().toISOString(),
    });
  } catch (e) {
    // A stack trace here reads as a broken script. This is a broken release:
    // an artifact that never arrived, or two that both did.
    console.error(`Cannot assemble ${args.out}: ${e.message}`);
    process.exit(1);
  }
  writeFileSync(args.out, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`${args.out}: ${Object.keys(manifest.platforms).join(", ")}`);
}
