import { useEffect } from "react";
import { useAccountsStore, useHistoryStore, useStatusStore, useUiStore } from "../store";
import { cmd } from "../ipc/commands";
import { events } from "../ipc/events";
import HistoryList from "./HistoryList";
import Search from "./Search";
import Footer from "./Footer";

export default function Popover() {
  const accounts = useAccountsStore((s) => s.accounts);
  const active = useAccountsStore((s) => s.active);
  const hydrateAccounts = useAccountsStore((s) => s.hydrate);
  const hydrateHistory = useHistoryStore((s) => s.hydrate);
  const addEntry = useHistoryStore((s) => s.add);
  const removeEntry = useHistoryStore((s) => s.remove);
  const setStatus = useStatusStore((s) => s.set);

  useEffect(() => {
    let unsub: Array<() => void> = [];
    (async () => {
      const accs = await cmd.listAccounts();
      hydrateAccounts(accs);
      const first = accs[0];
      if (first) {
        const rows = await cmd.listHistory({ user_id: first.user_id, limit: 100 });
        hydrateHistory(rows);
      }
      unsub.push(await events.onEntryAdded(({ user_id, entry }) => {
        if (user_id === useAccountsStore.getState().active) addEntry(entry);
      }));
      unsub.push(await events.onEntryDeleted(({ user_id, entry_id }) => {
        if (user_id === useAccountsStore.getState().active) removeEntry(entry_id);
      }));
      unsub.push(await events.onConnectionState(({ user_id, state, last_error }) => {
        setStatus(user_id, last_error !== undefined ? { state, last_error } : { state });
      }));
      unsub.push(await events.onPendingCount(({ user_id, count }) => {
        setStatus(user_id, { pending: count });
      }));
    })();
    return () => unsub.forEach((u) => u());
  }, [addEntry, hydrateAccounts, hydrateHistory, removeEntry, setStatus]);

  if (accounts.length === 0) {
    return (
      <div className="flex h-full flex-col p-4 gap-2 text-sm">
        <div className="font-semibold">No accounts paired yet.</div>
        <button
          className="rounded bg-blue-600 px-3 py-1.5 text-white hover:bg-blue-500"
          onClick={() => useUiStore.getState().setModal("pairing")}
        >
          Pair a device
        </button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <Search />
      <HistoryList />
      <Footer activeUserId={active!} />
    </div>
  );
}
