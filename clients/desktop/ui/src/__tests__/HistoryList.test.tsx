import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { useAccountsStore, useHistoryStore, useUiStore } from "../store";
import HistoryList from "../views/HistoryList";

describe("HistoryList", () => {
  beforeEach(() => {
    useUiStore.setState({ search: "", selectedIndex: 0, modal: null });
    useHistoryStore.setState({ entries: [
      { id: 1, user_id: "u", preview: "Hello", created_at: 1, device_id: "d" },
      { id: 2, user_id: "u", preview: "World", created_at: 2, device_id: "d" },
    ]});
    useAccountsStore.setState({
      accounts: [{ user_id: "u", device_id: "d", label: "mac", server_url: "https://s", status: "Online", pending: 0 }],
      active: "u",
    });
  });

  it("renders rows newest first", () => {
    render(<HistoryList />);
    const rows = screen.getAllByTestId("entry-row");
    expect(rows).toHaveLength(2);
  });

  it("filters by search term", () => {
    useUiStore.setState({ search: "world", selectedIndex: 0, modal: null });
    render(<HistoryList />);
    const rows = screen.getAllByTestId("entry-row");
    expect(rows).toHaveLength(1);
  });

  it("highlights the selected index", () => {
    useUiStore.setState({ search: "", selectedIndex: 1, modal: null });
    render(<HistoryList />);
    const rows = screen.getAllByTestId("entry-row");
    expect(rows[1]!).toHaveAttribute("data-selected", "true");
  });
});
