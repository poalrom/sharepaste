import { forwardRef } from "react";
import type { EntryView } from "../types";
import { cmd } from "../ipc/commands";
import { relativeAge } from "../lib/format";
import { useHistoryStore, useUiStore } from "../store";
import { IconButton } from "./fui";

/**
 * The two measurements a row does not get to choose.
 *
 * A picker's row sits in 360px of popover and a reader's row in 980px of
 * window, so the two breathe differently — the same argument ADR 0013 makes
 * about the two hint strips, which were never one component for exactly this
 * reason. Each list states its own, because the row is never told which window
 * it is in.
 */
export type RowMetrics = {
  /** Space between the row's slots. */
  gap: string;
  /** Width of the zero-padded index column. */
  index: string;
};

export type EntryRowProps = {
  entry: EntryView;
  /** 1-based position in the visible list, rendered zero-padded. */
  index: number;
  selected: boolean;
  /** The device doing the viewing: an entry captured here shows no Origin (plan §0.8). */
  ownDeviceId: string | undefined;
  /** The list's single clock, so a full page of rows does not run a timer each. */
  now: number;
  /** Pointing at a row addresses it, so the controls are always one motion away. */
  onPoint: () => void;
  /**
   * What a click on the row means, which is the one thing the two lists most
   * disagree about. A picker is opened to take something out of it, so a click
   * copies and gets the window out of the way (ADR 0002); a reader is a
   * document you walk, so a click only addresses the row and the pane beside it
   * is what copies (ADR 0003).
   */
  onActivate: () => void;
  /**
   * Whether the addressed row carries the controls column.
   *
   * The picker's row is the only place its reader can act from. The Main
   * Window's pane already offers the same three verbs beside the list, and a
   * second ✕ for one Entry is two places to look for the same act.
   */
  controls: boolean;
  /**
   * Whether the Preview carries its own tooltip — the untruncated counterpart
   * of a line that reads as one truncated line, as the Origin beside it always
   * has. Only a list with nothing else on screen holding the text has to supply
   * one: the Main Window's pane *is* that counterpart (ADR 0003).
   */
  previewTooltip: boolean;
  metrics: RowMetrics;
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
  if (entry.undecryptable) {
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
    showToast({ tone: "cyan", text: "COPIED", detail: entry.preview });
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

/**
 * **Resend**, as the popover's `↻` and the reader pane's button both mean it.
 *
 * A fresh act and not a retry (ADR 0015): the core puts it at the back of the
 * queue, so nothing is patched here. `entry-refused` set what this row says about
 * itself, and `entry-settled` or a second `entry-refused` is what changes it —
 * one source of truth rather than an optimistic local guess beside it.
 *
 * Exported for the same reason `copyEntry` and `deleteEntry` are: the main
 * window's reader pane offers the same verb, and a second copy of the error
 * handling is how the two would come to report a failure differently.
 */
export async function resendEntry(entry: EntryView): Promise<void> {
  try {
    await cmd.resendEntry({ user_id: entry.user_id, entry_id: entry.id });
  } catch (e) {
    console.error("resend failed", e);
  }
}

/**
 * What a row's time slot says, which is always the relay's last word about the
 * entry.
 *
 * The tint behind a row says the relay has not heard the latest act; this says
 * what the relay did say, and there is no local clock to stand in when it has
 * said nothing (ADR 0016). An entry the relay has never stamped carries
 * `last_use === 0`, and an age rendered from that reads as 1970 rather than as
 * new, so its slot is empty — nothing is lost beside it, because such an entry
 * was captured here and so never printed an Origin either.
 *
 * `last_use` and not `pending` is what that turns on, because the two are
 * different facts: the tint is about the queue, the slot is about the relay's
 * word. They no longer part company at a flush — `EntrySettled` carries the
 * relay's `created_at` and `last_use`, so a row that settles in place has an age
 * the moment it stops waiting. An entry with a queued *use* keeps the stale age,
 * because that age is still the last thing the relay stamped.
 *
 * Refused wins over Undecryptable on a row that is both: it is the one of the
 * two that can be acted on.
 *
 * Private, now that one row serves both lists. It was exported while each of
 * them made this decision for itself, which is how the two would have come to
 * disagree about it.
 */
type TimeSlot =
  /** A refusal, or a key this device does not hold: the slot's only alert-red. */
  | { tone: "alert"; text: string }
  /** The age the relay stamped, which the Origin joins when it is elsewhere. */
  | { tone: "relay"; age: string }
  /** The relay has never stamped this entry, so it has nothing to say. */
  | { tone: "silent" };

function timeSlot(entry: EntryView, now: number): TimeSlot {
  if (entry.refused_reason !== null) return { tone: "alert", text: entry.refused_reason };
  if (entry.undecryptable) return { tone: "alert", text: "KEY MISMATCH" };
  if (entry.last_use === 0) return { tone: "silent" };
  return { tone: "relay", age: relativeAge(entry.last_use, now) };
}

/**
 * The slot's alert-red treatment, on the one row both lists render.
 *
 * Bounded and truncated, because a refusal reason is the relay's prose rather
 * than one of this shell's own words. 40% of the row leaves every refusal ADR
 * 0015 admits — a 400 or a 413 — uncut at 360px of popover, and holds a verbose
 * one off the Preview, which is what says *which* entry was turned down. The
 * tooltip is the untruncated counterpart, as it is for the Origin beside it.
 */
const ALERT_SLOT =
  "max-w-[40%] shrink-0 truncate text-chrome uppercase tracking-phrase text-alert-400";

const EntryRow = forwardRef<HTMLLIElement, EntryRowProps>(function EntryRow(
  {
    entry,
    index,
    selected,
    ownDeviceId,
    now,
    onPoint,
    onActivate,
    controls,
    previewTooltip,
    metrics,
  },
  ref,
) {
  const { undecryptable } = entry;
  const refused = entry.refused_reason !== null;
  const elsewhere = entry.device_id !== ownDeviceId;
  const slot = timeSlot(entry, now);

  return (
    <li
      ref={ref}
      data-testid="entry-row"
      data-selected={selected}
      data-pending={entry.pending}
      className={`fui-row flex cursor-default items-center px-3 ${metrics.gap}`}
      // Movement, not enter: keyboard nav scrolls rows under a resting pointer,
      // and mouseenter would fire on that and snatch the selection back.
      onMouseMove={onPoint}
      onClick={onActivate}
    >
      {/* Dim measures 4.35:1 on the selected background, just under (plan §1). */}
      <span
        className={`${metrics.index} shrink-0 font-mono text-chrome tabular-nums ${selected ? "text-text-emitter" : "text-text-dim"}`}
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
          title={previewTooltip ? entry.preview : undefined}
        >
          {entry.preview}
        </span>
      )}

      {slot.tone === "alert" && (
        <span className={ALERT_SLOT} title={slot.text}>
          {slot.text}
        </span>
      )}
      {slot.tone === "relay" && (
        <span className="shrink-0 text-chrome tracking-phrase text-text-muted">
          {elsewhere && (
            <>
              {/*
                The tooltip is the untruncated counterpart of what is shown, not
                a second fallback: a label reads in full, an unlabelled legacy
                membership reads its full device id behind the 4-char slice
                (plan §4). Routing it through `origin_label` would hide the id.
              */}
              <span className="uppercase" title={entry.device_label?.trim() || entry.device_id}>
                {entry.origin_label.slice(0, 12)}
              </span>
              {" · "}
            </>
          )}
          {slot.age}
        </span>
      )}

      {/*
        Only the addressed row of a list that asked for the column carries
        controls. Reserving it on every row would leave a 44px hole to the right
        of every timestamp, and paying for it in opacity left two
        invisible-but-clickable buttons on each unaddressed row.
      */}
      {controls && selected && (
        <span className="flex shrink-0 items-center gap-1">
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
          {/* Only a refused act is owed a resend: a pending one is already on
              its way, and a settled one has nothing left to send. */}
          {refused && (
            <IconButton
              label="Resend entry"
              title="Resend to the relay"
              testId={`resend-entry-${entry.id}`}
              className="text-data"
              onClick={(e) => {
                e.stopPropagation();
                void resendEntry(entry);
              }}
            >
              ↻
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
      )}
    </li>
  );
});

export default EntryRow;
