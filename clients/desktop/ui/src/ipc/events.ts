import type { ConnectionState, EntryView, Contact, UpdateAvailable } from "../types";
import { tauri } from "./tauri";

export const events = {
  onPairingAdded:    (cb: (p: { user_id: string; device_id: string; label: string }) => void) => tauri.listen("pairing-added", cb),
  onPairingRemoved:  (cb: (p: { user_id: string }) => void) => tauri.listen("pairing-removed", cb),
  onActivePairingChanged:   (cb: (p: { user_id: string | null }) => void) => tauri.listen("active-pairing-changed", cb),
  onConnectionState: (cb: (p: { user_id: string; state: ConnectionState; last_error?: string }) => void) => tauri.listen("connection-state", cb),
  onEntryAdded:      (cb: (p: { user_id: string; entry: EntryView }) => void) => tauri.listen("entry-added", cb),
  onEntryDeleted:    (cb: (p: { user_id: string; entry_id: number }) => void) => tauri.listen("entry-deleted", cb),
  onEntrySettled:    (cb: (p: { user_id: string; entry_id: number; created_at: number | null; last_use: number | null }) => void) => tauri.listen("entry-settled", cb),
  onEntryRefused:    (cb: (p: { user_id: string; entry_id: number; reason: string }) => void) => tauri.listen("entry-refused", cb),
  onHistoryChanged:  (cb: (p: { user_id: string }) => void) => tauri.listen("history-changed", cb),
  onPendingCount:    (cb: (p: { user_id: string; count: number }) => void) => tauri.listen("pending-count", cb),
  onContact:       (cb: (p: Contact) => void) => tauri.listen("contact", cb),
  onPairShortcode:   (cb: (p: { code: string; expires_at: number }) => void) => tauri.listen("pair-shortcode", cb),
  onPairClaimed:     (cb: (p: { user_id: string; device_label?: string | null }) => void) => tauri.listen("pair-claimed", cb),
  onPairExpired:     (cb: () => void) => tauri.listen("pair-expired", () => cb()),
  onMainNavigate:    (cb: (p: { section: string; entry_id: number | null }) => void) => tauri.listen("main://navigate", cb),
  onUpdateAvailable: (cb: (p: UpdateAvailable) => void) => tauri.listen("update-available", cb),
};
