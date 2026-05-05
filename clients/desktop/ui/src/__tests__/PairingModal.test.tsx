import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { injectForTests, type Invoker, type Listener } from "../ipc/tauri";
import { useAccountsStore } from "../store/accounts";
import PairingModal from "../modals/PairingModal";

let invoke: ReturnType<typeof vi.fn<Invoker>>;

beforeEach(() => {
  invoke = vi.fn(async (cmd) => {
    if (cmd === "list_accounts") return [];
    if (cmd === "pair_start") return { code: "ABCDE FGHIJ", expires_at: Date.now() + 120_000 };
    return { user_id: "u", device_id: "d" };
  }) as ReturnType<typeof vi.fn<Invoker>>;
  const listen = vi.fn(async () => () => {}) as ReturnType<typeof vi.fn<Listener>>;
  injectForTests(invoke as never, listen as never);
  useAccountsStore.setState({ accounts: [], active: undefined });
});

describe("PairingModal", () => {
  it("starts on the chooser screen", () => {
    render(<PairingModal />);
    expect(screen.getByTestId("choose-invite")).toBeInTheDocument();
    expect(screen.getByTestId("choose-code")).toBeInTheDocument();
    expect(screen.getByTestId("choose-show-code")).toBeInTheDocument();
  });

  it("keeps the show-code option disabled without an active account", () => {
    render(<PairingModal />);
    const showCode = screen.getByTestId("choose-show-code");
    expect(showCode).toBeDisabled();
    expect(screen.getByText(/Pair this device first/i)).toBeInTheDocument();
  });

  it("navigates to the invite step", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-invite"));
    expect(screen.getByText(/Claim invite/i)).toBeInTheDocument();
  });

  it("warns on plain http to non-localhost", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-invite"));
    const url = screen.getByLabelText(/Server URL/i, { selector: "input" }) as HTMLInputElement;
    fireEvent.change(url, { target: { value: "http://example.com" } });
    expect(screen.getByTestId("insecure-warning")).toBeInTheDocument();
  });

  it("shows red border on invalid pair code", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-code"));
    const ta = screen.getByTestId("pair-code") as HTMLTextAreaElement;
    fireEvent.change(ta, { target: { value: "tiny" } });
    expect(ta.className).toContain("ring-red-500");
  });

  it("starts pairing for the active account and displays the returned code", async () => {
    useAccountsStore.setState({
      accounts: [
        { user_id: "u-active", device_id: "d1", label: "Laptop", server_url: "https://srv", status: "Online", pending: 0, is_active: true },
      ],
      active: "u-active",
    });

    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-show-code"));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("pair_start", { args: { user_id: "u-active" } });
    });
    expect(await screen.findByTestId("shortcode")).toHaveTextContent("ABCDE FGHIJ");
    expect(screen.getByTestId("countdown")).toBeInTheDocument();
  });

  it("hydrates accounts on mount before starting pairing from an existing account", async () => {
    invoke.mockImplementation(async (cmd) => {
      if (cmd === "list_accounts") {
        return [
          { user_id: "u-hydrated", device_id: "d1", label: "Laptop", server_url: "https://srv", status: "Online", pending: 0, is_active: true },
        ];
      }
      if (cmd === "pair_start") return { code: "VWXYZ 23456", expires_at: Date.now() + 120_000 };
      return { user_id: "u", device_id: "d" };
    });

    render(<PairingModal />);
    const showCode = screen.getByTestId("choose-show-code");

    await waitFor(() => expect(showCode).toBeEnabled());
    fireEvent.click(showCode);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("pair_start", { args: { user_id: "u-hydrated" } });
    });
    expect(await screen.findByTestId("shortcode")).toHaveTextContent("VWXYZ 23456");
  });

  it("starts pairing for the backend-active account after hydrating multiple accounts", async () => {
    invoke.mockImplementation(async (cmd) => {
      if (cmd === "list_accounts") {
        return [
          { user_id: "u-oldest", device_id: "d1", label: "Oldest", server_url: "https://srv", status: "Disconnected", pending: 0, is_active: false },
          { user_id: "u-active", device_id: "d2", label: "Active", server_url: "https://srv", status: "Connecting", pending: 0, is_active: true },
        ];
      }
      if (cmd === "pair_start") return { code: "LMNOP 78901", expires_at: Date.now() + 120_000 };
      return { user_id: "u", device_id: "d" };
    });

    render(<PairingModal />);
    const showCode = screen.getByTestId("choose-show-code");

    await waitFor(() => expect(showCode).toBeEnabled());
    fireEvent.click(showCode);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("pair_start", { args: { user_id: "u-active" } });
    });
  });

  it("shows a chooser error when starting pairing fails", async () => {
    invoke.mockImplementationOnce(async () => {
      throw { kind: "Network", message: "server unavailable" };
    });
    useAccountsStore.setState({
      accounts: [
        { user_id: "u-active", device_id: "d1", label: "Laptop", server_url: "https://srv", status: "Online", pending: 0, is_active: true },
      ],
      active: "u-active",
    });

    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-show-code"));

    expect(await screen.findByText("server unavailable")).toBeInTheDocument();
    expect(screen.getByTestId("choose-show-code")).toBeInTheDocument();
  });
});
