import { useEffect, useRef } from "react";
import { useActiveAccount, useFilteredEntries, useHistoryStore, useUiStore } from "../store";
import { useNow } from "../lib/useNow";
import EntryRow, { copyEntry, deleteEntry } from "./EntryRow";
import { PanelMessage } from "./fui";

/** The prune cap enforced in `store/history.ts`; the sentinel explains it in situ. */
const CACHE_CAP = 100;

export default function HistoryList() {
  const entries = useHistoryStore((s) => s.entries);
  const filtered = useFilteredEntries();
  const search = useUiStore((s) => s.search);
  const setSearch = useUiStore((s) => s.setSearch);
  const selectedIndex = useUiStore((s) => s.selectedIndex);
  const setSelectedIndex = useUiStore((s) => s.setSelectedIndex);
  const activePairing = useActiveAccount();
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
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex(Math.min(filtered.length - 1, selectedIndex + 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex(Math.max(0, selectedIndex - 1));
      } else if (e.key === "Enter" && target) {
        await copyEntry(target, { keepOpen: e.metaKey || e.ctrlKey });
      } else if (e.key === "Backspace" && (e.metaKey || e.ctrlKey)) {
        // Modified, because Search keeps the input focused essentially always:
        // a bare Backspace would collide with editing the query (plan §6).
        e.preventDefault();
        if (target) await deleteEntry(target);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [active, filtered, selectedIndex, setSelectedIndex]);

  if (filtered.length === 0) {
    if (entries.length === 0) return <PanelMessage title="HISTORY EMPTY" />;
    return (
      <PanelMessage
        title="NO MATCHES"
        detail={`Nothing matches "${search}"`}
        action={{ label: "⌫ CLEAR FILTER", onClick: () => setSearch(""), variant: "outline" }}
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
      {!search.trim() && entries.length >= CACHE_CAP && (
        <li className="px-3 py-2 text-center text-chrome tracking-phrase text-text-dim">
          — OLDEST OF {CACHE_CAP} CACHED —
        </li>
      )}
    </ul>
  );
}
