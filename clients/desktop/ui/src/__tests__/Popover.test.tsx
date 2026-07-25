import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, waitFor, within } from "@testing-library/react";
import { injectForTests, type Invoker, type Listener } from "../ipc/tauri";
import { useAccountsStore, useHistoryStore, useStatusStore, useUiStore } from "../store";
import type { Account } from "../types";
import Popover from "../views/Popover";

type Emit = (event: string, payload: unknown) => void;

// jsdom does not implement scrollIntoView; HistoryList calls it once rows render.
Element.prototype.scrollIntoView = vi.fn() as unknown as Element["scrollIntoView"];

/** Collects the listeners Popover registers so a test can fire a Tauri event. */
function makeBus() {
  const handlers = new Map<string, Array<(p: unknown) => void>>();
  const listen = vi.fn(async (event: string, cb: (p: unknown) => void) => {
    const list = handlers.get(event) ?? [];
    list.push(cb);
    handlers.set(event, list);
    return () => {
      handlers.set(event, (handlers.get(event) ?? []).filter((h) => h !== cb));
    };
  });
  const emit: Emit = (event, payload) => {
    for (const h of [...(handlers.get(event) ?? [])]) h(payload);
  };
  return { listen, emit, handlers };
}

let invoke: ReturnType<typeof vi.fn<Invoker>>;

const accounts: Account[] = [
  {
    user_id: "u-oldest",
    device_id: "d-oldest",
    label: "Oldest",
    server_url: "https://srv",
    status: "Disconnected",
    pending: 0,
    is_active: false,
  },
  {
    user_id: "u-active",
    device_id: "d-active",
    label: "Active",
    server_url: "https://srv",
    status: "Connecting",
    pending: 0,
    is_active: true,
  },
];

beforeEach(() => {
  invoke = vi.fn(async (cmd) => {
    if (cmd === "list_accounts") return accounts;
    if (cmd === "list_history") return [];
    return undefined;
  }) as ReturnType<typeof vi.fn<Invoker>>;
  const listen = vi.fn(async () => () => {}) as ReturnType<typeof vi.fn<Listener>>;
  injectForTests(invoke as never, listen as never);
  useAccountsStore.setState({ accounts: [], active: undefined });
  useHistoryStore.setState({ entries: [] });
  useStatusStore.setState({ byUser: {} });
  useUiStore.setState({ search: "", selectedIndex: 0, mainSection: "accounts" });
});

describe("Popover", () => {
  it("loads initial history for the account selected by hydration", async () => {
    render(<Popover />);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("list_history", {
        args: { user_id: "u-active", limit: 100 },
      });
    });
    expect(invoke).not.toHaveBeenCalledWith("list_history", {
      args: { user_id: "u-oldest", limit: 100 },
    });
  });

  it("renders the choose-account placeholder when accounts exist but none is active", async () => {
    const inactiveAccounts: Account[] = accounts.map((a) => ({ ...a, is_active: false, status: "Disconnected" }));
    invoke = vi.fn(async (cmd) => {
      if (cmd === "list_accounts") return inactiveAccounts;
      if (cmd === "list_history") return [];
      return undefined;
    }) as ReturnType<typeof vi.fn<Invoker>>;
    const listen = vi.fn(async () => () => {}) as ReturnType<typeof vi.fn<Listener>>;
    injectForTests(invoke as never, listen as never);

    const { findByTestId } = render(<Popover />);
    const button = await findByTestId("choose-account");
    fireEvent.click(button);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_main_window", {
        args: { section: "accounts" },
      });
      expect(invoke).toHaveBeenCalledWith("hide_popover", undefined);
    });
  });

  it("empty-state Pair button opens main window on pairing section", async () => {
    invoke = vi.fn(async (cmd) => {
      if (cmd === "list_accounts") return [];
      return undefined;
    }) as ReturnType<typeof vi.fn<Invoker>>;
    const listen = vi.fn(async () => () => {}) as ReturnType<typeof vi.fn<Listener>>;
    injectForTests(invoke as never, listen as never);

    const { findByText } = render(<Popover />);
    const button = await findByText("Pair a device");
    fireEvent.click(button);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_main_window", {
        args: { section: "pairing" },
      });
      expect(invoke).toHaveBeenCalledWith("hide_popover", undefined);
    });
  });
});

describe("Popover event subscriptions", () => {
  it("re-lists history for the active account on history-changed", async () => {
    const rows = [{ id: 7, user_id: "u-active", preview: "Refetched", created_at: 3, device_id: "d-active" }];
    let served: unknown[] = [];
    invoke = vi.fn(async (cmd) => {
      if (cmd === "list_accounts") return accounts;
      if (cmd === "list_history") return served;
      return undefined;
    }) as ReturnType<typeof vi.fn<Invoker>>;
    const bus = makeBus();
    injectForTests(invoke as never, bus.listen as never);

    render(<Popover />);
    await waitFor(() => expect(bus.handlers.get("history-changed")?.length).toBe(1));

    served = rows;
    act(() => bus.emit("history-changed", { user_id: "u-active" }));
    await waitFor(() => {
      expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([7]);
    });
  });

  it("ignores history-changed for a non-active account", async () => {
    const bus = makeBus();
    injectForTests(invoke as never, bus.listen as never);

    render(<Popover />);
    await waitFor(() => expect(bus.handlers.get("history-changed")?.length).toBe(1));
    const before = invoke.mock.calls.filter((c) => c[0] === "list_history").length;

    act(() => bus.emit("history-changed", { user_id: "u-oldest" }));
    expect(invoke.mock.calls.filter((c) => c[0] === "list_history")).toHaveLength(before);
  });

  it("surfaces a decryption failure in a dismissible banner", async () => {
    const bus = makeBus();
    injectForTests(invoke as never, bus.listen as never);

    const { findByTestId, queryByTestId } = render(<Popover />);
    await waitFor(() => expect(bus.handlers.get("decryption-error")?.length).toBe(1));
    expect(queryByTestId("decryption-error")).toBeNull();

    act(() => bus.emit("decryption-error", { user_id: "u-active", entry_id: 42 }));
    const banner = await findByTestId("decryption-error");
    expect(banner).toHaveTextContent("Could not decrypt entry #42.");

    fireEvent.click(within(banner).getByLabelText("Dismiss decryption error"));
    await waitFor(() => expect(queryByTestId("decryption-error")).toBeNull());
  });

  it("unsubscribes every listener on unmount", async () => {
    const bus = makeBus();
    injectForTests(invoke as never, bus.listen as never);

    const { unmount } = render(<Popover />);
    await waitFor(() => expect(bus.handlers.get("decryption-error")?.length).toBe(1));
    unmount();
    await waitFor(() => {
      expect(bus.handlers.get("history-changed")).toHaveLength(0);
      expect(bus.handlers.get("decryption-error")).toHaveLength(0);
    });
  });
});
