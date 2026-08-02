import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import {
  usePairingsStore,
  useHistoryStore,
  useStatusStore,
  useContactStore,
  useUiStore,
} from "../store";
import type { Pairing, EntryView } from "../types";
import Popover from "../views/Popover";

const accounts: Pairing[] = [
  {
    user_id: "u-oldest",
    device_id: "d-oldest",
    label: "Oldest",
    server_url: "https://srv",
    relay_host: "srv",
    status: "Disconnected",
    pending: 0,
    is_active: false,
  },
  {
    user_id: "u-active",
    device_id: "d-active",
    label: "Active",
    server_url: "https://srv",
    relay_host: "srv",
    status: "Connecting",
    pending: 0,
    is_active: true,
  },
];

let ipc: MockIpc;
// Mutable so a test can change what the backend serves without re-injecting.
let servedAccounts: Pairing[];
let servedHistory: EntryView[];
let servedLastContact: number | null;

beforeEach(() => {
  servedAccounts = accounts;
  servedHistory = [];
  servedLastContact = null;
  ipc = mockIpc({
    invoke: (command, args) => {
      if (command === "list_pairings") return servedAccounts;
      if (command === "list_history") return servedHistory;
      if (command === "get_contact") {
        const { user_id } = (args?.args ?? {}) as { user_id: string };
        return { user_id, last_contact_at: servedLastContact };
      }
      return undefined;
    },
  });
  usePairingsStore.setState({ pairings: [], active: undefined });
  useHistoryStore.setState({ entries: [] });
  useStatusStore.setState({ byUser: {} });
  useContactStore.setState({ lastContactByUser: {} });
  useUiStore.setState({ filter: "", selectedIndex: 0, mainSection: "history" });
  useUiStore.getState().dismissToast();
});

describe("Popover", () => {
  it("loads initial history for the Pairing selected by hydration", async () => {
    render(<Popover />);

    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("list_history", {
        args: { user_id: "u-active", limit: 100 },
      });
    });
    expect(ipc.invoke).not.toHaveBeenCalledWith("list_history", {
      args: { user_id: "u-oldest", limit: 100 },
    });
  });

  it("empty-state buttons open the matching main-window section", async () => {
    const cases: Array<{ served: Pairing[]; label: string; section: string }> = [
      {
        served: accounts.map((a) => ({ ...a, is_active: false, status: "Disconnected" })),
        label: "CHOOSE PAIRING",
        section: "pairings",
      },
      { served: [], label: "PAIR A DEVICE", section: "pairing" },
    ];

    for (const { served, label, section } of cases) {
      servedAccounts = served;
      usePairingsStore.setState({ pairings: [], active: undefined });
      ipc.invoke.mockClear();

      const view = render(<Popover />);
      fireEvent.click(await view.findByText(label));
      await waitFor(() => {
        expect(ipc.invoke).toHaveBeenCalledWith("open_main_window", {
          args: { section, entry_id: undefined },
        });
        expect(ipc.invoke).toHaveBeenCalledWith("hide_popover", undefined);
      });

      view.unmount();
    }
  });

  /*
   * The handoff to the reader (ADR 0003). It is an icon, not a binding: ADR
   * 0002 established the hint strip has no width for a fourth entry, and the
   * whole point is reachability for the case a collapsed preview cannot serve.
   */
  it("the History icon opens the reader on the selected entry", async () => {
    servedHistory = [
      { id: 11, user_id: "u-active", preview: "one", plaintext: "one", created_at: 1, last_use: 1, device_id: "d", origin_label: "d", undecryptable: false },
      { id: 22, user_id: "u-active", preview: "two", plaintext: "two", created_at: 2, last_use: 2, device_id: "d", origin_label: "d", undecryptable: false },
    ];
    const view = render(<Popover />);
    await waitFor(() => expect(view.getAllByTestId("entry-row")).toHaveLength(2));

    await act(async () => useUiStore.setState({ selectedIndex: 1 }));
    fireEvent.click(view.getByTestId("open-history"));

    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("open_main_window", {
        args: { section: "history", entry_id: 22 },
      }),
    );
  });
});

describe("Popover event subscriptions", () => {
  it("re-lists history for the Active Pairing on history-changed", async () => {
    render(<Popover />);
    await waitFor(() => expect(ipc.handlers.get("history-changed")).toHaveLength(1));

    servedHistory = [{ id: 7, user_id: "u-active", preview: "Refetched", plaintext: "Refetched", created_at: 3, last_use: 3, device_id: "d-active", origin_label: "d-ac", undecryptable: false }];
    act(() => ipc.emit("history-changed", { user_id: "u-active" }));
    await waitFor(() => {
      expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([7]);
    });
  });

  it("ignores history-changed for a non-active Pairing", async () => {
    render(<Popover />);
    await waitFor(() => expect(ipc.handlers.get("history-changed")).toHaveLength(1));
    const before = ipc.invoke.mock.calls.filter((c) => c[0] === "list_history").length;

    act(() => ipc.emit("history-changed", { user_id: "u-oldest" }));
    expect(ipc.invoke.mock.calls.filter((c) => c[0] === "list_history")).toHaveLength(before);
  });

  /*
   * The banner this replaces named an entry id that appeared nowhere in the UI.
   * The row is the surface now (plan §0.12), and there is no longer an event
   * either: `CoreEvent::DecryptionError` was deleted because `EntryAdded`
   * already carries `undecryptable`, so the row is the *only* thing that says
   * so. What this pins is that one row is enough — no banner, no strip, and
   * nothing the popover has to be told separately.
   */
  it("leaves an undecryptable entry to its row and raises no banner", async () => {
    servedHistory = [{ id: 42, user_id: "u-active", preview: "", plaintext: null, created_at: 1, last_use: 1, device_id: "d-active", origin_label: "d-ac", undecryptable: true }];
    const { findAllByTestId, queryByTestId } = render(<Popover />);
    await waitFor(() => expect(ipc.handlers.get("contact")).toHaveLength(1));

    expect(await findAllByTestId("entry-row")).toHaveLength(1);
    expect(queryByTestId("entry-row")).toHaveTextContent("UNDECRYPTABLE");
    expect(queryByTestId("toast")).toBeNull();
  });

  it("unsubscribes every listener on unmount", async () => {
    const { unmount } = render(<Popover />);
    await waitFor(() => expect(ipc.handlers.get("contact")).toHaveLength(1));
    unmount();
    await waitFor(() => {
      expect(ipc.handlers.get("history-changed")).toHaveLength(0);
      expect(ipc.handlers.get("contact")).toHaveLength(0);
    });
  });

  // The popover window is shown/hidden, never unmounted, so `autoFocus` fires
  // once per window lifetime. Reopening used to leave focus on whatever was
  // last clicked - and because HistoryList ignores keydown while a button holds
  // focus, that made the reopened popover keyboard-dead.
  it("returns focus to the Filter box each time the popover is shown", async () => {
    const { findByPlaceholderText, getByRole } = render(<Popover />);
    const input = await findByPlaceholderText("Filter history…");
    expect(document.activeElement).toBe(input);

    // Simulate the reported sequence: click a footer button, which opens the
    // main window and hides the popover, leaving focus on the button.
    const settings = getByRole("button", { name: "Settings" });
    settings.focus();
    expect(document.activeElement).toBe(settings);

    // Hidden, then shown again: the window regains focus.
    act(() => {
      window.dispatchEvent(new FocusEvent("focus"));
    });

    expect(document.activeElement).toBe(input);
  });

  it("selects any leftover query so typing replaces it", async () => {
    const { findByPlaceholderText } = render(<Popover />);
    const input = (await findByPlaceholderText("Filter history…")) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "old query" } });
    input.blur();

    act(() => {
      window.dispatchEvent(new FocusEvent("focus"));
    });

    expect(document.activeElement).toBe(input);
    expect(input.selectionStart).toBe(0);
    expect(input.selectionEnd).toBe("old query".length);
  });

  // The sweep has to be able to replay, which means remounting whatever carries
  // it. It rides an overlay rather than the panel element precisely so that
  // restarting it cannot tear the Filter box out from under the focus the same
  // `focus` event just restored - hence both halves of this assertion.
  it("replays the sweep on every show without remounting the panel", async () => {
    const view = render(<Popover />);
    await view.findByPlaceholderText("Filter history…");
    const panel = view.container.firstElementChild!;
    const first = panel.querySelector(".fui-sweep");
    expect(first).not.toBeNull();

    act(() => {
      window.dispatchEvent(new FocusEvent("focus"));
    });

    const replayed = panel.querySelector(".fui-sweep");
    expect(replayed).not.toBeNull();
    expect(replayed).not.toBe(first);
    expect(view.container.firstElementChild).toBe(panel);
  });
});

describe("Popover degraded strip", () => {
  // State arrives from `list_accounts` at hydration and from `connection-state`
  // thereafter, so the tests drive it the same two ways the app does rather
  // than pre-seeding a store that hydration would immediately overwrite.
  const serveState = (status: Pairing["status"]) => {
    servedAccounts = accounts.map((a) =>
      a.user_id === "u-active" ? { ...a, status } : a,
    );
  };

  it("states Contact as NEVER until this device has reached the relay", async () => {
    serveState("Disconnected");

    const strip = await render(<Popover />).findByTestId("degraded-strip");
    expect(strip).toHaveTextContent("OFFLINE");
    expect(strip).toHaveTextContent("LAST CONTACT NEVER");
  });

  // The popover opens long after the last contact event fired, so the age
  // has to come from the hydrating command rather than from a live event.
  it("states Contact as an age once get_contact answers", async () => {
    servedLastContact = Date.now() - 4 * 60_000;
    serveState("Disconnected");

    const strip = await render(<Popover />).findByTestId("degraded-strip");
    expect(strip).toHaveTextContent("OFFLINE");
    await waitFor(() => expect(strip).toHaveTextContent("LAST CONTACT 4m ago"));
  });

  // `run_sse_loop` enters Connecting at the top of every iteration, including
  // the very first, so treating it as degraded would flash a band across a
  // perfectly healthy cold start. The pulsing footer light carries it instead.
  it("shows no strip while the connection is merely Connecting", async () => {
    serveState("Connecting");

    const view = render(<Popover />);
    await view.findByPlaceholderText("Filter history…");
    expect(view.queryByTestId("degraded-strip")).toBeNull();
    expect(view.getByTestId("status")).toHaveTextContent("CONNECTING");
  });

  // ADR 0002: a healthy window says nothing about itself, so the band that
  // carries Contact does not exist while the connection is nominal. Without
  // seeding from list_accounts the store would read Disconnected here and a
  // healthy window would wear an OFFLINE band until the next transition fired.
  it("shows no strip at all while the connection is Online", async () => {
    serveState("Online");

    const view = render(<Popover />);
    await view.findByPlaceholderText("Filter history…");
    await waitFor(() => expect(view.getByTestId("status")).toHaveTextContent("ONLINE"));
    expect(view.queryByTestId("degraded-strip")).toBeNull();
  });

  it("carries last_error when the relay rejected this device", async () => {
    const view = render(<Popover />);
    await view.findByPlaceholderText("Filter history…");

    act(() =>
      ipc.emit("connection-state", {
        user_id: "u-active",
        state: "AuthFailed",
        last_error: "device token revoked",
      }),
    );

    const strip = await view.findByTestId("degraded-strip");
    expect(strip).toHaveTextContent("AUTH FAILED");
    expect(strip).toHaveTextContent("device token revoked");
  });
});

describe("Popover footer", () => {
  it("names the User only when more than one Pairing exists", async () => {
    servedAccounts = accounts.map((a) => ({ ...a, username: "alice" }));
    const many = render(<Popover />);
    expect(await many.findByText("ALICE")).toBeInTheDocument();
    many.unmount();

    usePairingsStore.setState({ pairings: [], active: undefined });
    servedAccounts = [{ ...accounts[1]!, username: "alice" }];
    const one = render(<Popover />);
    await one.findByPlaceholderText("Filter history…");
    expect(one.queryByText("ALICE")).toBeNull();
  });
});

describe("Popover toast", () => {
  it("clears a toast once its window elapses", async () => {
    const view = render(<Popover />);
    await view.findByPlaceholderText("Filter history…");

    vi.useFakeTimers();
    try {
      act(() =>
        useUiStore.getState().showToast({ tone: "cyan", text: "COPIED", detail: "ss://Y2hhY2hh" }),
      );
      const toast = view.getByTestId("toast");
      expect(toast).toHaveTextContent("[COPIED]");
      expect(toast).toHaveTextContent("ss://Y2hhY2hh");

      act(() => {
        vi.advanceTimersByTime(2200);
      });
      expect(view.queryByTestId("toast")).toBeNull();
      expect(useUiStore.getState().toast).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });
});

// These two only mean anything with Filter and HistoryList composed together:
// the whole justification for the modifier is that the Filter input holds
// focus, and the row's explanation is only real once a strip renders it.
describe("Popover keyboard and row surfaces", () => {
  const capturedAt = Date.now();
  const entry = (over: Partial<EntryView> = {}): EntryView => ({
    id: 7,
    user_id: "u-active",
    preview: "npm run dev",
    plaintext: "npm run dev",
    created_at: capturedAt,
    last_use: capturedAt,
    device_id: "d-active",
    origin_label: "d-ac",
    undecryptable: false,
    ...over,
  });

  it("deletes on ⌘⌫ while the Filter input holds focus", async () => {
    servedHistory = [entry()];
    const view = render(<Popover />);
    const input = await view.findByPlaceholderText("Filter history…");
    expect(document.activeElement).toBe(input);

    fireEvent.keyDown(input, { key: "Backspace", metaKey: true });

    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("delete_entry", {
        args: { user_id: "u-active", entry_id: 7 },
      }),
    );
  });

  it("leaves the entry alone on a bare Backspace in the Filter box", async () => {
    servedHistory = [entry()];
    const view = render(<Popover />);
    const input = await view.findByPlaceholderText("Filter history…");

    fireEvent.keyDown(input, { key: "Backspace" });

    await waitFor(() => expect(view.getAllByTestId("entry-row")).toHaveLength(1));
    expect(ipc.invoke).not.toHaveBeenCalledWith("delete_entry", expect.anything());
  });

  it("explains an Undecryptable entry in a strip instead of copying it", async () => {
    servedHistory = [entry({ preview: "", plaintext: null, undecryptable: true })];
    const view = render(<Popover />);
    const row = await view.findByTestId("entry-row");

    fireEvent.click(row);

    const strip = await view.findByTestId("toast");
    expect(strip).toHaveTextContent("[CAN'T COPY]");
    expect(strip).toHaveTextContent("encrypted with a key this device doesn't have");
    expect(ipc.invoke).not.toHaveBeenCalledWith("copy_to_clipboard", expect.anything());
    expect(ipc.invoke).not.toHaveBeenCalledWith("hide_popover", expect.anything());
  });
});
