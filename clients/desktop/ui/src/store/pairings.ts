import { create } from "zustand";
import type { Pairing, ConnectionState } from "../types";

export type PairingsState = {
  pairings: Pairing[];
  /** The **Active Pairing**: the one this device syncs and captures to. */
  active: string | undefined;
  hydrate: (rows: Pairing[]) => void;
  upsert: (p: Pairing) => void;
  remove: (user_id: string) => void;
  setActive: (user_id: string | undefined) => void;
  updateStatus: (user_id: string, status: ConnectionState) => void;
};

export const usePairingsStore = create<PairingsState>((set) => ({
  pairings: [],
  active: undefined,
  hydrate: (rows) =>
    set({
      pairings: rows,
      active: rows.find((p) => p.is_active)?.user_id,
    }),
  upsert: (p) =>
    set((s) => {
      const without = s.pairings.filter((x) => x.user_id !== p.user_id);
      return { pairings: [...without, p] };
    }),
  remove: (uid) =>
    set((s) => ({
      pairings: s.pairings.filter((p) => p.user_id !== uid),
      active: s.active === uid ? undefined : s.active,
    })),
  setActive: (active) =>
    set((s) => ({
      active,
      pairings: s.pairings.map((p) => ({ ...p, is_active: p.user_id === active })),
    })),
  updateStatus: (user_id, status) =>
    set((s) => ({
      pairings: s.pairings.map((p) =>
        p.user_id === user_id ? { ...p, status } : p,
      ),
    })),
}));

/**
 * The Active Pairing, or undefined when none is active.
 *
 * The store owns both `active` and `pairings`; rebuilding the join in each view
 * is how two views come to disagree about which Pairing they are describing.
 */
export function useActivePairing(): Pairing | undefined {
  return usePairingsStore((s) => s.pairings.find((p) => p.user_id === s.active));
}
