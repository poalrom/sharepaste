import type { Pairing, EntryView, Settings, Contact } from "../types";
import { tauri } from "./tauri";

export const cmd = {
  listPairings:        (): Promise<Pairing[]> => tauri.invoke("list_pairings"),
  pairWithInvite:      (args: { server_url: string; token: string; device_label: string }) =>
                         tauri.invoke<{ user_id: string; device_id: string }>("pair_with_invite", { args }),
  pairStart:           (args: { user_id: string }) =>
                         tauri.invoke<{ code: string; expires_at: number }>("pair_start", { args }),
  pairWithCode:        (args: { code: string; device_label: string }) =>
                         tauri.invoke<{ user_id: string; device_id: string }>("pair_with_code", { args }),
  forgetPairing:       (args: { user_id: string }) => tauri.invoke<void>("forget_pairing", { args }),
  setActivePairing:    (args: { user_id: string }) => tauri.invoke<void>("set_active_pairing", { args }),
  listHistory:         (args: { user_id: string; before_id?: number; limit: number }) =>
                         tauri.invoke<EntryView[]>("list_history", { args }),
  copyToClipboard:     (args: { user_id: string; entry_id: number }) => tauri.invoke<void>("copy_to_clipboard", { args }),
  deleteEntry:         (args: { user_id: string; entry_id: number }) => tauri.invoke<void>("delete_entry", { args }),
  clearHistory:        (args: { user_id: string }) => tauri.invoke<void>("clear_history", { args }),
  getContact:        (args: { user_id: string }) => tauri.invoke<Contact>("get_contact", { args }),
  getSettings:         (): Promise<Settings> => tauri.invoke("get_settings"),
  updateSettings:      (patch: Partial<Settings>): Promise<Settings> => tauri.invoke("update_settings", { patch }),
  hidePopover:         () => tauri.invoke<void>("hide_popover"),
  /**
   * Open the main window. `section` takes the rail's pane names plus `pairing`,
   * which means "the Pairings pane with the add-flow open"; `entry_id` is the
   * popover's handoff to the reader (`open_main_window` treats a stale id as no
   * selection).
   */
  openSection:         async (section: "history" | "pairings" | "settings" | "pairing", entry_id?: number) => {
                         await tauri.invoke<void>("open_main_window", { args: { section, entry_id } });
                         await tauri.invoke<void>("hide_popover");
                       },
};
