import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { usePairingsStore } from "../store";
import type { Pairing, Settings, UpdateStatus } from "../types";
import SettingsSection from "../views/sections/SettingsSection";

let ipc: MockIpc;

const pairing: Pairing = {
  user_id: "u-active", device_id: "d1", label: "Laptop", server_url: "https://srv",
  status: "Online", pending: 0, is_active: true,
};

const defaults: Settings = {
  capture_enabled: true, deny_list: [], autostart: false, hotkey: null, update_check_enabled: true,
};

/** Mutable so a test can change what the next check finds, mid-test. */
let updateStatus: UpdateStatus;

beforeEach(() => {
  updateStatus = { current_version: "0.2.0", available: null };
  ipc = mockIpc({
    invoke: (command, args) => {
      if (command === "get_settings") return defaults;
      if (command === "update_settings") {
        // The backend patches the stored row and echoes the whole of it back.
        return { ...defaults, ...((args?.patch ?? {}) as Partial<Settings>) };
      }
      if (command === "get_update_status" || command === "check_for_update") return updateStatus;
      if (command === "list_pairings") return [pairing];
      return null;
    },
  });
  usePairingsStore.setState({ pairings: [], active: undefined });
});

const invoked = (command: string) => ipc.invoke.mock.calls.some((c) => c[0] === command);

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
      update_check_enabled: true,
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
      update_check_enabled: true,
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
      if (command === "list_pairings") return [pairing] as never;
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

describe("SettingsSection updates", () => {
  it("reports the current version without contacting the update source", async () => {
    render(<SettingsSection />);
    expect(await screen.findByTestId("current-version")).toHaveTextContent("0.2.0");
    // Opening a pane is not consent to make a request; only the launch check
    // and the button below are.
    expect(invoked("check_for_update")).toBe(false);
    expect(screen.queryByTestId("install-update")).toBeNull();
  });

  it("switches the automatic check off", async () => {
    render(<SettingsSection />);
    fireEvent.click(await screen.findByTestId("update-check-enabled"));
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("update_settings", {
        patch: { update_check_enabled: false },
      });
    });
  });

  it("a manual check that finds a release shows its notes and installs on click", async () => {
    render(<SettingsSection />);
    const check = await screen.findByTestId("check-for-update");

    updateStatus = {
      current_version: "0.2.0",
      available: { version: "0.2.1", notes: "Fixes the tray." },
    };
    fireEvent.click(check);

    expect(await screen.findByTestId("update-notes")).toHaveTextContent("Fixes the tray.");
    expect(screen.getByTestId("update-offer")).toHaveTextContent("0.2.1");

    fireEvent.click(screen.getByTestId("install-update"));
    await waitFor(() => expect(invoked("install_update")).toBe(true));
  });

  it("a manual check that finds nothing says so and offers no install", async () => {
    render(<SettingsSection />);
    fireEvent.click(await screen.findByTestId("check-for-update"));
    await waitFor(() => expect(invoked("check_for_update")).toBe(true));
    expect(await screen.findByText(/up to date/i)).toBeTruthy();
    expect(screen.queryByTestId("install-update")).toBeNull();
  });

  it("surfaces a release the launch check found, with no click at all", async () => {
    render(<SettingsSection />);
    await screen.findByTestId("check-for-update");
    await act(async () => ipc.emit("update-available", { version: "0.3.0", notes: "New popover." }));
    expect(screen.getByTestId("update-offer")).toHaveTextContent("0.3.0");
    expect(screen.getByTestId("install-update")).toBeTruthy();
  });

  it("surfaces a failed check in the error line", async () => {
    render(<SettingsSection />);
    const check = await screen.findByTestId("check-for-update");
    ipc.invoke.mockImplementation(async (command) => {
      if (command === "check_for_update") throw new Error("update source unreachable");
      return null as never;
    });
    fireEvent.click(check);
    expect(await screen.findByText(/update source unreachable/)).toBeTruthy();
  });
});
