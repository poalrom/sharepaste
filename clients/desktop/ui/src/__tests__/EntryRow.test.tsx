import { describe, it, expect, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { useUiStore } from "../store";
import type { EntryView } from "../types";
import EntryRow from "../views/EntryRow";

const NOW = 1_700_000_000_000;
const DAY = 24 * 60 * 60_000;
const base: EntryView = {
  id: 1,
  user_id: "u",
  preview: "npm run dev",
  plaintext: "npm run dev",
  created_at: NOW - 2 * 60_000,
  last_use: NOW - 2 * 60_000,
  device_id: "own",
  origin_label: "own",
  undecryptable: false, pending: false, refused_reason: null,
};

let ipc: MockIpc;

/**
 * Rows are `<li>`; the list around them is the only thing HistoryList adds.
 * Selected by default: the controls exist only on the addressed row, so an
 * unselected fixture would pass the "no copy button" assertions for the wrong
 * reason.
 */
function renderRow(entry: Partial<EntryView> = {}, ownDeviceId = "own", selected = true) {
  return render(
    <ul>
      <EntryRow
        entry={{ ...base, ...entry }}
        index={1}
        selected={selected}
        ownDeviceId={ownDeviceId}
        now={NOW}
        onPoint={() => {}}
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
    renderRow({ preview: "", undecryptable: true, pending: false, refused_reason: null });
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
    renderRow({ device_id: "other", device_label: "iPhone-15", origin_label: "iPhone-15" });
    expect(screen.getByTestId("entry-row")).toHaveTextContent("iPhone-15 · 2m");
  });

  it("omits the origin for an entry captured on this device", () => {
    renderRow({ device_id: "own", device_label: "this-mac", origin_label: "this-mac" });
    const row = screen.getByTestId("entry-row");
    expect(row).not.toHaveTextContent("this-mac");
    expect(row).toHaveTextContent("2m");
  });

  // The meta has to reach the row's edge, so the control column cannot be
  // reserved on rows that are not being addressed - and a reserved-but-
  // transparent column left two clickable buttons over every timestamp.
  it("gives an unaddressed row no controls at all", () => {
    renderRow({}, "own", false);
    expect(screen.queryByRole("button")).toBeNull();
  });

  // The age column reads the Use, not the capture. An entry recalled two
  // minutes ago sits at the head of the list, and "21d" beside it would make
  // the order look broken rather than the row look old.
  it("ages a recalled entry from its last use and not its capture", () => {
    renderRow({ created_at: NOW - 21 * DAY, last_use: NOW - 2 * 60_000 });
    const row = screen.getByTestId("entry-row");
    expect(row).toHaveTextContent("2m");
    expect(row).not.toHaveTextContent("21d");
  });
});
