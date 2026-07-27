import type { MainSection } from "../../store/ui";

const ITEMS: Array<{ key: MainSection; glyph: string; label: string }> = [
  { key: "history", glyph: "▤", label: "History" },
  { key: "pairings", glyph: "◎", label: "Pairings" },
  { key: "settings", glyph: "⊕", label: "Settings" },
];

/**
 * The pane switcher.
 *
 * A rail rather than tabs because the window now has a pane whose whole job is
 * vertical reading, and a tab strip spends the top of it. Selection is drawn
 * exactly as a selected list row is — cyan wash, 2px left edge — so the window
 * has one visual grammar for "this is the thing you are looking at".
 */
export default function Rail({
  section,
  onSelect,
  version,
}: {
  section: MainSection;
  onSelect: (s: MainSection) => void;
  version: string;
}) {
  return (
    <nav
      role="tablist"
      aria-orientation="vertical"
      aria-label="Sections"
      className="flex w-[76px] shrink-0 flex-col justify-between border-r border-emitter bg-surface-panel"
    >
      <div className="flex flex-col">
        {ITEMS.map((item) => (
          <button
            key={item.key}
            type="button"
            role="tab"
            data-testid={`rail-${item.key}`}
            aria-selected={section === item.key}
            onClick={() => onSelect(item.key)}
            className="fui-rail-item"
          >
            <span aria-hidden="true" className="font-mono text-base leading-none">
              {item.glyph}
            </span>
            <span className="font-mono text-[9px] uppercase leading-none tracking-word">
              {item.label}
            </span>
          </button>
        ))}
      </div>
      <div className="border-t border-hairline px-1.5 py-2.5 text-center font-mono text-chrome tracking-phrase text-text-dim">
        v{version}
      </div>
    </nav>
  );
}
