import { useMemo } from "react";
import { create } from "zustand";
import type { EntryView } from "../types";
import { useUiStore } from "./ui";

/**
 * How many rows the relay has ordered that this store keeps.
 *
 * The same hundred `entries_cache` prunes at, and enforced here for the same
 * reason it is there: the store is a cache of a cache. Both surfaces name it too,
 * because both draw a sentinel when it bites.
 */
const CACHE_CAP = 100;

export type HistoryState = {
  entries: EntryView[];
  hydrate: (rows: EntryView[]) => void;
  add: (e: EntryView) => void;
  remove: (id: number) => void;
  /**
   * One act reached the relay, so its row has stopped waiting.
   *
   * In place and by id, with no refetch: nothing reorders at a flush — the relay
   * stamps a pending act exactly where the device already showed it — and the id
   * does not change either, so the reader's selection and the popover's keyboard
   * cursor stay where they were.
   *
   * The relay's numbers arrive with the event and are written here, because a row
   * is drawn from what the relay last said about it: dropping the tint while
   * leaving `created_at` at zero would leave the row's time slot empty and the
   * reader pane still reading `WAITING FOR THE RELAY`, on a row that is settled.
   *
   * `null` is "the relay said nothing about this number" and leaves it alone — a
   * **Use** does not restamp a creation, and a queued use of an entry the relay
   * has since dropped stamps neither, yet its act still left the queue.
   */
  settle: (id: number, createdAt: number | null, lastUse: number | null) => void;
  /** The relay turned an act down for what it is. Same rules as `settle`. */
  refuse: (id: number, reason: string) => void;
  clear: () => void;
};

export const useHistoryStore = create<HistoryState>((set) => ({
  entries: [],
  hydrate: (rows) => set({ entries: rows }),
  add: (entry) => set((s) => ({ entries: dedupePrepend(s.entries, entry) })),
  remove: (id) => set((s) => ({ entries: s.entries.filter((e) => e.id !== id) })),
  settle: (id, createdAt, lastUse) =>
    set((s) => ({
      entries: patch(s.entries, id, {
        pending: false,
        refused_reason: null,
        ...(createdAt === null ? {} : { created_at: createdAt }),
        ...(lastUse === null ? {} : { last_use: lastUse }),
      }),
    })),
  refuse: (id, reason) =>
    set((s) => ({
      entries: patch(s.entries, id, { pending: true, refused_reason: reason }),
    })),
  clear: () => set({ entries: [] }),
}));

/**
 * One incremental change to the History, kept only long enough to survive a
 * snapshot that was already on the wire when it happened.
 */
type Change =
  | { kind: "added"; user_id: string; entry: EntryView }
  | { kind: "deleted"; user_id: string; entry_id: number }
  | { kind: "settled"; user_id: string; entry_id: number; created_at: number | null; last_use: number | null }
  | { kind: "refused"; user_id: string; entry_id: number; reason: string };

/** The replay log of every snapshot currently in flight. */
const inFlight = new Set<Change[]>();

/**
 * Note an incremental change, so no snapshot can silently roll it back.
 *
 * **The defect this exists for**, reproduced twice on a Windows smoke run and
 * recorded as anomaly A of `.scratch/mobile-client/issues/06`: an offline burst
 * flushed, the relay gained every row, and one of them was on screen
 * afterwards. Each surface subscribed to `entry-added` only *after* its first
 * `list_history` had answered, so an Entry the uploader cached in between was
 * announced to a listener that did not exist and absent from the snapshot. It
 * stayed lost, because nothing later provokes a refetch: the relay's echo of an
 * Entry this device uploaded deliberately raises neither event, and a backfill
 * that ingests only rows the cache already holds does not advance the watermark
 * and so raises none either.
 *
 * Subscribing before the first snapshot is half the fix and not the whole of
 * it — a snapshot requested before a change and applied after it would undo it.
 * Every subscription that mutates this store calls this as well, and
 * [`hydrateFrom`] replays what it recorded.
 */
export function noteChange(change: Change): void {
  for (const log of inFlight) log.push(change);
}

/**
 * Replace the list with a fresh snapshot of one Pairing's History, then re-apply
 * whatever [`noteChange`] recorded while it was on the wire.
 *
 * Returns the rows applied, or `undefined` when `stale` says the answer is for
 * a Pairing no longer on screen — `HistoryList` keys nothing per user, so a slow
 * response for the pairing just left must not land on top of the one now shown.
 *
 * Replayed changes are filtered by `userId`: the same stream carries every
 * Pairing, and only this one's belongs in this snapshot.
 */
export async function hydrateFrom(
  userId: string,
  fetch: () => Promise<EntryView[]>,
  stale?: () => boolean,
): Promise<EntryView[] | undefined> {
  const log: Change[] = [];
  inFlight.add(log);
  let rows: EntryView[];
  try {
    rows = await fetch();
  } finally {
    // Before the replay below, or an `add` would record onto the very log it is
    // being read from.
    inFlight.delete(log);
  }
  if (stale?.()) return undefined;
  const store = useHistoryStore.getState();
  store.hydrate(rows);
  for (const change of log) {
    if (change.user_id !== userId) continue;
    if (change.kind === "added") store.add(change.entry);
    else if (change.kind === "deleted") store.remove(change.entry_id);
    else if (change.kind === "settled") store.settle(change.entry_id, change.created_at, change.last_use);
    else store.refuse(change.entry_id, change.reason);
  }
  return rows;
}

/**
 * The rows the popover is actually showing.
 *
 * Both the list and the Filter field's count suffix have to agree on this, and
 * a second copy of the predicate is how they would stop agreeing. Matching runs
 * against `plaintext` rather than the one-line `preview`, so a query still
 * finds a word that only appears on an entry's third line. An Undecryptable
 * entry has no plaintext and matches nothing, which is the truth about it.
 */
export function useFilteredEntries(): EntryView[] {
  const entries = useHistoryStore((s) => s.entries);
  const filter = useUiStore((s) => s.filter);
  return useMemo(() => {
    const needle = filter.trim().toLowerCase();
    if (!needle) return entries;
    return entries.filter((e) => e.plaintext?.toLowerCase().includes(needle) ?? false);
  }, [entries, filter]);
}

/**
 * Prepend one entry, dropping any older copy of it, and keep the list inside the
 * cache cap.
 *
 * **The cap spares un-flushed rows**, exactly as `entries_cache::prune` does. It
 * bounds a cache of what the relay has ordered; an act this device has not
 * delivered is undelivered clipboard content, and dropping one to keep a number
 * down is the trade ADR 0014 refuses. Counting every row would silently truncate
 * the display of an offline burst past a hundred — and the list-end sentinel is
 * counted the same way, so nothing would even say so.
 */
function dedupePrepend(existing: EntryView[], next: EntryView): EntryView[] {
  const without = existing.filter((e) => e.id !== next.id);
  const rows = [next, ...without];
  let settled = 0;
  return rows.filter((e) => e.pending || ++settled <= CACHE_CAP);
}

/**
 * Change what one row says about itself, leaving its place alone.
 *
 * A new array, because zustand compares by reference, and a new object only for
 * the row that moved — every other row keeps its identity so React re-renders
 * one `<li>` rather than a hundred. A row this list has never heard of is
 * ignored: the events carry every Pairing, and the surfaces filter by the one on
 * screen.
 */
function patch(existing: EntryView[], id: number, fields: Partial<EntryView>): EntryView[] {
  if (!existing.some((e) => e.id === id)) return existing;
  return existing.map((e) => (e.id === id ? { ...e, ...fields } : e));
}
