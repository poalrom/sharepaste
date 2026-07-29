import { describe, it, expect, beforeEach } from "vitest";
import { useHistoryStore } from "../store/history";
import { usePairingsStore } from "../store/pairings";
import { useUiStore } from "../store/ui";

describe("history store", () => {
  beforeEach(() => useHistoryStore.setState({ entries: [] }));

  it("add prepends and dedupes by id", () => {
    const { add } = useHistoryStore.getState();
    add({ id: 1, user_id: "u", preview: "a", plaintext: "a", created_at: 1, device_id: "d", origin_label: "d", undecryptable: false });
    add({ id: 2, user_id: "u", preview: "b", plaintext: "b", created_at: 2, device_id: "d", origin_label: "d", undecryptable: false });
    add({ id: 1, user_id: "u", preview: "a-new", plaintext: "a-new", created_at: 3, device_id: "d", origin_label: "d", undecryptable: false });
    const state = useHistoryStore.getState();
    expect(state.entries.map((e) => e.id)).toEqual([1, 2]);
    expect(state.entries[0]?.preview).toBe("a-new");
  });

  it("remove filters by id", () => {
    const { add, remove } = useHistoryStore.getState();
    add({ id: 1, user_id: "u", preview: "a", plaintext: "a", created_at: 1, device_id: "d", origin_label: "d", undecryptable: false });
    remove(1);
    expect(useHistoryStore.getState().entries.length).toBe(0);
  });
});

describe("pairings store", () => {
  beforeEach(() => usePairingsStore.setState({ pairings: [], active: undefined }));

  it("hydrate sets active to the row flagged is_active", () => {
    usePairingsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", relay_host: "s", status: "Online", pending: 0, is_active: true },
      { user_id: "b", device_id: "d", label: "y", server_url: "https://s", relay_host: "s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    expect(usePairingsStore.getState().active).toBe("a");
  });

  it("hydrate leaves active undefined when no row is flagged", () => {
    usePairingsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", relay_host: "s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    expect(usePairingsStore.getState().active).toBeUndefined();
  });

  it("removing a non-active pairing leaves active alone", () => {
    usePairingsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", relay_host: "s", status: "Online", pending: 0, is_active: true },
      { user_id: "b", device_id: "d", label: "y", server_url: "https://s", relay_host: "s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    usePairingsStore.getState().remove("b");
    expect(usePairingsStore.getState().active).toBe("a");
  });

  it("removing the Active Pairing clears active and waits for backend", () => {
    usePairingsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", relay_host: "s", status: "Online", pending: 0, is_active: true },
      { user_id: "b", device_id: "d", label: "y", server_url: "https://s", relay_host: "s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    usePairingsStore.getState().remove("a");
    expect(usePairingsStore.getState().active).toBeUndefined();
  });
});

describe("useUiStore mainSection", () => {
  it("defaults to 'history'", () => {
    expect(useUiStore.getState().mainSection).toBe("history");
  });

  it("setMainSection updates the field", () => {
    useUiStore.getState().setMainSection("settings");
    expect(useUiStore.getState().mainSection).toBe("settings");
  });

  // A query left over from the last visit would silently hide rows the
  // returning reader expects to see, and the selection it indexed is gone.
  it("switching pane or Viewed Pairing drops the filter and the selection", () => {
    useUiStore.setState({ search: "ss://", selectedIndex: 4 });
    useUiStore.getState().setMainSection("history");
    expect(useUiStore.getState().search).toBe("");
    expect(useUiStore.getState().selectedIndex).toBe(0);

    useUiStore.setState({ search: "npm", selectedIndex: 2 });
    useUiStore.getState().setViewedUserId("u-other");
    expect(useUiStore.getState().viewedUserId).toBe("u-other");
    expect(useUiStore.getState().search).toBe("");
    expect(useUiStore.getState().selectedIndex).toBe(0);
  });
});
