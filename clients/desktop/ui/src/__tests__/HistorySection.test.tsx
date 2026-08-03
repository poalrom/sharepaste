import { beforeEach, describe, expect, it } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { capturedAt } from "../lib/format";
import { useContactStore, useHistoryStore, usePairingsStore, useUiStore } from "../store";
import type { Contact, EntryView, Pairing } from "../types";
import HistorySection from "../views/main/HistorySection";

const NOW = 1_700_000_000_000;
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

/** The cap `EntryDetail` stops laying out at; nothing in the product bounds an entry. */
const RENDER_CAP = 65_536;
/** The cap `entries_cache` prunes at, which the list-end sentinel names. */
const CACHE_CAP = 100;

const pairingA: Pairing = {
  user_id: "u-a",
  device_id: "dev-a",
  label: "MBP-14",
  username: "alice",
  server_url: "https://relay.one",
  relay_host: "relay.one",
  status: "Online",
  pending: 0,
  is_active: true,
};
const pairingB: Pairing = {
  user_id: "u-b",
  device_id: "dev-b",
  label: "MBP-14",
  username: "bob",
  server_url: "https://relay.two",
  relay_host: "relay.two",
  status: "Disconnected",
  pending: 0,
  is_active: false,
};

/**
 * An entry the row and the reader must render differently.
 *
 * `preview` is the facade's Preview — one line, every control character a
 * space, trimmed — and `plaintext` is the text it was built from. Stated rather
 * than derived: re-deriving it here would be the second normaliser this split
 * exists to delete.
 */
const MULTILINE = "ssh admin@10.0.0.4\n  -i ~/.ssh/id_ed25519\n  -p 2222";
const MULTILINE_PREVIEW = "ssh admin@10.0.0.4   -i ~/.ssh/id_ed25519   -p 2222";

/**
 * An entry whose third line falls past the Preview's 80-character cap, and the
 * word that lives only there.
 *
 * The reason the Filter reads `plaintext`. `borogoves` appears nowhere in
 * `LONG_PREVIEW`, so a filter over the Preview cannot find this row at all —
 * which is what truncating the one field into the other would have caused.
 */
const THIRD_LINE_WORD = "borogoves";
const LONG_PLAINTEXT =
  "twas brillig and the slithy toves did gyre and gimble in the wabe\n" +
  "  all mimsy were the\n" +
  `  ${THIRD_LINE_WORD}`;
/** Its Preview, as the facade builds one: controls to spaces, trimmed, 80 characters. */
const LONG_PREVIEW =
  "twas brillig and the slithy toves did gyre and gimble in the wabe   all mimsy we";

/** Longer than the render cap, with a marker on each side of the cut. */
const OVERSIZE = `HEAD-${"A".repeat(RENDER_CAP)}-TAIL`;

/**
 * In the order the facade hands them back, which is Last Use first (ADR 0011).
 *
 * Entry 12 was captured six hours ago and used half an hour ago: the one row
 * here whose two facts differ, and the reason it sits above an entry captured
 * more recently than it was.
 */
const entriesA: EntryView[] = [
  { id: 11, user_id: "u-a", preview: MULTILINE_PREVIEW, plaintext: MULTILINE, created_at: NOW - 2 * MINUTE, last_use: NOW - 2 * MINUTE, device_id: "dev-a", device_label: "MBP-14", origin_label: "MBP-14", undecryptable: false },
  { id: 12, user_id: "u-a", preview: "bravo", plaintext: "bravo", created_at: NOW - 6 * HOUR, last_use: NOW - 30 * MINUTE, device_id: "dev-phone", device_label: "IPHONE-15", origin_label: "IPHONE-15", undecryptable: false },
  { id: 13, user_id: "u-a", preview: "charlie", plaintext: "charlie", created_at: NOW - 3 * HOUR, last_use: NOW - 3 * HOUR, device_id: "dev-a", device_label: "MBP-14", origin_label: "MBP-14", undecryptable: false },
];

/**
 * Deliberately shares entry id 13 with `entriesA`: the popover's seed names an
 * id, so a seed that outlives its one hydration would land on this row.
 */
const entriesB: EntryView[] = [
  { id: 21, user_id: "u-b", preview: "from the laptop", plaintext: "from the laptop", created_at: NOW - MINUTE, last_use: NOW - MINUTE, device_id: "dev-a", device_label: "MBP-14", origin_label: "MBP-14", undecryptable: false },
  { id: 13, user_id: "u-b", preview: "same id other pairing", plaintext: "same id other pairing", created_at: NOW - 2 * MINUTE, last_use: NOW - 2 * MINUTE, device_id: "dev-b", device_label: "PIXEL-9", origin_label: "PIXEL-9", undecryptable: false },
];

const bulkEntries = (count: number): EntryView[] =>
  Array.from({ length: count }, (_, i) => ({
    id: 200 + i,
    user_id: "u-a",
    preview: `entry-${i}`,
    plaintext: `entry-${i}`,
    created_at: NOW - i * MINUTE,
    last_use: NOW - i * MINUTE,
    device_id: "dev-a",
    origin_label: "dev-",
    undecryptable: false,
  }));

let ipc: MockIpc;
let historyByUser: Record<string, EntryView[]>;
let lastContactByUser: Record<string, number | null>;

/** Reads the `user_id` a command was scoped to, failing loudly if the shape drifts. */
function userIdOf(payload?: Record<string, unknown>): string {
  const args = payload?.args;
  if (args && typeof args === "object" && "user_id" in args && typeof args.user_id === "string") {
    return args.user_id;
  }
  throw new Error(`expected { args: { user_id } }, got ${JSON.stringify(payload)}`);
}

/**
 * Every command name the pane has sent, in order.
 *
 * `not.toHaveBeenCalledWith(name, expect.anything())` cannot express "never, on
 * any payload" — `expect.anything()` does not match the `undefined` a no-arg
 * command sends — and every "must not fire" assertion below needs exactly that.
 */
const commandsSent = (): string[] => ipc.invoke.mock.calls.map(([command]) => command);

/** Which visible row is addressed, by 0-based position; -1 when none is. */
const selectedRowIndex = (): number =>
  screen.getAllByTestId("main-entry-row").findIndex((row) => row.dataset.selected === "true");

/**
 * The filter takes focus on mount and keeps it. That is the pane's resting
 * state rather than an exception, which is the whole reason delete carries a
 * modifier (ADR 0003) — so every key below is fired from inside the filter,
 * and this asserts the premise each time.
 */
function filterInput(): HTMLElement {
  const input = screen.getByLabelText("Filter history");
  expect(document.activeElement).toBe(input);
  return input;
}

/**
 * Renders the pane and waits out the two fetches its mount fires.
 *
 * The first paint of a hydrating pane is indistinguishable from `HISTORY EMPTY`
 * and from "no row selected", so asserting before `list_history` lands would
 * let several of the tests below pass for the wrong reason.
 */
async function renderPane(now = NOW): Promise<void> {
  render(<HistorySection now={now} />);
  const viewed = useUiStore.getState().viewedUserId ?? usePairingsStore.getState().active;
  if (viewed === undefined) return;
  await waitFor(() => {
    expect(useHistoryStore.getState().entries).toEqual(historyByUser[viewed] ?? []);
    expect(useContactStore.getState().lastContactByUser).toHaveProperty(viewed);
  });
}

beforeEach(() => {
  historyByUser = { "u-a": [...entriesA], "u-b": [...entriesB] };
  lastContactByUser = { "u-a": NOW - 30 * MINUTE, "u-b": NOW - 3 * HOUR };
  ipc = mockIpc({
    invoke: (command, payload) => {
      if (command === "list_history") return historyByUser[userIdOf(payload)] ?? [];
      if (command === "get_contact") {
        const user_id = userIdOf(payload);
        return { user_id, last_contact_at: lastContactByUser[user_id] ?? null } satisfies Contact;
      }
      return undefined;
    },
  });
  usePairingsStore.setState({ pairings: [pairingA, pairingB], active: "u-a" });
  useHistoryStore.setState({ entries: [] });
  useContactStore.setState({ lastContactByUser: {} });
  useUiStore.setState({
    filter: "",
    selectedIndex: 0,
    mainSection: "history",
    pairingFlowOpen: false,
    viewedUserId: undefined,
    seedEntryId: undefined,
    toast: undefined,
  });
});

describe("HistorySection — the reader", () => {
  /*
    The entire justification for the pane (ADR 0003): the popover can only
    collapse an entry to one whitespace-normalised line, so three URLs that
    diverge at character 60 are indistinguishable there by construction.
  */
  it("renders the full plaintext beside a row that shows one collapsed line", async () => {
    await renderPane();

    const row = screen.getAllByTestId("main-entry-row")[0]!;
    expect(row.textContent).toContain(MULTILINE_PREVIEW);
    expect(row.textContent).not.toContain("\n");

    // Exact, not `toContain`: the pane's contract is the original text whole.
    expect(screen.getByTestId("entry-detail-body").textContent).toBe(MULTILINE);
  });

  /*
    The behaviour that made truncating `preview` into one field unacceptable:
    a query has to reach a word the Preview does not carry.
  */
  it("matches a query against a word past the Preview's cap on an entry's third line", async () => {
    expect(LONG_PREVIEW).not.toContain(THIRD_LINE_WORD);
    historyByUser["u-a"] = [
      { id: 14, user_id: "u-a", preview: LONG_PREVIEW, plaintext: LONG_PLAINTEXT, created_at: NOW - 4 * MINUTE, last_use: NOW - MINUTE, device_id: "dev-a", device_label: "MBP-14", origin_label: "MBP-14", undecryptable: false },
      ...entriesA,
    ];
    await renderPane();

    fireEvent.change(screen.getByLabelText("Filter history"), {
      target: { value: THIRD_LINE_WORD },
    });

    const rows = screen.getAllByTestId("main-entry-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]!.textContent).toContain(LONG_PREVIEW);
    // And the reader beside it still holds the whole entry, third line included.
    expect(screen.getByTestId("entry-detail-body").textContent).toBe(LONG_PLAINTEXT);
  });

  /*
    A needle is a fragment, not prose. Left to the platform's text services,
    macOS floats a correction bubble over the first row offering to turn
    `tail` into `Tail`, and would capitalise the first letter besides.
  */
  it("declines every text service the filter field can decline", async () => {
    await renderPane();
    const input = screen.getByLabelText("Filter history");

    expect(input).toHaveAttribute("spellcheck", "false");
    expect(input).toHaveAttribute("autocorrect", "off");
    expect(input).toHaveAttribute("autocapitalize", "off");
  });

  /*
    Two facts where the row can only show one. The row's age is the Use
    (ADR 0011), so a six-hour-old entry recalled half an hour ago reads as
    `30m` there — and without the capture time stated here, nothing on any
    surface would say how old the entry actually is.
  */
  it("states when a reordered entry was used beside when it was captured", async () => {
    await renderPane();
    fireEvent.click(screen.getAllByTestId("main-entry-row")[1]!);

    expect(screen.getAllByTestId("main-entry-row")[1]!).toHaveTextContent("30m");
    const meta = screen.getByText(/CAPTURED/);
    expect(meta).toHaveTextContent(`CAPTURED ${capturedAt(NOW - 6 * HOUR, NOW)}`);
    expect(meta).toHaveTextContent("6h ago");
    expect(meta).toHaveTextContent(`USED ${capturedAt(NOW - 30 * MINUTE, NOW)}`);
  });

  // An entry never used since capture has `last_use == created_at`, and a USED
  // reading back the capture time would state a second event that never
  // happened.
  it("says nothing about use for an entry never used since capture", async () => {
    await renderPane();

    const meta = screen.getByText(/CAPTURED/);
    expect(meta).toHaveTextContent(`CAPTURED ${capturedAt(NOW - 2 * MINUTE, NOW)}`);
    expect(meta).not.toHaveTextContent("USED");
  });

  it("falls back to a prompt when the addressed position holds no entry", async () => {
    historyByUser["u-a"] = [];
    await renderPane();
    expect(screen.getByText(/Select an entry to read it in full/)).toBeInTheDocument();
  });
});

describe("HistorySection — the Viewed Pairing", () => {
  /*
    Viewed and Active are different things (CONTEXT.md): reading one pairing
    changes nothing about sync or capture. An implementation that "helpfully"
    activated the pairing being read would pass every other test in this file.
  */
  it("re-reads history for the newly Viewed Pairing without making it Active", async () => {
    await renderPane();

    fireEvent.change(screen.getByTestId("viewed-pairing"), { target: { value: "u-b" } });

    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("list_history", {
        args: { user_id: "u-b", limit: CACHE_CAP },
      }),
    );
    // The *list* is what re-read; the same text also lands in the reader beside it.
    await waitFor(() =>
      expect(screen.getAllByTestId("main-entry-row")[0]!).toHaveTextContent("from the laptop"),
    );
    expect(commandsSent()).not.toContain("set_active_pairing");
    expect(usePairingsStore.getState().active).toBe("u-a");
  });

  it("shows no frozen band while the Viewed Pairing is the Active one", async () => {
    await renderPane();
    expect(screen.queryByTestId("viewed-band")).toBeNull();
  });

  /*
    A non-Active pairing has no session, so `list_history` handed back a frozen
    snapshot: nothing will arrive in it and nothing captured here will join it.
    Without the band the footer (Active) and the rows (Viewed) silently disagree.
  */
  it("bands a non-Active Viewed Pairing with how long ago it was last reached", async () => {
    useUiStore.setState({ viewedUserId: "u-b" });
    await renderPane();

    expect(screen.getByTestId("viewed-band")).toHaveTextContent("LAST CONTACT 3h ago");
  });

  it("reads a pairing this device has never reached as NEVER", async () => {
    lastContactByUser["u-b"] = null;
    useUiStore.setState({ viewedUserId: "u-b" });
    await renderPane();

    expect(screen.getByTestId("viewed-band")).toHaveTextContent("LAST CONTACT NEVER");
    expect(screen.getByTestId("viewed-band")).not.toHaveTextContent(/ago/i);
  });

  // The band's control is the one place in this pane that *is* allowed to move
  // the device's sync and capture target.
  it("switches the device only when MAKE ACTIVE is used", async () => {
    useUiStore.setState({ viewedUserId: "u-b" });
    await renderPane();

    fireEvent.click(screen.getByTestId("make-active"));

    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("set_active_pairing", { args: { user_id: "u-b" } }),
    );
  });

  it("offers no pairing selector when there is only one pairing to view", async () => {
    usePairingsStore.setState({ pairings: [pairingA], active: "u-a" });
    await renderPane();

    expect(screen.queryByTestId("viewed-pairing")).toBeNull();
  });

  it("offers the selector, set to the Viewed Pairing, once there is a choice", async () => {
    await renderPane();
    expect(screen.getByTestId("viewed-pairing")).toHaveValue("u-a");
  });
});

describe("HistorySection — keyboard, fired from inside the filter", () => {
  it("walks the list one row at a time", async () => {
    await renderPane();

    fireEvent.keyDown(filterInput(), { key: "ArrowDown" });
    expect(selectedRowIndex()).toBe(1);

    fireEvent.keyDown(filterInput(), { key: "ArrowUp" });
    expect(selectedRowIndex()).toBe(0);
  });

  /*
    Clamped where the popover wraps: ten rows in a picker are a ring you spin,
    a hundred rows beside a reading pane are a document you walk, and wrapping
    off the end of one reads as having lost your place.
  */
  it("clamps at the first row instead of wrapping to the last", async () => {
    await renderPane();

    fireEvent.keyDown(filterInput(), { key: "ArrowUp" });

    expect(selectedRowIndex()).toBe(0);
  });

  it("clamps at the last row instead of wrapping to the first", async () => {
    await renderPane();

    fireEvent.keyDown(filterInput(), { key: "ArrowDown" });
    fireEvent.keyDown(filterInput(), { key: "ArrowDown" });
    expect(selectedRowIndex()).toBe(2);

    fireEvent.keyDown(filterInput(), { key: "ArrowDown" });
    expect(selectedRowIndex()).toBe(2);
  });

  // In the popover `⏎` means *copy and get out of the way*; a window has
  // nothing to get out of the way of, so here it copies and stays (ADR 0003).
  it("copies the addressed entry on Enter and does not hide anything", async () => {
    await renderPane();

    fireEvent.keyDown(filterInput(), { key: "ArrowDown" });
    fireEvent.keyDown(filterInput(), { key: "Enter" });

    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("copy_to_clipboard", {
        args: { user_id: "u-a", entry_id: 12 },
      }),
    );
    expect(commandsSent()).not.toContain("hide_popover");
  });

  it("deletes the addressed entry on Ctrl+Shift+Backspace", async () => {
    await renderPane();

    fireEvent.keyDown(filterInput(), { key: "Backspace", ctrlKey: true, shiftKey: true });

    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("delete_entry", {
        args: { user_id: "u-a", entry_id: 11 },
      }),
    );
    await waitFor(() => expect(screen.getAllByTestId("main-entry-row")).toHaveLength(2));
  });

  // The same binding under the keycaps a Mac actually prints.
  it("deletes the addressed entry on Shift+Meta+Backspace", async () => {
    await renderPane();

    fireEvent.keyDown(filterInput(), { key: "Backspace", metaKey: true, shiftKey: true });

    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("delete_entry", {
        args: { user_id: "u-a", entry_id: 11 },
      }),
    );
  });

  /*
    Unshifted, the combination is the field's: macOS erases to the start of the
    line with it and Windows erases the previous word, so a list that took it
    destroyed an entry under a key the platform had already promised to the
    text being edited (ADR 0013).
  */
  it("clears the query on Meta+Backspace and deletes nothing", async () => {
    await renderPane();
    fireEvent.change(filterInput(), { target: { value: "entry" } });
    expect(filterInput()).toHaveValue("entry");

    fireEvent.keyDown(filterInput(), { key: "Backspace", metaKey: true });
    await act(async () => {});

    expect(filterInput()).toHaveValue("");
    expect(commandsSent()).not.toContain("delete_entry");
    expect(screen.getAllByTestId("main-entry-row")).toHaveLength(3);
  });

  it("clears the query on Ctrl+Backspace too", async () => {
    await renderPane();
    fireEvent.change(filterInput(), { target: { value: "entry" } });

    fireEvent.keyDown(filterInput(), { key: "Backspace", ctrlKey: true });
    await act(async () => {});

    expect(filterInput()).toHaveValue("");
    expect(commandsSent()).not.toContain("delete_entry");
  });

  // Bare, the key belongs to the query being typed, and the mock's bare
  // `Delete` binding was rejected over it.
  it("leaves the entry alone on a bare Backspace", async () => {
    await renderPane();

    fireEvent.keyDown(filterInput(), { key: "Backspace" });
    await act(async () => {});

    expect(commandsSent()).not.toContain("delete_entry");
    expect(screen.getAllByTestId("main-entry-row")).toHaveLength(3);
  });
});

describe("HistorySection — the list-end sentinel", () => {
  it("names the cache cap once the list is standing at it", async () => {
    historyByUser["u-a"] = bulkEntries(CACHE_CAP);
    await renderPane();

    expect(screen.getByTestId("list-end")).toHaveTextContent(`OLDEST OF ${CACHE_CAP} KEPT`);
  });

  // A user who has never hit the cap must never be told there is one.
  it("stays silent for a list nowhere near the cap", async () => {
    historyByUser["u-a"] = bulkEntries(9);
    await renderPane();

    expect(screen.queryByTestId("list-end")).toBeNull();
  });

  // A filtered list is short for a reason of the reader's own making, so the
  // cap is not what is hiding the rows.
  it("stays silent once a filter is what shortened the list", async () => {
    historyByUser["u-a"] = bulkEntries(CACHE_CAP);
    await renderPane();
    expect(screen.getByTestId("list-end")).toBeInTheDocument();

    fireEvent.change(filterInput(), { target: { value: "entry-1" } });

    expect(screen.getAllByTestId("main-entry-row")).toHaveLength(11);
    expect(screen.queryByTestId("list-end")).toBeNull();
  });
});

describe("HistorySection — origin", () => {
  it("names the origin device for an entry captured elsewhere", async () => {
    await renderPane();

    const rows = screen.getAllByTestId("main-entry-row");
    expect(rows[1]!).toHaveTextContent("IPHONE-15");
  });

  it("omits the origin for an entry captured on this device", async () => {
    await renderPane();

    const rows = screen.getAllByTestId("main-entry-row");
    // The fixture labels this device too, so an implementation that printed
    // Origin unconditionally would say MBP-14 here.
    expect(rows[0]!).not.toHaveTextContent("MBP-14");
    expect(rows[0]!).toHaveTextContent("2m");
  });

  /*
    "This device" is the *Viewed* Pairing's device, not the Active one's: a
    pairing is a User on a relay and this machine holds a separate device id
    under each. Reading u-b, an entry from dev-a is from somewhere else.
  */
  it("resolves this device against the Viewed Pairing, not the Active one", async () => {
    useUiStore.setState({ viewedUserId: "u-b" });
    await renderPane();

    const rows = screen.getAllByTestId("main-entry-row");
    expect(rows[0]!).toHaveTextContent("MBP-14");
    expect(rows[1]!).not.toHaveTextContent("PIXEL-9");
  });
});

describe("HistorySection — undecryptable entries", () => {
  const undecryptable: EntryView = {
    id: 99,
    user_id: "u-a",
    // As it arrives from the facade: no cached plaintext at all, which is both
    // an empty Preview and the flag. The flag is what the row must read.
    preview: "",
    plaintext: null,
    created_at: NOW - MINUTE,
    last_use: NOW - MINUTE,
    device_id: "dev-a",
    origin_label: "dev-",
    undecryptable: true,
  };

  beforeEach(() => {
    historyByUser["u-a"] = [undecryptable, ...entriesA];
  });

  it("marks the row KEY MISMATCH rather than rendering it blank", async () => {
    await renderPane();

    expect(screen.getAllByTestId("main-entry-row")[0]!).toHaveTextContent("KEY MISMATCH");
  });

  /*
    Disabled rather than hidden: the control the reader is looking for has to
    still be where they are looking, saying no. Deleting stays live, because
    ciphertext this device cannot read is exactly what someone wants gone.
  */
  it("disables COPY while leaving delete live", async () => {
    await renderPane();

    expect(screen.getByTestId("detail-copy")).toBeDisabled();
    fireEvent.click(screen.getByTestId("detail-copy"));
    await act(async () => {});
    expect(commandsSent()).not.toContain("copy_to_clipboard");

    fireEvent.click(screen.getByTestId("detail-delete-99"));
    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("delete_entry", {
        args: { user_id: "u-a", entry_id: 99 },
      }),
    );
  });
});

describe("HistorySection — the popover's handoff", () => {
  it("selects the entry the popover named and consumes the seed", async () => {
    useUiStore.setState({ seedEntryId: 13 });
    await renderPane();

    expect(selectedRowIndex()).toBe(2);
    expect(screen.getByTestId("entry-detail-body").textContent).toBe("charlie");
    expect(useUiStore.getState().seedEntryId).toBeUndefined();
  });

  /*
    Consumed once, not merely applied once: entry id 13 exists under both
    pairings, so a seed left set would re-fire on the next pairing the reader
    switches to and drag the selection somewhere they never asked for.
  */
  it("does not re-fire the seed on the next pairing switched to", async () => {
    useUiStore.setState({ seedEntryId: 13 });
    await renderPane();

    fireEvent.change(screen.getByTestId("viewed-pairing"), { target: { value: "u-b" } });
    await waitFor(() =>
      expect(screen.getAllByTestId("main-entry-row")[0]!).toHaveTextContent("from the laptop"),
    );

    expect(selectedRowIndex()).toBe(0);
  });

  // A stale id has to select nothing *and* clear itself: leaving `selectedIndex`
  // at the -1 that `findIndex` returns would leave the pane with no addressed
  // row and no way for the arrows to describe one.
  it("selects nothing for a stale seed and still clears it", async () => {
    useUiStore.setState({ seedEntryId: 4242 });
    await renderPane();

    expect(useUiStore.getState().seedEntryId).toBeUndefined();
    expect(selectedRowIndex()).toBe(0);

    fireEvent.keyDown(filterInput(), { key: "ArrowDown" });
    expect(selectedRowIndex()).toBe(1);
  });
});

describe("HistorySection — the oversize guard", () => {
  beforeEach(() => {
    // On `plaintext`: the cap guards what the reader lays out, and the row's
    // Preview is 80 characters whatever the entry weighs.
    historyByUser["u-a"] = [
      { ...entriesA[0]!, preview: "HEAD-AAAA", plaintext: OVERSIZE },
      entriesA[1]!,
      entriesA[2]!,
    ];
  });

  it("cuts an entry past the render cap and reveals the rest on request", async () => {
    await renderPane();

    expect(screen.getByTestId("entry-detail-body").textContent).toContain("HEAD-");
    expect(screen.getByTestId("entry-detail-body").textContent).not.toContain("-TAIL");

    fireEvent.click(screen.getByTestId("show-all"));

    expect(screen.getByTestId("entry-detail-body").textContent).toContain("-TAIL");
    expect(screen.queryByTestId("show-all")).toBeNull();
  });

  // Laying out megabytes is the cost the cap exists to avoid, so the reveal is
  // opt-in on every visit rather than remembered for the entry.
  it("re-arms the cut when the selection leaves the entry and comes back", async () => {
    await renderPane();
    fireEvent.click(screen.getByTestId("show-all"));
    expect(screen.getByTestId("entry-detail-body").textContent).toContain("-TAIL");

    const rows = screen.getAllByTestId("main-entry-row");
    fireEvent.click(rows[1]!);
    expect(screen.getByTestId("entry-detail-body").textContent).toBe("bravo");

    fireEvent.click(rows[0]!);
    expect(screen.getByTestId("entry-detail-body").textContent).not.toContain("-TAIL");
    expect(screen.getByTestId("show-all")).toBeInTheDocument();
  });
});

describe("HistorySection — empty states", () => {
  it("says this device holds no keys when there are no pairings", async () => {
    usePairingsStore.setState({ pairings: [], active: undefined });
    await renderPane();

    expect(screen.getByText("NO PAIRINGS ON THIS DEVICE")).toBeInTheDocument();
    expect(commandsSent()).not.toContain("list_history");
  });

  // The only thing to do from here is pair, and `pairing` is a route meaning
  // "the Pairings pane with the add-flow open" — two pieces of state, not one.
  it("routes into the Pairings pane with the add-flow open", async () => {
    usePairingsStore.setState({ pairings: [], active: undefined });
    await renderPane();

    fireEvent.click(screen.getByText("ADD A PAIRING"));

    expect(useUiStore.getState().mainSection).toBe("pairings");
    expect(useUiStore.getState().pairingFlowOpen).toBe(true);
  });

  it("says the history is empty when the pairing has no entries", async () => {
    historyByUser["u-a"] = [];
    await renderPane();

    expect(screen.getByText("HISTORY EMPTY")).toBeInTheDocument();
    expect(screen.queryByTestId("main-entry-row")).toBeNull();
  });

  it("distinguishes a filter that matches nothing, and offers a way out", async () => {
    await renderPane();

    fireEvent.change(filterInput(), { target: { value: "zzz" } });

    expect(screen.getByText("NO MATCHES")).toBeInTheDocument();
    expect(screen.getByText('Nothing matches "zzz"')).toBeInTheDocument();
    // Distinct from HISTORY EMPTY: the entries are still there, just hidden.
    expect(screen.queryByText("HISTORY EMPTY")).toBeNull();

    fireEvent.click(screen.getByText("CLEAR FILTER"));

    expect(screen.getAllByTestId("main-entry-row")).toHaveLength(3);
    expect(filterInput()).toHaveValue("");
  });
});
