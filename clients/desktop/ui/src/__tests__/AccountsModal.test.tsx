import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { injectForTests, type Invoker, type Listener } from "../ipc/tauri";
import { useAccountsStore } from "../store";
import type { Account } from "../types";
import AccountsModal from "../modals/AccountsModal";

const accounts: Account[] = [
  { user_id: "u-active", device_id: "d1", label: "Laptop", server_url: "https://srv", status: "Connecting", pending: 0, is_active: true },
  { user_id: "u-other", device_id: "d2", label: "Desktop", server_url: "https://srv", status: "Disconnected", pending: 0, is_active: false },
];

let invoke: ReturnType<typeof vi.fn<Invoker>>;
let currentAccounts: Account[];
let registeredListeners: Record<string, (payload: unknown) => void>;

beforeEach(() => {
  currentAccounts = [...accounts];
  registeredListeners = {};
  invoke = vi.fn(async (cmd, payload) => {
    if (cmd === "list_accounts") return currentAccounts;
    if (cmd === "set_active_account") {
      const target = (payload as { args: { user_id: string } }).args.user_id;
      currentAccounts = currentAccounts.map((a) => ({ ...a, is_active: a.user_id === target }));
      return undefined;
    }
    if (cmd === "forget_account") {
      const target = (payload as { args: { user_id: string } }).args.user_id;
      currentAccounts = currentAccounts.filter((a) => a.user_id !== target);
      return undefined;
    }
    if (cmd === "open_modal") return undefined;
    return undefined;
  }) as ReturnType<typeof vi.fn<Invoker>>;
  const listen = vi.fn(async (event: string, cb: (payload: unknown) => void) => {
    registeredListeners[event] = cb;
    return () => { delete registeredListeners[event]; };
  }) as ReturnType<typeof vi.fn<Listener>>;
  injectForTests(invoke as never, listen as never);
  useAccountsStore.setState({ accounts: [], active: undefined });
});

describe("AccountsModal", () => {
  it("renders Active badge for the active account and Use button for others", async () => {
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    expect(screen.getByTestId("active-badge-u-active")).toBeInTheDocument();
    expect(screen.getByTestId("use-u-other")).toBeInTheDocument();
    expect(screen.queryByTestId("active-badge-u-other")).toBeNull();
  });

  it("clicking trash opens an inline confirmation strip below the row", async () => {
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("trash-u-other"));
    expect(screen.getByTestId("confirm-strip-u-other")).toBeInTheDocument();
    expect(screen.queryByTestId("confirm-strip-u-active")).toBeNull();
  });

  it("Cancel collapses the confirmation strip without invoking forget", async () => {
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("trash-u-other"));
    fireEvent.click(screen.getByTestId("cancel-u-other"));
    expect(screen.queryByTestId("confirm-strip-u-other")).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("forget_account", expect.anything());
  });

  it("Forget invokes forget_account and clears the strip", async () => {
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("trash-u-other"));
    fireEvent.click(screen.getByTestId("confirm-forget-u-other"));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("forget_account", { args: { user_id: "u-other" } }),
    );
    await act(async () => {
      registeredListeners["account-removed"]?.({ user_id: "u-other" });
    });
    await waitFor(() => expect(screen.queryByText("Desktop")).toBeNull());
  });

  it("Use invokes set_active_account", async () => {
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("use-u-other"));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_active_account", { args: { user_id: "u-other" } }),
    );
  });

  it("renders empty state and opens pairing modal", async () => {
    currentAccounts = [];
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByTestId("empty-pair")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("empty-pair"));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("open_modal", { args: { kind: "pairing" } }),
    );
  });
});
