import { describe, it, expect, beforeEach, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { useUiStore } from "../store";
import type { EntryView } from "../types";
import EntryRow, { copyEntry, type EntryRowProps } from "../views/EntryRow";

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
/** The list's own "this row is the addressed one now", which is all a pane click does. */
const addressed = vi.fn();

/**
 * One of the two configurations the row serves, exactly as its list passes it.
 *
 * The row is never told which window it is in, so every case below reads its
 * expectation off an argument rather than off a surface's name: what a click
 * means, and whether the controls column and the Preview's own tooltip were
 * asked for. Everything not named here — the whole slot readout — is the same
 * on both, which is the point of there being one row.
 */
type Surface = {
  name: string;
  props: Pick<EntryRowProps, "controls" | "previewTooltip" | "metrics">;
  /** What a click on this list's row does. */
  onActivate: (entry: EntryView) => void;
  /** And what it must be seen to have done. */
  expectActivated: () => Promise<void>;
  /** The same, on a row this device holds no key for. */
  expectActivatedUndecryptable: () => Promise<void>;
};

const SURFACES: Surface[] = [
  {
    name: "the picker's row",
    props: {
      controls: true,
      previewTooltip: true,
      metrics: { gap: "gap-2", index: "w-4" },
    },
    // A pick: copy it and get the window out of the way (ADR 0002).
    onActivate: (entry) => void copyEntry(entry, { keepOpen: false }),
    expectActivated: async () => {
      await waitFor(() => {
        expect(ipc.invoke).toHaveBeenCalledWith("copy_to_clipboard", {
          args: { user_id: "u", entry_id: 1 },
        });
        expect(ipc.invoke).toHaveBeenCalledWith("hide_popover", undefined);
      });
    },
    expectActivatedUndecryptable: async () => {
      await waitFor(() => {
        expect(useUiStore.getState().toast).toMatchObject({
          tone: "alert",
          text: "CAN'T COPY",
          detail: "This entry was encrypted with a key this device doesn't have.",
        });
      });
      expect(ipc.invoke).not.toHaveBeenCalled();
    },
  },
  {
    name: "the reader's row",
    props: {
      controls: false,
      previewTooltip: false,
      metrics: { gap: "gap-2.5", index: "w-[18px]" },
    },
    // Addressing is the whole of it. The pane beside this list is what reads,
    // and its own COPY is what copies (ADR 0003).
    onActivate: () => addressed(),
    expectActivated: async () => {
      expect(addressed).toHaveBeenCalled();
      expect(ipc.invoke).not.toHaveBeenCalled();
    },
    expectActivatedUndecryptable: async () => {
      expect(addressed).toHaveBeenCalled();
      // Nothing is explained because nothing was attempted: a list that does
      // not copy has no refusal to report.
      expect(useUiStore.getState().toast).toBeUndefined();
      expect(ipc.invoke).not.toHaveBeenCalled();
    },
  },
];

/**
 * An act this device holds and the relay has not: a capture with no relay id and
 * so no stamp at all, which is why both timestamps are 0 (ADR 0016).
 */
const unflushed: Partial<EntryView> = {
  preview: "offline copy",
  plaintext: "offline copy",
  created_at: 0,
  last_use: 0,
  pending: true,
};

const refused: Partial<EntryView> = {
  ...unflushed,
  refused_reason: "payload too large",
};

/**
 * Rows are `<li>`; the list around them is the only thing either caller adds.
 * Selected by default: the controls exist only on the addressed row, so an
 * unselected fixture would pass the "no copy button" assertions for the wrong
 * reason.
 */
function renderRow(
  surface: Surface,
  entry: Partial<EntryView> = {},
  ownDeviceId: string | undefined = "own",
  selected = true,
) {
  const full: EntryView = { ...base, ...entry };
  return render(
    <ul>
      <EntryRow
        entry={full}
        index={1}
        selected={selected}
        ownDeviceId={ownDeviceId}
        now={NOW}
        onPoint={() => {}}
        onActivate={() => surface.onActivate(full)}
        {...surface.props}
      />
    </ul>,
  );
}

describe.each(SURFACES)("EntryRow — $name", (surface) => {
  beforeEach(() => {
    ipc = mockIpc();
    addressed.mockClear();
    useUiStore.getState().dismissToast();
  });

  it("does with a click what its own list says a click means", async () => {
    renderRow(surface);
    fireEvent.click(screen.getByTestId("entry-row"));
    await surface.expectActivated();
  });

  it("copies and stays open when ⧉ is used", async () => {
    renderRow(surface);
    const keepOpen = screen.queryByRole("button", { name: "Copy and keep open" });
    // Where the controls column was not asked for, the verb is not on the row
    // at all — the reader pane beside that list is where it lives.
    if (!surface.props.controls) {
      expect(keepOpen).toBeNull();
      return;
    }

    fireEvent.click(keepOpen!);
    await waitFor(() => {
      expect(ipc.invoke).toHaveBeenCalledWith("copy_to_clipboard", {
        args: { user_id: "u", entry_id: 1 },
      });
      expect(useUiStore.getState().toast).toMatchObject({
        tone: "cyan",
        text: "COPIED",
        detail: "npm run dev",
      });
    });
    expect(ipc.invoke).not.toHaveBeenCalledWith("hide_popover", undefined);
  });

  it("explains itself instead of copying an undecryptable entry", async () => {
    renderRow(surface, { preview: "", undecryptable: true, pending: false, refused_reason: null });
    expect(screen.queryByRole("button", { name: "Copy and keep open" })).toBeNull();

    fireEvent.click(screen.getByTestId("entry-row"));
    await surface.expectActivatedUndecryptable();
  });

  it("names the origin device for an entry captured elsewhere", () => {
    renderRow(surface, { device_id: "other", device_label: "iPhone-15", origin_label: "iPhone-15" });
    expect(screen.getByTestId("entry-row")).toHaveTextContent("iPhone-15 · 2m");
  });

  it("omits the origin for an entry captured on this device", () => {
    renderRow(surface, { device_id: "own", device_label: "this-mac", origin_label: "this-mac" });
    const row = screen.getByTestId("entry-row");
    expect(row).not.toHaveTextContent("this-mac");
    expect(row).toHaveTextContent("2m");
  });

  /*
    The tooltip is the untruncated counterpart of a Preview that reads as one
    truncated line, and only a list with nothing beside it has to supply one:
    the reader's pane renders the whole entry already (ADR 0003).
  */
  it("gives the Preview its own tooltip only where nothing else holds the text", () => {
    renderRow(surface);
    const preview = screen.getByText("npm run dev");
    if (surface.props.previewTooltip) {
      expect(preview).toHaveAttribute("title", "npm run dev");
    } else {
      expect(preview).not.toHaveAttribute("title");
    }
  });

  // The meta has to reach the row's edge, so the control column cannot be
  // reserved on rows that are not being addressed - and a reserved-but-
  // transparent column left two clickable buttons over every timestamp.
  it("gives an unaddressed row no controls at all", () => {
    renderRow(surface, {}, "own", false);
    expect(screen.queryByRole("button")).toBeNull();
  });

  // The age column reads the Use, not the capture. An entry recalled two
  // minutes ago sits at the head of the list, and "21d" beside it would make
  // the order look broken rather than the row look old.
  it("ages a recalled entry from its last use and not its capture", () => {
    renderRow(surface, { created_at: NOW - 21 * DAY, last_use: NOW - 2 * 60_000 });
    const row = screen.getByTestId("entry-row");
    expect(row).toHaveTextContent("2m");
    expect(row).not.toHaveTextContent("21d");
  });

  describe("an act the relay has not heard", () => {
    /*
      The tint is the whole statement, and the slot's silence is the other half
      of it: there is one clock in this system and it is the relay's, so an entry
      it has never stamped has no age. `relativeAge(0)` would print the age of
      the epoch — 655mo — beside the newest row in the list.
    */
    it("tints an un-flushed capture and leaves its time slot empty", () => {
      // Unaddressed, so the row's whole text is the index and the Preview: the
      // controls of a selected row would be part of it.
      renderRow(surface, unflushed, "own", false);
      const row = screen.getByTestId("entry-row");
      expect(row).toHaveAttribute("data-pending", "true");
      expect(row.textContent).toBe("01offline copy");
    });

    // Un-flushed is not unusable: the payload is an Entry from the moment of
    // capture, so the row's own verbs work on it like any other.
    it("still copies and deletes an un-flushed capture from the row", async () => {
      renderRow(surface, unflushed);
      if (!surface.props.controls) {
        expect(screen.queryByRole("button")).toBeNull();
        return;
      }

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
      renderRow(surface, unflushed);
      expect(screen.queryByTestId("resend-entry-1")).toBeNull();
    });

    /*
      A queued *use* is the other kind of pending, and it acts on an entry the
      relay already stamped. The slot keeps that stamp: it is stale by definition —
      the relay has not recorded the use yet — and stale is what the relay's last
      word is.
    */
    it("keeps the stale relay age on an entry whose use is still queued", () => {
      renderRow(surface, { pending: true, last_use: NOW - 30 * 60_000 });
      const row = screen.getByTestId("entry-row");
      expect(row).toHaveAttribute("data-pending", "true");
      expect(row).toHaveTextContent("30m");
    });
  });

  describe("a refused act", () => {
    /*
      The reason is a word rather than a colour on purpose: selection wins over
      the tint in this list, so a refused row that said nothing would be a plain
      row for as long as it was the addressed one.
    */
    it("states the relay's reason where the age goes, in alert", () => {
      renderRow(surface, refused);
      const reason = screen.getByText("payload too large");
      expect(reason).toHaveClass("text-alert-400");
      expect(screen.getByTestId("entry-row")).toHaveAttribute("data-pending", "true");
    });

    // Orthogonal facts, one slot. Refused wins because it is the one of the two a
    // person can do something about, and `↻` is that something.
    it("shows the refusal, not the key mismatch, on a row that is both", () => {
      renderRow(surface, { ...refused, preview: "", plaintext: null, undecryptable: true });
      const row = screen.getByTestId("entry-row");
      expect(row).toHaveTextContent("payload too large");
      expect(row).not.toHaveTextContent("KEY MISMATCH");
      // The Preview column is unchanged by any of this.
      expect(row).toHaveTextContent("UNDECRYPTABLE");
    });

    it("resends the row's own act on ↻, without copying it", async () => {
      renderRow(surface, refused);
      if (!surface.props.controls) {
        expect(screen.queryByTestId("resend-entry-1")).toBeNull();
        return;
      }

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
});
