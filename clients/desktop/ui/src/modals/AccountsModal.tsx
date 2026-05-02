import { useEffect, useState } from "react";
import type { Account } from "../types";
import { cmd } from "../ipc/commands";

export default function AccountsModal() {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [active, setActive] = useState<string | undefined>();
  const [error, setError] = useState<string>();

  const refresh = async () => {
    try { setAccounts(await cmd.listAccounts()); }
    catch (e) { setError(String(e)); }
  };

  useEffect(() => { refresh(); }, []);

  return (
    <div className="flex flex-col gap-3 p-6 text-sm">
      <h1 className="text-base font-semibold">Accounts</h1>
      <ul className="flex flex-col gap-2">
        {accounts.map((a) => (
          <li key={a.user_id} className="rounded border border-zinc-700 p-3 flex justify-between items-center">
            <div>
              <div className="font-semibold">{a.label}</div>
              <div className="text-xs text-zinc-400">{a.user_id} @ {a.server_url}</div>
              <div className="text-xs text-zinc-400">status: {a.status} · pending: {a.pending}</div>
            </div>
            <div className="flex gap-2">
              <button
                data-testid={`switch-${a.user_id}`}
                className="rounded bg-blue-600 px-2 py-1 text-white"
                onClick={async () => {
                  await cmd.setActiveAccount({ user_id: a.user_id });
                  setActive(a.user_id);
                }}
              >
                {active === a.user_id ? "Active" : "Use"}
              </button>
              <button
                className="rounded bg-red-600 px-2 py-1 text-white"
                onClick={async () => {
                  if (!confirm(`Forget ${a.label}? Local history and key will be erased.`)) return;
                  await cmd.forgetAccount({ user_id: a.user_id });
                  await refresh();
                }}
              >
                Forget
              </button>
            </div>
          </li>
        ))}
      </ul>
      {error && <div className="text-xs text-red-400">{error}</div>}
    </div>
  );
}
