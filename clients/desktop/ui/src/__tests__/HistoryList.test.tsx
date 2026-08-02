import { describe, it, expect, beforeEach, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { usePairingsStore, useHistoryStore, useUiStore } from "../store";
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
      { id: 1, user_id: "u", preview: "Hello", plaintext: "Hello", created_at: 1, last_use: 1, device_id: "d", origin_label: "d", undecryptable: false },
      { id: 2, user_id: "u", preview: "World", plaintext: "World", created_at: 2, last_use: 2, device_id: "d", origin_label: "d", undecryptable: false },
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

  it("deletes the selected entry on ⌘⌫", async () => {
    render(<HistoryList />);
    fireEvent.keyDown(window, { key: "Backspace", metaKey: true });
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("delete_entry", { args: { user_id: "u", entry_id: 1 } });
    });
    await waitFor(() => {
      expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([2]);
    });
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
