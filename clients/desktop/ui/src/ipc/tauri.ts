import { invoke as realInvoke } from "@tauri-apps/api/core";
import { listen as realListen, type UnlistenFn } from "@tauri-apps/api/event";

export type Invoker = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
export type Listener = <P>(event: string, cb: (payload: P) => void) => Promise<UnlistenFn>;

let _invoke: Invoker = <T>(cmd: string, args?: Record<string, unknown>) =>
  realInvoke<T>(cmd, args);
let _listen: Listener = <P>(event: string, cb: (payload: P) => void) =>
  realListen<P>(event, ({ payload }) => cb(payload));

export const tauri = {
  invoke: <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => _invoke<T>(cmd, args),
  listen: <P>(event: string, cb: (payload: P) => void): Promise<UnlistenFn> => _listen<P>(event, cb),
};

export function injectForTests(invoke: Invoker, listen: Listener) {
  _invoke = invoke;
  _listen = listen;
}
