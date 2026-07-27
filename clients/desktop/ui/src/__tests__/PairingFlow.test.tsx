import { describe, it, expect, beforeEach } from "vitest";
import { act, render, screen, fireEvent, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { useUiStore } from "../store";
import PairingFlow from "../views/sections/PairingFlow";

/** Passes the field's plausibility check: base32 alphabet, full-length. */
const PLAUSIBLE_CODE = "ABCDEFGH234567".repeat(6);

let ipc: MockIpc;

beforeEach(() => {
  ipc = mockIpc({
    invoke: (command) => {
      if (command === "pair_start") return { code: "ABCDE FGHIJ", expires_at: Date.now() + 120_000 };
      return { user_id: "u", device_id: "d" };
    },
  });
  // The state the pane mounts the standalone flow in; closing it writes here.
  useUiStore.setState({ pairingFlowOpen: true });
});

describe("PairingFlow", () => {
  /*
   * Show-code moved onto the pairing card (plan §7), where the card names which
   * pairing the code is for. What is left is the two ways of pairing *this*
   * device, and a third option reappearing is what this counts.
   */
  it("starts on a chooser offering exactly two ways to pair", () => {
    render(<PairingFlow />);
    expect(screen.getByText("How are you pairing?")).toBeInTheDocument();
    expect(screen.getByTestId("choose-invite")).toBeInTheDocument();
    expect(screen.getByTestId("choose-code")).toBeInTheDocument();
    expect(screen.queryByTestId("choose-show-code")).toBeNull();
    expect(screen.getAllByRole("button")).toHaveLength(2);
  });

  it("marks an implausible pair code invalid and refuses to submit it", () => {
    render(<PairingFlow />);
    fireEvent.click(screen.getByTestId("choose-code"));
    const field = screen.getByTestId("pair-code");

    fireEvent.change(field, { target: { value: "tiny" } });
    expect(field).toHaveAttribute("data-invalid", "true");
    expect(screen.getByText(/valid pair code/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "PAIR" })).toBeDisabled();

    fireEvent.change(field, { target: { value: PLAUSIBLE_CODE } });
    expect(field).toHaveAttribute("data-invalid", "false");
    expect(screen.getByRole("button", { name: "PAIR" })).toBeEnabled();
  });

  it("renders a shortcode the relay broadcasts, with the window it stays claimable in", async () => {
    render(<PairingFlow />);
    await waitFor(() => expect(ipc.handlers.get("pair-shortcode")).toHaveLength(1));

    act(() => ipc.emit("pair-shortcode", { code: "VWXYZ 23456", expires_at: Date.now() + 120_000 }));

    expect(await screen.findByTestId("shortcode")).toHaveTextContent("VWXYZ 23456");
    expect(screen.getByTestId("countdown")).toHaveTextContent(/EXPIRES IN \d:\d\d/);
  });

  it("confirms the device that claimed the code, then returns to the chooser", async () => {
    render(<PairingFlow />);
    await waitFor(() => expect(ipc.handlers.get("pair-claimed")).toHaveLength(1));

    act(() => ipc.emit("pair-claimed", { user_id: "u-active", device_label: "Pixel 9" }));
    expect(await screen.findByText('Paired a new device "Pixel 9"')).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "DONE" }));
    expect(screen.getByText("How are you pairing?")).toBeInTheDocument();
    // The standalone flow is the pane's add box; DONE is what collapses it.
    expect(useUiStore.getState().pairingFlowOpen).toBe(false);
  });

  /*
   * `pair_start` has always taken any user id; the flow used to gate show-code
   * on the Active Pairing for no reason the command shares (plan §7). Mounted
   * for a card, it is already the code — there is no chooser behind it.
   */
  it("skips the chooser and starts a code for forUserId", async () => {
    render(<PairingFlow forUserId="u-other" />);

    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("pair_start", { args: { user_id: "u-other" } }),
    );
    expect(screen.queryByTestId("choose-invite")).toBeNull();
    expect(await screen.findByTestId("shortcode")).toHaveTextContent("ABCDE FGHIJ");
    expect(screen.getByTestId("pair-panel-close-u-other")).toBeInTheDocument();
  });

  it("surfaces a pair_start rejection instead of a code that never arrives", async () => {
    ipc.invoke.mockImplementationOnce(async () => {
      throw { kind: "Network", message: "server unavailable" };
    });
    render(<PairingFlow forUserId="u-other" />);

    expect(await screen.findByTestId("pair-error")).toHaveTextContent("server unavailable");
    expect(screen.getByTestId("shortcode")).toHaveTextContent("REQUESTING…");
  });

  /*
   * ADR 0004: *account* is on Pairing's _Avoid_ line in CONTEXT.md, and the
   * invite form is the copy-heaviest surface in the flow.
   */
  it("writes the invite form in glossary words", () => {
    const view = render(<PairingFlow />);
    fireEvent.click(screen.getByTestId("choose-invite"));
    // Anchors the check on the form's own copy: the chooser has no "account"
    // in it either, so a click that never advanced would pass for free.
    expect(screen.getByText("Claim invite")).toBeInTheDocument();
    expect(view.container.textContent).not.toMatch(/account/i);
  });
});
