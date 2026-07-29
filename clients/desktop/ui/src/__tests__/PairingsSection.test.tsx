import { beforeEach, describe, expect, it } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { usePairingsStore, useUiStore } from "../store";
import type { Pairing } from "../types";
import PairingsSection from "../views/sections/PairingsSection";

const PAIRINGS: Pairing[] = [
  { user_id: "u-active", device_id: "d1", label: "Laptop", username: "alice", server_url: "https://relay.one", relay_host: "relay.one", status: "Online", pending: 0, is_active: true },
  { user_id: "u-other", device_id: "d2", label: "Desktop", username: "bob", server_url: "https://relay.two", relay_host: "relay.two", status: "Disconnected", pending: 0, is_active: false },
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
/** What `list_pairings` answers; a test may replace it before rendering. */
let rows: Pairing[];

beforeEach(() => {
  rows = [...PAIRINGS];
  ipc = mockIpc({
    invoke: (command, payload) => {
      if (command === "list_pairings") return rows;
      if (command === "pair_start") return { code: "MNOPQ 78901", expires_at: Date.now() + 120_000 };
      if (command === "set_active_pairing") {
        const target = targetUserId(payload);
        rows = rows.map((p) => ({ ...p, is_active: p.user_id === target }));
        return undefined;
      }
      if (command === "forget_pairing") {
        const target = targetUserId(payload);
        rows = rows.filter((p) => p.user_id !== target);
        return undefined;
      }
      return undefined;
    },
  });
  usePairingsStore.setState({ pairings: [], active: undefined });
  useUiStore.setState({ mainSection: "pairings", pairingFlowOpen: false });
});

/**
 * Renders and waits out the `list_pairings` the pane fires on mount.
 *
 * Waits on the add-a-pairing row rather than on a card, because it appears with
 * the first hydrated row whatever the fixture happens to be named.
 */
async function renderPairings() {
  const view = render(<PairingsSection />);
  await screen.findByTestId("add-pairing-row");
  return view;
}

describe("PairingsSection", () => {
  it("badges the Active Pairing and offers USE on the others", async () => {
    await renderPairings();
    expect(screen.getByTestId("pair-active-u-active")).toHaveTextContent("ACTIVE");
    expect(screen.queryByTestId("pair-use-u-active")).toBeNull();
    expect(screen.getByTestId("pair-use-u-other")).toBeInTheDocument();
    expect(screen.queryByTestId("pair-active-u-other")).toBeNull();
  });

  /*
   * A Pairing's `label` is the Device Label this machine chose when it paired,
   * so heading the card with it made every Pairing look like an account named
   * after the local machine — a real bug, recorded in ADR 0004. The card is
   * about a User; the Device Label is stated separately and says whose it is.
   */
  it("heads each card with the User, never with this machine's Device Label", async () => {
    await renderPairings();
    expect(screen.getByText("alice")).toBeInTheDocument();
    expect(screen.getByText("bob")).toBeInTheDocument();
    // An element reading exactly "Laptop" is a heading, and that is the bug.
    expect(screen.queryByText("Laptop")).toBeNull();
    expect(screen.getByText(/^THIS DEVICE: Laptop$/)).toBeInTheDocument();
  });

  it("falls back to the opaque user id when no username has been mirrored yet", async () => {
    rows = [{ ...PAIRINGS[0]!, username: null }];
    await renderPairings();
    expect(screen.getByText("u-active")).toBeInTheDocument();
  });

  /*
   * Plan §1.4. `list_pairings` reports `Disconnected` for every pairing holding
   * no session, which for one nobody asked to connect is its resting state and
   * not a fault — OFFLINE on every non-active row at every open is crying wolf.
   * Alert stays reserved for the state that needs a human.
   */
  it("rests a non-active pairing at STANDBY and keeps AUTH FAILED for a real fault", async () => {
    await renderPairings();
    expect(screen.getByTestId("pair-status-u-active")).toHaveTextContent("ONLINE");

    const other = screen.getByTestId("pair-status-u-other");
    expect(other).toHaveTextContent("STANDBY");
    expect(other).not.toHaveTextContent("OFFLINE");

    await waitFor(() => expect(ipc.handlers.get("connection-state")).toHaveLength(1));
    act(() => ipc.emit("connection-state", { user_id: "u-other", state: "AuthFailed" }));
    expect(screen.getByTestId("pair-status-u-other")).toHaveTextContent("AUTH FAILED");
  });

  /*
   * Plan §1.8: the mock promised queue depth per row and its markup never drew
   * it. This card is the only surface that can report captures stranded on a
   * pairing this device has since switched away from.
   */
  it("renders the pending queue depth, and renders nothing at zero", async () => {
    rows = [PAIRINGS[0]!, { ...PAIRINGS[1]!, pending: 3 }];
    await renderPairings();
    expect(screen.getByTestId("pair-pending-u-other")).toHaveTextContent("3 PENDING");
    expect(screen.queryByTestId("pair-pending-u-active")).toBeNull();
  });

  /*
   * The restriction the redesign lifted (plan §7): show-code used to be reachable
   * only for the Active Pairing, though `pair_start` has always taken any user
   * id. The card is what names the pairing the code belongs to.
   */
  it("+ DEVICE starts a code for that pairing, not for the Active one", async () => {
    await renderPairings();
    fireEvent.click(screen.getByTestId("pair-add-device-u-other"));

    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("pair_start", { args: { user_id: "u-other" } }),
    );
    expect(ipc.invoke).not.toHaveBeenCalledWith("pair_start", { args: { user_id: "u-active" } });
    expect(await screen.findByTestId("shortcode")).toHaveTextContent("MNOPQ 78901");
    expect(screen.getByTestId("pair-panel-close-u-other")).toBeInTheDocument();
  });

  it("opens the confirmation strip on the card whose ⌫ was pressed, and no other", async () => {
    await renderPairings();
    fireEvent.click(screen.getByTestId("pair-forget-u-other"));
    expect(screen.getByTestId("confirm-strip-u-other")).toBeInTheDocument();
    expect(screen.queryByTestId("confirm-strip-u-active")).toBeNull();
  });

  /*
   * Two pairings can share a heading — `alice` on the production relay and
   * `alice` on a lab instance (ADR 0004) — and this is the one action that
   * cannot be undone, so the strip names what the heading cannot.
   */
  it("names user_id @ host in the forget confirmation, not the shared heading", async () => {
    rows = [
      { ...PAIRINGS[0]!, user_id: "u-prod", username: "alice", server_url: "https://relay.one", relay_host: "relay.one" },
      { ...PAIRINGS[1]!, user_id: "u-lab", username: "alice", server_url: "https://relay.lab", relay_host: "relay.lab" },
    ];
    await renderPairings();
    fireEvent.click(screen.getByTestId("pair-forget-u-lab"));

    const strip = screen.getByTestId("confirm-strip-u-lab");
    expect(strip).toHaveTextContent("u-lab @ relay.lab");
    expect(strip).not.toHaveTextContent("alice");
  });

  it("CANCEL collapses the confirmation without invoking anything", async () => {
    await renderPairings();
    fireEvent.click(screen.getByTestId("pair-forget-u-other"));
    fireEvent.click(screen.getByTestId("cancel-forget-u-other"));
    expect(screen.queryByTestId("confirm-strip-u-other")).toBeNull();
    expect(ipc.invoke).not.toHaveBeenCalledWith("forget_pairing", expect.anything());
  });

  it("FORGET invokes forget_pairing, clears the strip, and drops the card", async () => {
    await renderPairings();
    fireEvent.click(screen.getByTestId("pair-forget-u-other"));
    fireEvent.click(screen.getByTestId("confirm-forget-u-other"));

    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("forget_pairing", { args: { user_id: "u-other" } }),
    );
    await waitFor(() => expect(screen.queryByText("bob")).toBeNull());
    expect(screen.queryByTestId("confirm-strip-u-other")).toBeNull();
    expect(screen.getByText("alice")).toBeInTheDocument();
  });

  it("USE invokes set_active_pairing for that pairing", async () => {
    await renderPairings();
    fireEvent.click(screen.getByTestId("pair-use-u-other"));
    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("set_active_pairing", { args: { user_id: "u-other" } }),
    );
  });

  /*
   * ADR 0002 locates cipher disclosure beside pairing — one line per card, where
   * the decision to trust that relay is being made — rather than in permanent
   * chrome. The mock's badge named AES-256-GCM, which `core/crypto.rs` does not
   * seal with.
   */
  it("discloses XCHACHA20-POLY1305 on every card and names no other cipher", async () => {
    const view = await renderPairings();
    expect(screen.getAllByText("XCHACHA20-POLY1305")).toHaveLength(rows.length);
    expect(view.container.textContent).not.toMatch(/AES/i);
  });

  /*
   * ADR 0004: *account* has always been on Pairing's _Avoid_ line in CONTEXT.md
   * and the code now agrees with it. The confirm strip is opened first, being
   * the copy the old word was likeliest to survive in.
   */
  it("puts the word 'account' in front of nobody", async () => {
    const view = await renderPairings();
    fireEvent.click(screen.getByTestId("pair-forget-u-other"));
    expect(view.container.textContent).not.toMatch(/account/i);
  });
});
