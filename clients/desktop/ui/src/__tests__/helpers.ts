import { afterEach, vi, type Mock } from "vitest";
import { injectForTests, type Invoker, type Listener, type WindowControls } from "../ipc/tauri";

/**
 * Test-side implementation of a Tauri command. The return value is resolved as
 * the command result, so handlers may be plain (non-async) functions.
 */
export type InvokeHandler = (cmd: string, args?: Record<string, unknown>) => unknown;

export type MockInvoke = Mock<Invoker>;
export type MockListen = Mock<Listener>;

export type MockIpc = {
  invoke: MockInvoke;
  listen: MockListen;
  /** Listeners currently registered by the component under test, keyed by event. */
  handlers: Map<string, Array<(payload: never) => void>>;
  /** Delivers `payload` to every listener registered for `event`. */
  emit: (event: string, payload?: unknown) => void;
  /**
   * The main window's own titlebar controls. Spied rather than stubbed away
   * because with `decorations(false)` these three buttons are the only way to
   * minimise, maximise or close, so a test has to be able to assert they fire.
   */
  window: { [K in keyof WindowControls]: Mock<WindowControls[K]> };
};

const inertInvoke: Invoker = async () => undefined as never;
const inertListen: Listener = async () => () => {};
const inertWindow = (): WindowControls => ({
  minimize: async () => {},
  toggleMaximize: async () => {},
  close: async () => {},
});

/**
 * Installs mock IPC transports for the duration of one test and returns the
 * spies. The default listener is a bus: whatever the component subscribes to
 * lands in `handlers` and can be fired with `emit`.
 */
export function mockIpc(opts: { invoke?: InvokeHandler; listen?: Listener } = {}): MockIpc {
  const handlers = new Map<string, Array<(payload: never) => void>>();
  const handle = opts.invoke ?? (() => undefined);

  const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>) =>
    handle(cmd, args),
  ) as unknown as MockInvoke;

  const bus: Listener = async (event, cb) => {
    const list = handlers.get(event) ?? [];
    list.push(cb as (payload: never) => void);
    handlers.set(event, list);
    return () => {
      handlers.set(event, (handlers.get(event) ?? []).filter((h) => h !== cb));
    };
  };
  const listen = vi.fn(opts.listen ?? bus) as unknown as MockListen;
  const win = {
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
  };

  // vi.fn() erases the callee's type parameter, so the generic Invoker/Listener
  // shapes have to be reasserted at the injection boundary.
  injectForTests(invoke as unknown as Invoker, listen as unknown as Listener, win);

  return {
    invoke,
    listen,
    handlers,
    window: win,
    emit: (event, payload) => {
      for (const h of [...(handlers.get(event) ?? [])]) (h as (p: unknown) => void)(payload);
    },
  };
}

// Registered once per test file at import time: no test may leak its mocks into
// the next one, and a late-unmounting component talks to an inert transport.
afterEach(() => {
  injectForTests(inertInvoke, inertListen, inertWindow());
});
