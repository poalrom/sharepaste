import type { Pairing, EntryView, Settings, Contact, UpdateStatus } from "../types";
import { tauri } from "./tauri";

/**
 * How many rows a surface asks the core for in one page.
 *
 * Larger than [`HISTORY_CAP`], and it has to be: that cap
 * bounds the region the relay has ordered, and the un-flushed region is unbounded
 * on purpose — an act this device has not delivered is undelivered clipboard
 * content, and evicting one to protect a number is the trade ADR 0014 refuses. A
 * page of a hundred would hide exactly the oldest offline captures, which are the
 * ones about to flush first. The core clamps to its own ceiling either way, and
 * neither surface pages, so one page has to be the whole list.
 */
export const HISTORY_PAGE = 1000;

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
  /**
   * `before` is a `(last_use, id)` cursor and not an id: id stopped being the
   * order (ADR 0011), so paging by it alone would both skip and repeat rows.
   * Nothing passes it — the core clamps every page to its own ceiling, and
   * `HISTORY_PAGE` is already above it.
   */
  listHistory:         (args: { user_id: string; before?: { last_use: number; id: number }; limit: number }) =>
                         tauri.invoke<EntryView[]>("list_history", { args }),
  copyToClipboard:     (args: { user_id: string; entry_id: number }) => tauri.invoke<void>("copy_to_clipboard", { args }),
  deleteEntry:         (args: { user_id: string; entry_id: number }) => tauri.invoke<void>("delete_entry", { args }),
  resendEntry:         (args: { user_id: string; entry_id: number }) => tauri.invoke<void>("resend_entry", { args }),
  clearHistory:        (args: { user_id: string }) => tauri.invoke<void>("clear_history", { args }),
  getContact:        (args: { user_id: string }) => tauri.invoke<Contact>("get_contact", { args }),
  getSettings:         (): Promise<Settings> => tauri.invoke("get_settings"),
  updateSettings:      (patch: Partial<Settings>): Promise<Settings> => tauri.invoke("update_settings", { patch }),
  hidePopover:         () => tauri.invoke<void>("hide_popover"),
  getUpdateStatus:     (): Promise<UpdateStatus> => tauri.invoke("get_update_status"),
  checkForUpdate:      (): Promise<UpdateStatus> => tauri.invoke("check_for_update"),
  /** Downloads, installs and relaunches. Never call this without a click. */
  installUpdate:       () => tauri.invoke<void>("install_update"),
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
