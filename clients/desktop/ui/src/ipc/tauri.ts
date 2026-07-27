import { invoke as realInvoke } from "@tauri-apps/api/core";
import { listen as realListen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type Invoker = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
export type Listener = <P>(event: string, cb: (payload: P) => void) => Promise<UnlistenFn>;

/**
 * The three things a window with `decorations(false)` has to do for itself.
 *
 * Behind the same seam as `invoke`/`listen` rather than called directly from
 * the titlebar, because `getCurrentWindow()` reads `__TAURI_INTERNALS__` — which
 * only the webview injects — so an unmediated call turns every render of the
 * shell into a crash outside Tauri.
 */
export type WindowControls = {
  minimize: () => Promise<void>;
  toggleMaximize: () => Promise<void>;
  close: () => Promise<void>;
};

let _invoke: Invoker = <T>(cmd: string, args?: Record<string, unknown>) =>
  realInvoke<T>(cmd, args);
let _listen: Listener = <P>(event: string, cb: (payload: P) => void) =>
  realListen<P>(event, ({ payload }) => cb(payload));
// Resolved per call, not once at module scope, for the same reason.
let _window: WindowControls = {
  minimize: () => getCurrentWindow().minimize(),
  toggleMaximize: () => getCurrentWindow().toggleMaximize(),
  close: () => getCurrentWindow().close(),
};

export const tauri = {
  invoke: <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => _invoke<T>(cmd, args),
  listen: <P>(event: string, cb: (payload: P) => void): Promise<UnlistenFn> => _listen<P>(event, cb),
  window: {
    minimize: () => _window.minimize(),
    toggleMaximize: () => _window.toggleMaximize(),
    close: () => _window.close(),
  },
};

export function injectForTests(invoke: Invoker, listen: Listener, win?: WindowControls) {
  _invoke = invoke;
  _listen = listen;
  if (win) _window = win;
}
