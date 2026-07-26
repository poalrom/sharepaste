import { create } from "zustand";
import type { Account, ConnectionState } from "../types";

export type AccountsState = {
  accounts: Account[];
  active: string | undefined;
  hydrate: (rows: Account[]) => void;
  upsert: (a: Account) => void;
  remove: (user_id: string) => void;
  setActive: (user_id: string | undefined) => void;
  updateStatus: (user_id: string, status: ConnectionState) => void;
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
  updateStatus: (user_id, status) =>
    set((s) => ({
      accounts: s.accounts.map((a) =>
        a.user_id === user_id ? { ...a, status } : a,
      ),
    })),
}));

/**
 * The Pairing this window is showing, or undefined when none is active.
 *
 * The store owns both `active` and `accounts`; rebuilding the join in each view
 * is how two views come to disagree about which Pairing they are describing.
 */
export function useActiveAccount(): Account | undefined {
  return useAccountsStore((s) => s.accounts.find((a) => a.user_id === s.active));
}
