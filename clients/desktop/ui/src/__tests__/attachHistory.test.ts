import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { waitFor } from "@testing-library/react";
import { attachHistory, showHistory, type HistoryScope } from "../attachHistory";
import { mockIpc, type MockIpc } from "./helpers";
import {
  useContactStore,
  useHistoryStore,
  usePairingsStore,
  useStatusStore,
  useUiStore,
} from "../store";
import type { EntryView, Pairing } from "../types";

/**
 * The four the ordering rule is about. An Entry the uploader caches before
 * these exist is announced to nobody and absent from the snapshot that follows,
 * and nothing later provokes a refetch.
 */
const ENTRY_EVENTS = ["entry-added", "entry-deleted", "entry-settled", "entry-refused"] as const;

const pairings: Pairing[] = [
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
  {
    user_id: "u-other",
    device_id: "d-other",
    label: "Other",
    server_url: "https://srv",
    relay_host: "srv",
    status: "Disconnected",
    pending: 2,
    is_active: false,
  },
];

/**
 * The two adapters the shipped app hands this seam, asserted here rather than
 * through the surfaces that pass them: the main window's copy of this wiring
 * had no test of its own at all, and the popover's could only reach the
 * ordering rule indirectly, through a render and an emit.
 */
const SCOPES: Array<[string, HistoryScope]> = [
  [
    "popover · the Active Pairing",
    { userId: () => usePairingsStore.getState().active, showsHistory: true },
  ],
  [
    "main window · the Viewed Pairing",
    {
      userId: () => useUiStore.getState().viewedUserId ?? usePairingsStore.getState().active,
      showsHistory: false,
    },
  ],
];

let ipc: MockIpc;
let detach: (() => void) | undefined;
let servedHistory: Promise<EntryView[]> | EntryView[];
let servedContact: number | null;

/** Every command name the module has sent, in order. */
const commandsSent = (): string[] => ipc.invoke.mock.calls.map(([command]) => command);

function userIdOf(payload?: Record<string, unknown>): string {
  const args = payload?.args as { user_id?: string } | undefined;
  if (typeof args?.user_id !== "string") {
    throw new Error(`expected { args: { user_id } }, got ${JSON.stringify(payload)}`);
  }
  return args.user_id;
}

const entry = (id: number, user_id = "u-active"): EntryView => ({
  id,
  user_id,
  preview: `entry-${id}`,
  plaintext: `entry-${id}`,
  created_at: id,
  last_use: id,
  device_id: "d-active",
  origin_label: "d-ac",
  undecryptable: false,
  pending: false,
  refused_reason: null,
});

function serve(overrides: { listPairings?: () => unknown } = {}): void {
  ipc = mockIpc({
    invoke: (command, payload) => {
      if (command === "list_pairings") return overrides.listPairings?.() ?? pairings;
      if (command === "list_history") return servedHistory;
      if (command === "get_contact") {
        return { user_id: userIdOf(payload), last_contact_at: servedContact };
      }
      return undefined;
    },
  });
}

beforeEach(() => {
  servedHistory = [];
  servedContact = null;
  serve();
  usePairingsStore.setState({ pairings: [], active: undefined });
  useHistoryStore.setState({ entries: [] });
  useStatusStore.setState({ byUser: {} });
  useContactStore.setState({ lastContactByUser: {} });
  useUiStore.setState({ viewedUserId: undefined, filter: "", selectedIndex: 0 });
});

afterEach(() => {
  detach?.();
  detach = undefined;
});

describe("attachHistory — the ordering rule", () => {
  /*
   * Anomaly A of `.scratch/mobile-client/issues/06`, reproduced twice on a
   * Windows smoke run: an offline burst of three flushed, the relay gained
   * three rows, and one of them was on screen afterwards.
   *
   * This is the rule stated directly at the module's own seam. The snapshot
   * command is held open, so reaching this assertion at all proves the four
   * subscriptions were registered before the first `await` — not merely before
   * the answer came back.
   */
  it.each(SCOPES)(
    "registers the four entry subscriptions before its first command answers (%s)",
    async (_name, scope) => {
      serve({ listPairings: () => new Promise<never>(() => {}) });

      detach = attachHistory(scope);

      await waitFor(() => expect(commandsSent()).toContain("list_pairings"));
      for (const event of ENTRY_EVENTS) {
        expect(ipc.handlers.get(event)).toHaveLength(1);
      }
      // Still in flight, so nothing below it in the module has run: the four
      // above are all that exist yet, and they exist already.
      expect(commandsSent()).toEqual(["list_pairings"]);
      expect(ipc.handlers.get("history-changed") ?? []).toHaveLength(0);
    },
  );

  /*
   * The other half of the same fix. Subscribing first only means the
   * announcement is heard; a snapshot requested before it and applied after it
   * would still roll it back, so `noteChange` records it and `hydrateFrom`
   * replays it.
   *
   * The popover takes its own snapshot here. The main window's is taken by
   * `HistorySection` through `showHistory`, which is the same call this module
   * makes — so both surfaces are asserted against the composition they ship.
   */
  it.each(SCOPES)(
    "keeps an Entry announced while the History snapshot is in flight (%s)",
    async (_name, scope) => {
      let release: (rows: EntryView[]) => void = () => {};
      servedHistory = new Promise<EntryView[]>((resolve) => {
        release = resolve;
      });

      detach = attachHistory(scope);
      await waitFor(() => expect(ipc.handlers.get("entry-added")).toHaveLength(1));
      if (!scope.showsHistory) void showHistory(scope.userId());
      await waitFor(() => expect(commandsSent()).toContain("list_history"));

      const flushed = entry(3);
      flushed.preview = "offline-burst-three";
      ipc.emit("entry-added", { user_id: "u-active", entry: flushed });

      // The snapshot was taken before that Entry existed, and must not undo it.
      release([]);
      await waitFor(() =>
        expect(useHistoryStore.getState().entries.map((e) => e.preview)).toEqual([
          "offline-burst-three",
        ]),
      );
    },
  );
});

describe("attachHistory — what it seeds and what it releases", () => {
  /*
   * `list_pairings` already knows each session's state; without seeding it here
   * the surface reads Disconnected until the next transition happens to fire,
   * and a window opened onto a healthy session shows the degraded strip
   * indefinitely.
   */
  it.each(SCOPES)("seeds the status of every Pairing from list_pairings (%s)", async (_n, scope) => {
    detach = attachHistory(scope);

    await waitFor(() =>
      expect(useStatusStore.getState().byUser["u-active"]).toEqual({
        state: "Connecting",
        pending: 0,
      }),
    );
    expect(useStatusStore.getState().byUser["u-other"]).toEqual({
      state: "Disconnected",
      pending: 2,
    });
  });

  /*
   * Contact is stamped by traffic the surface was not open for, so no event
   * fires until the next byte and it has to be asked for.
   */
  it("hydrates Contact for the Pairing a scope-showing surface displays", async () => {
    servedContact = 1234;
    detach = attachHistory(SCOPES[0]![1]);

    await waitFor(() =>
      expect(useContactStore.getState().lastContactByUser).toEqual({ "u-active": 1234 }),
    );
  });

  /*
   * A surface that does not show its own scope here shows more than one
   * Pairing at once — the main window's footer names the Active Pairing while
   * its pane names the Viewed one — so every Pairing's Contact has to be on
   * hand before either is chosen.
   */
  it("hydrates Contact for every Pairing when the surface does not show its own", async () => {
    servedContact = 99;
    detach = attachHistory(SCOPES[1]![1]);

    await waitFor(() =>
      expect(useContactStore.getState().lastContactByUser).toEqual({
        "u-active": 99,
        "u-other": 99,
      }),
    );
  });

  it.each(SCOPES)("releases every handle on teardown (%s)", async (_name, scope) => {
    const teardown = attachHistory(scope);
    // `contact` is the last handle registered, so its arrival dates them all.
    await waitFor(() => expect(ipc.handlers.get("contact")).toHaveLength(1));

    teardown();

    for (const [event, registered] of ipc.handlers) {
      expect(registered, `${event} still has a listener`).toHaveLength(0);
    }
  });

  /*
   * The scope is read at call time, never closed over: these listeners are
   * registered once and must not pin the Pairing they were born with.
   */
  it("accepts entries for whatever the scope names now, not at attach", async () => {
    useUiStore.setState({ viewedUserId: "u-active" });
    detach = attachHistory(SCOPES[1]![1]);
    await waitFor(() => expect(ipc.handlers.get("entry-added")).toHaveLength(1));

    useUiStore.setState({ viewedUserId: "u-other" });
    ipc.emit("entry-added", { user_id: "u-other", entry: entry(7, "u-other") });

    expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([7]);
  });

  /*
   * A refetch, not a re-show: `history-changed` says the rows moved, not that
   * the surface changed what it is looking at, so nothing asks for Contact.
   */
  it("refetches the History on history-changed without asking for Contact again", async () => {
    detach = attachHistory(SCOPES[0]![1]);
    await waitFor(() => expect(ipc.handlers.get("history-changed")).toHaveLength(1));
    const contactCalls = commandsSent().filter((c) => c === "get_contact").length;

    servedHistory = [entry(5)];
    ipc.emit("history-changed", { user_id: "u-active" });

    await waitFor(() =>
      expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([5]),
    );
    expect(commandsSent().filter((c) => c === "get_contact")).toHaveLength(contactCalls);
  });
});
