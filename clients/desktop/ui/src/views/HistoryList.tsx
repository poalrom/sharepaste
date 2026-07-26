import { useMemo, useEffect, useRef } from "react";
import { useHistoryStore, useUiStore, useAccountsStore } from "../store";
import { cmd } from "../ipc/commands";
import EntryRow from "./EntryRow";

export default function HistoryList() {
  const entries = useHistoryStore((s) => s.entries);
  const search = useUiStore((s) => s.search);
  const selectedIndex = useUiStore((s) => s.selectedIndex);
  const setSelectedIndex = useUiStore((s) => s.setSelectedIndex);
  const active = useAccountsStore((s) => s.active);

  const filtered = useMemo(() => {
    if (!search.trim()) return entries;
    const needle = search.toLowerCase();
    return entries.filter((e) => e.preview.toLowerCase().includes(needle));
  }, [entries, search]);

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
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex(Math.min(filtered.length - 1, selectedIndex + 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex(Math.max(0, selectedIndex - 1));
      } else if (e.key === "Enter") {
        const target = filtered[selectedIndex];
        if (target) {
          try {
            await cmd.copyToClipboard({ user_id: target.user_id, entry_id: target.id });
            await cmd.hidePopover();
          } catch (err) {
            console.error("copy failed", err);
          }
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [active, filtered, selectedIndex, setSelectedIndex]);

  if (filtered.length === 0) {
    return <div className="flex flex-1 items-center justify-center text-sm text-zinc-500">No entries.</div>;
  }
  return (
    <ul className="flex-1 overflow-auto">
      {filtered.map((e, i) => (
        <EntryRow key={e.id} entry={e} selected={i === selectedIndex} ref={i === selectedIndex ? selectedRef : undefined} />
      ))}
    </ul>
  );
}
