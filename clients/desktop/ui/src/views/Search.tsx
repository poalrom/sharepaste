import { useEffect, useRef } from "react";
import { useUiStore } from "../store";

export default function Search() {
  const search = useUiStore((s) => s.search);
  const setSearch = useUiStore((s) => s.setSearch);
  const ref = useRef<HTMLInputElement>(null);

  // The popover window is shown and hidden, never unmounted (popover.rs uses
  // show()/hide()), so `autoFocus` fires exactly once in the window's lifetime.
  // Without this, focus stays wherever the last interaction left it - typically
  // the footer button that opened the main window and hid the popover - and
  // since HistoryList ignores keydown while a button holds focus, the reopened
  // popover is keyboard-dead until the user clicks something.
  //
  // Keying off the window's focus event is safe precisely because the popover
  // hides on Focused(false): it can never be visible-but-unfocused, so "window
  // gained focus" always means "just shown". Focus moving *within* the popover
  // does not fire this, so it never fights the user's own focus choices.
  useEffect(() => {
    const focusSearch = () => {
      const input = ref.current;
      if (!input) return;
      input.focus();
      // Select rather than clear: the list is still filtered by the old query,
      // so leaving it visible is honest, and typing replaces it.
      input.select();
    };
    focusSearch();
    window.addEventListener("focus", focusSearch);
    return () => window.removeEventListener("focus", focusSearch);
  }, []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const input = ref.current;
      if (!input || document.activeElement === input) return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key.length !== 1) return;
      input.focus();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <div className="border-b border-zinc-700 p-2">
      <input
        ref={ref}
        className="w-full rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100 outline-none focus:ring-1 focus:ring-blue-500"
        placeholder="Search history…"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
    </div>
  );
}
