import { useUiStore } from "../store";

export default function Search() {
  const search = useUiStore((s) => s.search);
  const setSearch = useUiStore((s) => s.setSearch);
  return (
    <div className="border-b border-zinc-700 p-2">
      <input
        autoFocus
        className="w-full rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100 outline-none focus:ring-1 focus:ring-blue-500"
        placeholder="Search history…"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
    </div>
  );
}
