import { useEffect, useState } from "react";
import type { Settings } from "../../types";
import { cmd } from "../../ipc/commands";

export default function SettingsSection() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string>();

  useEffect(() => {
    cmd.getSettings().then(setSettings).catch((e) => setError(String(e)));
  }, []);

  if (!settings) return <div className="p-6 text-sm">Loading…</div>;

  const update = async (patch: Partial<Settings>) => {
    try {
      const next = await cmd.updateSettings(patch);
      setSettings(next);
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
        value={settings.hotkey ?? ""}
        onChange={(e) => update({ hotkey: e.target.value || null })}
      />

      {error && <div className="text-xs text-red-400">{error}</div>}
    </div>
  );
}
