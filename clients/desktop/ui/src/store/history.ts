import { useMemo } from "react";
import { create } from "zustand";
import type { EntryView } from "../types";
import { useUiStore } from "./ui";

/**
 * How many rows the relay has ordered that this store keeps.
 *
 * **Owned by `sharepaste_core::storage::history::MAX_PER_USER`, and copied here
 * only because a compile-time constant cannot ride an IPC call.** That
 * declaration is the one owner; `sharepaste_core::facade::MAX_PER_USER` is a
 * re-export of it, and it is the declaration — not the re-export — that
 * `store.test.ts` reads, so a copy that stops agreeing goes red. Everything else
 * this store knows arrives from the core at runtime; a number the surfaces need
 * before any answer comes back cannot. The same trick
 * `.github/scripts/check-versions.mjs` uses to hold the manifests to one version
 * rather than trusting them to stay level.
 *
 * It is enforced here for the reason the core enforces it: the store is a cache
 * of a cache. Both surfaces draw a sentinel naming it, through [`atHistoryCap`].
 */
export const HISTORY_CAP = 100;

/**
 * Which rows the cap is about: the ones the relay has ordered.
 *
 * **Not the un-flushed ones, and this is the only place that says so.** The cap
 * bounds a cache of what the relay holds; nothing bounds the acts this device
 * has not delivered yet, because an act this device has not delivered is
 * undelivered clipboard content and evicting one to protect a display invariant
 * is the trade ADR 0014 refuses — `history::prune` exempts exactly the same rows
 * for exactly that reason.
 *
 * So an offline burst is kept whole however long it runs, and the sentinel stays
 * down while it does. Counting every row would truncate the burst *and* announce
 * a limit that never bit, which is the one thing the sentinel exists to avoid.
 */
const counted = (e: EntryView): boolean => !e.pending;

/**
 * Whether the cap has bitten — what both surfaces draw their list-end sentinel
 * on, so that neither has to restate [`counted`].
 */
export const atHistoryCap = (entries: EntryView[]): boolean =>
  entries.filter(counted).length >= HISTORY_CAP;

export type HistoryState = {
  entries: EntryView[];
  hydrate: (rows: EntryView[]) => void;
  /**
   * A row the core has just announced, at the head of the list.
   *
   * The head is not a rule this store derives. `EntryAdded` is the core saying
   * an act happened, and `HistoryChanged` follows it with the page the core has
   * ordered — so this position is the one the next snapshot restates, written
   * here only so the row is not somewhere else for the width of that round
   * trip. Any older copy of the same id goes, because the announcement is the
   * newer account of it.
   */
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
  add: (entry) =>
    set((s) => ({ entries: withinCap([entry, ...s.entries.filter((e) => e.id !== entry.id)]) })),
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
 * Half of the fix for anomaly A of `.scratch/mobile-client/issues/06`, whose
 * other half — subscribing to the four entry events before the first snapshot
 * is asked for — is [`attachHistory`]'s, and is written up there. Subscribing
 * first is not sufficient on its own: a snapshot requested before a change and
 * applied after it would undo the change it never saw.
 *
 * So every subscription that mutates this store calls this before mutating it,
 * and [`hydrateFrom`] replays what it recorded.
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
 * The list with the oldest rows over [`HISTORY_CAP`] dropped, exactly as
 * `history::prune` drops them: oldest first, and only among the rows
 * [`counted`] admits.
 *
 * On `add` alone. A snapshot arrives already pruned by the module that owns the
 * cap, and nothing else here grows the list.
 */
function withinCap(rows: EntryView[]): EntryView[] {
  let kept = 0;
  return rows.filter((e) => !counted(e) || ++kept <= HISTORY_CAP);
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
