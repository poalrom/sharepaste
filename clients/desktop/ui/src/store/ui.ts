import { create } from "zustand";
import type { Tone } from "../views/fui/tone";

export type MainSection = "accounts" | "settings" | "pairing";

/**
 * A transient band across the popover: `COPIED`, a copy failure, or the reason
 * an undecryptable entry cannot be copied.
 *
 * `seq` exists so that copying the same entry twice restarts the dismissal
 * timer instead of letting the first one cut the second short.
 */
export type Toast = { tone: Tone; text: string; detail?: string; seq: number };

export type UiState = {
  search: string;
  selectedIndex: number;
  mainSection: MainSection;
  /** Explicit `| undefined`: `exactOptionalPropertyTypes` is on, and clearing writes the key. */
  toast?: Toast | undefined;
  setSearch: (s: string) => void;
  setSelectedIndex: (i: number) => void;
  setMainSection: (m: MainSection) => void;
  showToast: (toast: Omit<Toast, "seq">) => void;
  dismissToast: () => void;
};

export const useUiStore = create<UiState>((set) => ({
  search: "",
  selectedIndex: 0,
  mainSection: "accounts",
  toast: undefined,
  setSearch: (search) => set({ search, selectedIndex: 0 }),
  setSelectedIndex: (selectedIndex) => set({ selectedIndex }),
  setMainSection: (mainSection) => set({ mainSection }),
  showToast: (toast) => set((s) => ({ toast: { ...toast, seq: (s.toast?.seq ?? 0) + 1 } })),
  dismissToast: () => set({ toast: undefined }),
}));
