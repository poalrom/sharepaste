import { create } from "zustand";
import type { ConnectionState } from "../types";

export type StatusState = {
  byUser: Record<string, { state: ConnectionState; pending: number; last_error?: string }>;
  set: (user_id: string, patch: Partial<StatusState["byUser"][string]>) => void;
};

export const useStatusStore = create<StatusState>((set, get) => ({
  byUser: {},
  set: (user_id, patch) => set({
    byUser: {
      ...get().byUser,
      [user_id]: {
        state: get().byUser[user_id]?.state ?? "Disconnected",
        pending: get().byUser[user_id]?.pending ?? 0,
        ...get().byUser[user_id],
        ...patch,
      },
    },
  }),
}));
