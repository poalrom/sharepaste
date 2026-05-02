import { create } from "zustand";

export type ModalKind = null | "pairing" | "settings" | "accounts";

export type UiState = {
  modal: ModalKind;
  search: string;
  selectedIndex: number;
  setModal: (m: ModalKind) => void;
  setSearch: (s: string) => void;
  setSelectedIndex: (i: number) => void;
};

export const useUiStore = create<UiState>((set) => ({
  modal: null,
  search: "",
  selectedIndex: 0,
  setModal: (modal) => set({ modal }),
  setSearch: (search) => set({ search, selectedIndex: 0 }),
  setSelectedIndex: (selectedIndex) => set({ selectedIndex }),
}));
