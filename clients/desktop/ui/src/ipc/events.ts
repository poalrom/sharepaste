import type { ConnectionState, EntryView } from "../types";
import { tauri } from "./tauri";

export const events = {
  onAccountAdded:    (cb: (p: { user_id: string; device_id: string; label: string }) => void) => tauri.listen("account-added", cb),
  onAccountRemoved:  (cb: (p: { user_id: string }) => void) => tauri.listen("account-removed", cb),
  onActiveChanged:   (cb: (p: { user_id: string | null }) => void) => tauri.listen("active-changed", cb),
  onConnectionState: (cb: (p: { user_id: string; state: ConnectionState; last_error?: string }) => void) => tauri.listen("connection-state", cb),
  onEntryAdded:      (cb: (p: { user_id: string; entry: EntryView }) => void) => tauri.listen("entry-added", cb),
  onEntryDeleted:    (cb: (p: { user_id: string; entry_id: number }) => void) => tauri.listen("entry-deleted", cb),
  onHistoryChanged:  (cb: (p: { user_id: string }) => void) => tauri.listen("history-changed", cb),
  onPendingCount:    (cb: (p: { user_id: string; count: number }) => void) => tauri.listen("pending-count", cb),
  onDecryptionError: (cb: (p: { user_id: string; entry_id: number }) => void) => tauri.listen("decryption-error", cb),
  onPairShortcode:   (cb: (p: { code: string; expires_at: number }) => void) => tauri.listen("pair-shortcode", cb),
  onPairClaimed:     (cb: (p: { user_id: string; device_label?: string | null }) => void) => tauri.listen("pair-claimed", cb),
  onPairExpired:     (cb: () => void) => tauri.listen("pair-expired", () => cb()),
  onMainNavigate:    (cb: (section: "accounts" | "settings" | "pairing") => void) => tauri.listen("main://navigate", cb),
};
