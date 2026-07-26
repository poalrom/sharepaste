import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { useAccountsStore, useUiStore } from "../store";
import type { Account } from "../types";
import AccountsSection from "../views/sections/AccountsSection";

const accounts: Account[] = [
  { user_id: "u-active", device_id: "d1", label: "Laptop", server_url: "https://srv", status: "Connecting", pending: 0, is_active: true },
  { user_id: "u-other", device_id: "d2", label: "Desktop", server_url: "https://srv", status: "Disconnected", pending: 0, is_active: false },
];

/** Reads the `user_id` the component sent, failing loudly if the shape drifts. */
function targetUserId(payload?: Record<string, unknown>): string {
  const args = payload?.args;
  if (args && typeof args === "object" && "user_id" in args && typeof args.user_id === "string") {
    return args.user_id;
  }
  throw new Error(`expected { args: { user_id } }, got ${JSON.stringify(payload)}`);
}

let ipc: MockIpc;
let currentAccounts: Account[];

beforeEach(() => {
  currentAccounts = [...accounts];
  ipc = mockIpc({
    invoke: (command, payload) => {
      if (command === "list_accounts") return currentAccounts;
      if (command === "set_active_account") {
        const target = targetUserId(payload);
        currentAccounts = currentAccounts.map((a) => ({ ...a, is_active: a.user_id === target }));
        return undefined;
      }
      if (command === "forget_account") {
        const target = targetUserId(payload);
        currentAccounts = currentAccounts.filter((a) => a.user_id !== target);
        return undefined;
      }
      return undefined;
    },
  });
  useAccountsStore.setState({ accounts: [], active: undefined });
  useUiStore.setState({ mainSection: "accounts" });
});

describe("AccountsSection", () => {
  it("renders Active badge for the active account and Use button for others", async () => {
    render(<AccountsSection />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    expect(screen.getByTestId("active-badge-u-active")).toBeInTheDocument();
    expect(screen.getByTestId("use-u-other")).toBeInTheDocument();
    expect(screen.queryByTestId("active-badge-u-other")).toBeNull();
  });

  it("clicking trash opens an inline confirmation strip below the row", async () => {
    render(<AccountsSection />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("trash-u-other"));
    expect(screen.getByTestId("confirm-strip-u-other")).toBeInTheDocument();
    expect(screen.queryByTestId("confirm-strip-u-active")).toBeNull();
  });

  it("Cancel collapses the confirmation strip without invoking forget", async () => {
    render(<AccountsSection />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("trash-u-other"));
    fireEvent.click(screen.getByTestId("cancel-u-other"));
    expect(screen.queryByTestId("confirm-strip-u-other")).toBeNull();
    expect(ipc.invoke).not.toHaveBeenCalledWith("forget_account", expect.anything());
  });

  it("Forget invokes forget_account, clears the strip, and removes the account row", async () => {
    render(<AccountsSection />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("trash-u-other"));
    fireEvent.click(screen.getByTestId("confirm-forget-u-other"));
    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("forget_account", { args: { user_id: "u-other" } }),
    );
    await waitFor(() => expect(screen.queryByText("Desktop")).toBeNull());
  });

  it("Use invokes set_active_account", async () => {
    render(<AccountsSection />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("use-u-other"));
    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("set_active_account", { args: { user_id: "u-other" } }),
    );
  });
});
