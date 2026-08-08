/**
 * Injected by `vite.config.ts` from `src-tauri/tauri.conf.json` — the version
 * the release gate treats as authoritative; printed on the main window's rail.
 */
declare const __APP_VERSION__: string;

/**
 * Vite's `?raw` import: a file's bytes as a string, resolved when the importer
 * is transformed.
 *
 * Declared here rather than by pulling in `vite/client` wholesale, because this
 * project uses exactly one of them: `store.test.ts` reads the core's own
 * `history.rs` this way to hold `HISTORY_CAP` to the constant that owns it.
 */
declare module "*?raw" {
  const contents: string;
  export default contents;
}
