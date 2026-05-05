import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, waitFor } from "@testing-library/react";
import { injectForTests, type Invoker, type Listener } from "../ipc/tauri";
import { useAccountsStore, useHistoryStore, useStatusStore, useUiStore } from "../store";
import type { Account } from "../types";
import Popover from "../views/Popover";

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
  useUiStore.setState({ modal: null, search: "", selectedIndex: 0 });
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
      if (cmd === "open_modal") return undefined;
      return undefined;
    }) as ReturnType<typeof vi.fn<Invoker>>;
    const listen = vi.fn(async () => () => {}) as ReturnType<typeof vi.fn<Listener>>;
    injectForTests(invoke as never, listen as never);

    const { findByTestId } = render(<Popover />);
    const button = await findByTestId("choose-account");
    fireEvent.click(button);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("open_modal", { args: { kind: "accounts" } }),
    );
  });
});
