import { useState } from "react";
import { agePhrase, byteSize, capturedAt } from "../../lib/format";
import type { EntryView } from "../../types";
import { copyEntry, deleteEntry } from "../EntryRow";
import { IconButton, PanelMessage } from "../fui";

/**
 * Where the pane stops laying out text and offers the rest on request.
 *
 * Nothing caps an entry's size — not capture, not `entries_cache`, not the
 * relay (plan §2) — and `plaintext` is the complete text, in memory, for every
 * row. So this is a guard on *rendering*, not on the data: `COPY`
 * still copies the whole thing, and the byte count in the header is what
 * explains the cut.
 */
const RENDER_CAP = 65536;

type Props = {
  entry: EntryView | undefined;
  /** 1-based position in the visible list, rendered zero-padded as `ENTRY 04`. */
  index: number;
  /** The device doing the viewing: an entry captured here states no Origin. */
  ownDeviceId: string | undefined;
  /** The pane's single clock, shared with the list beside it. */
  now: number;
};

/**
 * The reading pane: the only surface in the product that shows an entry whole
 * (ADR 0003), where the popover can only collapse it to one line.
 *
 * Unmasked, deliberately and on the record: the list beside it prints the same
 * first line and `COPY` puts the real thing on the system clipboard, so masking
 * here would be theatre.
 */
export default function EntryDetail({ entry, index, ownDeviceId, now }: Props) {
  /*
   * The cap re-arms every time the selection moves, including back onto an
   * entry that was revealed a moment ago: laying out megabytes is the cost the
   * cap exists to avoid, so it stays opt-in each visit.
   *
   * Adjusted during render rather than in an effect, per React's own
   * prop-change pattern: an effect runs *after* paint, so the incoming entry
   * would be laid out whole for one frame before being cut back — precisely
   * the frame being guarded against.
   */
  const [showAll, setShowAll] = useState(false);
  const [shownId, setShownId] = useState(entry?.id);
  if (shownId !== entry?.id) {
    setShownId(entry?.id);
    setShowAll(false);
  }

  if (!entry) {
    return (
      <div className="flex min-h-0 min-w-0 flex-col items-center justify-center border border-hairline bg-void-1000 p-3 text-center text-data text-text-dim">
        Select an entry to read it in full.
      </div>
    );
  }

  const { undecryptable } = entry;
  const elsewhere = entry.device_id !== ownDeviceId;
  /*
   * `plaintext` and not `preview`: this is the one surface that shows an entry
   * whole, and `preview` is one flattened line of it by definition. It is
   * `null` only for an Undecryptable entry, which takes the branch below
   * instead of this text.
   */
  const text = entry.plaintext ?? "";
  const capped = text.length > RENDER_CAP && !showAll;

  return (
    <div className="flex min-h-0 min-w-0 flex-col border border-hairline bg-void-1000">
      <header className="fui-band flex shrink-0 items-center justify-between gap-3 border-b border-hairline px-3 py-1.5">
        <span className="text-label tracking-word text-text-muted">
          ENTRY {String(index).padStart(2, "0")}
        </span>
        <span className="shrink-0 font-mono text-chrome text-text-dim">
          {byteSize(text)}
        </span>
      </header>

      {undecryptable ? (
        <PanelMessage
          title="UNDECRYPTABLE"
          detail="Encrypted with a key this device doesn't have."
        />
      ) : (
        <div
          data-testid="entry-detail-body"
          className="fui-scroll min-h-0 flex-1 overflow-y-auto whitespace-pre-wrap break-words p-3 font-mono text-data leading-relaxed text-text-body"
        >
          {capped ? text.slice(0, RENDER_CAP) : text}
          {capped && (
            <div className="mt-3">
              <button
                type="button"
                data-testid="show-all"
                className="fui-action"
                data-variant="outline"
                onClick={() => setShowAll(true)}
              >
                SHOW ALL
              </button>
            </div>
          )}
        </div>
      )}

      <footer className="fui-band flex shrink-0 flex-col gap-2 border-t border-hairline p-3">
        <span className="font-mono text-chrome uppercase tracking-phrase text-text-dim">
          {/* Origin in full here, where the row could only afford 12 chars. */}
          {elsewhere && `${entry.origin_label} · `}
          CAPTURED {capturedAt(entry.created_at, now)} ·{" "}
          <span className="normal-case">{agePhrase(entry.created_at, now)}</span>
          {/*
            Only when the two differ. An entry never used since capture carries
            `last_use == created_at`, and a USED reading back the capture time
            would state a second event that never happened. The age beside
            CAPTURED stays the capture's: the row in the list already says how
            long ago the Use was, and this pane is where the other fact lives.
          */}
          {entry.last_use !== entry.created_at && ` · USED ${capturedAt(entry.last_use, now)}`}
        </span>
        <span className="flex items-center gap-2">
          {/*
            Disabled rather than hidden for an undecryptable entry: the control
            the reader is looking for has to still be where they are looking,
            saying no. The panel above it gives the reason.
          */}
          <button
            type="button"
            data-testid="detail-copy"
            className="fui-action flex-1 disabled:cursor-not-allowed disabled:bg-void-600 disabled:text-text-dim disabled:hover:bg-void-600"
            data-variant="solid"
            disabled={undecryptable}
            onClick={() => void copyEntry(entry, { keepOpen: true })}
          >
            COPY
          </button>
          {/* Deleting stays live: ciphertext this device cannot read is exactly
              what someone wants gone. */}
          <IconButton
            label="Delete entry"
            tone="alert"
            testId={`detail-delete-${entry.id}`}
            className="text-data"
            onClick={() => void deleteEntry(entry)}
          >
            ✕
          </IconButton>
        </span>
      </footer>
    </div>
  );
}
