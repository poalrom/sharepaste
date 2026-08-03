import { useEffect, useRef, type ReactNode } from "react";
import { cmd } from "../../ipc/commands";
import { agePhrase } from "../../lib/format";
import {
  hydrateFrom,
  useContactStore,
  useFilteredEntries,
  useHistoryStore,
  usePairingsStore,
  useUiStore,
} from "../../store";
import { ALERT_SLOT, copyEntry, deleteEntry, timeSlot } from "../EntryRow";
import { PanelMessage, Strip } from "../fui";
import EntryDetail from "./EntryDetail";

/** The cap `entries_cache` prunes at; the sentinel names it where it bites. */
const CACHE_CAP = 100;

/**
 * Each platform's keys, named the way that platform's own keycaps are — the
 * same rule `HintStrip` applies, restated rather than imported: that strip is a
 * three-binding row sized for 360px of popover, this is two bindings sharing a
 * line with a filter, and one shared array would tie the two layouts together
 * for the sake of one shared word.
 */
const IS_MAC = /mac|iphone|ipad|ipod/i.test(navigator.platform || navigator.userAgent);
const KEY = IS_MAC
  ? { mod: "⌘", shift: "⇧", enter: "⏎", back: "⌫", join: "" }
  : { mod: "CTRL", shift: "SHIFT", enter: "ENTER", back: "BKSP", join: "+" };

/* Each platform prints its modifier chain in its own order: macOS puts ⇧
   before ⌘, Windows names CTRL first. */
const DELETE_KEYS = IS_MAC ? [KEY.shift, KEY.mod, KEY.back] : [KEY.mod, KEY.shift, KEY.back];

/* No arrow hint, no `DEL`, and nothing for the filter's own `⌘⌫`: ADR 0002 cut
   the first as chrome that only restates the obvious, the second because it
   reads as the Delete key, and the third is the text field behaving the way
   the platform's own text fields do (ADR 0013). */
const HINTS = [
  { keys: [KEY.enter], action: "COPY" },
  { keys: DELETE_KEYS, action: "DELETE" },
];

/**
 * History as a reader: the popover's list beside a pane that renders the
 * selected entry in full (ADR 0003).
 *
 * The pairing selector at the top sets the **Viewed Pairing** and nothing else.
 * Every entry command is user-scoped and none require the pairing to be active,
 * so reading one pairing while the device syncs another costs no backend call —
 * which is exactly why the choice must not turn into `set_active_pairing`.
 */
export default function HistorySection({ now }: { now: number }) {
  const entries = useHistoryStore((s) => s.entries);
  const hydrate = useHistoryStore((s) => s.hydrate);
  const filtered = useFilteredEntries();
  const filter = useUiStore((s) => s.filter);
  const setFilter = useUiStore((s) => s.setFilter);
  const selectedIndex = useUiStore((s) => s.selectedIndex);
  const setSelectedIndex = useUiStore((s) => s.setSelectedIndex);
  const viewedUserId = useUiStore((s) => s.viewedUserId);
  const setViewedUserId = useUiStore((s) => s.setViewedUserId);
  const pairings = usePairingsStore((s) => s.pairings);
  const activeUserId = usePairingsStore((s) => s.active);
  const setLastContact = useContactStore((s) => s.setLastContact);

  // Undefined only while pairings are still loading, or when this device holds
  // none at all; an unset Viewed Pairing means "follow the Active one".
  const viewed = viewedUserId ?? activeUserId;
  const pairing = pairings.find((p) => p.user_id === viewed);
  const lastContactAt = useContactStore((s) =>
    viewed === undefined ? null : s.lastContactByUser[viewed] ?? null,
  );

  const selectedRef = useRef<HTMLLIElement | null>(null);

  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex, filtered.length]);

  useEffect(() => {
    if (viewed === undefined) {
      hydrate([]);
      return;
    }
    // `store/history.ts` keys nothing per user, so a slow response for the
    // pairing we just left would land on top of the one now on screen.
    let cancelled = false;
    void (async () => {
      try {
        const rows = await hydrateFrom(
          viewed,
          () => cmd.listHistory({ user_id: viewed, limit: CACHE_CAP }),
          () => cancelled,
        );
        if (rows === undefined) return;
        // The popover's handoff, consumed once: it named an entry, not a
        // position, and only the hydrated list can turn one into the other.
        const { seedEntryId, setSeedEntryId } = useUiStore.getState();
        if (seedEntryId === undefined) return;
        const at = rows.findIndex((e) => e.id === seedEntryId);
        if (at >= 0) setSelectedIndex(at);
        // Cleared either way: a stale id selects nothing, and left set it would
        // fire again on the next pairing the reader switches to.
        setSeedEntryId(undefined);
      } catch (e) {
        console.error("list history failed", e);
      }
    })();
    void (async () => {
      try {
        const contact = await cmd.getContact({ user_id: viewed });
        if (!cancelled && contact) setLastContact(contact.user_id, contact.last_contact_at);
      } catch (e) {
        // The band falls back to NEVER, which is the honest reading of "no
        // contact record this device can produce".
        console.error("get contact failed", e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [viewed, hydrate, setSelectedIndex, setLastContact]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      /*
        The filter holds focus from the moment the pane mounts, and that is the
        resting state rather than an exception — a binding that dies while it
        does is a binding that never fires. So every one of these works from
        inside the filter.

        `⌘⌫`/`Ctrl+⌫` is the field's, not the list's: both platforms already
        bind it inside a text field — delete-to-line-start on macOS,
        delete-previous-word on Windows — so taking it for the list made a
        native editing key destroy an entry (ADR 0013). Deleting adds ⇧, which
        neither platform reserves for editing.

        Only a `<select>` and a `<textarea>` are skipped: arrows and Enter walk
        and commit an option list, and neither is the list's to intercept.
      */
      if (e.target instanceof HTMLElement && e.target.closest("select, textarea")) return;
      const target = filtered[selectedIndex];
      if (e.key === "Backspace" && (e.metaKey || e.ctrlKey)) {
        // Prevented either way: with the query cleared here, the browser's own
        // delete-to-line-start would fire against a value that no longer exists.
        e.preventDefault();
        if (e.shiftKey) {
          if (target) void deleteEntry(target);
        } else {
          setFilter("");
        }
        return;
      }
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        if (filtered.length === 0) return;
        const step = e.key === "ArrowDown" ? 1 : -1;
        // Clamped, where the popover wraps: ten rows in a picker are a ring you
        // spin, a hundred rows beside a reading pane are a document you walk,
        // and wrapping off the end of one reads as having lost your place.
        setSelectedIndex(Math.min(filtered.length - 1, Math.max(0, selectedIndex + step)));
      } else if (e.key === "Enter" && target) {
        // Copies and stays. A window has nothing to get out of the way of, so
        // there is no `⌘⏎` here and no hide (ADR 0003).
        void copyEntry(target, { keepOpen: true });
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [filtered, selectedIndex, setSelectedIndex, setFilter]);

  /*
    The sentinel below is a statement about retention, so only the rows the
    relay has ordered count toward the cap it names. Nothing bounds the
    un-flushed region — evicting an act this device has not delivered to protect
    a display invariant is the trade ADR 0014 refuses — so a page of un-flushed
    captures is a page of rows the cap never touched, and counting them would
    announce a limit that has not bitten.
  */
  const settled = entries.filter((e) => !e.pending).length;

  // Exactly one of: the rows, or the reason there are none.
  let list: ReactNode;
  if (pairings.length === 0) {
    list = (
      <PanelMessage
        title="NO PAIRINGS ON THIS DEVICE"
        detail="This device holds no keys."
        action={{
          label: "ADD A PAIRING",
          onClick: () => {
            const ui = useUiStore.getState();
            ui.setMainSection("pairings");
            ui.setPairingFlowOpen(true);
          },
        }}
      />
    );
  } else if (entries.length === 0) {
    list = <PanelMessage title="HISTORY EMPTY" />;
  } else if (filtered.length === 0) {
    list = (
      <PanelMessage
        title="NO MATCHES"
        detail={`Nothing matches "${filter}"`}
        action={{ label: "CLEAR FILTER", onClick: () => setFilter(""), variant: "outline" }}
      />
    );
  } else {
    list = (
      <ul>
        {filtered.map((entry, i) => {
          const selected = i === selectedIndex;
          // An undecryptable row still counts, so the index stays continuous.
          const { undecryptable } = entry;
          const elsewhere = entry.device_id !== pairing?.device_id;
          const slot = timeSlot(entry, now);
          return (
            <li
              key={entry.id}
              ref={selected ? selectedRef : undefined}
              data-testid="main-entry-row"
              data-selected={selected}
              data-pending={entry.pending}
              className="fui-row flex cursor-default items-center gap-2.5 px-3"
              // Movement, not enter: keyboard nav scrolls rows under a resting
              // pointer, and mouseenter would fire on that and snatch the
              // selection back.
              onMouseMove={() => setSelectedIndex(i)}
              // Addressing a row is all a click does here. The pane beside it
              // is what reads, and its COPY is what copies.
              onClick={() => setSelectedIndex(i)}
            >
              <span
                className={`w-[18px] shrink-0 font-mono text-chrome tabular-nums ${selected ? "text-text-emitter" : "text-text-dim"}`}
              >
                {String(i + 1).padStart(2, "0")}
              </span>

              {undecryptable ? (
                <span className="min-w-0 flex-1 truncate text-label tracking-word text-alert-400">
                  UNDECRYPTABLE
                </span>
              ) : (
                <span className="min-w-0 flex-1 truncate font-mono text-data text-text-body">
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
                        The tooltip is the untruncated counterpart of what is
                        shown, not a second fallback: a label reads in full, an
                        unlabelled legacy membership reads its full device id
                        behind the 4-char slice. Routing it through
                        `origin_label` would hide the id.
                      */}
                      <span
                        className="uppercase"
                        title={entry.device_label?.trim() || entry.device_id}
                      >
                        {entry.origin_label.slice(0, 12)}
                      </span>
                      {" · "}
                    </>
                  )}
                  {slot.age}
                </span>
              )}
            </li>
          );
        })}
        {/*
          Only at the cap, and only unfiltered: a user with nine entries must
          never be shown a limit that has not bitten them, and a filtered list
          is short for a reason of the reader's own making.
        */}
        {!filter.trim() && settled >= CACHE_CAP && (
          <li
            data-testid="list-end"
            className="px-3 py-2 text-center text-chrome tracking-phrase text-text-dim"
          >
            — OLDEST OF {CACHE_CAP} KEPT —
          </li>
        )}
      </ul>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 p-3.5">
      {/*
        A pairing that is not the Active one has no session, so `list_history`
        handed back a frozen snapshot: no entry will arrive here and nothing
        this device captures will join it. The footer describes the Active
        Pairing and the rows describe this one, so without the band the two
        would silently disagree.
      */}
      {pairing !== undefined && pairing.user_id !== activeUserId && (
        <Strip tone="standby" testId="viewed-band">
          <span>
            NOT SYNCING · LAST CONTACT{" "}
            {lastContactAt === null ? (
              "NEVER"
            ) : (
              <span className="normal-case">{agePhrase(lastContactAt, now)}</span>
            )}
          </span>
          <button
            type="button"
            data-testid="make-active"
            className="fui-action ml-auto"
            data-variant="outline"
            onClick={() => void cmd.setActivePairing({ user_id: pairing.user_id })}
          >
            MAKE ACTIVE
          </button>
        </Strip>
      )}

      <div className="flex shrink-0 items-center gap-3">
        {/*
          Only worth a control when there is a choice to make: one pairing has
          nothing to switch between, and the footer already names it.
        */}
        {pairings.length > 1 && (
          <select
            data-testid="viewed-pairing"
            aria-label="Viewed pairing"
            className="fui-field w-auto"
            value={viewed ?? ""}
            // Viewing, not switching. Making this call `set_active_pairing`
            // would move the device's capture and sync target every time
            // someone glanced at another pairing's history (ADR 0003).
            onChange={(e) => setViewedUserId(e.target.value)}
          >
            {pairings.map((p) => (
              <option key={p.user_id} value={p.user_id}>
                {`${p.username ?? p.user_id} @ ${p.relay_host}`}
              </option>
            ))}
          </select>
        )}

        {/*
          Prefix and count sit inside the field's box rather than beside it:
          floated outside they read as two more controls in a row that already
          has three, instead of as parts of the one thing being typed into.
        */}
        <div className="relative flex min-w-0 flex-1 items-center">
          <span
            aria-hidden="true"
            className="pointer-events-none absolute left-2.5 text-text-dim"
          >
            ⌕
          </span>
          <input
            // The pane exists to be typed into: without this the window opens
            // keyboard-dead and the filter reads as decoration.
            autoFocus
            /*
              A needle is a fragment, not prose: macOS reads `tail` as a
              misspelling and floats its own `Tail ×` correction bubble over
              the first row, and would capitalise it. Nothing here is dictated
              to a person, so every text service the field can decline, it does.
            */
            spellCheck={false}
            autoCorrect="off"
            autoCapitalize="off"
            className="fui-field min-w-0 flex-1 pl-7 pr-14"
            placeholder="Filter history…"
            aria-label="Filter history"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <span className="pointer-events-none absolute right-2.5 text-chrome tabular-nums text-text-dim">
            {filtered.length}/{entries.length}
          </span>
        </div>

        <span className="flex shrink-0 items-center gap-3">
          {HINTS.map((hint) => (
            <span key={hint.action} className="flex items-center gap-1.5">
              <kbd className="fui-key">{hint.keys.join(KEY.join)}</kbd>
              <span className="text-label tracking-phrase text-text-muted">{hint.action}</span>
            </span>
          ))}
        </span>
      </div>

      {/*
        `minmax(0, 1fr)` on the row, not just `min-h-0` on the container: an
        auto row sizes to a hundred rows of content, which would push both
        columns past the pane and scroll the window instead of themselves.
      */}
      <div
        className="grid min-h-0 flex-1 gap-3"
        style={{ gridTemplateColumns: "1fr minmax(260px, 320px)", gridTemplateRows: "minmax(0, 1fr)" }}
      >
        <div className="fui-scroll flex min-w-0 flex-col overflow-y-auto border border-hairline bg-void-1000">
          {list}
        </div>

        <EntryDetail
          entry={filtered[selectedIndex]}
          index={selectedIndex + 1}
          ownDeviceId={pairing?.device_id}
          now={now}
        />
      </div>
    </div>
  );
}
