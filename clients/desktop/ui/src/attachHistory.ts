import { cmd, HISTORY_PAGE } from "./ipc/commands";
import { events } from "./ipc/events";
import {
  hydrateFrom,
  noteChange,
  useContactStore,
  useHistoryStore,
  usePairingsStore,
  useStatusStore,
  useUiStore,
} from "./store";
import type { ConnectionState, EntryView } from "./types";

/**
 * Which Pairing's History a surface is showing, and whether this attach is what
 * puts it on screen.
 */
export type HistoryScope = {
  /**
   * The Pairing on screen, read at call time rather than closed over: the
   * listeners below are registered once and must not pin the Pairing they were
   * born with.
   */
  userId: () => string | undefined;
  /**
   * Whether this attach also shows the History it is scoped to.
   *
   * The popover has one pane and one scope — the Active Pairing — so the attach
   * shows it, and re-shows it whenever the Active Pairing moves. The main
   * window's `HistorySection` shows the **Viewed Pairing** on its own, keyed on
   * the pairing the reader picked, so an attach that also showed it would take
   * the same snapshot twice.
   *
   * It decides how wide Contact is seeded for the same reason: a surface that
   * does not show its own scope here has more than one Pairing on screen at
   * once — the main window's footer names the Active Pairing while its pane
   * names the Viewed one — so every Pairing's Contact has to be on hand before
   * either is chosen.
   */
  showsHistory: boolean;
};

/**
 * Put a surface on the live History and keep it there, returning its teardown.
 *
 * Every surface that shows a History needs the same six things right, and
 * before this they each re-derived them:
 *
 * 1. **Every subscription a relay drives comes before the first `await`.** The
 *    four entry events, and the two readings rule 4 seeds. This is anomaly A of
 *    `.scratch/mobile-client/issues/06`, reproduced twice on a Windows smoke
 *    run: an offline burst flushed, the relay gained every row, and one of them
 *    was on screen afterwards. An Entry the uploader caches between a
 *    `list_history` and the subscription is announced to a listener that does
 *    not exist and absent from that snapshot, and it stays lost — the relay's
 *    echo of an Entry this device uploaded deliberately raises no event, and a
 *    backfill that ingests only rows the cache already holds does not advance
 *    the watermark and so raises none either.
 *
 *    The three a *user* drives — a Pairing added, forgotten, or made Active —
 *    stay below the snapshot, where their handlers can assume a hydrated list.
 *    Nothing but a hand produces them, and no hand reaches this app in the
 *    fraction of a second between its window opening and this line.
 * 2. **`noteChange` fires on all four entry events, before the store
 *    mutation.** Subscribing first is only half the fix: a snapshot requested
 *    before a change and applied after it would undo it. [`hydrateFrom`]
 *    replays what was recorded.
 * 3. **Every snapshot carries a staleness guard**, because the History store
 *    keys nothing per user and a slow answer for the Pairing just left must not
 *    land on top of the one now shown.
 * 4. **Status is seeded from `list_pairings`**, which already knows each
 *    session's state — and the seed yields, per field, to anything the live
 *    subscription has already said. Without the seed a surface reads
 *    Disconnected until the next transition happens to fire. Without the yield
 *    it reads whatever the snapshot caught mid-connect, because rule 2's
 *    hazard applies here too and there is no replay to undo it. Either way it
 *    is wrong until the *next* transition, and a healthy session has none:
 *    `SessionCtx::transition` is the only thing that speaks, and it speaks only
 *    on an edge. The popover is where this showed — `build_popover_window` runs
 *    one line before `spawn_sync_for_existing_pairings`, so the session reaches
 *    Online while the webview is still attaching, and the window is shown and
 *    hidden rather than remounted, so nothing ever takes a second snapshot.
 * 5. **Contact is asked for by command**, because it is stamped by traffic the
 *    surface was not open for and no event fires until the next byte. It needs
 *    no yield of its own: `get_contact` prefers the same live cell the event's
 *    reading was flushed out of, so the answer to the later question is never
 *    the older reading.
 * 6. **Every handle is released on teardown.**
 *
 * A verb and not a noun: *session* already names a live Relay connection in
 * `clients/core`, and one word meaning two things is the fault ADR 0004 was
 * written to remove.
 */
export function attachHistory(scope: HistoryScope): () => void {
  const unsub: Array<() => void> = [];
  let detached = false;
  const stale = () => detached;
  const inScope = (user_id: string) => user_id === scope.userId();
  const keep = (off: () => void) => (detached ? off() : unsub.push(off));
  // Rule 4's yield. Membership, not history: the newest reading is the one in
  // the store already, and these only say the snapshot has nothing to add.
  const liveState = new Map<string, ConnectionState>();
  const livePending = new Set<string>();

  void (async () => {
    // Rule 1: these four first, before this function awaits anything else —
    // above all before the `list_history` an Entry announced here has to
    // survive.
    keep(await events.onEntryAdded(({ user_id, entry }) => {
      noteChange({ kind: "added", user_id, entry });
      if (inScope(user_id)) useHistoryStore.getState().add(entry);
    }));
    keep(await events.onEntryDeleted(({ user_id, entry_id }) => {
      noteChange({ kind: "deleted", user_id, entry_id });
      if (inScope(user_id)) useHistoryStore.getState().remove(entry_id);
    }));
    // In place and by id, with no refetch: nothing reorders at a flush and the
    // id does not change, so the cursor the surface holds — the popover's
    // keyboard selection, the reader's row — stays where it was. The relay's
    // stamp rides along, or the row would stop waiting and go on saying the
    // relay has never stamped it.
    keep(await events.onEntrySettled(({ user_id, entry_id, created_at, last_use }) => {
      noteChange({ kind: "settled", user_id, entry_id, created_at, last_use });
      if (inScope(user_id)) useHistoryStore.getState().settle(entry_id, created_at, last_use);
    }));
    keep(await events.onEntryRefused(({ user_id, entry_id, reason }) => {
      noteChange({ kind: "refused", user_id, entry_id, reason });
      if (inScope(user_id)) useHistoryStore.getState().refuse(entry_id, reason);
    }));

    // Rule 1's other half. These two are relay-driven, so they fire during the
    // attach itself; the seed below defers to whatever they have already
    // written. `pairing-added` and the rest are further down, where a hydrated
    // list is safe to assume.
    keep(await events.onConnectionState(({ user_id, state, last_error }) => {
      liveState.set(user_id, state);
      useStatusStore.getState().set(
        user_id,
        last_error !== undefined ? { state, last_error } : { state },
      );
      usePairingsStore.getState().updateStatus(user_id, state);
    }));
    keep(await events.onPendingCount(({ user_id, count }) => {
      livePending.add(user_id);
      useStatusStore.getState().set(user_id, { pending: count });
    }));
    keep(await events.onContact(({ user_id, last_contact_at }) => {
      useContactStore.getState().setLastContact(user_id, last_contact_at);
    }));

    const pairings = await cmd.listPairings();
    if (detached) return;
    usePairingsStore.getState().hydrate(pairings);
    // The rows arrive carrying the same snapshot the seed below is yielding on,
    // so a state already announced has to be put back on top of them.
    for (const [user_id, state] of liveState) {
      usePairingsStore.getState().updateStatus(user_id, state);
    }
    // Rule 4: the state each session is already in, for the Pairings the two
    // subscriptions above have not spoken for.
    for (const p of pairings) {
      const patch: { state?: ConnectionState; pending?: number } = {};
      if (!liveState.has(p.user_id)) patch.state = p.status;
      if (!livePending.has(p.user_id)) patch.pending = p.pending;
      if (patch.state !== undefined || patch.pending !== undefined) {
        useStatusStore.getState().set(p.user_id, patch);
      }
    }
    // Rule 5, at whichever width this scope needs it — and, for a surface that
    // shows its own scope, the first snapshot the four above exist to survive.
    if (scope.showsHistory) void showHistory(scope.userId(), stale);
    else for (const p of pairings) seedContact(p.user_id, stale);

    keep(await events.onHistoryChanged(({ user_id }) => {
      // A refetch, not a re-show: the rows moved, not what the surface is
      // looking at, so Contact is not asked for again.
      if (inScope(user_id)) void loadHistory(user_id, stale);
    }));
    keep(await events.onPairingAdded(() => {
      cmd.listPairings().then(usePairingsStore.getState().hydrate).catch(() => {});
    }));
    keep(await events.onPairingRemoved(({ user_id }) => {
      usePairingsStore.getState().remove(user_id);
      // The Viewed Pairing can outlive the pairing it named; drop it so the
      // pane falls back to the Active one rather than showing a ghost.
      if (useUiStore.getState().viewedUserId === user_id) {
        useUiStore.getState().setViewedUserId(undefined);
      }
    }));
    keep(await events.onActivePairingChanged(({ user_id }) => {
      usePairingsStore.getState().setActive(user_id ?? undefined);
      // Only for a surface whose scope *is* the Active Pairing; anywhere else
      // this event has just moved the footer, not the pane.
      if (scope.showsHistory) void showHistory(scope.userId(), stale);
    }));
  })();

  return () => {
    detached = true;
    for (const off of unsub) off();
    unsub.length = 0;
  };
}

/**
 * Show one Pairing's History: its rows, and the Contact band above them.
 *
 * Resolves to the rows applied, or `undefined` when there was nothing to show
 * or `stale` says the answer is for a Pairing no longer on screen. `HistoryList`
 * keys nothing per user, so a slow response for the pairing just left would
 * otherwise land on top of the one now shown.
 *
 * `HistorySection` calls this directly, because the **Viewed Pairing** it shows
 * moves without any event saying so.
 */
export function showHistory(
  userId: string | undefined,
  stale?: () => boolean,
): Promise<EntryView[] | undefined> {
  if (userId === undefined) {
    useHistoryStore.getState().hydrate([]);
    return Promise.resolve(undefined);
  }
  // Started before the Contact command so the History is the first thing on the
  // wire: it is what the surface is waiting to paint.
  const rows = loadHistory(userId, stale);
  seedContact(userId, stale);
  return rows;
}

/** The rows alone, for a change that moved them without moving the scope. */
function loadHistory(
  userId: string,
  stale?: () => boolean,
): Promise<EntryView[] | undefined> {
  return hydrateFrom(
    userId,
    () => cmd.listHistory({ user_id: userId, limit: HISTORY_PAGE }),
    stale,
  ).catch((e) => {
    console.error("list history failed", e);
    return undefined;
  });
}

/** Rule 5 for one Pairing. */
function seedContact(userId: string, stale?: () => boolean): void {
  cmd.getContact({ user_id: userId })
    .then((c) => {
      if (c && !stale?.()) useContactStore.getState().setLastContact(c.user_id, c.last_contact_at);
    })
    .catch((e) => {
      // The band falls back to NEVER, which is the honest reading of "no
      // contact record this device can produce". A backend older than this
      // command answers undefined rather than rejecting.
      console.error("get contact failed", e);
    });
}
