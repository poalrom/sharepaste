import { describe, it, expect, beforeEach } from "vitest";
import { useHistoryStore } from "../store/history";
import { useAccountsStore } from "../store/accounts";
import { useUiStore } from "../store/ui";

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

  it("hydrate sets active to the row flagged is_active", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Online", pending: 0, is_active: true },
      { user_id: "b", device_id: "d", label: "y", server_url: "https://s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    expect(useAccountsStore.getState().active).toBe("a");
  });

  it("hydrate leaves active undefined when no row is flagged", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    expect(useAccountsStore.getState().active).toBeUndefined();
  });

  it("removing a non-active account leaves active alone", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Online", pending: 0, is_active: true },
      { user_id: "b", device_id: "d", label: "y", server_url: "https://s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    useAccountsStore.getState().remove("b");
    expect(useAccountsStore.getState().active).toBe("a");
  });

  it("removing the active account clears active and waits for backend", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Online", pending: 0, is_active: true },
      { user_id: "b", device_id: "d", label: "y", server_url: "https://s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    useAccountsStore.getState().remove("a");
    expect(useAccountsStore.getState().active).toBeUndefined();
  });
});

describe("useUiStore mainSection", () => {
  it("defaults to 'accounts'", () => {
    expect(useUiStore.getState().mainSection).toBe("accounts");
  });

  it("setMainSection updates the field", () => {
    useUiStore.getState().setMainSection("pairing");
    expect(useUiStore.getState().mainSection).toBe("pairing");
  });
});
