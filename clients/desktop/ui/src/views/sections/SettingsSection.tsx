import { useEffect, useRef, useState } from "react";
import type { Settings } from "../../types";
import { cmd } from "../../ipc/commands";
import { useAccountsStore } from "../../store";

export default function SettingsSection() {
  const activeUserId = useAccountsStore((s) => s.active);
  const hydrateAccounts = useAccountsStore((s) => s.hydrate);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string>();
  const [hotkeyDraft, setHotkeyDraft] = useState("");
  const [confirmingClear, setConfirmingClear] = useState(false);
  const committedHotkey = useRef<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    cmd.getSettings().then((s) => {
      if (cancelled) return;
      setSettings(s);
      setHotkeyDraft(s.hotkey ?? "");
      committedHotkey.current = s.hotkey ?? null;
    }).catch((e) => {
      if (!cancelled) setError(String(e));
    });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (activeUserId) return;
    let cancelled = false;
    cmd.listAccounts()
      .then((accs) => {
        if (!cancelled && !useAccountsStore.getState().active) hydrateAccounts(accs);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => { cancelled = true; };
  }, [activeUserId, hydrateAccounts]);

  if (!settings) return <div className="p-6 text-sm">Loading…</div>;

  const update = async (patch: Partial<Settings>) => {
    try {
      const next = await cmd.updateSettings(patch);
      setSettings(next);
    } catch (e) {
      setError(String(e));
    }
  };

  // The hotkey is only persisted on blur or Enter: every update_settings re-registers
  // the global shortcut, so committing per keystroke would leave it unbound while typing.
  const commitHotkey = () => {
    const hotkey = hotkeyDraft || null;
    if (hotkey === committedHotkey.current) return;
    committedHotkey.current = hotkey;
    update({ hotkey });
  };

  const clearHistory = async () => {
    if (!activeUserId) return;
    try {
      await cmd.clearHistory({ user_id: activeUserId });
      setConfirmingClear(false);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="flex flex-col gap-4 p-6 text-sm">
      <h1 className="text-base font-semibold">Settings</h1>

      <label className="flex items-center gap-2">
        <input
          data-testid="capture-enabled"
          type="checkbox"
          checked={settings.capture_enabled}
          onChange={(e) => update({ capture_enabled: e.target.checked })}
        />
        Capture clipboard changes
      </label>

      <label className="flex items-center gap-2">
        <input
          data-testid="autostart"
          type="checkbox"
          checked={settings.autostart}
          onChange={(e) => update({ autostart: e.target.checked })}
        />
        Launch at login
      </label>

      <label className="text-xs text-zinc-400">Deny-list (one bundle id per line)</label>
      <textarea
        data-testid="deny-list"
        rows={4}
        className="rounded bg-zinc-800 px-2 py-1 font-mono text-xs"
        value={settings.deny_list.join("\n")}
        onChange={(e) => update({ deny_list: e.target.value.split("\n").map((s) => s.trim()).filter(Boolean) })}
      />

      <label className="text-xs text-zinc-400">Global hotkey (e.g. <code>Cmd+Shift+V</code>; empty to unbind)</label>
      <input
        data-testid="hotkey"
        className="rounded bg-zinc-800 px-2 py-1 font-mono text-xs"
        value={hotkeyDraft}
        onChange={(e) => setHotkeyDraft(e.target.value)}
        onBlur={commitHotkey}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commitHotkey();
          }
        }}
      />

      <div className="rounded border border-zinc-700">
        <div className="flex items-center justify-between p-3">
          <div>
            <div className="font-semibold">Clear history</div>
            <div className="text-xs text-zinc-400">
              Deletes every stored entry on the server and on all of your devices.
            </div>
          </div>
          <button
            data-testid="clear-history"
            disabled={!activeUserId}
            className="rounded border border-zinc-700 px-2 py-1 text-zinc-200 hover:bg-zinc-800 disabled:opacity-50"
            onClick={() => setConfirmingClear(true)}
          >
            Clear…
          </button>
        </div>
        {confirmingClear && (
          <div
            data-testid="confirm-strip-clear-history"
            className="border-t border-zinc-700 bg-zinc-900/40 p-3 flex items-center justify-between gap-3"
          >
            <div className="text-xs text-zinc-300">
              Erase all clipboard history for every device on this account?
            </div>
            <div className="flex items-center gap-2">
              <button
                data-testid="cancel-clear-history"
                className="rounded border border-zinc-700 px-2 py-1 text-zinc-200 hover:bg-zinc-800"
                onClick={() => setConfirmingClear(false)}
              >
                Cancel
              </button>
              <button
                data-testid="confirm-clear-history"
                className="rounded bg-red-600 px-2 py-1 text-white hover:bg-red-500"
                onClick={clearHistory}
              >
                Clear history
              </button>
            </div>
          </div>
        )}
      </div>

      {error && <div className="text-xs text-red-400">{error}</div>}
    </div>
  );
}
