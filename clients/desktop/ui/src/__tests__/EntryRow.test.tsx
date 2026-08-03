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

/**
 * An act this device holds and the relay has not: a capture with no relay id and
 * so no stamp at all, which is why both timestamps are 0 (ADR 0013).
 */
const unflushed: Partial<EntryView> = {
  preview: "offline copy",
  plaintext: "offline copy",
  created_at: 0,
  last_use: 0,
  pending: true,
};

describe("EntryRow — an act the relay has not heard", () => {
  beforeEach(() => {
    ipc = mockIpc();
    useUiStore.getState().dismissToast();
  });

  /*
    The tint is the whole statement, and the slot's silence is the other half of
    it: there is one clock in this system and it is the relay's, so an entry it
    has never stamped has no age. `relativeAge(0)` would print the age of the
    epoch — 655mo — beside the newest row in the list.
  */
  it("tints an un-flushed capture and leaves its time slot empty", () => {
    // Unaddressed, so the row's whole text is the index and the Preview: the
    // controls of a selected row would be part of it.
    renderRow(unflushed, "own", false);
    const row = screen.getByTestId("entry-row");
    expect(row).toHaveAttribute("data-pending", "true");
    expect(row.textContent).toBe("01offline copy");
  });

  // Un-flushed is not unusable: the payload is an Entry from the moment of
  // capture, so the row's own verbs work on it like any other.
  it("still copies and deletes an un-flushed capture from the row", async () => {
    renderRow(unflushed);
    fireEvent.click(screen.getByRole("button", { name: "Copy and keep open" }));
    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("copy_to_clipboard", {
        args: { user_id: "u", entry_id: 1 },
      }),
    );

    fireEvent.click(screen.getByTestId("delete-entry-1"));
    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("delete_entry", {
        args: { user_id: "u", entry_id: 1 },
      }),
    );
  });

  // Nothing to resend while the act is still on its way; the control appears
  // only once the relay has turned it down.
  it("offers no resend on a row the relay has merely not reached yet", () => {
    renderRow(unflushed);
    expect(screen.queryByTestId("resend-entry-1")).toBeNull();
  });

  /*
    A queued *use* is the other kind of pending, and it acts on an entry the
    relay already stamped. The slot keeps that stamp: it is stale by definition —
    the relay has not recorded the use yet — and stale is what the relay's last
    word is.
  */
  it("keeps the stale relay age on an entry whose use is still queued", () => {
    renderRow({ pending: true, last_use: NOW - 30 * 60_000 });
    const row = screen.getByTestId("entry-row");
    expect(row).toHaveAttribute("data-pending", "true");
    expect(row).toHaveTextContent("30m");
  });
});

describe("EntryRow — a refused act", () => {
  const refused: Partial<EntryView> = {
    ...unflushed,
    refused_reason: "payload too large",
  };

  beforeEach(() => {
    ipc = mockIpc();
    useUiStore.getState().dismissToast();
  });

  /*
    The reason is a word rather than a colour on purpose: selection wins over
    the tint in this list, so a refused row that said nothing would be a plain
    row for as long as it was the addressed one.
  */
  it("states the relay's reason where the age goes, in alert", () => {
    renderRow(refused);
    const reason = screen.getByText("payload too large");
    expect(reason).toHaveClass("text-alert-400");
    expect(screen.getByTestId("entry-row")).toHaveAttribute("data-pending", "true");
  });

  // Orthogonal facts, one slot. Refused wins because it is the one of the two a
  // person can do something about, and `↻` is that something.
  it("shows the refusal, not the key mismatch, on a row that is both", () => {
    renderRow({ ...refused, preview: "", plaintext: null, undecryptable: true });
    const row = screen.getByTestId("entry-row");
    expect(row).toHaveTextContent("payload too large");
    expect(row).not.toHaveTextContent("KEY MISMATCH");
    // The Preview column is unchanged by any of this.
    expect(row).toHaveTextContent("UNDECRYPTABLE");
  });

  it("resends the row's own act on ↻, without copying it", async () => {
    renderRow(refused);
    fireEvent.click(screen.getByTestId("resend-entry-1"));
    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("resend_entry", {
        args: { user_id: "u", entry_id: 1 },
      }),
    );
    // The row's own click copies, so the control has to stop the event reaching it.
    expect(ipc.invoke).not.toHaveBeenCalledWith("copy_to_clipboard", expect.anything());
  });
});
