import { describe, it, expect, beforeEach, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { useAccountsStore, useHistoryStore, useUiStore } from "../store";
import HistoryList from "../views/HistoryList";

let ipc: MockIpc;
// Stubbed in test-setup.ts: jsdom has no scrollIntoView.
const scrollIntoView = vi.mocked(Element.prototype.scrollIntoView);

describe("HistoryList", () => {
  beforeEach(() => {
    scrollIntoView.mockClear();
    ipc = mockIpc();
    useUiStore.setState({ search: "", selectedIndex: 0, mainSection: "accounts" });
    useHistoryStore.setState({ entries: [
      { id: 1, user_id: "u", preview: "Hello", created_at: 1, device_id: "d" },
      { id: 2, user_id: "u", preview: "World", created_at: 2, device_id: "d" },
    ]});
    useAccountsStore.setState({
      accounts: [{ user_id: "u", device_id: "d", label: "mac", server_url: "https://s", status: "Online", pending: 0, is_active: true }],
      active: "u",
    });
  });

  it("filters by search term", () => {
    useUiStore.setState({ search: "world", selectedIndex: 0, mainSection: "accounts" });
    render(<HistoryList />);
    const rows = screen.getAllByTestId("entry-row");
    expect(rows).toHaveLength(1);
  });

  it("highlights the selected index", () => {
    useUiStore.setState({ search: "", selectedIndex: 1, mainSection: "accounts" });
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

  it("deletes an entry without copying it and drops it from the store", async () => {
    render(<HistoryList />);
    fireEvent.click(screen.getByTestId("delete-entry-2"));
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("delete_entry", { args: { user_id: "u", entry_id: 2 } });
    });
    expect(ipc.invoke).not.toHaveBeenCalledWith("copy_to_clipboard", expect.anything());
    await waitFor(() => {
      expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([1]);
    });
    expect(screen.getAllByTestId("entry-row")).toHaveLength(1);
  });

  it("keeps the row in the list when delete fails", async () => {
    ipc.invoke.mockImplementation(async (command) => {
      if (command === "delete_entry") throw new Error("nope");
      return undefined as never;
    });
    const err = vi.spyOn(console, "error").mockImplementation(() => {});
    render(<HistoryList />);
    fireEvent.click(screen.getByTestId("delete-entry-2"));
    await waitFor(() => expect(err).toHaveBeenCalled());
    expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([1, 2]);
    err.mockRestore();
  });
});
