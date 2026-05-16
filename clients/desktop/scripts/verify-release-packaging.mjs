import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const desktopRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const repoPath = (...parts) => join(desktopRoot, ...parts);

const checks = [
  {
    name: "release Windows builds use the GUI subsystem",
    pass() {
      const main = readFileSync(repoPath("src-tauri", "src", "main.rs"), "utf8");
      return main.includes(
        '#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]',
      );
    },
  },
  {
    name: "favicon is available as a Vite public asset",
    pass() {
      return existsSync(repoPath("ui", "public", "favicon.ico"));
    },
  },
  {
    name: "main window HTML declares the favicon",
    pass() {
      const html = readFileSync(repoPath("ui", "main.html"), "utf8");
      return html.includes('<link rel="icon" href="/favicon.ico" />');
    },
  },
  {
    name: "popover HTML declares the favicon",
    pass() {
      const html = readFileSync(repoPath("ui", "popover.html"), "utf8");
      return html.includes('<link rel="icon" href="/favicon.ico" />');
    },
  },
];

let failed = false;
for (const check of checks) {
  if (!check.pass()) {
    console.error(`FAIL ${check.name}`);
    failed = true;
  }
}

if (failed) {
  process.exit(1);
}

console.log("release packaging checks passed");
