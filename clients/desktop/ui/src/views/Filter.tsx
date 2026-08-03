import { useEffect, useRef } from "react";
import { useFilteredEntries, useHistoryStore, useUiStore } from "../store";

export default function Filter() {
  const filter = useUiStore((s) => s.filter);
  const setFilter = useUiStore((s) => s.setFilter);
  const filtered = useFilteredEntries();
  const total = useHistoryStore((s) => s.entries).length;
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
    const focusFilter = () => {
      const input = ref.current;
      if (!input) return;
      input.focus();
      // Select rather than clear: the list is still filtered by the old query,
      // so leaving it visible is honest, and typing replaces it.
      input.select();
    };
    focusFilter();
    window.addEventListener("focus", focusFilter);
    return () => window.removeEventListener("focus", focusFilter);
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
    <div className="fui-band flex h-10 items-center gap-2 border-b border-hairline px-3 focus-within:border-emitter">
      <span aria-hidden="true" className="text-text-dim">
        ⌕
      </span>
      <input
        ref={ref}
        /*
          A needle is a fragment, not prose: macOS reads `tail` as a
          misspelling and floats its own `Tail ×` correction bubble over the
          first row, and would capitalise it. Nothing here is dictated to a
          person, so every text service the field can decline, it does.
        */
        spellCheck={false}
        autoCorrect="off"
        autoCapitalize="off"
        className="min-w-0 flex-1 bg-transparent font-mono text-data text-text-body outline-none placeholder:text-text-dim"
        placeholder="Filter history…"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />
      {total > 0 && (
        <span className="shrink-0 text-chrome tabular-nums text-text-dim">
          {filtered.length}/{total}
        </span>
      )}
    </div>
  );
}
