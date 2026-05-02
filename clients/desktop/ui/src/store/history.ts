import { create } from "zustand";
import type { EntryView } from "../types";

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

function dedupePrepend(existing: EntryView[], next: EntryView): EntryView[] {
  const without = existing.filter((e) => e.id !== next.id);
  return [next, ...without].slice(0, 100);
}
