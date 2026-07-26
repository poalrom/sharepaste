import { create } from "zustand";

export type ContactState = {
  /** Last contact with the relay per user, epoch ms; null once known to be never. */
  lastContactByUser: Record<string, number | null>;
  setLastContact: (user_id: string, last_contact_at: number | null) => void;
};

export const useContactStore = create<ContactState>((set) => ({
  lastContactByUser: {},
  setLastContact: (user_id, last_contact_at) =>
    set((s) => ({ lastContactByUser: { ...s.lastContactByUser, [user_id]: last_contact_at } })),
}));
