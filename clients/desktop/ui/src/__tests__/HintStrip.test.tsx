import { afterAll, describe, expect, it, vi } from "vitest";
import { render, within } from "@testing-library/react";

const REAL_PLATFORM = navigator.platform;

/**
 * The strip reads the platform once at import time, so each case needs a fresh
 * module rather than a re-render.
 */
async function renderOn(platform: string) {
  Object.defineProperty(window.navigator, "platform", { value: platform, configurable: true });
  vi.resetModules();
  // Dynamic by necessity: this exercises the module-loading boundary itself.
  // A static import binds one platform for the whole file, and the platform is
  // read at module scope, so only a re-import after resetModules re-reads it.
  const { default: HintStrip } = await import("../views/HintStrip");
  return render(<HintStrip />);
}

const readHints = (container: HTMLElement) =>
  [...container.querySelectorAll("kbd")].map((kbd) => ({
    key: kbd.textContent,
    action: kbd.nextElementSibling?.textContent,
  }));

afterAll(() => {
  Object.defineProperty(window.navigator, "platform", {
    value: REAL_PLATFORM,
    configurable: true,
  });
});

describe("HintStrip", () => {
  // The reader is by definition someone who does not know the binding yet, so
  // the key has to be named the way it is printed on the keyboard in front of
  // them: a Windows user does not read ⌘ or ⌫.
  it("names the keys in words on Windows", async () => {
    const view = await renderOn("Win32");
    expect(readHints(view.container)).toEqual([
      { key: "ENTER", action: "COPY" },
      { key: "CTRL+ENTER", action: "KEEP OPEN" },
      { key: "CTRL+SHIFT+BKSP", action: "DELETE" },
    ]);
  });

  it("uses the glyphs a mac keyboard is labelled with", async () => {
    const view = await renderOn("MacIntel");
    expect(readHints(view.container)).toEqual([
      { key: "⏎", action: "COPY" },
      { key: "⌘⏎", action: "KEEP OPEN" },
      { key: "⇧⌘⌫", action: "DELETE" },
    ]);
  });

  // "DEL" was the old label and reads as the Delete key, which is not the
  // binding. Every action is a whole verb.
  it("spells every action out rather than abbreviating it", async () => {
    const view = await renderOn("Win32");
    const strip = view.getByTestId("hint-strip");
    for (const action of ["COPY", "KEEP OPEN", "DELETE"]) {
      expect(within(strip).getByText(action)).toBeInTheDocument();
    }
    expect(strip.textContent).not.toMatch(/\bDEL\b|\bNAV\b/);
  });

  it("pairs every key with an action, so no keycap stands unexplained", async () => {
    const view = await renderOn("Win32");
    const hints = readHints(view.container);
    expect(hints).toHaveLength(3);
    expect(hints.every((h) => h.key && h.action)).toBe(true);
  });
});
