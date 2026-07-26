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
 * Both the list and the search field's count suffix have to agree on this, and
 * a second copy of the predicate is how they would stop agreeing. Matching runs
 * against the raw plaintext rather than the normalised preview, so a query can
 * still find a word that only appears on an entry's third line.
 */
export function useFilteredEntries(): EntryView[] {
  const entries = useHistoryStore((s) => s.entries);
  const search = useUiStore((s) => s.search);
  return useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (!needle) return entries;
    return entries.filter((e) => e.preview.toLowerCase().includes(needle));
  }, [entries, search]);
}

function dedupePrepend(existing: EntryView[], next: EntryView): EntryView[] {
  const without = existing.filter((e) => e.id !== next.id);
  return [next, ...without].slice(0, 100);
}
