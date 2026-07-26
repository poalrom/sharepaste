import { beforeEach, describe, expect, it } from "vitest";
import { act, fireEvent, render, waitFor, within } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { useAccountsStore, useHistoryStore, useStatusStore, useUiStore } from "../store";
import type { Account, EntryView } from "../types";
import Popover from "../views/Popover";

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

let ipc: MockIpc;
// Mutable so a test can change what the backend serves without re-injecting.
let servedAccounts: Account[];
let servedHistory: EntryView[];

beforeEach(() => {
  servedAccounts = accounts;
  servedHistory = [];
  ipc = mockIpc({
    invoke: (command) => {
      if (command === "list_accounts") return servedAccounts;
      if (command === "list_history") return servedHistory;
      return undefined;
    },
  });
  useAccountsStore.setState({ accounts: [], active: undefined });
  useHistoryStore.setState({ entries: [] });
  useStatusStore.setState({ byUser: {} });
  useUiStore.setState({ search: "", selectedIndex: 0, mainSection: "accounts" });
});

describe("Popover", () => {
  it("loads initial history for the account selected by hydration", async () => {
    render(<Popover />);

    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("list_history", {
        args: { user_id: "u-active", limit: 100 },
      });
    });
    expect(ipc.invoke).not.toHaveBeenCalledWith("list_history", {
      args: { user_id: "u-oldest", limit: 100 },
    });
  });

  it("empty-state buttons open the matching main-window section", async () => {
    const cases: Array<{ served: Account[]; label: string; section: string }> = [
      {
        served: accounts.map((a) => ({ ...a, is_active: false, status: "Disconnected" })),
        label: "Choose account",
        section: "accounts",
      },
      { served: [], label: "Pair a device", section: "pairing" },
    ];

    for (const { served, label, section } of cases) {
      servedAccounts = served;
      useAccountsStore.setState({ accounts: [], active: undefined });
      ipc.invoke.mockClear();

      const view = render(<Popover />);
      fireEvent.click(await view.findByText(label));
      await waitFor(() => {
        expect(ipc.invoke).toHaveBeenCalledWith("open_main_window", { args: { section } });
        expect(ipc.invoke).toHaveBeenCalledWith("hide_popover", undefined);
      });

      view.unmount();
    }
  });
});

describe("Popover event subscriptions", () => {
  it("re-lists history for the active account on history-changed", async () => {
    render(<Popover />);
    await waitFor(() => expect(ipc.handlers.get("history-changed")).toHaveLength(1));

    servedHistory = [{ id: 7, user_id: "u-active", preview: "Refetched", created_at: 3, device_id: "d-active" }];
    act(() => ipc.emit("history-changed", { user_id: "u-active" }));
    await waitFor(() => {
      expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([7]);
    });
  });

  it("ignores history-changed for a non-active account", async () => {
    render(<Popover />);
    await waitFor(() => expect(ipc.handlers.get("history-changed")).toHaveLength(1));
    const before = ipc.invoke.mock.calls.filter((c) => c[0] === "list_history").length;

    act(() => ipc.emit("history-changed", { user_id: "u-oldest" }));
    expect(ipc.invoke.mock.calls.filter((c) => c[0] === "list_history")).toHaveLength(before);
  });

  it("surfaces a decryption failure in a dismissible banner", async () => {
    const { findByTestId, queryByTestId } = render(<Popover />);
    await waitFor(() => expect(ipc.handlers.get("decryption-error")).toHaveLength(1));
    expect(queryByTestId("decryption-error")).toBeNull();

    act(() => ipc.emit("decryption-error", { user_id: "u-active", entry_id: 42 }));
    const banner = await findByTestId("decryption-error");
    expect(banner).toHaveTextContent("Could not decrypt entry #42.");

    fireEvent.click(within(banner).getByLabelText("Dismiss decryption error"));
    await waitFor(() => expect(queryByTestId("decryption-error")).toBeNull());
  });

  it("unsubscribes every listener on unmount", async () => {
    const { unmount } = render(<Popover />);
    await waitFor(() => expect(ipc.handlers.get("decryption-error")).toHaveLength(1));
    unmount();
    await waitFor(() => {
      expect(ipc.handlers.get("history-changed")).toHaveLength(0);
      expect(ipc.handlers.get("decryption-error")).toHaveLength(0);
    });
  });

  // The popover window is shown/hidden, never unmounted, so `autoFocus` fires
  // once per window lifetime. Reopening used to leave focus on whatever was
  // last clicked - and because HistoryList ignores keydown while a button holds
  // focus, that made the reopened popover keyboard-dead.
  it("returns focus to the search box each time the popover is shown", async () => {
    const { findByPlaceholderText, getByRole } = render(<Popover />);
    const input = await findByPlaceholderText("Search history…");
    expect(document.activeElement).toBe(input);

    // Simulate the reported sequence: click a footer button, which opens the
    // main window and hides the popover, leaving focus on the button.
    const settings = getByRole("button", { name: "Settings" });
    settings.focus();
    expect(document.activeElement).toBe(settings);

    // Hidden, then shown again: the window regains focus.
    act(() => {
      window.dispatchEvent(new FocusEvent("focus"));
    });

    expect(document.activeElement).toBe(input);
  });

  it("selects any leftover query so typing replaces it", async () => {
    const { findByPlaceholderText } = render(<Popover />);
    const input = (await findByPlaceholderText("Search history…")) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "old query" } });
    input.blur();

    act(() => {
      window.dispatchEvent(new FocusEvent("focus"));
    });

    expect(document.activeElement).toBe(input);
    expect(input.selectionStart).toBe(0);
    expect(input.selectionEnd).toBe("old query".length);
  });
});
