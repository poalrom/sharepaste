import { describe, it, expect, beforeEach } from "vitest";
import { hydrateFrom, noteChange, useHistoryStore } from "../store/history";
import type { EntryView } from "../types";
import { usePairingsStore } from "../store/pairings";
import { useUiStore } from "../store/ui";

describe("history store", () => {
  beforeEach(() => useHistoryStore.setState({ entries: [] }));

  it("add prepends and dedupes by id", () => {
    const { add } = useHistoryStore.getState();
    add({ id: 1, user_id: "u", preview: "a", plaintext: "a", created_at: 1, last_use: 1, device_id: "d", origin_label: "d", undecryptable: false, pending: false, refused_reason: null });
    add({ id: 2, user_id: "u", preview: "b", plaintext: "b", created_at: 2, last_use: 2, device_id: "d", origin_label: "d", undecryptable: false, pending: false, refused_reason: null });
    add({ id: 1, user_id: "u", preview: "a-new", plaintext: "a-new", created_at: 3, last_use: 3, device_id: "d", origin_label: "d", undecryptable: false, pending: false, refused_reason: null });
    const state = useHistoryStore.getState();
    expect(state.entries.map((e) => e.id)).toEqual([1, 2]);
    expect(state.entries[0]?.preview).toBe("a-new");
  });

  it("remove filters by id", () => {
    const { add, remove } = useHistoryStore.getState();
    add({ id: 1, user_id: "u", preview: "a", plaintext: "a", created_at: 1, last_use: 1, device_id: "d", origin_label: "d", undecryptable: false, pending: false, refused_reason: null });
    remove(1);
    expect(useHistoryStore.getState().entries.length).toBe(0);
  });

  const row = (id: number, user_id = "u"): EntryView => ({
    id, user_id, preview: `p${id}`, plaintext: `p${id}`, created_at: id,
    last_use: id, device_id: "d", origin_label: "d", undecryptable: false, pending: false, refused_reason: null,
  });

  /*
   * The store's own cap is a cache of a cache, and it spares un-flushed rows for
   * the reason `entries_cache::prune` does: an act this device has not delivered
   * is undelivered clipboard content, and dropping one to keep a number down is
   * the trade ADR 0014 refuses. Counting every row would silently truncate the
   * display of an offline burst past a hundred — and the list-end sentinel is
   * counted the same way, so nothing would even say so.
   */
  it("the cap counts the settled rows and spares the un-flushed ones", () => {
    const { add } = useHistoryStore.getState();
    for (let i = 1; i <= 100; i += 1) add(row(i));
    expect(useHistoryStore.getState().entries).toHaveLength(100);

    // One more the relay has ordered: the oldest of the hundred goes.
    add(row(101));
    const settled = useHistoryStore.getState().entries;
    expect(settled).toHaveLength(100);
    expect(settled.map((e) => e.id)).not.toContain(1);

    // And fifty offline captures on top of a full cache are fifty more rows.
    for (let i = 200; i < 250; i += 1) {
      add({ ...row(i), created_at: 0, last_use: 0, pending: true });
    }
    const both = useHistoryStore.getState().entries;
    expect(both).toHaveLength(150);
    expect(both.filter((e) => e.pending)).toHaveLength(50);
    expect(both.filter((e) => !e.pending)).toHaveLength(100);
    expect(both[0]?.id, "the newest act still leads").toBe(249);
  });

  /*
   * Anomaly A of `.scratch/mobile-client/issues/06`: an offline burst flushed,
   * the relay took every row, and one of them was on screen afterwards. Each
   * surface subscribed to `entry-added` after its first `list_history` had
   * answered, so an Entry cached in between was announced to nobody — and a
   * snapshot older than the announcement must not undo it either.
   */
  it("hydrateFrom replays a change noted while the snapshot was in flight", async () => {
    let release: (rows: EntryView[]) => void = () => {};
    const held = new Promise<EntryView[]>((resolve) => {
      release = resolve;
    });
    const done = hydrateFrom("u", () => held);
    noteChange({ kind: "added", user_id: "u", entry: row(3) });
    release([row(1), row(2)]);
    await done;
    expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([3, 1, 2]);
  });

  it("hydrateFrom replays only the Pairing the snapshot was for", async () => {
    const done = hydrateFrom("u", async () => [row(1)]);
    noteChange({ kind: "added", user_id: "other", entry: row(9, "other") });
    await done;
    expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([1]);
  });

  it("hydrateFrom drops a snapshot for a Pairing no longer on screen", async () => {
    useHistoryStore.getState().hydrate([row(5)]);
    const applied = await hydrateFrom("u", async () => [row(1)], () => true);
    expect(applied).toBeUndefined();
    expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([5]);
  });

  it("a change noted with no snapshot in flight is not replayed later", async () => {
    noteChange({ kind: "added", user_id: "u", entry: row(7) });
    await hydrateFrom("u", async () => [row(1)]);
    expect(useHistoryStore.getState().entries.map((e) => e.id)).toEqual([1]);
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
    useUiStore.setState({ filter: "ss://", selectedIndex: 4 });
    useUiStore.getState().setMainSection("history");
    expect(useUiStore.getState().filter).toBe("");
    expect(useUiStore.getState().selectedIndex).toBe(0);

    useUiStore.setState({ filter: "npm", selectedIndex: 2 });
    useUiStore.getState().setViewedUserId("u-other");
    expect(useUiStore.getState().viewedUserId).toBe("u-other");
    expect(useUiStore.getState().filter).toBe("");
    expect(useUiStore.getState().selectedIndex).toBe(0);
  });
});
