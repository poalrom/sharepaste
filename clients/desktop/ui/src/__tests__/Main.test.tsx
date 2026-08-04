import { describe, expect, it, beforeEach, afterEach } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { useUiStore, type MainSection } from "../store/ui";
import { usePairingsStore } from "../store/pairings";
import { useHistoryStore } from "../store/history";
import { useContactStore, useStatusStore } from "../store";
import Main from "../views/Main";
import tauri from "../../../src-tauri/tauri.conf.json" with { type: "json" };

let ipc: MockIpc;

beforeEach(() => {
  ipc = mockIpc({
    invoke: (command) => {
      if (command === "get_settings") {
        return { capture_enabled: true, deny_list: [], autostart: false, hotkey: null };
      }
      if (command === "list_pairings") return [];
      if (command === "list_history") return [];
      return undefined;
    },
  });
  useUiStore.setState({
    filter: "",
    selectedIndex: 0,
    mainSection: "history",
    pairingFlowOpen: false,
    viewedUserId: undefined,
    seedEntryId: undefined,
    toast: undefined,
  });
  usePairingsStore.setState({ pairings: [], active: undefined });
  useHistoryStore.setState({ entries: [] });
  useStatusStore.setState({ byUser: {} });
  useContactStore.setState({ lastContactByUser: {} });
});

afterEach(() => {
  window.history.replaceState({}, "", "/");
});

/**
 * Drains the panes' async mount effects inside act(). Without this React warns
 * about the state updates that land after the test body returns.
 */
const settle = () => act(async () => {});

async function renderMain() {
  const view = render(<Main />);
  await settle();
  return view;
}

describe("Main shell", () => {
  it("routes the initial section from ?section=, falling back to history", async () => {
    const cases: Array<{ url: string; section: MainSection; rail: string; flow: boolean }> = [
      { url: "/main.html?section=settings", section: "settings", rail: "rail-settings", flow: false },
      { url: "/main.html?section=pairings", section: "pairings", rail: "rail-pairings", flow: false },
      // `pairing` is a route value, not a pane: it selects Pairings with the
      // add-flow already open, which is what the tray's "Pair device…" wants.
      { url: "/main.html?section=pairing", section: "pairings", rail: "rail-pairings", flow: true },
      { url: "/main.html", section: "history", rail: "rail-history", flow: false },
      { url: "/main.html?section=nonsense", section: "history", rail: "rail-history", flow: false },
    ];

    for (const { url, section, rail, flow } of cases) {
      useUiStore.setState({ mainSection: "history", pairingFlowOpen: false });
      window.history.replaceState({}, "", url);

      const view = await renderMain();
      expect(useUiStore.getState().mainSection).toBe(section);
      expect(useUiStore.getState().pairingFlowOpen).toBe(flow);
      expect(view.getByTestId(rail)).toHaveAttribute("aria-selected", "true");

      view.unmount();
    }
  });

  it("clicking a rail item updates the active section", async () => {
    await renderMain();
    fireEvent.click(screen.getByTestId("rail-settings"));
    expect(useUiStore.getState().mainSection).toBe("settings");
    await settle();
  });

  it("has no rail item for the pairing route value", async () => {
    await renderMain();
    expect(screen.queryByTestId("rail-pairing")).toBeNull();
  });

  /*
   * The rail is the only place the app states which build it is. It read
   * `ui/package.json` until 0.7.0 and so said v0.1.0 for six releases; this
   * pins it to the manifest the release gate makes every other version agree
   * with, so the next drift fails here rather than on a user's screen.
   */
  it("prints the shipped version on the rail", async () => {
    await renderMain();
    expect(screen.getByTestId("rail-version")).toHaveTextContent(`v${tauri.version}`);
  });

  /*
   * With `decorations(false)` these three are the only way to minimise, restore
   * or close: nothing behind them is the OS's job any more.
   */
  it("the titlebar's own controls drive the window", async () => {
    await renderMain();
    fireEvent.click(screen.getByTestId("win-minimise"));
    fireEvent.click(screen.getByTestId("win-maximise"));
    fireEvent.click(screen.getByTestId("win-close"));
    expect(ipc.window.minimize).toHaveBeenCalledOnce();
    expect(ipc.window.toggleMaximize).toHaveBeenCalledOnce();
    expect(ipc.window.close).toHaveBeenCalledOnce();
    await settle();
  });

  it("main://navigate flips the section and unpacks the pairing route", async () => {
    await renderMain();
    expect(ipc.handlers.get("main://navigate")).toHaveLength(1);

    await act(async () => ipc.emit("main://navigate", { section: "settings", entry_id: null }));
    expect(useUiStore.getState().mainSection).toBe("settings");

    await act(async () => ipc.emit("main://navigate", { section: "pairing", entry_id: null }));
    expect(useUiStore.getState().mainSection).toBe("pairings");
    expect(useUiStore.getState().pairingFlowOpen).toBe(true);
    await settle();
  });

  /*
   * The popover's handoff. It hands over the entry it had selected because a
   * collapsed preview cannot distinguish two strings that diverge past the
   * truncation — which is the case that justifies the reader existing.
   */
  it("seeds the reader's selection from ?entry= and from a navigate payload", async () => {
    window.history.replaceState({}, "", "/main.html?section=history&entry=42");
    const view = await renderMain();
    expect(useUiStore.getState().seedEntryId).toBe(42);

    await act(async () => ipc.emit("main://navigate", { section: "history", entry_id: 7 }));
    expect(useUiStore.getState().seedEntryId).toBe(7);
    view.unmount();
    await settle();
  });

  it("ignores a non-numeric ?entry=", async () => {
    window.history.replaceState({}, "", "/main.html?section=history&entry=nonsense");
    await renderMain();
    expect(useUiStore.getState().seedEntryId).toBeUndefined();
  });

  /*
   * The footer states device-wide facts, so it names the Active Pairing even
   * while the History pane is pointed somewhere else. The two are allowed to
   * disagree; a footer that followed the Viewed Pairing would leave nothing on
   * screen saying what the machine is actually doing.
   */
  it("footer reports the Active Pairing, not the Viewed one", async () => {
    ipc = mockIpc({
      invoke: (command) => {
        if (command === "list_pairings") {
          return [
            { user_id: "u-active", device_id: "d1", label: "mac", username: "alice", server_url: "https://relay.one", relay_host: "relay.one", status: "Online", pending: 0, is_active: true },
            { user_id: "u-other", device_id: "d2", label: "mac", username: "bob", server_url: "https://relay.two", relay_host: "relay.two", status: "Disconnected", pending: 0, is_active: false },
          ];
        }
        if (command === "list_history") return [];
        if (command === "get_contact") return { user_id: "u-active", last_contact_at: null };
        return undefined;
      },
    });
    useUiStore.setState({ viewedUserId: "u-other" });

    const view = await renderMain();
    expect(view.getByTestId("footer-identity")).toHaveTextContent("ALICE@RELAY.ONE");
    expect(view.getByTestId("footer-status")).toHaveTextContent("ONLINE");
  });

  it("footer says so when nothing is active", async () => {
    const view = await renderMain();
    expect(view.getByTestId("footer-status")).toHaveTextContent("NO ACTIVE PAIRING");
    expect(view.queryByTestId("footer-identity")).toBeNull();
  });

  /*
   * ADR 0002 cut the cipher badge from permanent chrome and located it beside
   * pairing. The mock put `AES-256-GCM` here, which is not even the cipher this
   * product uses (`core/crypto.rs` is XChaCha20-Poly1305).
   */
  it("footer carries no cipher badge", async () => {
    const view = await renderMain();
    expect(view.container.textContent).not.toMatch(/AES|CHACHA/i);
  });

  /*
   * A Viewed Pairing that outlives the pairing it named would leave the pane
   * showing a ghost; it falls back to the Active Pairing instead.
   */
  it("clears the Viewed Pairing when that pairing is forgotten", async () => {
    useUiStore.setState({ viewedUserId: "u-gone" });
    await renderMain();

    await act(async () => ipc.emit("pairing-removed", { user_id: "u-gone" }));
    expect(useUiStore.getState().viewedUserId).toBeUndefined();
  });
});
