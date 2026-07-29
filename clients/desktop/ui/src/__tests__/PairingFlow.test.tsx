import { describe, it, expect, beforeEach } from "vitest";
import { act, render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QRCodeSVG } from "qrcode.react";
import { mockIpc, type MockIpc } from "./helpers";
import { useUiStore } from "../store";
import PairingFlow from "../views/sections/PairingFlow";

/**
 * One short code in its two forms: compact as `shortcode::encode` emits it and
 * `shortcode::decode` accepts it, grouped in fives as `group_for_display` hands
 * it to the pane. Full-length base32, so the compact form also passes the pair
 * field's plausibility check.
 */
const COMPACT_CODE = "ABCDEFGH234567".repeat(6);
const GROUPED_CODE = COMPACT_CODE.replace(/(.{5})(?=.)/g, "$1 ");

/**
 * The module geometry qrcode.react draws, which is a function of the bytes the
 * symbol carries. Two renders agree only if they encode the same string, so
 * this is as close to "what a camera would read" as jsdom gets.
 */
function geometryOf(root: Element): string {
  const svg = root.querySelector("svg");
  if (svg === null) throw new Error("no QR svg under the given element");
  return [...svg.querySelectorAll("path")].map((p) => p.getAttribute("d")).join("|");
}

/**
 * An independent render of `value` to compare geometry against. Level and
 * margin have to match the pane's, because both move modules around.
 */
function referenceQr(value: string): Element {
  return render(<QRCodeSVG value={value} size={168} level="M" marginSize={4} />).container;
}

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

    fireEvent.change(field, { target: { value: COMPACT_CODE } });
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

  /*
   * The code is 122 characters against a 120-second slot, so the QR is how it
   * actually reaches a phone — but the typed form stays beside it, because a
   * refused camera permission is the path with no other way in.
   */
  it("renders a QR beside the typed code, encoding the compact form", async () => {
    render(<PairingFlow />);
    await waitFor(() => expect(ipc.handlers.get("pair-shortcode")).toHaveLength(1));
    expect(screen.queryByTestId("shortcode-qr")).toBeNull();

    act(() => ipc.emit("pair-shortcode", { code: GROUPED_CODE, expires_at: Date.now() + 120_000 }));

    const qr = await screen.findByTestId("shortcode-qr");
    expect(screen.getByTestId("shortcode")).toHaveTextContent(GROUPED_CODE);

    /*
     * Both forms encode cleanly and both round-trip through the decoder, so
     * only the geometry tells them apart: the negative half is what fails if
     * the pane ever hands the camera the string it shows the person.
     */
    expect(geometryOf(qr)).toBe(geometryOf(referenceQr(COMPACT_CODE)));
    expect(geometryOf(qr)).not.toBe(geometryOf(referenceQr(GROUPED_CODE)));
  });

  /*
   * A QR outliving its slot is worse than a stale typed code: nobody reads it
   * before pointing a camera at it, and the countdown it contradicts is off to
   * one side. Same `codeWindow` and same tick as the countdown, so it goes when
   * the bar empties; the typed code and its expiry message are untouched.
   */
  it("withdraws the QR once the code's window closes", async () => {
    render(<PairingFlow />);
    await waitFor(() => expect(ipc.handlers.get("pair-shortcode")).toHaveLength(1));

    act(() => ipc.emit("pair-shortcode", { code: GROUPED_CODE, expires_at: Date.now() + 200 }));
    expect(await screen.findByTestId("shortcode-qr")).toBeInTheDocument();

    await waitFor(() => expect(screen.queryByTestId("shortcode-qr")).toBeNull(), { timeout: 3000 });
    expect(screen.getByTestId("shortcode")).toHaveTextContent(GROUPED_CODE);
    expect(screen.getByTestId("countdown")).toHaveTextContent("EXPIRES IN 0:00");
  });
});
