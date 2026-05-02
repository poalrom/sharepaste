import type { EntryView } from "../types";
import { cmd } from "../ipc/commands";

type Props = { entry: EntryView; selected: boolean };

export default function EntryRow({ entry, selected }: Props) {
  return (
    <li
      data-testid="entry-row"
      data-selected={selected}
      className={`px-3 py-2 text-sm cursor-default ${selected ? "bg-zinc-700" : "hover:bg-zinc-800"}`}
      onClick={() => cmd.copyToClipboard({ user_id: entry.user_id, entry_id: entry.id })}
    >
      <div className="truncate">{entry.preview || <span className="text-zinc-500">(undecryptable)</span>}</div>
    </li>
  );
}
