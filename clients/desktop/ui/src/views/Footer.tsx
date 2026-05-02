import { useStatusStore, useUiStore } from "../store";

export default function Footer({ activeUserId }: { activeUserId: string }) {
  const status = useStatusStore((s) => s.byUser[activeUserId]);
  const setModal = useUiStore((s) => s.setModal);
  const stateText = status?.state ?? "Disconnected";
  const pending = status?.pending ?? 0;
  return (
    <div className="border-t border-zinc-700 px-3 py-1.5 text-xs flex justify-between items-center text-zinc-300">
      <span>
        <span data-testid="status">{stateText}</span>
        {pending > 0 ? <span className="ml-2 text-amber-400">· {pending} pending</span> : null}
        {status?.last_error ? <span className="ml-2 text-red-400">· {status.last_error}</span> : null}
      </span>
      <span className="space-x-2">
        <button onClick={() => setModal("accounts")} className="hover:underline">Accounts</button>
        <button onClick={() => setModal("settings")} className="hover:underline">Settings</button>
      </span>
    </div>
  );
}
