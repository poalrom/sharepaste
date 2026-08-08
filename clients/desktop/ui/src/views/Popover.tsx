import { useEffect, useState } from "react";
import { attachHistory } from "../attachHistory";
import { cmd } from "../ipc/commands";
import { agePhrase } from "../lib/format";
import { useNow } from "../lib/useNow";
import {
  usePairingsStore,
  useStatusStore,
  useContactStore,
  useUiStore,
} from "../store";
import Filter from "./Filter";
import Footer from "./Footer";
import HintStrip from "./HintStrip";
import HistoryList from "./HistoryList";
import { CONNECTION } from "./connection";
import { PanelMessage, Strip } from "./fui";

/** Mirrors --dur-sweep / --dur-toast: the CSS owns the animation, this owns the state. */
const SWEEP_MS = 900;
const TOAST_MS = 2200;

export default function Popover() {
  const pairings = usePairingsStore((s) => s.pairings);
  const active = usePairingsStore((s) => s.active);
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
      useUiStore.getState().setFilter("");
      useUiStore.getState().setSelectedIndex(0);
    };
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, []);

  // `focus` is the signal that the window was just shown, for the reason
  // Filter.tsx documents: it is shown and hidden, never unmounted, and it hides
  // on Focused(false), so it can never be visible-but-unfocused.
  //
  // The sweep rides its own overlay rather than the panel element because
  // restarting the animation means remounting whatever carries it, and
  // remounting the panel would tear the Filter box out from under the focus the
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

  // The popover's scope is the Active Pairing itself, so this attach is also
  // what shows it: there is no second pane here that would take the snapshot.
  useEffect(
    () =>
      attachHistory({
        userId: () => usePairingsStore.getState().active,
        showsHistory: true,
      }),
    [],
  );

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
          <Filter />
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
