import { useEffect, useRef } from "react";
import { useActivePairing, useFilteredEntries, useHistoryStore, useUiStore } from "../store";
import { useNow } from "../lib/useNow";
import EntryRow, { copyEntry, deleteEntry } from "./EntryRow";
import { PanelMessage } from "./fui";

/** The prune cap enforced in `store/history.ts`; the sentinel explains it in situ. */
const CACHE_CAP = 100;

export default function HistoryList() {
  const entries = useHistoryStore((s) => s.entries);
  const filtered = useFilteredEntries();
  const filter = useUiStore((s) => s.filter);
  const setFilter = useUiStore((s) => s.setFilter);
  const selectedIndex = useUiStore((s) => s.selectedIndex);
  const setSelectedIndex = useUiStore((s) => s.setSelectedIndex);
  const activePairing = useActivePairing();
  const active = activePairing?.user_id;
  const ownDeviceId = activePairing?.device_id;
  // One clock for the whole list: every row reads the same tick.
  const now = useNow(60_000);

  const selectedRef = useRef<HTMLLIElement | null>(null);

  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex, filtered.length]);

  useEffect(() => {
    const handler = async (e: KeyboardEvent) => {
      if (!active) return;
      // Focus can legitimately sit on the footer Accounts/Settings buttons or a
      // row's delete control; let those own their own Enter/space handling.
      if (e.target instanceof HTMLElement && e.target.closest("button")) return;
      const target = filtered[selectedIndex];
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        // Guarded because this listener is on the window and outlives the
        // rendered list: with no rows, the modulo below would be NaN and
        // selectedIndex would never recover.
        if (filtered.length === 0) return;
        const step = e.key === "ArrowDown" ? 1 : -1;
        // Wraps, so the oldest entry is one key from the newest. `+ length`
        // keeps the operand positive, since JS `%` returns the sign of it.
        setSelectedIndex((selectedIndex + step + filtered.length) % filtered.length);
      } else if (e.key === "Enter" && target) {
        await copyEntry(target, { keepOpen: e.metaKey || e.ctrlKey });
      } else if (e.key === "Backspace" && (e.metaKey || e.ctrlKey)) {
        // `Filter` keeps the input focused essentially always, so a bare
        // Backspace belongs to the query being typed — and so does the
        // modified one: both platforms bind `⌘⌫`/`Ctrl+⌫` inside a text field
        // already (ADR 0013). Deleting an entry adds ⇧ on top. Prevented
        // either way, so the browser's own erase never runs beside this.
        e.preventDefault();
        if (e.shiftKey) {
          if (target) await deleteEntry(target);
        } else {
          setFilter("");
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [active, filtered, selectedIndex, setSelectedIndex, setFilter]);

  if (filtered.length === 0) {
    if (entries.length === 0) return <PanelMessage title="HISTORY EMPTY" />;
    return (
      <PanelMessage
        title="NO MATCHES"
        detail={`Nothing matches "${filter}"`}
        action={{ label: "⌫ CLEAR FILTER", onClick: () => setFilter(""), variant: "outline" }}
      />
    );
  }

  return (
    <ul className="fui-scroll flex-1 overflow-auto">
      {filtered.map((e, i) => (
        <EntryRow
          key={e.id}
          entry={e}
          index={i + 1}
          selected={i === selectedIndex}
          ownDeviceId={ownDeviceId}
          now={now}
          onPoint={() => setSelectedIndex(i)}
          ref={i === selectedIndex ? selectedRef : undefined}
        />
      ))}
      {/*
        Counted over the settled rows alone. The sentinel is a statement about
        *retention* — the hundred rows the relay has ordered — and the caps no
        longer bound the un-flushed region at all: an act this device has not
        delivered is undelivered clipboard content, and evicting one to protect a
        display invariant is the trade ADR 0014 refuses. Counting every row would
        fire this at a page of offline captures, about a cap that has not bitten.
      */}
      {!filter.trim() && entries.filter((e) => !e.pending).length >= CACHE_CAP && (
        <li className="px-3 py-2 text-center text-chrome tracking-phrase text-text-dim">
          — OLDEST OF {CACHE_CAP} CACHED —
        </li>
      )}
    </ul>
  );
}
