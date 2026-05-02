import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { injectForTests } from "../ipc/tauri";
import SettingsModal from "../modals/SettingsModal";

let invokeMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  invokeMock = vi.fn(async (cmd: string, args?: any) => {
    if (cmd === "get_settings") {
      return { capture_enabled: true, deny_list: [], autostart: false, hotkey: null };
    }
    if (cmd === "update_settings") {
      return { capture_enabled: !!args?.patch?.capture_enabled,
               deny_list: args?.patch?.deny_list ?? [],
               autostart: !!args?.patch?.autostart,
               hotkey: args?.patch?.hotkey ?? null };
    }
    return null;
  });
  injectForTests(invokeMock as never, (async () => () => {}) as never);
});

describe("SettingsModal", () => {
  it("toggles capture_enabled and calls update_settings", async () => {
    render(<SettingsModal />);
    const cb = await screen.findByTestId("capture-enabled");
    fireEvent.click(cb);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_settings", expect.objectContaining({
        patch: expect.objectContaining({ capture_enabled: false }),
      }));
    });
  });

  it("renders deny-list as newline-separated", async () => {
    invokeMock.mockImplementationOnce(async () => ({
      capture_enabled: true, deny_list: ["a", "b"], autostart: false, hotkey: null,
    }));
    render(<SettingsModal />);
    const ta = await screen.findByTestId("deny-list") as HTMLTextAreaElement;
    expect(ta.value).toBe("a\nb");
  });
});
