import { describe, it, expect, beforeEach } from "vitest";
import { act, render, screen, fireEvent, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { useAccountsStore } from "../store/accounts";
import type { Account } from "../types";
import PairingSection from "../views/sections/PairingSection";

const activeAccount: Account = {
  user_id: "u-active", device_id: "d1", label: "Laptop", server_url: "https://srv",
  status: "Online", pending: 0, is_active: true,
};

let ipc: MockIpc;

beforeEach(() => {
  ipc = mockIpc({
    invoke: (command) => {
      if (command === "list_accounts") return [];
      if (command === "pair_start") return { code: "ABCDE FGHIJ", expires_at: Date.now() + 120_000 };
      return { user_id: "u", device_id: "d" };
    },
  });
  useAccountsStore.setState({ accounts: [], active: undefined });
});

/** Puts the store in the "already paired" state the show-code flow requires. */
function withActiveAccount() {
  useAccountsStore.setState({ accounts: [activeAccount], active: "u-active" });
}

describe("PairingSection", () => {
  it("starts on the chooser screen", () => {
    render(<PairingSection />);
    expect(screen.getByTestId("choose-invite")).toBeInTheDocument();
    expect(screen.getByTestId("choose-code")).toBeInTheDocument();
    expect(screen.getByTestId("choose-show-code")).toBeInTheDocument();
  });

  it("keeps the show-code option disabled without an active account", () => {
    render(<PairingSection />);
    const showCode = screen.getByTestId("choose-show-code");
    expect(showCode).toBeDisabled();
    expect(screen.getByText(/Pair this device first/i)).toBeInTheDocument();
  });

  it("shows red border on invalid pair code", () => {
    render(<PairingSection />);
    fireEvent.click(screen.getByTestId("choose-code"));
    const ta = screen.getByTestId("pair-code") as HTMLTextAreaElement;
    fireEvent.change(ta, { target: { value: "tiny" } });
    expect(ta.className).toContain("ring-red-500");
  });

  it("starts pairing for the active account and shows the code the backend broadcasts", async () => {
    withActiveAccount();
    render(<PairingSection />);
    await waitFor(() => expect(ipc.handlers.get("pair-shortcode")).toHaveLength(1));

    fireEvent.click(screen.getByTestId("choose-show-code"));
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("pair_start", { args: { user_id: "u-active" } });
    });

    act(() => ipc.emit("pair-shortcode", { code: "VWXYZ 23456", expires_at: Date.now() + 120_000 }));
    expect(await screen.findByTestId("shortcode")).toHaveTextContent("VWXYZ 23456");
    expect(screen.getByTestId("countdown")).toBeInTheDocument();
  });

  it("shows paired-device confirmation when a pair is consumed", async () => {
    withActiveAccount();
    render(<PairingSection />);
    await waitFor(() => expect(ipc.handlers.get("pair-claimed")).toHaveLength(1));

    act(() => ipc.emit("pair-claimed", { user_id: "u-active", device_label: "Pixel 9" }));

    expect(await screen.findByText('Paired a new device "Pixel 9"')).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Ok" }));
    expect(screen.getByText("How are you pairing?")).toBeInTheDocument();
  });

  it("shows a chooser error when starting pairing fails", async () => {
    ipc.invoke.mockImplementationOnce(async () => {
      throw { kind: "Network", message: "server unavailable" };
    });
    withActiveAccount();

    render(<PairingSection />);
    fireEvent.click(screen.getByTestId("choose-show-code"));

    expect(await screen.findByText("server unavailable")).toBeInTheDocument();
    expect(screen.getByTestId("choose-show-code")).toBeInTheDocument();
  });
});
