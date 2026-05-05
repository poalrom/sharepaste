import { create } from "zustand";

export type MainSection = "accounts" | "settings" | "pairing";

export type UiState = {
  search: string;
  selectedIndex: number;
  mainSection: MainSection;
  setSearch: (s: string) => void;
  setSelectedIndex: (i: number) => void;
  setMainSection: (m: MainSection) => void;
};

export const useUiStore = create<UiState>((set) => ({
  search: "",
  selectedIndex: 0,
  mainSection: "accounts",
  setSearch: (search) => set({ search, selectedIndex: 0 }),
  setSelectedIndex: (selectedIndex) => set({ selectedIndex }),
  setMainSection: (mainSection) => set({ mainSection }),
}));
