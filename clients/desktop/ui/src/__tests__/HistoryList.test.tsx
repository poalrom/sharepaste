import { describe, it, expect, beforeEach, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { usePairingsStore, useHistoryStore, useUiStore } from "../store";
import type { EntryView } from "../types";
import HistoryList from "../views/HistoryList";

let ipc: MockIpc;
// Stubbed in test-setup.ts: jsdom has no scrollIntoView.
const scrollIntoView = vi.mocked(Element.prototype.scrollIntoView);

describe("HistoryList", () => {
  beforeEach(() => {
    scrollIntoView.mockClear();
    ipc = mockIpc();
    useUiStore.setState({ filter: "", selectedIndex: 0, mainSection: "history" });
    useHistoryStore.setState({ entries: [
      { id: 1, user_id: "u", preview: "Hello", plaintext: "Hello", created_at: 1, last_use: 1, device_id: "d", origin_label: "d", undecryptable: false, pending: false, refused_reason: null },
      { id: 2, user_id: "u", preview: "World", plaintext: "World", created_at: 2, last_use: 2, device_id: "d", origin_label: "d", undecryptable: false, pending: false, refused_reason: null },
    ]});
    usePairingsStore.setState({
      pairings: [{ user_id: "u", device_id: "d", label: "mac", server_url: "https://s", relay_host: "s", status: "Online", pending: 0, is_active: true }],
      active: "u",
    });
  });

  it("filters by what was typed", () => {
    useUiStore.setState({ filter: "world", selectedIndex: 0, mainSection: "history" });
    render(<HistoryList />);
    const rows = screen.getAllByTestId("entry-row");
    expect(rows).toHaveLength(1);
  });

  it("highlights the selected index", () => {
    useUiStore.setState({ filter: "", selectedIndex: 1, mainSection: "history" });
    render(<HistoryList />);
    const rows = screen.getAllByTestId("entry-row");
    expect(rows[1]!).toHaveAttribute("data-selected", "true");
  });

  it("scrolls the selected row into view without jumping when already visible", () => {
    render(<HistoryList />);
    scrollIntoView.mockClear();
    fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(useUiStore.getState().selectedIndex).toBe(1);
    expect(scrollIntoView).toHaveBeenCalledWith({ block: "nearest" });
  });

  // The list is short and the window is a picker: the oldest entry should be
  // one key away from the newest, not ten.
  it("wraps to the last row when Up is pressed on the first", () => {
    render(<HistoryList />);
    fireEvent.keyDown(window, { key: "ArrowUp" });
    expect(useUiStore.getState().selectedIndex).toBe(1);
  });

  it("wraps to the first row when Down is pressed on the last", () => {
    useUiStore.setState({ selectedIndex: 1 });
    render(<HistoryList />);
    fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(useUiStore.getState().selectedIndex).toBe(0);
  });

  // The window listener outlives the rendered rows, so arrowing on an empty
  // list must not leave selectedIndex as NaN.
  it("survives arrow keys with no rows to select", () => {
    useUiStore.setState({ filter: "matches nothing" });
    render(<HistoryList />);
    fireEvent.keyDown(window, { key: "ArrowUp" });
    expect(useUiStore.getState().selectedIndex).toBe(0);
  });

  it("ignores Enter when a button has focus so the button's own handler wins", async () => {
    render(<HistoryList />);
    const button = screen.getByTestId("delete-entry-1");
    button.focus();
    fireEvent.keyDown(button, { key: "Enter" });
    await waitFor(() => expect(ipc.invoke).not.toHaveBeenCalled());
  });

  it("still copies on Enter when nothing is focused", async () => {
    render(<HistoryList />);
    fireEvent.keyDown(window, { key: "Enter" });
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("copy_to_clipboard", { args: { user_id: "u", entry_id: 1 } });
      expect(ipc.invoke).toHaveBeenCalledWith("hide_popover", undefined);
    });
  });

  // Controls live only on the addressed row, so a mouse delete is always two
  // motions: point at the row, then hit its ✕. Pointing is what addresses it.
  const pointAtDeleteFor = (rowIndex: number, entryId: number) => {
    fireEvent.mouseMove(screen.getAllByTestId("entry-row")[rowIndex]!);
    fireEvent.click(screen.getByTestId(`delete-entry-${entryId}`));
  };

  it("deletes an entry without copying it and drops it from the store", async () => {
    render(<HistoryList />);
    pointAtDeleteFor(1, 2);
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("delete_entry", { args: { user_id: "u", entry_id: 2 } });
    });
    expect(ipc.invoke).not.toHaveBeenCalledWith("copy_to_clipboard", expect.anything());
    await waitFor(() => {
      expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([1]);
    });
    expect(screen.getAllByTestId("entry-row")).toHaveLength(1);
  });

  it("addresses the row the pointer moves over", () => {
    render(<HistoryList />);
    const rows = screen.getAllByTestId("entry-row");
    expect(rows[0]).toHaveAttribute("data-selected", "true");

    fireEvent.mouseMove(rows[1]!);
    expect(screen.getAllByTestId("entry-row")[1]).toHaveAttribute("data-selected", "true");
    expect(screen.getAllByTestId("entry-row")[0]).toHaveAttribute("data-selected", "false");
  });

  it("keeps the row in the list when delete fails", async () => {
    ipc.invoke.mockImplementation(async (command) => {
      if (command === "delete_entry") throw new Error("nope");
      return undefined as never;
    });
    const err = vi.spyOn(console, "error").mockImplementation(() => {});
    render(<HistoryList />);
    pointAtDeleteFor(1, 2);
    await waitFor(() => expect(err).toHaveBeenCalled());
    expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([1, 2]);
    err.mockRestore();
  });

  it("deletes the selected entry on ⇧⌘⌫", async () => {
    render(<HistoryList />);
    fireEvent.keyDown(window, { key: "Backspace", metaKey: true, shiftKey: true });
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("delete_entry", { args: { user_id: "u", entry_id: 1 } });
    });
    await waitFor(() => {
      expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([2]);
    });
  });

  // Unshifted the combination is the field's on both platforms — erase to the
  // start of the line on macOS, erase the previous word on Windows — so the
  // list hands it back to the query it was aimed at (ADR 0013).
  it("clears the query on ⌘⌫ instead of deleting", async () => {
    useUiStore.setState({ filter: "world", selectedIndex: 0, mainSection: "history" });
    render(<HistoryList />);
    fireEvent.keyDown(window, { key: "Backspace", metaKey: true });
    await waitFor(() => expect(useUiStore.getState().filter).toBe(""));
    expect(ipc.invoke).not.toHaveBeenCalledWith("delete_entry", expect.anything());
    expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([1, 2]);
  });

  // Filter holds the input focused essentially always, so an unmodified key
  // would delete the entry the user was only trying to edit the query for.
  it("does not delete on a bare Backspace", async () => {
    render(<HistoryList />);
    fireEvent.keyDown(window, { key: "Backspace" });
    await waitFor(() => expect(ipc.invoke).not.toHaveBeenCalled());
    expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([1, 2]);
  });
});

/**
 * The queue above the relay's history: an offline capture is a row from the
 * moment it is made, and the only row in this list with no relay stamp on it.
 */
describe("HistoryList — the un-flushed region", () => {
  /**
   * The relay stamped neither timestamp, because it has never seen this entry:
   * the row exists on the strength of the capture alone (ADR 0016).
   */
  const unflushed: EntryView = {
    id: 3, user_id: "u", preview: "offline copy", plaintext: "offline copy",
    created_at: 0, last_use: 0, device_id: "d", origin_label: "d",
    undecryptable: false, pending: true, refused_reason: null,
  };
  const settled: EntryView = {
    id: 2, user_id: "u", preview: "World", plaintext: "World", created_at: 2, last_use: 2,
    device_id: "d", origin_label: "d", undecryptable: false, pending: false, refused_reason: null,
  };

  // A relay stamp has to be measured against the clock the row renders with.
  const NOW = Date.now();
  const MINUTE = 60_000;
  const HOUR = 60 * MINUTE;

  beforeEach(() => {
    ipc = mockIpc();
    // Addressed row second, so the head reads as an unselected reader sees it:
    // a selected row's own controls would otherwise join the row's text. The tint
    // is unaffected either way — the amber wash wins over the selected one, and
    // selection keeps the cyan edge that is its own mark.
    useUiStore.setState({ filter: "", selectedIndex: 1, mainSection: "history" });
    useHistoryStore.setState({ entries: [unflushed, settled] });
    usePairingsStore.setState({
      pairings: [{ user_id: "u", device_id: "d", label: "mac", server_url: "https://s", relay_host: "s", status: "Online", pending: 0, is_active: true }],
      active: "u",
    });
  });

  /*
    Driven through `settle`, which is what `entry-settled` calls: the flush is
    the moment the tint has to retreat, and it must retreat without the list
    moving. Nothing reorders at a flush — the relay stamps a pending act exactly
    where the device already showed it — so a reordering here would be the list
    contradicting itself for no reason a reader could see.
  */
  it("drops the tint when the act settles, leaving the order alone", () => {
    render(<HistoryList />);
    expect(screen.getAllByTestId("entry-row")[0]).toHaveAttribute("data-pending", "true");
    const before = screen.getAllByTestId("entry-row").map((r) => r.textContent);

    act(() => useHistoryStore.getState().settle(3, NOW - 30 * MINUTE, NOW - 30 * MINUTE));

    expect(screen.getAllByTestId("entry-row")[0]).toHaveAttribute("data-pending", "false");
    expect(screen.getAllByTestId("entry-row").map((r) => r.textContent)).toEqual(before.map(
      (t) => t === "01offline copy" ? "01offline copy30m" : t,
    ));
  });

  /*
    The lie this effort removed, told in the other direction. `EntrySettled`
    carries the relay's own `created_at` and `last_use`, so the moment the tint
    retreats the row has an age; a settled row with an empty slot would be saying
    the relay has never stamped it, on a row the relay has just stamped. Nothing
    else provokes a refetch, so it would say so for as long as the window stayed
    open. `relativeAge(0)` prints 655mo, which is why the slot cannot simply
    render whatever it holds.
  */
  it("fills the slot from the relay's stamp the moment the act settles", () => {
    render(<HistoryList />);
    expect(screen.getAllByTestId("entry-row")[0]!.textContent).toBe("01offline copy");

    act(() => useHistoryStore.getState().settle(3, NOW - 6 * HOUR, NOW - 6 * HOUR));

    const row = screen.getAllByTestId("entry-row")[0]!;
    expect(row.textContent).toBe("01offline copy6h");
    expect(row.textContent).not.toContain("655mo");
  });

  // The row carries plaintext from capture, so the predicate has something to
  // match long before the relay knows the entry exists.
  it("finds an un-flushed capture by its text", () => {
    useUiStore.setState({ filter: "offline", selectedIndex: 0, mainSection: "history" });
    render(<HistoryList />);
    const rows = screen.getAllByTestId("entry-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveTextContent("offline copy");
  });

  /*
    The sentinel is a statement about *retention*: the hundred rows the relay has
    ordered. The caps no longer bound the un-flushed region at all (ADR 0014), so
    counting every row would name a cap that has not bitten — which is exactly what
    the sentinel exists so a person with nine entries never sees.
  */
  it("names the cache cap over the settled rows alone", () => {
    const settledRows = Array.from({ length: 100 }, (_, i) => ({
      ...settled,
      id: 1_000 + i,
      preview: `kept ${i}`,
      plaintext: `kept ${i}`,
    }));
    useUiStore.setState({ filter: "", selectedIndex: 0, mainSection: "history" });

    // A page of un-flushed captures says nothing, however full it is.
    useHistoryStore.setState({
      entries: settledRows.map((e) => ({ ...e, created_at: 0, last_use: 0, pending: true })),
    });
    const view = render(<HistoryList />);
    expect(screen.queryByText(/OLDEST OF/)).toBeNull();
    view.unmount();

    // The same hundred, settled, is the cap actually biting.
    useHistoryStore.setState({ entries: settledRows });
    render(<HistoryList />);
    expect(screen.getByText(/OLDEST OF 100 CACHED/)).toBeInTheDocument();
  });
});
