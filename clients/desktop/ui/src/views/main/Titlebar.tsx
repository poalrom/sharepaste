import { tauri } from "../../ipc/tauri";
import { IconButton } from "../fui";
import type { MainSection } from "../../store/ui";

const SLUG: Record<MainSection, string> = {
  history: "/ HISTORY",
  pairings: "/ PAIRINGS",
  settings: "/ SETTINGS",
};

/**
 * The window's own titlebar, because the OS one is off.
 *
 * `decorations(false)` is not decoration: the panel's notch is cut into the
 * window's corners, and a square OS titlebar above it reads as a rendering
 * fault. The cost is that dragging, minimising and maximising are ours — hence
 * `data-tauri-drag-region` on the bar and explicit buttons on the right, which
 * are excluded from the drag region so a click on `✕` is a click, not a drag.
 *
 * The mock also printed `// CONSOLE` here. Dropped: `CONTEXT.md` puts "console"
 * on the Main Window's _Avoid_ line, and the slug beside it already says which
 * pane you are in.
 */
export default function Titlebar({ section }: { section: MainSection }) {
  return (
    <div
      data-tauri-drag-region
      data-testid="titlebar"
      className="fui-band flex h-[30px] shrink-0 select-none items-center justify-between border-b border-hairline pl-3 pr-1"
    >
      <span data-tauri-drag-region className="flex items-center gap-2 text-label uppercase">
        <span className="tracking-word text-text-emitter">SHAREPASTE</span>
        <span className="font-mono text-chrome tracking-phrase text-text-dim">{SLUG[section]}</span>
      </span>
      <span className="flex items-center gap-0.5">
        <IconButton label="Minimise" testId="win-minimise" onClick={() => void tauri.window.minimize()}>
          <span aria-hidden="true" className="text-chrome leading-none">▼</span>
        </IconButton>
        <IconButton label="Maximise" testId="win-maximise" onClick={() => void tauri.window.toggleMaximize()}>
          <span aria-hidden="true" className="text-chrome leading-none">⤢</span>
        </IconButton>
        <IconButton label="Close" tone="alert" testId="win-close" onClick={() => void tauri.window.close()}>
          <span aria-hidden="true" className="text-chrome leading-none">✕</span>
        </IconButton>
      </span>
    </div>
  );
}
