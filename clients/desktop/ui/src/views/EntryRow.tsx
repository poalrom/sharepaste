import { forwardRef } from "react";
import type { EntryView } from "../types";
import { cmd } from "../ipc/commands";
import { useHistoryStore } from "../store";

type Props = { entry: EntryView; selected: boolean };

const EntryRow = forwardRef<HTMLLIElement, Props>(function EntryRow({ entry, selected }, ref) {
  return (
    <li
      ref={ref}
      data-testid="entry-row"
      data-selected={selected}
      className={`group flex items-center gap-2 px-3 py-2 text-sm cursor-default ${selected ? "bg-zinc-700" : "hover:bg-zinc-800"}`}
      onClick={async () => {
        try {
          await cmd.copyToClipboard({ user_id: entry.user_id, entry_id: entry.id });
          await cmd.hidePopover();
        } catch (e) {
          console.error("copy failed", e);
        }
      }}
    >
      <div className="min-w-0 flex-1 truncate">
        {entry.preview || <span className="text-zinc-500">(undecryptable)</span>}
      </div>
      <button
        aria-label="Delete entry"
        data-testid={`delete-entry-${entry.id}`}
        className={`shrink-0 rounded p-1 text-zinc-400 hover:bg-zinc-900 hover:text-red-300 focus-visible:opacity-100 group-hover:opacity-100 ${selected ? "opacity-100" : "opacity-0"}`}
        onClick={async (e) => {
          e.stopPropagation();
          try {
            await cmd.deleteEntry({ user_id: entry.user_id, entry_id: entry.id });
            useHistoryStore.getState().remove(entry.id);
          } catch (err) {
            console.error("delete failed", err);
          }
        }}
      >
        <TrashIcon />
      </button>
    </li>
  );
});

export default EntryRow;

function TrashIcon() {
  return (
    <svg
      width="14"
      height="14"
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
