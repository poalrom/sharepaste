import { describe, it, expect, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { useUiStore } from "../store";
import type { EntryView } from "../types";
import EntryRow from "../views/EntryRow";

const NOW = 1_700_000_000_000;
const base: EntryView = {
  id: 1,
  user_id: "u",
  preview: "npm run dev",
  created_at: NOW - 2 * 60_000,
  device_id: "own",
};

let ipc: MockIpc;

/** Rows are `<li>`; the list around them is the only thing HistoryList adds. */
function renderRow(entry: Partial<EntryView> = {}, ownDeviceId = "own") {
  return render(
    <ul>
      <EntryRow
        entry={{ ...base, ...entry }}
        index={1}
        selected={false}
        ownDeviceId={ownDeviceId}
        now={NOW}
      />
    </ul>,
  );
}

describe("EntryRow", () => {
  beforeEach(() => {
    ipc = mockIpc();
    useUiStore.getState().dismissToast();
  });

  it("copies and hides the popover on a row click", async () => {
    renderRow();
    fireEvent.click(screen.getByTestId("entry-row"));
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("copy_to_clipboard", { args: { user_id: "u", entry_id: 1 } });
      expect(ipc.invoke).toHaveBeenCalledWith("hide_popover", undefined);
    });
  });

  it("copies and stays open when ⧉ is used", async () => {
    renderRow();
    fireEvent.click(screen.getByRole("button", { name: "Copy and keep open" }));
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("copy_to_clipboard", { args: { user_id: "u", entry_id: 1 } });
      expect(useUiStore.getState().toast).toMatchObject({
        tone: "cyan",
        text: "COPIED",
        detail: "npm run dev",
      });
    });
    expect(ipc.invoke).not.toHaveBeenCalledWith("hide_popover", undefined);
  });

  it("explains itself instead of copying an undecryptable entry", async () => {
    renderRow({ preview: "" });
    expect(screen.queryByRole("button", { name: "Copy and keep open" })).toBeNull();
    fireEvent.click(screen.getByTestId("entry-row"));
    await waitFor(() => {
      expect(useUiStore.getState().toast).toMatchObject({
        tone: "alert",
        text: "CAN'T COPY",
        detail: "This entry was encrypted with a key this device doesn't have.",
      });
    });
    expect(ipc.invoke).not.toHaveBeenCalled();
  });

  it("names the origin device for an entry captured elsewhere", () => {
    renderRow({ device_id: "other", device_label: "iPhone-15" });
    expect(screen.getByTestId("entry-row")).toHaveTextContent("iPhone-15 · 2m");
  });

  it("omits the origin for an entry captured on this device", () => {
    renderRow({ device_id: "own", device_label: "this-mac" });
    const row = screen.getByTestId("entry-row");
    expect(row).not.toHaveTextContent("this-mac");
    expect(row).toHaveTextContent("2m");
  });
});
