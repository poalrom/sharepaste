import { describe, expect, it, vi } from "vitest";
import { injectForTests, type Invoker, type Listener } from "../ipc/tauri";
import { cmd } from "../ipc/commands";

describe("cmd.openSection", () => {
  it("invokes open_main_window then hide_popover", async () => {
    const calls: Array<{ cmd: string; args: unknown }> = [];
    const invoke: Invoker = async (cmd, args) => {
      calls.push({ cmd, args: args ?? null });
      return undefined as never;
    };
    const listen: Listener = async () => () => {};
    injectForTests(invoke, listen);

    await cmd.openSection("pairing");

    expect(calls).toEqual([
      { cmd: "open_main_window", args: { args: { section: "pairing" } } },
      { cmd: "hide_popover", args: null },
    ]);
  });
});
