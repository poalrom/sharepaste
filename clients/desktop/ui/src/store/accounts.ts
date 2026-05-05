import { create } from "zustand";
import type { Account } from "../types";

export type AccountsState = {
  accounts: Account[];
  active: string | undefined;
  hydrate: (rows: Account[]) => void;
  upsert: (a: Account) => void;
  remove: (user_id: string) => void;
  setActive: (user_id: string | undefined) => void;
};

export const useAccountsStore = create<AccountsState>((set) => ({
  accounts: [],
  active: undefined,
  hydrate: (rows) =>
    set({
      accounts: rows,
      active: rows.find((a) => a.is_active)?.user_id,
    }),
  upsert: (a) =>
    set((s) => {
      const without = s.accounts.filter((x) => x.user_id !== a.user_id);
      return { accounts: [...without, a] };
    }),
  remove: (uid) =>
    set((s) => ({
      accounts: s.accounts.filter((a) => a.user_id !== uid),
      active: s.active === uid ? undefined : s.active,
    })),
  setActive: (active) =>
    set((s) => ({
      active,
      accounts: s.accounts.map((a) => ({ ...a, is_active: a.user_id === active })),
    })),
}));
