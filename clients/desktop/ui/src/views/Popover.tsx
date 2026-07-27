import { useEffect, useState } from "react";
import { cmd } from "../ipc/commands";
import { events } from "../ipc/events";
import { agePhrase } from "../lib/format";
import { useNow } from "../lib/useNow";
import {
  usePairingsStore,
  useHistoryStore,
  useStatusStore,
  useContactStore,
  useUiStore,
} from "../store";
import Footer from "./Footer";
import HintStrip from "./HintStrip";
import HistoryList from "./HistoryList";
import Search from "./Search";
import { CONNECTION } from "./connection";
import { PanelMessage, Strip } from "./fui";

/** Mirrors --dur-sweep / --dur-toast: the CSS owns the animation, this owns the state. */
const SWEEP_MS = 900;
const TOAST_MS = 2200;

export default function Popover() {
  const pairings = usePairingsStore((s) => s.pairings);
  const active = usePairingsStore((s) => s.active);
  const hydratePairings = usePairingsStore((s) => s.hydrate);
  const hydrateHistory = useHistoryStore((s) => s.hydrate);
  const addEntry = useHistoryStore((s) => s.add);
  const removeEntry = useHistoryStore((s) => s.remove);
  const setStatus = useStatusStore((s) => s.set);
  const setLastContact = useContactStore((s) => s.setLastContact);
  const status = useStatusStore((s) => (active ? s.byUser[active] : undefined));
  const lastContactAt = useContactStore((s) => (active ? s.lastContactByUser[active] ?? null : null));
  const toast = useUiStore((s) => s.toast);
  const now = useNow(60_000);
  const [sweep, setSweep] = useState(0);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      cmd.hidePopover().catch((err) => console.error("hide failed", err));
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  useEffect(() => {
    const onBlur = () => {
      useUiStore.getState().setSearch("");
      useUiStore.getState().setSelectedIndex(0);
    };
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, []);

  // `focus` is the signal that the window was just shown, for the reason
  // Search.tsx documents: it is shown and hidden, never unmounted, and it hides
  // on Focused(false), so it can never be visible-but-unfocused.
  //
  // The sweep rides its own overlay rather than the panel element because
  // restarting the animation means remounting whatever carries it, and
  // remounting the panel would tear the search box out from under the focus the
  // same event just restored. The counter restarts a sweep still in flight.
  useEffect(() => {
    let timer: number | undefined;
    const play = () => {
      setSweep((n) => n + 1);
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setSweep(0), SWEEP_MS);
    };
    play();
    window.addEventListener("focus", play);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("focus", play);
    };
  }, []);

  // Keyed on `seq`, not the toast object, so copying the same entry twice
  // restarts the window instead of inheriting what is left of the first one.
  const toastSeq = toast?.seq;
  useEffect(() => {
    if (toastSeq === undefined) return;
    const timer = window.setTimeout(() => useUiStore.getState().dismissToast(), TOAST_MS);
    return () => window.clearTimeout(timer);
  }, [toastSeq]);

  useEffect(() => {
    let unsub: Array<() => void> = [];
    let cancelled = false;
    (async () => {
      const accs = await cmd.listPairings();
      if (cancelled) return;
      hydratePairings(accs);
      // `list_accounts` already knows each session's state; without seeding it
      // here the store reads Disconnected until the next transition happens to
      // fire, and a popover opened onto a healthy session shows the degraded
      // strip indefinitely.
      for (const a of accs) setStatus(a.user_id, { state: a.status, pending: a.pending });
      const activeUserId = usePairingsStore.getState().active;
      if (activeUserId) {
        const rows = await cmd.listHistory({ user_id: activeUserId, limit: 100 });
        if (!cancelled) hydrateHistory(rows);
        // Contact is stamped by traffic the popover was not open for, so
        // without this the strip reads NEVER until the next event fires. A
        // backend older than this command answers undefined, not a rejection.
        cmd.getContact({ user_id: activeUserId })
          .then((c) => c && setLastContact(c.user_id, c.last_contact_at))
          .catch(() => {});
      }
      unsub.push(await events.onEntryAdded(({ user_id, entry }) => {
        if (user_id === usePairingsStore.getState().active) addEntry(entry);
      }));
      unsub.push(await events.onEntryDeleted(({ user_id, entry_id }) => {
        if (user_id === usePairingsStore.getState().active) removeEntry(entry_id);
      }));
      unsub.push(await events.onConnectionState(({ user_id, state, last_error }) => {
        setStatus(user_id, last_error !== undefined ? { state, last_error } : { state });
      }));
      unsub.push(await events.onPendingCount(({ user_id, count }) => {
        setStatus(user_id, { pending: count });
      }));
      unsub.push(await events.onPairingAdded(() => {
        cmd.listPairings().then(hydratePairings);
      }));
      unsub.push(await events.onPairingRemoved(({ user_id }) => {
        usePairingsStore.getState().remove(user_id);
      }));
      unsub.push(await events.onActivePairingChanged(({ user_id }) => {
        const next = user_id ?? undefined;
        usePairingsStore.getState().setActive(next);
        if (next) {
          cmd.listHistory({ user_id: next, limit: 100 }).then(hydrateHistory).catch(() => {});
          cmd.getContact({ user_id: next })
            .then((c) => c && setLastContact(c.user_id, c.last_contact_at))
            .catch(() => {});
        } else {
          hydrateHistory([]);
        }
      }));
      unsub.push(await events.onHistoryChanged(({ user_id }) => {
        if (user_id !== usePairingsStore.getState().active) return;
        cmd.listHistory({ user_id, limit: 100 }).then(hydrateHistory).catch(() => {});
      }));
      unsub.push(await events.onContact(({ user_id, last_contact_at }) => {
        setLastContact(user_id, last_contact_at);
      }));
    })();
    return () => {
      cancelled = true;
      unsub.forEach((u) => u());
    };
  }, [addEntry, hydratePairings, hydrateHistory, removeEntry, setLastContact, setStatus]);

  const conn = CONNECTION[status?.state ?? "Disconnected"];
  const degraded = active !== undefined && conn.degraded ? conn : undefined;
  const authError = status?.state === "AuthFailed" ? status.last_error : undefined;

  return (
    <div className="fui-panel flex h-full flex-col overflow-hidden bg-surface-panel">
      {sweep > 0 && (
        <div key={sweep} className="fui-sweep pointer-events-none absolute inset-0 z-10" aria-hidden="true" />
      )}

      <header className="fui-band flex h-[26px] shrink-0 items-center justify-between border-b border-hairline px-3">
        <span className="text-chrome tracking-word text-cyan-300">
          SHAREPASTE <span className="text-text-dim">//</span> RELAY
        </span>
        <span className="border border-hairline px-1.5 py-px text-chrome tracking-phrase text-text-dim">
          ESC
        </span>
      </header>

      {pairings.length === 0 ? (
        <PanelMessage
          title="NO PAIRINGS ON THIS DEVICE"
          action={{
            label: "PAIR A DEVICE",
            variant: "solid",
            onClick: () =>
              cmd.openSection("pairing").catch((err) => console.error("open pairing failed", err)),
          }}
        />
      ) : !active ? (
        <PanelMessage
          title="NO ACTIVE PAIRING"
          action={{
            label: "CHOOSE PAIRING",
            variant: "outline",
            testId: "choose-pairing",
            onClick: () =>
              cmd.openSection("pairings").catch((err) => console.error("open pairings failed", err)),
          }}
        />
      ) : (
        <>
          <Search />
          {degraded && (
            <Strip tone={degraded.tone} testId="degraded-strip">
              <span className="shrink-0">{degraded.label}</span>
              {authError && <span className="min-w-0 truncate">· {authError}</span>}
              <span className="shrink-0">
                · LAST CONTACT{" "}
                {lastContactAt === null ? (
                  "NEVER"
                ) : (
                  <span className="normal-case">{agePhrase(lastContactAt, now)}</span>
                )}
              </span>
            </Strip>
          )}
          <HistoryList />
        </>
      )}

      {toast && (
        <Strip tone={toast.tone} testId="toast">
          <span className="shrink-0 font-semibold tracking-word">[{toast.text}]</span>
          {toast.detail && (
            <span className="min-w-0 truncate font-mono normal-case">{toast.detail}</span>
          )}
        </Strip>
      )}

      {active && <HintStrip />}

      <Footer activeUserId={active} />
    </div>
  );
}
