import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { useAccountsStore } from "../store";
import type { Account, Settings } from "../types";
import SettingsSection from "../views/sections/SettingsSection";

let ipc: MockIpc;

const account: Account = {
  user_id: "u-active", device_id: "d1", label: "Laptop", server_url: "https://srv",
  status: "Online", pending: 0, is_active: true,
};

const defaults: Settings = { capture_enabled: true, deny_list: [], autostart: false, hotkey: null };

beforeEach(() => {
  ipc = mockIpc({
    invoke: (command, args) => {
      if (command === "get_settings") return defaults;
      if (command === "update_settings") {
        // The component only ever sends a Settings patch; echo it back like the backend does.
        const patch = (args?.patch ?? {}) as Partial<Settings>;
        return {
          capture_enabled: !!patch.capture_enabled,
          deny_list: patch.deny_list ?? [],
          autostart: !!patch.autostart,
          hotkey: patch.hotkey ?? null,
        };
      }
      if (command === "list_accounts") return [account];
      return null;
    },
  });
  useAccountsStore.setState({ accounts: [], active: undefined });
});

describe("SettingsSection", () => {
  it("toggles capture_enabled and calls update_settings", async () => {
    render(<SettingsSection />);
    const cb = await screen.findByTestId("capture-enabled");
    fireEvent.click(cb);
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("update_settings", expect.objectContaining({
        patch: expect.objectContaining({ capture_enabled: false }),
      }));
    });
  });

  it("renders deny-list as newline-separated", async () => {
    ipc.invoke.mockImplementationOnce(async () => ({
      capture_enabled: true, deny_list: ["a", "b"], autostart: false, hotkey: null,
    }));
    render(<SettingsSection />);
    const ta = await screen.findByTestId("deny-list") as HTMLTextAreaElement;
    expect(ta.value).toBe("a\nb");
  });

  it("persists the hotkey once, on blur, not on every keystroke", async () => {
    render(<SettingsSection />);
    const input = await screen.findByTestId("hotkey") as HTMLInputElement;
    const typed = "Ctrl+Shift+V";
    for (let i = 1; i <= typed.length; i++) {
      fireEvent.change(input, { target: { value: typed.slice(0, i) } });
    }
    expect(ipc.invoke.mock.calls.filter((c) => c[0] === "update_settings")).toHaveLength(0);

    fireEvent.blur(input);
    await waitFor(() => {
      expect(ipc.invoke.mock.calls.filter((c) => c[0] === "update_settings")).toHaveLength(1);
    });
    expect(ipc.invoke).toHaveBeenCalledWith("update_settings", { patch: { hotkey: "Ctrl+Shift+V" } });
  });

  it("persists the hotkey on Enter", async () => {
    render(<SettingsSection />);
    const input = await screen.findByTestId("hotkey") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "Cmd+Shift+V" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("update_settings", { patch: { hotkey: "Cmd+Shift+V" } });
    });
    fireEvent.blur(input);
    expect(ipc.invoke.mock.calls.filter((c) => c[0] === "update_settings")).toHaveLength(1);
  });

  it("clearing the hotkey unbinds it with null", async () => {
    ipc.invoke.mockImplementationOnce(async () => ({
      capture_enabled: true, deny_list: [], autostart: false, hotkey: "Cmd+Shift+V",
    }));
    render(<SettingsSection />);
    const input = await screen.findByTestId("hotkey") as HTMLInputElement;
    expect(input.value).toBe("Cmd+Shift+V");
    fireEvent.change(input, { target: { value: "" } });
    fireEvent.blur(input);
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("update_settings", { patch: { hotkey: null } });
    });
  });

  it("clears history only after the confirm strip is accepted", async () => {
    render(<SettingsSection />);
    const button = await screen.findByTestId("clear-history");
    await waitFor(() => expect(button).not.toBeDisabled());
    expect(screen.queryByTestId("confirm-strip-clear-history")).toBeNull();

    fireEvent.click(button);
    expect(await screen.findByTestId("confirm-strip-clear-history")).toBeTruthy();
    expect(ipc.invoke).not.toHaveBeenCalledWith("clear_history", expect.anything());

    fireEvent.click(screen.getByTestId("confirm-clear-history"));
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("clear_history", { args: { user_id: "u-active" } });
    });
    await waitFor(() => expect(screen.queryByTestId("confirm-strip-clear-history")).toBeNull());
  });

  it("cancelling the clear-history confirm strip invokes nothing", async () => {
    render(<SettingsSection />);
    const button = await screen.findByTestId("clear-history");
    await waitFor(() => expect(button).not.toBeDisabled());
    fireEvent.click(button);
    fireEvent.click(await screen.findByTestId("cancel-clear-history"));
    await waitFor(() => expect(screen.queryByTestId("confirm-strip-clear-history")).toBeNull());
    expect(ipc.invoke).not.toHaveBeenCalledWith("clear_history", expect.anything());
  });

  it("surfaces a clear-history failure in the error line", async () => {
    ipc.invoke.mockImplementation(async (command) => {
      if (command === "get_settings") return defaults as never;
      if (command === "list_accounts") return [account] as never;
      if (command === "clear_history") throw new Error("server unreachable");
      return null as never;
    });
    render(<SettingsSection />);
    const button = await screen.findByTestId("clear-history");
    await waitFor(() => expect(button).not.toBeDisabled());
    fireEvent.click(button);
    fireEvent.click(await screen.findByTestId("confirm-clear-history"));
    expect(await screen.findByText(/server unreachable/)).toBeTruthy();
  });
});
