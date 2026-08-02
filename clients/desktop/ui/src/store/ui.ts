import { create } from "zustand";
import type { Tone } from "../views/fui/tone";

/**
 * The panes on the main window's rail.
 *
 * `pairing` is deliberately absent: it is a *route* value meaning "the Pairings
 * pane with the add-flow open", not a pane of its own, and it is unpacked into
 * `mainSection` + `pairingFlowOpen` the moment it arrives.
 */
export type MainSection = "history" | "pairings" | "settings";

/**
 * A transient band across the bottom of a pane: `COPIED`, a copy failure, or
 * the reason an undecryptable entry cannot be copied.
 *
 * `seq` exists so that copying the same entry twice restarts the dismissal
 * timer instead of letting the first one cut the second short.
 */
export type Toast = { tone: Tone; text: string; detail?: string; seq: number };

export type UiState = {
  filter: string;
  selectedIndex: number;
  mainSection: MainSection;
  /** True when the Pairings pane should show the add-a-pairing flow expanded. */
  pairingFlowOpen: boolean;
  /**
   * The **Viewed Pairing**: whose History the main window is showing. Undefined
   * means "follow the Active Pairing", which is where every window starts —
   * this is view state and is never persisted (see CONTEXT.md).
   */
  viewedUserId?: string | undefined;
  /**
   * An entry id handed over by the popover, consumed once by the History pane
   * after its first hydration. A stale id selects nothing.
   */
  seedEntryId?: number | undefined;
  /** Explicit `| undefined`: `exactOptionalPropertyTypes` is on, and clearing writes the key. */
  toast?: Toast | undefined;
  setFilter: (s: string) => void;
  setSelectedIndex: (i: number) => void;
  setMainSection: (m: MainSection) => void;
  setPairingFlowOpen: (open: boolean) => void;
  setViewedUserId: (user_id: string | undefined) => void;
  setSeedEntryId: (entry_id: number | undefined) => void;
  showToast: (toast: Omit<Toast, "seq">) => void;
  dismissToast: () => void;
};

export const useUiStore = create<UiState>((set) => ({
  filter: "",
  selectedIndex: 0,
  mainSection: "history",
  pairingFlowOpen: false,
  viewedUserId: undefined,
  seedEntryId: undefined,
  toast: undefined,
  setFilter: (filter) => set({ filter, selectedIndex: 0 }),
  setSelectedIndex: (selectedIndex) => set({ selectedIndex }),
  // Switching panes drops the filter with them: a query left over from the last
  // visit would silently hide rows the returning user expects to see.
  setMainSection: (mainSection) => set({ mainSection, filter: "", selectedIndex: 0 }),
  setPairingFlowOpen: (pairingFlowOpen) => set({ pairingFlowOpen }),
  setViewedUserId: (viewedUserId) => set({ viewedUserId, filter: "", selectedIndex: 0 }),
  setSeedEntryId: (seedEntryId) => set({ seedEntryId }),
  showToast: (toast) => set((s) => ({ toast: { ...toast, seq: (s.toast?.seq ?? 0) + 1 } })),
  dismissToast: () => set({ toast: undefined }),
}));
