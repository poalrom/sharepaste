import { forwardRef } from "react";
import type { EntryView } from "../types";
import { cmd } from "../ipc/commands";
import { normalizePreview, originLabel, relativeAge } from "../lib/format";
import { useHistoryStore, useUiStore } from "../store";
import { IconButton } from "./fui";

export type EntryRowProps = {
  entry: EntryView;
  /** 1-based position in the visible list, rendered zero-padded. */
  index: number;
  selected: boolean;
  /** The device doing the viewing: an entry captured here shows no Origin (plan §0.8). */
  ownDeviceId: string | undefined;
  /** The list's single clock, so a full page of rows does not run a timer each. */
  now: number;
};

/**
 * Copying, as the row click, `⧉`, `⏎` and `⌘⏎` all mean it.
 *
 * An undecryptable entry is ciphertext this device holds no key for, so there
 * is nothing to put on the clipboard: it says so and stays open rather than
 * hiding on a silent no-op.
 */
export async function copyEntry(entry: EntryView, opts: { keepOpen: boolean }): Promise<void> {
  const { showToast } = useUiStore.getState();
  if (entry.preview === "") {
    showToast({
      tone: "alert",
      text: "CAN'T COPY",
      detail: "This entry was encrypted with a key this device doesn't have.",
    });
    return;
  }
  try {
    await cmd.copyToClipboard({ user_id: entry.user_id, entry_id: entry.id });
  } catch (e) {
    console.error("copy failed", e);
    showToast({ tone: "alert", text: "COPY FAILED" });
    return;
  }
  if (opts.keepOpen) {
    showToast({ tone: "cyan", text: "COPIED", detail: normalizePreview(entry.preview) });
    return;
  }
  // A failed hide is not a failed copy: the text is already on the clipboard,
  // so the alert toast would be a lie.
  await cmd.hidePopover().catch((e) => console.error("hide failed", e));
}

/**
 * Deleting, as `✕` and `⌘⌫` both mean it — unguarded by decision (plan §0.13).
 *
 * The row stays put when the relay refuses, because the store is only pruned
 * once the delete it fans out has actually been accepted.
 */
export async function deleteEntry(entry: EntryView): Promise<void> {
  try {
    await cmd.deleteEntry({ user_id: entry.user_id, entry_id: entry.id });
    useHistoryStore.getState().remove(entry.id);
  } catch (e) {
    console.error("delete failed", e);
  }
}

const EntryRow = forwardRef<HTMLLIElement, EntryRowProps>(function EntryRow(
  { entry, index, selected, ownDeviceId, now },
  ref,
) {
  // Nothing on the wire flags it: a NULL plaintext arrives as an empty preview.
  const undecryptable = entry.preview === "";
  const elsewhere = entry.device_id !== ownDeviceId;

  return (
    <li
      ref={ref}
      data-testid="entry-row"
      data-selected={selected}
      className="fui-row group flex cursor-default items-center gap-2 px-3"
      onClick={() => void copyEntry(entry, { keepOpen: false })}
    >
      {/* Dim measures 4.35:1 on the selected background, just under (plan §1). */}
      <span
        className={`w-4 shrink-0 font-mono text-chrome tabular-nums ${selected ? "text-text-emitter" : "text-text-dim"}`}
      >
        {String(index).padStart(2, "0")}
      </span>

      {undecryptable ? (
        <span className="min-w-0 flex-1 truncate text-label tracking-word text-alert-400">
          UNDECRYPTABLE
        </span>
      ) : (
        <span
          className="min-w-0 flex-1 truncate font-mono text-data text-text-body"
          title={normalizePreview(entry.preview, 400)}
        >
          {normalizePreview(entry.preview)}
        </span>
      )}

      {undecryptable ? (
        <span className="shrink-0 text-chrome tracking-phrase text-alert-400">KEY MISMATCH</span>
      ) : (
        <span className="shrink-0 text-chrome tracking-phrase text-text-muted">
          {elsewhere && (
            <>
              {/*
                The tooltip is the untruncated counterpart of what is shown, not
                a second fallback: a label reads in full, an unlabelled legacy
                membership reads its full device id behind the 4-char slice
                (plan §4). Routing it through originLabel would hide the id.
              */}
              <span className="uppercase" title={entry.device_label?.trim() || entry.device_id}>
                {originLabel(entry.device_label, entry.device_id).slice(0, 12)}
              </span>
              {" · "}
            </>
          )}
          {relativeAge(entry.created_at, now)}
        </span>
      )}

      {/* Reserved rather than swapped in on hover, so the meta never reflows (plan §1.5). */}
      <span
        className={`flex w-11 shrink-0 items-center justify-end gap-1 transition-opacity duration-fast group-focus-within:opacity-100 group-hover:opacity-100 ${selected ? "opacity-100" : "opacity-0"}`}
      >
        {!undecryptable && (
          <IconButton
            label="Copy and keep open"
            title="Copy and keep open"
            className="text-data"
            onClick={(e) => {
              e.stopPropagation();
              void copyEntry(entry, { keepOpen: true });
            }}
          >
            ⧉
          </IconButton>
        )}
        <IconButton
          label="Delete entry"
          tone="alert"
          testId={`delete-entry-${entry.id}`}
          className="text-data"
          onClick={(e) => {
            e.stopPropagation();
            void deleteEntry(entry);
          }}
        >
          ✕
        </IconButton>
      </span>
    </li>
  );
});

export default EntryRow;
