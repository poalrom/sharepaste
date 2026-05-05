import { useEffect } from "react";
import { useAccountsStore, useHistoryStore, useStatusStore, useUiStore } from "../store";
import { cmd } from "../ipc/commands";
import { events } from "../ipc/events";
import HistoryList from "./HistoryList";
import Search from "./Search";
import Footer from "./Footer";
import PairingModal from "../modals/PairingModal";

export default function Popover() {
  const accounts = useAccountsStore((s) => s.accounts);
  const active = useAccountsStore((s) => s.active);
  const hydrateAccounts = useAccountsStore((s) => s.hydrate);
  const hydrateHistory = useHistoryStore((s) => s.hydrate);
  const addEntry = useHistoryStore((s) => s.add);
  const removeEntry = useHistoryStore((s) => s.remove);
  const setStatus = useStatusStore((s) => s.set);
  const modal = useUiStore((s) => s.modal);
  const setModal = useUiStore((s) => s.setModal);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      if (useUiStore.getState().modal !== null) {
        setModal(null);
      } else {
        cmd.hidePopover().catch((err) => console.error("hide failed", err));
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [setModal]);

  useEffect(() => {
    const onBlur = () => {
      useUiStore.getState().setSearch("");
      useUiStore.getState().setSelectedIndex(0);
    };
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, []);

  useEffect(() => {
    let unsub: Array<() => void> = [];
    let cancelled = false;
    (async () => {
      const accs = await cmd.listAccounts();
      if (cancelled) return;
      hydrateAccounts(accs);
      const activeUserId = useAccountsStore.getState().active;
      if (activeUserId) {
        const rows = await cmd.listHistory({ user_id: activeUserId, limit: 100 });
        if (!cancelled) hydrateHistory(rows);
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
      unsub.push(await events.onAccountAdded(() => {
        cmd.listAccounts().then(hydrateAccounts);
      }));
      unsub.push(await events.onAccountRemoved(({ user_id }) => {
        useAccountsStore.getState().remove(user_id);
      }));
      unsub.push(await events.onActiveChanged(({ user_id }) => {
        const next = user_id ?? undefined;
        useAccountsStore.getState().setActive(next);
        if (next) {
          cmd.listHistory({ user_id: next, limit: 100 }).then(hydrateHistory).catch(() => {});
        } else {
          hydrateHistory([]);
        }
      }));
    })();
    return () => {
      cancelled = true;
      unsub.forEach((u) => u());
    };
  }, [addEntry, hydrateAccounts, hydrateHistory, removeEntry, setStatus]);

  if (modal === "pairing") {
    return (
      <div className="flex h-full flex-col">
        <PairingModal onClose={() => setModal(null)} />
      </div>
    );
  }

  if (accounts.length === 0) {
    return (
      <div className="flex h-full flex-col p-4 gap-2 text-sm">
        <div className="font-semibold">No accounts paired yet.</div>
        <button
          className="rounded bg-blue-600 px-3 py-1.5 text-white hover:bg-blue-500"
          onClick={() => setModal("pairing")}
        >
          Pair a device
        </button>
      </div>
    );
  }

  if (!active) {
    return (
      <div className="flex h-full flex-col p-4 gap-2 text-sm">
        <div className="font-semibold">No active account.</div>
        <button
          data-testid="choose-account"
          className="self-start rounded bg-blue-600 px-3 py-1.5 text-white hover:bg-blue-500"
          onClick={() => cmd.openModal("accounts").catch((err) => console.error("open accounts failed", err))}
        >
          Choose account
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
