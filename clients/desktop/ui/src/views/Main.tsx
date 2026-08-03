import { useEffect, useState } from "react";
import { cmd, HISTORY_PAGE } from "../ipc/commands";
import { events } from "../ipc/events";
import { useNow } from "../lib/useNow";
import {
  hydrateFrom,
  noteChange,
  useContactStore,
  useHistoryStore,
  usePairingsStore,
  useStatusStore,
  useUiStore,
  type MainSection,
} from "../store";
import { Strip } from "./fui";
import HistorySection from "./main/HistorySection";
import MainFooter from "./main/MainFooter";
import Rail from "./main/Rail";
import Titlebar from "./main/Titlebar";
import PairingsSection from "./sections/PairingsSection";
import SettingsSection from "./sections/SettingsSection";

/** Mirrors --dur-toast: the CSS owns the animation, this owns the state. */
const TOAST_MS = 2200;

const TITLE: Record<MainSection, string> = {
  history: "History",
  pairings: "Pairings",
  settings: "Settings",
};

/**
 * Unpacks a route value into pane + flow.
 *
 * `pairing` is the tray's "Pair device…" and the popover's empty state: it is
 * the Pairings pane with the add-flow already open, not a pane of its own
 * (ADR 0004).
 */
function route(section: string | null): { pane: MainSection; flow: boolean } | undefined {
  if (section === "pairing") return { pane: "pairings", flow: true };
  if (section === "history" || section === "pairings" || section === "settings") {
    return { pane: section, flow: false };
  }
  return undefined;
}

export default function Main() {
  const section = useUiStore((s) => s.mainSection);
  const setSection = useUiStore((s) => s.setMainSection);
  const setPairingFlowOpen = useUiStore((s) => s.setPairingFlowOpen);
  const setSeedEntryId = useUiStore((s) => s.setSeedEntryId);
  const toast = useUiStore((s) => s.toast);
  const hydratePairings = usePairingsStore((s) => s.hydrate);
  const hydrateHistory = useHistoryStore((s) => s.hydrate);
  const addEntry = useHistoryStore((s) => s.add);
  const removeEntry = useHistoryStore((s) => s.remove);
  const settleEntry = useHistoryStore((s) => s.settle);
  const refuseEntry = useHistoryStore((s) => s.refuse);
  const setStatus = useStatusStore((s) => s.set);
  const setLastContact = useContactStore((s) => s.setLastContact);
  const now = useNow(60_000);
  const entryCount = useHistoryStore((s) => s.entries.length);
  const pairingCount = usePairingsStore((s) => s.pairings.length);

  /*
   * The mock printed a flavour code here — `HST-00`, `ACC-01`. Dropped as
   * decoration that resembles information (ADR 0002); the slot instead states
   * the one count each pane is about.
   */
  const paneCode =
    section === "history"
      ? `${entryCount} ${entryCount === 1 ? "ENTRY" : "ENTRIES"}`
      : section === "pairings"
        ? `${pairingCount} ON THIS DEVICE`
        : "THIS DEVICE";

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const target = route(params.get("section"));
    if (target) {
      setSection(target.pane);
      setPairingFlowOpen(target.flow);
    }
    const entry = Number(params.get("entry"));
    if (Number.isSafeInteger(entry) && entry > 0) setSeedEntryId(entry);
  }, [setSection, setPairingFlowOpen, setSeedEntryId]);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      const off = await events.onMainNavigate(({ section: next, entry_id }) => {
        const target = route(next);
        if (!target) return;
        setSection(target.pane);
        setPairingFlowOpen(target.flow);
        if (entry_id !== null) setSeedEntryId(entry_id);
      });
      if (cancelled) off();
      else unsub = off;
    })();
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [setSection, setPairingFlowOpen, setSeedEntryId]);

  // The main window is its own webview with its own store, so it subscribes to
  // the same stream the popover does rather than inheriting anything from it.
  useEffect(() => {
    const unsub: Array<() => void> = [];
    let cancelled = false;
    (async () => {
      // The entry subscriptions come first, before anything is awaited: the
      // reason is `noteChange`'s, and `HistorySection` takes the snapshot they
      // have to survive.
      unsub.push(await events.onEntryAdded(({ user_id, entry }) => {
        noteChange({ kind: "added", user_id, entry });
        if (user_id === viewedUserId()) addEntry(entry);
      }));
      unsub.push(await events.onEntryDeleted(({ user_id, entry_id }) => {
        noteChange({ kind: "deleted", user_id, entry_id });
        if (user_id === viewedUserId()) removeEntry(entry_id);
      }));
      // In place and by id, with no refetch: nothing reorders at a flush and the
      // id does not change, so the reader's selection stays where it was. The
      // relay's stamp rides along, or the row would stop waiting and go on saying
      // the relay has never stamped it.
      unsub.push(await events.onEntrySettled(({ user_id, entry_id, created_at, last_use }) => {
        noteChange({ kind: "settled", user_id, entry_id, created_at, last_use });
        if (user_id === viewedUserId()) settleEntry(entry_id, created_at, last_use);
      }));
      unsub.push(await events.onEntryRefused(({ user_id, entry_id, reason }) => {
        noteChange({ kind: "refused", user_id, entry_id, reason });
        if (user_id === viewedUserId()) refuseEntry(entry_id, reason);
      }));
      const rows = await cmd.listPairings();
      if (cancelled) return;
      hydratePairings(rows);
      // `list_pairings` already knows each session's state; without seeding it
      // here the footer reads Disconnected until the next transition fires.
      for (const p of rows) setStatus(p.user_id, { state: p.status, pending: p.pending });
      for (const p of rows) {
        cmd.getContact({ user_id: p.user_id })
          .then((c) => c && setLastContact(c.user_id, c.last_contact_at))
          .catch(() => {});
      }
      unsub.push(await events.onHistoryChanged(({ user_id }) => {
        if (user_id !== viewedUserId()) return;
        void hydrateFrom(user_id, () => cmd.listHistory({ user_id, limit: HISTORY_PAGE }))
          .catch(() => {});
      }));
      unsub.push(await events.onConnectionState(({ user_id, state, last_error }) => {
        setStatus(user_id, last_error !== undefined ? { state, last_error } : { state });
        usePairingsStore.getState().updateStatus(user_id, state);
      }));
      unsub.push(await events.onPendingCount(({ user_id, count }) => {
        setStatus(user_id, { pending: count });
      }));
      unsub.push(await events.onPairingAdded(() => {
        cmd.listPairings().then(hydratePairings).catch(() => {});
      }));
      unsub.push(await events.onPairingRemoved(({ user_id }) => {
        usePairingsStore.getState().remove(user_id);
        // The Viewed Pairing can outlive the pairing it named; drop it so the
        // pane falls back to the Active one rather than showing a ghost.
        if (useUiStore.getState().viewedUserId === user_id) {
          useUiStore.getState().setViewedUserId(undefined);
        }
      }));
      unsub.push(await events.onActivePairingChanged(({ user_id }) => {
        usePairingsStore.getState().setActive(user_id ?? undefined);
      }));
      unsub.push(await events.onContact(({ user_id, last_contact_at }) => {
        setLastContact(user_id, last_contact_at);
      }));
    })();
    return () => {
      cancelled = true;
      unsub.forEach((u) => u());
    };
  }, [
    addEntry, hydrateHistory, hydratePairings, refuseEntry, removeEntry, setLastContact,
    settleEntry, setStatus,
  ]);

  // Keyed on `seq`, not the toast object, so the same message twice restarts
  // the window instead of inheriting what is left of the first one.
  const toastSeq = toast?.seq;
  useEffect(() => {
    if (toastSeq === undefined) return;
    const t = window.setTimeout(() => useUiStore.getState().dismissToast(), TOAST_MS);
    return () => window.clearTimeout(t);
  }, [toastSeq]);

  return (
    <div className="fui-panel flex h-full flex-col overflow-hidden bg-surface-panel">
      <Titlebar section={section} />

      <div className="flex min-h-0 flex-1">
        <Rail section={section} onSelect={setSection} version={__APP_VERSION__} />

        <div className="flex min-w-0 flex-1 flex-col">
          <header className="flex h-[38px] shrink-0 items-center justify-between border-b border-hairline bg-surface-panel px-3.5">
            <span className="flex items-baseline gap-2.5">
              <h1 className="m-0 text-sm font-semibold uppercase tracking-phrase text-text-body">
                {TITLE[section]}
              </h1>
              <span
                data-testid="pane-code"
                className="font-mono text-chrome uppercase tracking-phrase text-text-dim"
              >
                {paneCode}
              </span>
            </span>
          </header>

          {section === "history" && <HistorySection now={now} />}
          {section === "pairings" && <PairingsSection />}
          {section === "settings" && <SettingsSection />}

          {toast && (
            <Strip tone={toast.tone} testId="toast">
              <span className="shrink-0 font-semibold tracking-word">[{toast.text}]</span>
              {toast.detail && (
                <span className="min-w-0 truncate font-mono normal-case">{toast.detail}</span>
              )}
            </Strip>
          )}
        </div>
      </div>

      <MainFooter now={now} />
    </div>
  );
}

/**
 * Which Pairing's history the live-entry subscriptions should accept.
 *
 * Read from the stores at call time rather than closed over: these listeners
 * are registered once and must not pin the Viewed Pairing they were born with.
 */
function viewedUserId(): string | undefined {
  return useUiStore.getState().viewedUserId ?? usePairingsStore.getState().active;
}
