import { useMemo } from "react";
import { create } from "zustand";
import type { EntryView } from "../types";
import { useUiStore } from "./ui";

export type HistoryState = {
  entries: EntryView[];
  hydrate: (rows: EntryView[]) => void;
  add: (e: EntryView) => void;
  remove: (id: number) => void;
  clear: () => void;
};

export const useHistoryStore = create<HistoryState>((set) => ({
  entries: [],
  hydrate: (rows) => set({ entries: rows }),
  add: (entry) => set((s) => ({ entries: dedupePrepend(s.entries, entry) })),
  remove: (id) => set((s) => ({ entries: s.entries.filter((e) => e.id !== id) })),
  clear: () => set({ entries: [] }),
}));

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

function dedupePrepend(existing: EntryView[], next: EntryView): EntryView[] {
  const without = existing.filter((e) => e.id !== next.id);
  return [next, ...without].slice(0, 100);
}
