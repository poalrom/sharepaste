import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// The version the pipeline treats as authoritative and `check-versions.mjs`
// makes every other manifest agree with. `package.json` is not it: that number
// is npm plumbing nobody bumps, and reading it here printed v0.1.0 on the rail
// of every build from 0.2.0 to 0.7.0.
import tauri from "../src-tauri/tauri.conf.json" with { type: "json" };

export default defineConfig({
  plugins: [react()],
  // The rail prints the build it is; the mock's hard-coded "V0.4.2" would start
  // lying the first time the version moved.
  define: { __APP_VERSION__: JSON.stringify(tauri.version) },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2022",
    rollupOptions: {
      input: {
        popover: "popover.html",
        main: "main.html",
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    globals: true,
  },
});
