import { describe, it, expect, beforeEach } from "vitest";
import { useHistoryStore } from "../store/history";
import { useAccountsStore } from "../store/accounts";

describe("history store", () => {
  beforeEach(() => useHistoryStore.setState({ entries: [] }));

  it("add prepends and dedupes by id", () => {
    const { add } = useHistoryStore.getState();
    add({ id: 1, user_id: "u", preview: "a", created_at: 1, device_id: "d" });
    add({ id: 2, user_id: "u", preview: "b", created_at: 2, device_id: "d" });
    add({ id: 1, user_id: "u", preview: "a-new", created_at: 3, device_id: "d" });
    const state = useHistoryStore.getState();
    expect(state.entries.map((e) => e.id)).toEqual([1, 2]);
    expect(state.entries[0]?.preview).toBe("a-new");
  });

  it("remove filters by id", () => {
    const { add, remove } = useHistoryStore.getState();
    add({ id: 1, user_id: "u", preview: "a", created_at: 1, device_id: "d" });
    remove(1);
    expect(useHistoryStore.getState().entries.length).toBe(0);
  });
});

describe("accounts store", () => {
  beforeEach(() => useAccountsStore.setState({ accounts: [], active: undefined }));

  it("hydrate sets active to first row", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Online", pending: 0 },
    ]);
    expect(useAccountsStore.getState().active).toBe("a");
  });

  it("hydrate prefers the backend-active account over the first disconnected row", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "oldest", device_id: "d1", label: "Oldest", server_url: "https://s", status: "Disconnected", pending: 0 },
      { user_id: "active", device_id: "d2", label: "Active", server_url: "https://s", status: "Connecting", pending: 0 },
    ]);
    expect(useAccountsStore.getState().active).toBe("active");
  });

  it("removing active falls back to next account", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Online", pending: 0 },
      { user_id: "b", device_id: "d", label: "y", server_url: "https://s", status: "Online", pending: 0 },
    ]);
    useAccountsStore.getState().remove("a");
    expect(useAccountsStore.getState().active).toBe("b");
  });
});
