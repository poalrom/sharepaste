import { useEffect, useState } from "react";
import { cmd } from "../../ipc/commands";
import { events } from "../../ipc/events";
import { useAccountsStore, useUiStore } from "../../store";
import type { Account } from "../../types";

export default function AccountsSection() {
  const accounts = useAccountsStore((s) => s.accounts);
  const hydrate = useAccountsStore((s) => s.hydrate);
  const removeFromStore = useAccountsStore((s) => s.remove);
  const setActiveInStore = useAccountsStore((s) => s.setActive);
  const [confirmingUserId, setConfirmingUserId] = useState<string | undefined>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    let cancelled = false;
    const refresh = async () => {
      try {
        const rows = await cmd.listAccounts();
        if (!cancelled) hydrate(rows);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    };
    refresh();
    const subPromise = (async () => {
      const unsubs: Array<() => void> = [];
      unsubs.push(await events.onAccountAdded(refresh));
      unsubs.push(
        await events.onAccountRemoved(({ user_id }) => {
          removeFromStore(user_id);
          setConfirmingUserId((curr) => (curr === user_id ? undefined : curr));
        }),
      );
      unsubs.push(
        await events.onActiveChanged(({ user_id }) => {
          setActiveInStore(user_id ?? undefined);
        }),
      );
      return unsubs;
    })();
    return () => {
      cancelled = true;
      subPromise.then((unsubs) => unsubs.forEach((u) => u()));
    };
  }, [hydrate, removeFromStore, setActiveInStore]);

  if (accounts.length === 0) {
    return (
      <div className="flex flex-col gap-3 p-6 text-sm">
        <h1 className="text-base font-semibold">Accounts</h1>
        <div className="text-zinc-300">No accounts. Pair a device to get started.</div>
        <button
          data-testid="empty-pair"
          className="self-start rounded bg-blue-600 px-3 py-1.5 text-white hover:bg-blue-500"
          onClick={() => useUiStore.getState().setMainSection("pairing")}
        >
          Pair a device
        </button>
        {error && <div className="text-xs text-red-400">{error}</div>}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3 p-6 text-sm">
      <h1 className="text-base font-semibold">Accounts</h1>
      <ul className="flex flex-col gap-2">
        {accounts.map((a) => (
          <li key={a.user_id} className="rounded border border-zinc-700">
            <div className="flex items-center justify-between p-3">
              <div>
                <div className="font-semibold">{a.label}</div>
                <div className="text-xs text-zinc-400">
                  {a.user_id} @ {a.server_url}
                </div>
                <div className="text-xs text-zinc-400">
                  status: {a.status} · pending: {a.pending}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {a.is_active ? (
                  <span
                    data-testid={`active-badge-${a.user_id}`}
                    className="rounded bg-emerald-700 px-2 py-1 text-xs uppercase tracking-wide text-white"
                  >
                    Active
                  </span>
                ) : (
                  <button
                    data-testid={`use-${a.user_id}`}
                    className="rounded bg-blue-600 px-2 py-1 text-white hover:bg-blue-500"
                    onClick={async () => {
                      try {
                        await cmd.setActiveAccount({ user_id: a.user_id });
                      } catch (e) {
                        setError(String(e));
                      }
                    }}
                  >
                    Use
                  </button>
                )}
                <button
                  aria-label={`Disconnect ${a.label}`}
                  data-testid={`trash-${a.user_id}`}
                  className="rounded p-1 text-zinc-300 hover:bg-zinc-800 hover:text-red-300"
                  onClick={() => setConfirmingUserId(a.user_id)}
                >
                  <TrashIcon />
                </button>
              </div>
            </div>
            {confirmingUserId === a.user_id && (
              <ConfirmStrip
                account={a}
                onCancel={() => setConfirmingUserId(undefined)}
                onConfirm={async () => {
                  try {
                    await cmd.forgetAccount({ user_id: a.user_id });
                    setConfirmingUserId(undefined);
                  } catch (e) {
                    setError(String(e));
                  }
                }}
              />
            )}
          </li>
        ))}
      </ul>
      {error && <div className="text-xs text-red-400">{error}</div>}
    </div>
  );
}

function ConfirmStrip(props: {
  account: Account;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div
      data-testid={`confirm-strip-${props.account.user_id}`}
      className="border-t border-zinc-700 bg-zinc-900/40 p-3 flex items-center justify-between gap-3"
    >
      <div className="text-xs text-zinc-300">
        Erase local history and key for {props.account.label}?
      </div>
      <div className="flex items-center gap-2">
        <button
          data-testid={`cancel-${props.account.user_id}`}
          className="rounded border border-zinc-700 px-2 py-1 text-zinc-200 hover:bg-zinc-800"
          onClick={props.onCancel}
        >
          Cancel
        </button>
        <button
          data-testid={`confirm-forget-${props.account.user_id}`}
          className="rounded bg-red-600 px-2 py-1 text-white hover:bg-red-500"
          onClick={props.onConfirm}
        >
          Forget
        </button>
      </div>
    </div>
  );
}

function TrashIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3 6h18" />
      <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
      <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </svg>
  );
}
