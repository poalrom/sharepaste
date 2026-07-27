#!/usr/bin/env node
// Pulls one version's section out of clients/desktop/CHANGELOG.md.
//
// The text is shown twice — on the release page and in the in-app update
// prompt — so it is read from one place and never re-derived from commits.
//
//   node .github/scripts/changelog.mjs <changelog-path> <version>
//
// Prints the section body on stdout. Exits 1 if the version has no section.

import { readFileSync } from "node:fs";

/** `## 1.2.3` or `## 1.2.3 - anything`, but not `### 1.2.3`. */
const HEADING = /^##[^#]\s*(\S+)/;

/**
 * The body under `## <version>`, up to the next `##` heading, trimmed.
 * `null` when no such heading exists.
 */
export function extractSection(markdown, version) {
  const lines = markdown.split(/\r?\n/);
  let start = -1;
  for (let i = 0; i < lines.length; i++) {
    const match = HEADING.exec(lines[i]);
    if (!match) continue;
    if (start >= 0) return lines.slice(start, i).join("\n").trim();
    if (match[1] === version) start = i + 1;
  }
  return start >= 0 ? lines.slice(start).join("\n").trim() : null;
}

if (import.meta.filename === process.argv[1]) {
  const [path, version] = process.argv.slice(2);
  if (!path || !version) {
    console.error("usage: changelog.mjs <changelog-path> <version>");
    process.exit(2);
  }
  const section = extractSection(readFileSync(path, "utf8"), version);
  if (section === null) {
    console.error(`${path} has no '## ${version}' section.`);
    console.error("A release publishes its notes; write the section before bumping the version.");
    process.exit(1);
  }
  process.stdout.write(`${section}\n`);
}
